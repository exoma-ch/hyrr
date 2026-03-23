"""3D compute orchestrator for HYRR.

Runs pencil-beam transport through a TetrahedralMesh, computing
production rates and activities per ray segment. Uses Rust backend
for stopping power and Bateman equations, with Python math helpers
for per-segment integration and Bohr straggling.
"""

from __future__ import annotations

from collections import defaultdict
from dataclasses import dataclass, field
from typing import TYPE_CHECKING

import numpy as np
import numpy.typing as npt
import scipy.constants as const

from hyrr.geometry import RaySegment, TetrahedralMesh, cast_pencil_beam
from hyrr.models import Beam, IsotopeResult
from hyrr._math_utils import compute_production_rate
from hyrr._native_bridge import bateman_activity, dedx_MeV_per_cm, saturation_yield
from hyrr._straggling import bohr_straggling_variance_per_cm

if TYPE_CHECKING:
    from hyrr.db import DatabaseProtocol


@dataclass
class TetResult:
    """Accumulated result for a single tetrahedron."""

    tet_index: int
    material_id: int
    isotope_production_rates: dict[str, float] = field(default_factory=dict)
    energy_deposited_MeV: float = 0.0
    volume_cm3: float = 0.0
    n_rays_hit: int = 0


@dataclass
class SolidResult:
    """Aggregated result for a CAD solid (by material_id)."""

    material_id: int
    material_name: str
    isotope_results: dict[str, IsotopeResult] = field(default_factory=dict)


@dataclass
class Geometry3DResult:
    """Full result from a 3D pencil-beam simulation."""

    tet_results: dict[int, TetResult]
    solid_results: dict[int, SolidResult]
    n_rays: int
    beam: Beam


def compute_3d(
    db: DatabaseProtocol,
    mesh: TetrahedralMesh,
    beam: Beam,
    irradiation_time_s: float,
    cooling_time_s: float,
    n_rays: int = 19,
    *,
    beam_position: npt.NDArray[np.float64] | None = None,
    beam_direction: npt.NDArray[np.float64] | None = None,
    beam_radius_cm: float | None = None,
    progress: bool = True,
) -> Geometry3DResult:
    """Run a 3D pencil-beam simulation through a tetrahedral mesh.

    Per ray, per segment: computes production rates using the segment's
    energy_in/out and material. Straggling accumulates along each ray.
    Results are aggregated per-tet and per-solid (material_id).

    Beam position, direction, and radius are read from ``beam.position``,
    ``beam.direction``, and ``beam.profile.spot_radius_cm`` by default.
    Explicit keyword arguments override the beam's values.

    Args:
        db: Nuclear data provider.
        mesh: Tetrahedral mesh with materials.
        beam: Beam specification (includes profile, position, direction).
        irradiation_time_s: Irradiation duration [s].
        cooling_time_s: Cooling duration [s].
        n_rays: Number of rays in pencil beam.
        beam_position: Override beam center origin [cm], shape (3,).
        beam_direction: Override beam direction (unit vector), shape (3,).
        beam_radius_cm: Override beam spot radius [cm].
        progress: Show tqdm progress bar (default True).

    Returns:
        Geometry3DResult with per-tet and per-solid production.
    """
    from hyrr.projectile import resolve_projectile

    proj = resolve_projectile(beam.projectile)

    # Resolve position/direction/radius from beam or overrides
    pos = beam_position if beam_position is not None else beam.position_array
    dir_ = beam_direction if beam_direction is not None else beam.direction_array
    if beam_radius_cm is not None:
        radius = beam_radius_cm
    elif beam.profile is not None:
        radius = beam.profile.spot_radius_cm
    else:
        radius = 0.0  # pencil beam

    rays = cast_pencil_beam(mesh, pos, dir_, radius, n_rays)

    # Per-tet accumulators
    tet_results: dict[int, TetResult] = {}
    # Per-solid (material_id) rate accumulators
    solid_rates: dict[int, dict[str, float]] = defaultdict(lambda: defaultdict(float))

    if progress:
        from tqdm.auto import tqdm

        ray_iter = tqdm(rays, desc="Rays", unit="ray")
    else:
        ray_iter = rays

    for ray_segments in ray_iter:
        _process_ray(
            db,
            mesh,
            beam,
            proj,
            ray_segments,
            irradiation_time_s,
            tet_results,
            solid_rates,
        )

    # Build SolidResults with Bateman equations
    solid_results: dict[int, SolidResult] = {}
    solid_items = list(solid_rates.items())
    if progress:
        from tqdm.auto import tqdm

        solid_items = tqdm(solid_items, desc="Bateman", unit="solid")

    for mat_id, rates in solid_items:
        mat_info = mesh.materials[mat_id]
        iso_results: dict[str, IsotopeResult] = {}

        for iso_name, total_rate in rates.items():
            # Average over rays
            avg_rate = total_rate / n_rays

            # Look up decay data by parsing name
            Z, A, state = _parse_isotope_name(db, iso_name)
            decay = db.get_decay_data(Z, A, state)
            half_life = decay.half_life_s if decay else None

            time_grid, activity = bateman_activity(
                avg_rate,
                half_life,
                irradiation_time_s,
                cooling_time_s,
            )
            sat_yield = saturation_yield(avg_rate, half_life, beam.current_mA)

            iso_results[iso_name] = IsotopeResult(
                name=iso_name,
                Z=Z,
                A=A,
                state=state,
                half_life_s=half_life,
                production_rate=avg_rate,
                saturation_yield_Bq_uA=sat_yield,
                activity_Bq=float(activity[-1]) if len(activity) > 0 else 0.0,
                time_grid_s=time_grid,
                activity_vs_time_Bq=activity,
            )

        solid_results[mat_id] = SolidResult(
            material_id=mat_id,
            material_name=mat_info.name,
            isotope_results=iso_results,
        )

    return Geometry3DResult(
        tet_results=tet_results,
        solid_results=solid_results,
        n_rays=n_rays,
        beam=beam,
    )


def _process_ray(
    db: DatabaseProtocol,
    mesh: TetrahedralMesh,
    beam: Beam,
    proj,
    segments: list[RaySegment],
    irradiation_time_s: float,
    tet_results: dict[int, TetResult],
    solid_rates: dict[int, dict[str, float]],
) -> None:
    """Process a single ray through the mesh, accumulating results."""
    energy = beam.energy_MeV
    sigma_sq = beam.energy_spread_MeV**2

    for seg in segments:
        mat_info = mesh.materials[seg.material_id]
        composition = mat_info.composition
        density = mat_info.density_g_cm3

        # Stopping power closure
        def dedx_fn(E, _comp=composition, _dens=density):
            return dedx_MeV_per_cm(db, beam.projectile, _comp, _dens, E)

        # Energy out for this segment
        energy_out = _compute_segment_energy_out(
            dedx_fn,
            energy,
            seg.path_length_cm,
            n_steps=100,
        )
        if energy_out <= 0:
            break

        # Straggling
        dsig2_dz = bohr_straggling_variance_per_cm(
            proj.Z,
            composition,
            density,
            mat_info.atomic_masses,
        )
        sigma_E_out_sq = sigma_sq + dsig2_dz * seg.path_length_cm

        # Build sigma_E_fn for this segment
        _s0_sq = sigma_sq
        _ds2 = dsig2_dz
        sigma_E_fn = None
        if _s0_sq > 1e-18 or _ds2 * seg.path_length_cm > 1e-18:

            def sigma_E_fn(z, s0=_s0_sq, ds=_ds2):
                return (s0 + ds * z) ** 0.5

        # Volume and atom count for the segment
        # Approximate: cylindrical segment with beam area
        area = np.pi * 1.0**2  # placeholder 1 cm² per ray
        volume = seg.path_length_cm * area
        avg_A = sum(w * mat_info.atomic_masses[Z] for Z, w in composition) / sum(
            w for _, w in composition
        )
        n_atoms = density * volume * const.Avogadro / avg_A

        # Initialize tet result
        if seg.tet_index not in tet_results:
            tet_results[seg.tet_index] = TetResult(
                tet_index=seg.tet_index,
                material_id=seg.material_id,
            )
        tet_results[seg.tet_index].n_rays_hit += 1
        tet_results[seg.tet_index].energy_deposited_MeV += energy - energy_out
        tet_results[seg.tet_index].volume_cm3 = volume

        # Production for each cross-section in the material
        # This requires knowing which isotopes are in the material
        # For now, we query all elements in the composition
        for Z_i, w_i in composition:
            if w_i <= 0:
                continue
            abundances = db.get_natural_abundances(Z_i)
            for A_iso, (frac_abundance, _atomic_mass) in abundances.items():
                weight = w_i * frac_abundance
                xs_list = db.get_cross_sections(beam.projectile, Z_i, A_iso)
                for xs in xs_list:
                    prate, _, _, _ = compute_production_rate(
                        xs.energies_MeV,
                        xs.xs_mb,
                        dedx_fn,
                        energy_in_MeV=energy,
                        energy_out_MeV=energy_out,
                        n_target_atoms=n_atoms,
                        beam_particles_per_s=beam.particles_per_second,
                        target_volume_cm3=volume,
                        sigma_E_fn=sigma_E_fn,
                    )

                    scaled_rate = prate * weight

                    symbol = db.get_element_symbol(xs.residual_Z)
                    state_suffix = xs.state if xs.state else ""
                    name = f"{symbol}-{xs.residual_A}{state_suffix}"

                    tet_results[seg.tet_index].isotope_production_rates[name] = (
                        tet_results[seg.tet_index].isotope_production_rates.get(
                            name, 0.0
                        )
                        + scaled_rate
                    )
                    solid_rates[seg.material_id][name] += scaled_rate

        # Propagate to next segment
        energy = energy_out
        sigma_sq = sigma_E_out_sq


def _compute_segment_energy_out(
    dedx_fn,
    energy_in: float,
    thickness_cm: float,
    n_steps: int = 100,
) -> float:
    """Forward-Euler energy loss through a segment."""
    dx = thickness_cm / n_steps
    energy = energy_in
    for _ in range(n_steps):
        loss = float(np.asarray(dedx_fn(energy)).flat[0]) * dx
        energy -= loss
        if energy <= 0:
            return 0.0
    return energy


def _parse_isotope_name(
    db: DatabaseProtocol,
    name: str,
) -> tuple[int, int, str]:
    """Parse 'Tc-99m' → (Z, A, state)."""
    # Split on '-'
    parts = name.split("-")
    symbol = parts[0]
    rest = parts[1] if len(parts) > 1 else ""

    # Extract state suffix (m, g, etc.)
    state = ""
    a_str = rest
    for suffix in ("m2", "m", "g"):
        if rest.endswith(suffix) and rest[: -len(suffix)].isdigit():
            state = suffix
            a_str = rest[: -len(suffix)]
            break

    Z = db.get_element_Z(symbol)
    A = int(a_str) if a_str.isdigit() else 0
    return Z, A, state
