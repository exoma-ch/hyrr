"""Stopping power calculations.

PSTAR/ASTAR table lookup with interpolation, Bragg additivity
for compounds, velocity scaling for d/t/³He projectiles, and
pycatima (SRIM-based) fallback when no tables are available.

Stopping power source is tracked per element and exposed via
:func:`get_stopping_source` for downstream reporting.

Energy straggling (Bohr formula) is also provided here.
"""

from __future__ import annotations

import logging
import math
from collections.abc import Callable
from typing import TYPE_CHECKING

import numpy as np
from scipy.interpolate import interp1d

if TYPE_CHECKING:
    from hyrr.db import DatabaseProtocol

logger = logging.getLogger(__name__)

# Stopping power source labels
SOURCE_PSTAR = "PSTAR"
SOURCE_ASTAR = "ASTAR"
SOURCE_CATIMA = "pycatima/SRIM"


def _make_interpolator(
    energies_MeV: np.ndarray,
    dedx: np.ndarray,
) -> Callable[[float], float]:
    """Create a log-log cubic interpolator for stopping power data.

    Uses log-log scale for better accuracy across orders of magnitude.
    Extrapolation is allowed at boundaries using fill_value="extrapolate".
    """
    log_E = np.log(energies_MeV)
    log_S = np.log(dedx)
    kind = "cubic" if len(energies_MeV) >= 4 else "linear"
    interp = interp1d(log_E, log_S, kind=kind, fill_value="extrapolate")  # pyright: ignore[reportArgumentType]

    def lookup(energy: float | np.ndarray) -> float | np.ndarray:
        result = np.exp(interp(np.log(energy)))
        if np.ndim(energy) == 0:
            return float(result)
        return result

    return lookup


def _generate_catima_table(
    source: str,
    target_Z: int,
) -> tuple[np.ndarray, np.ndarray]:
    """Generate a stopping power table using pycatima (SRIM-based).

    Returns (energies_MeV, dedx_MeV_cm2_per_g) arrays for the reference
    particle (proton for PSTAR, alpha for ASTAR).
    """
    import pycatima  # noqa: F811

    if source == SOURCE_PSTAR:
        proj_A, proj_Z = 1, 1
    else:
        proj_A, proj_Z = 4, 2

    # Energy grid in MeV/u: 0.001 to 300 MeV/u (log-spaced, 200 points)
    energies_per_u = np.geomspace(0.001, 300.0, 200)
    dedx_values = []

    mat = pycatima.get_material(target_Z)
    for e_per_u in energies_per_u:
        p = pycatima.Projectile(proj_A, proj_Z)
        p.T(float(e_per_u))
        dedx_values.append(pycatima.dedx(p, mat))

    dedx_arr = np.array(dedx_values)

    # Filter out zero/negative values
    mask = dedx_arr > 0
    # Convert energy to total MeV (for proton, MeV/u == MeV)
    energies_total = energies_per_u * proj_A

    return energies_total[mask], dedx_arr[mask]


# Cache: key -> (interpolator_fn, source_label)
_interpolator_cache: dict[tuple[int, str, int], tuple[Callable[[float], float], str]] = {}


def _get_interpolator(
    db: DatabaseProtocol,
    source: str,
    target_Z: int,
) -> tuple[Callable[[float], float], str]:
    """Get or create a cached interpolator for a specific element.

    Returns (interpolator, source_label) where source_label is one of
    SOURCE_PSTAR, SOURCE_ASTAR, or SOURCE_CATIMA.

    Falls back to pycatima when no PSTAR/ASTAR data exists in the database.
    """
    key = (id(db), source, target_Z)
    if key not in _interpolator_cache:
        energies, dedx = db.get_stopping_power(source, target_Z)
        if len(energies) > 0:
            actual_source = source
        else:
            # No table data — generate from pycatima
            logger.debug(
                "No %s data for Z=%d, falling back to pycatima", source, target_Z
            )
            energies, dedx = _generate_catima_table(source, target_Z)
            actual_source = SOURCE_CATIMA
        _interpolator_cache[key] = (_make_interpolator(energies, dedx), actual_source)
    return _interpolator_cache[key]


_heavy_interpolator_cache: dict[tuple[int, int, int], tuple[Callable[[float], float], str]] = {}


def _generate_catima_table_heavy(
    proj_Z: int,
    proj_A: int,
    target_Z: int,
) -> tuple[np.ndarray, np.ndarray]:
    """Generate stopping power table using pycatima for arbitrary heavy ion."""
    import pycatima

    energies_per_u = np.geomspace(0.001, 300.0, 200)
    dedx_values = []
    mat = pycatima.get_material(target_Z)
    for e_per_u in energies_per_u:
        p = pycatima.Projectile(proj_A, proj_Z)
        p.T(float(e_per_u))
        dedx_values.append(pycatima.dedx(p, mat))
    dedx_arr = np.array(dedx_values)
    mask = dedx_arr > 0
    energies_total = energies_per_u * proj_A
    return energies_total[mask], dedx_arr[mask]


def _get_heavy_interpolator(
    proj_Z: int,
    proj_A: int,
    target_Z: int,
) -> tuple[Callable[[float], float], str]:
    """Get or create a cached interpolator for a heavy-ion projectile."""
    key = (proj_Z, proj_A, target_Z)
    if key not in _heavy_interpolator_cache:
        energies, dedx = _generate_catima_table_heavy(proj_Z, proj_A, target_Z)
        _heavy_interpolator_cache[key] = (_make_interpolator(energies, dedx), SOURCE_CATIMA)
    return _heavy_interpolator_cache[key]


def elemental_dedx(
    db: DatabaseProtocol,
    projectile: str,
    target_Z: int,
    energy_MeV: float | np.ndarray,
) -> float | np.ndarray:
    """Look up mass stopping power for a projectile in a pure element.

    Uses PSTAR/ASTAR tables with velocity scaling for Z<=2 projectiles.
    For Z>2 (heavy ions), uses pycatima directly.

    Falls back to pycatima (SRIM-based) when no table data is available.

    Args:
        db: Database providing stopping power tables
        projectile: Projectile name (e.g., 'p', 'd', 'C-12', 'O-16')
        target_Z: Target element atomic number
        energy_MeV: Projectile kinetic energy [MeV] (scalar or array)

    Returns:
        Mass stopping power [MeV·cm²/g] (scalar or array matching input)
    """
    from hyrr.projectile import resolve_projectile

    proj = resolve_projectile(projectile)

    if proj.Z == 1:
        lookup_energy = energy_MeV / proj.A
        source = SOURCE_PSTAR
        interp_fn, _ = _get_interpolator(db, source, target_Z)
        return interp_fn(lookup_energy)
    elif proj.Z == 2:
        lookup_energy = energy_MeV * (4.0 / proj.A)
        source = SOURCE_ASTAR
        interp_fn, _ = _get_interpolator(db, source, target_Z)
        return interp_fn(lookup_energy)
    else:
        interp_fn, _ = _get_heavy_interpolator(proj.Z, proj.A, target_Z)
        return interp_fn(energy_MeV)


def get_stopping_source(
    db: DatabaseProtocol,
    projectile: str,
    target_Z: int,
) -> str:
    """Return the stopping power source label for an element.

    This indicates whether PSTAR/ASTAR tables or pycatima fallback
    was used. Call after at least one :func:`elemental_dedx` invocation
    so the cache is populated.

    Returns:
        One of 'PSTAR', 'ASTAR', or 'pycatima/SRIM'.
    """
    from hyrr.projectile import resolve_projectile

    proj = resolve_projectile(projectile)
    if proj.Z > 2:
        return SOURCE_CATIMA
    source = SOURCE_PSTAR if proj.Z == 1 else SOURCE_ASTAR
    _interp_fn, actual_source = _get_interpolator(db, source, target_Z)
    return actual_source


def get_stopping_sources(
    db: DatabaseProtocol,
    projectile: str,
    composition: list[tuple[int, float]],
) -> dict[int, str]:
    """Return the stopping power source for each element in a composition.

    Args:
        db: Database providing stopping power tables.
        projectile: Projectile type.
        composition: List of (Z, mass_fraction) pairs.

    Returns:
        Dict mapping Z -> source label.
    """
    return {Z: get_stopping_source(db, projectile, Z) for Z, _ in composition}


def compound_dedx(
    db: DatabaseProtocol,
    projectile: str,
    composition: list[tuple[int, float]],
    energy_MeV: float | np.ndarray,
) -> float | np.ndarray:
    """Compute compound stopping power via Bragg additivity.

    S_compound = Σ w_i × S_i(E)

    where w_i is the mass fraction and S_i is the elemental mass stopping power.

    Args:
        db: Database providing stopping power tables
        projectile: Projectile type
        composition: List of (Z, mass_fraction) pairs. Must sum to ~1.0.
        energy_MeV: Projectile energy [MeV] (scalar or array)

    Returns:
        Compound mass stopping power [MeV·cm²/g]
    """
    total: float | np.ndarray = 0.0
    for Z, mass_frac in composition:
        total = total + mass_frac * elemental_dedx(db, projectile, Z, energy_MeV)
    return total


def dedx_MeV_per_cm(
    db: DatabaseProtocol,
    projectile: str,
    composition: list[tuple[int, float]],
    density_g_cm3: float,
    energy_MeV: float | np.ndarray,
) -> float | np.ndarray:
    """Compute linear stopping power [MeV/cm].

    dE/dx [MeV/cm] = S [MeV·cm²/g] × ρ [g/cm³]

    Args:
        db: Database providing stopping power tables
        projectile: Projectile type
        composition: List of (Z, mass_fraction) pairs
        density_g_cm3: Material density
        energy_MeV: Projectile energy [MeV] (scalar or array)

    Returns:
        Linear stopping power [MeV/cm]
    """
    return compound_dedx(db, projectile, composition, energy_MeV) * density_g_cm3


def compute_thickness_from_energy(
    db: DatabaseProtocol,
    projectile: str,
    composition: list[tuple[int, float]],
    density_g_cm3: float,
    energy_in_MeV: float,
    energy_out_MeV: float,
    n_points: int = 1000,
) -> float:
    """Compute target thickness [cm] from energy loss.

    Integrates dx = dE / (dE/dx) from E_out to E_in using the midpoint rule.

    Args:
        db: Database providing stopping power tables
        projectile: Projectile type
        composition: List of (Z, mass_fraction) pairs
        density_g_cm3: Material density
        energy_in_MeV: Beam energy entering the target [MeV]
        energy_out_MeV: Beam energy exiting the target [MeV]
        n_points: Number of integration points

    Returns:
        Thickness in cm.
    """
    energies = np.linspace(energy_out_MeV, energy_in_MeV, n_points)
    dE = energies[1] - energies[0]
    midpoints = energies[:-1] + dE / 2
    dedx_arr = dedx_MeV_per_cm(db, projectile, composition, density_g_cm3, midpoints)
    return float(np.sum(dE / dedx_arr))


def compute_energy_out(
    db: DatabaseProtocol,
    projectile: str,
    composition: list[tuple[int, float]],
    density_g_cm3: float,
    energy_in_MeV: float,
    thickness_cm: float,
    n_points: int = 1000,
) -> float:
    """Compute exit energy after traversing a material of known thickness.

    Steps the beam through the material using forward Euler integration
    of dE/dx from the entrance face.

    Args:
        db: Database providing stopping power tables.
        projectile: Projectile type.
        composition: List of (Z, mass_fraction) pairs.
        density_g_cm3: Material density [g/cm³].
        energy_in_MeV: Beam energy entering the material [MeV].
        thickness_cm: Material thickness [cm].
        n_points: Number of integration steps.

    Returns:
        Exit energy [MeV]. Clamped to 0 if the beam stops.
    """
    if thickness_cm <= 0:
        return energy_in_MeV

    dx = thickness_cm / n_points
    energy = energy_in_MeV

    for _ in range(n_points):
        loss = dedx_MeV_per_cm(db, projectile, composition, density_g_cm3, energy) * dx
        energy -= loss
        if energy <= 0:
            return 0.0

    return energy


# ---------------------------------------------------------------------------
# Energy straggling (Bohr formula)
# ---------------------------------------------------------------------------

# e² = 1.4399764 MeV·fm  (Coulomb constant × e²)
_E2_MEV_FM = 1.4399764
_NA = 6.02214076e23


def bohr_straggling_variance_per_cm(
    projectile_Z: int,
    composition: list[tuple[int, float]],
    density_g_cm3: float,
    atomic_masses: dict[int, float],
) -> float:
    """Bohr energy-straggling variance per unit path length [MeV²/cm].

    dσ²_E/dz = 4π (e²)² Z_proj² × Σ_i (Z_i × n_i)

    where n_i = ρ N_A w_i / A_i is the number density contribution
    from element i.

    Args:
        projectile_Z: Charge number of the projectile.
        composition: List of (Z, mass_fraction) pairs.
        density_g_cm3: Material density [g/cm³].
        atomic_masses: Mapping Z → average atomic mass [u].

    Returns:
        dσ²_E/dz in [MeV²/cm].
    """
    # e² in MeV·cm: 1.4399764 MeV·fm × 1e-13 cm/fm
    e2_MeV_cm = _E2_MEV_FM * 1.0e-13

    sum_Zn = 0.0
    for Z_i, w_i in composition:
        if w_i <= 0:
            continue
        A_i = atomic_masses[Z_i]
        n_i = density_g_cm3 * _NA * w_i / A_i  # [atoms/cm³]
        sum_Zn += Z_i * n_i

    return 4.0 * math.pi * e2_MeV_cm**2 * projectile_Z**2 * sum_Zn


def cumulative_straggling_sigma(
    sigma_E0_MeV: float,
    projectile_Z: int,
    composition: list[tuple[int, float]],
    density_g_cm3: float,
    atomic_masses: dict[int, float],
    thickness_cm: float,
) -> float:
    """Energy straggling σ_E [MeV] after traversing a thickness.

    σ_E = sqrt(σ₀² + dσ²/dz × Δz)

    Args:
        sigma_E0_MeV: Initial energy spread σ_E [MeV].
        projectile_Z: Charge number of the projectile.
        composition: List of (Z, mass_fraction) pairs.
        density_g_cm3: Material density [g/cm³].
        atomic_masses: Mapping Z → average atomic mass [u].
        thickness_cm: Path length [cm].

    Returns:
        σ_E [MeV] after traversal.
    """
    dsigma2_dz = bohr_straggling_variance_per_cm(
        projectile_Z, composition, density_g_cm3, atomic_masses,
    )
    return math.sqrt(sigma_E0_MeV**2 + dsigma2_dz * thickness_cm)
