"""Orchestrator: wire all physics modules into a single compute_stack call.

This is the main entry point for running a full HYRR simulation.
Given a :class:`TargetStack` and a :class:`DatabaseProtocol`, it computes
production rates, activities, depth profiles, and heat for every layer
and every residual isotope.
"""

from __future__ import annotations

import logging
from typing import TYPE_CHECKING

import numpy as np
import scipy.constants as const

from hyrr.chains import discover_chains, solve_chain
from hyrr.models import (
    DepthPoint,
    IsotopeResult,
    LayerResult,
    StackResult,
)
from hyrr.projectile import resolve_projectile
from hyrr.production import (
    bateman_activity,
    compute_production_rate,
    generate_depth_profile,
    saturation_yield,
)
from hyrr.stopping import (
    bohr_straggling_variance_per_cm,
    compute_energy_out,
    compute_thickness_from_energy,
    dedx_MeV_per_cm,
    get_stopping_sources,
)

if TYPE_CHECKING:
    from hyrr.db import DatabaseProtocol
    from hyrr.models import CurrentProfile, Layer, TargetStack

logger = logging.getLogger(__name__)


def _layer_composition(layer: Layer) -> list[tuple[int, float]]:
    """Convert layer's (Element, atom_fraction) pairs to (Z, mass_fraction).

    mass_i = atom_frac_i * <A>_i where <A>_i is the abundance-weighted
    average atomic mass of the element, then normalize so fractions sum to 1.
    """
    raw: list[tuple[int, float]] = []
    for elem, atom_frac in layer.elements:
        avg_mass = sum(A * ab for A, ab in elem.isotopes.items())
        raw.append((elem.Z, atom_frac * avg_mass))

    total = sum(w for _, w in raw)
    if total <= 0:
        msg = "Layer composition has zero total mass"
        raise ValueError(msg)
    return [(Z, w / total) for Z, w in raw]


def compute_stack(
    db: DatabaseProtocol,
    stack: TargetStack,
    enable_chains: bool = True,
) -> StackResult:
    """Run the full HYRR simulation pipeline for a target stack.

    For each layer in order:
    1. Resolve composition to (Z, mass_fraction) for stopping power
    2. Determine thickness and exit energy
    3. Compute production rates for every residual isotope
    4. Solve Bateman equations for time-dependent activity
    5. Build depth profile with heat deposition
    6. Propagate exit energy to the next layer

    Args:
        db: Nuclear data provider (database or mock).
        stack: Target stack specification (beam + layers + timing).
        enable_chains: If True, use coupled decay chain solver with source
            attribution. If False, use independent Bateman solver (legacy).

    Returns:
        Complete simulation result.
    """
    beam = stack.beam
    irr_time = stack.irradiation_time_s
    cool_time = stack.cooling_time_s
    area = stack.area_cm2

    energy_in = beam.energy_MeV
    accumulated_sigma_sq = beam.energy_spread_MeV ** 2
    layer_results: list[LayerResult] = []

    proj = resolve_projectile(beam.projectile)
    for layer in stack.layers:
        sigma_E_in = accumulated_sigma_sq ** 0.5
        lr = _compute_layer(
            db, beam.projectile, beam.current_mA,
            beam.particles_per_second, proj.Z,
            layer, energy_in, irr_time, cool_time, area,
            enable_chains=enable_chains,
            current_profile=stack.current_profile,
            sigma_E_initial=sigma_E_in,
        )
        layer_results.append(lr)
        energy_in = lr.energy_out
        # Accumulate straggling variance for next layer
        accumulated_sigma_sq = lr.sigma_E_out_MeV ** 2

    return StackResult(
        stack=stack,
        layer_results=layer_results,
        irradiation_time_s=irr_time,
        cooling_time_s=cool_time,
    )


def _compute_layer(
    db: DatabaseProtocol,
    projectile: str,
    current_mA: float,
    particles_per_s: float,
    projectile_Z: int,
    layer: Layer,
    energy_in: float,
    irr_time: float,
    cool_time: float,
    area: float,
    enable_chains: bool = True,
    current_profile: CurrentProfile | None = None,
    sigma_E_initial: float = 0.0,
) -> LayerResult:
    """Compute results for a single layer."""
    composition = _layer_composition(layer)
    density = layer.density_g_cm3

    # --- resolve thickness / energy_out ---
    if layer.energy_out_MeV is not None:
        energy_out = layer.energy_out_MeV
        thickness = compute_thickness_from_energy(
            db, projectile, composition, density, energy_in, energy_out,
        )
    elif layer.thickness_cm is not None:
        thickness = layer.thickness_cm
        energy_out = compute_energy_out(
            db, projectile, composition, density, energy_in, thickness,
        )
    else:
        # areal_density_g_cm2
        assert layer.areal_density_g_cm2 is not None
        thickness = layer.areal_density_g_cm2 / density
        energy_out = compute_energy_out(
            db, projectile, composition, density, energy_in, thickness,
        )

    # Set computed fields on the mutable Layer
    layer._energy_in = energy_in
    layer._energy_out = energy_out
    layer._thickness = thickness

    # --- stopping power sources ---
    sp_sources = get_stopping_sources(db, projectile, composition)

    # --- dedx closure ---
    def dedx_fn(E: float) -> float:
        return dedx_MeV_per_cm(db, projectile, composition, density, E)

    # --- energy straggling ---
    # Compute average atomic mass per element for Bohr formula
    atomic_masses: dict[int, float] = {}
    for elem, _ in layer.elements:
        atomic_masses[elem.Z] = sum(A * ab for A, ab in elem.isotopes.items())

    sigma_E_fn = None
    dsigma2_dz = 0.0
    if sigma_E_initial > 1.0e-9 or any(w > 0 for _, w in composition):
        dsigma2_dz = bohr_straggling_variance_per_cm(
            projectile_Z, composition, density, atomic_masses,
        )
    sigma_E_out = (sigma_E_initial**2 + dsigma2_dz * thickness) ** 0.5

    # Only build closure when there's meaningful energy spread
    if sigma_E_initial > 1.0e-9 or dsigma2_dz * thickness > 1.0e-18:
        _sig0_sq = sigma_E_initial**2
        _dsig2 = dsigma2_dz

        def sigma_E_fn(z: float) -> float:
            return (_sig0_sq + _dsig2 * z) ** 0.5

    # --- target geometry ---
    volume = thickness * area
    avg_A = layer.average_atomic_mass
    n_atoms = (density * volume * const.Avogadro) / avg_A

    # --- per-element, per-isotope, per-residual production ---
    isotope_results: dict[str, IsotopeResult] = {}

    # Track the first production_rate call's grid for depth profile
    first_energies = None
    first_dedx = None

    for elem, atom_frac in layer.elements:
        for A, isotope_abundance in elem.isotopes.items():
            weight = atom_frac * isotope_abundance
            if weight <= 0:
                continue

            xs_list = db.get_cross_sections(projectile, elem.Z, A)
            for xs in xs_list:
                prate, energies, xs_interp, dedx_values = compute_production_rate(
                    xs.energies_MeV,
                    xs.xs_mb,
                    dedx_fn,
                    energy_in_MeV=energy_in,
                    energy_out_MeV=energy_out,
                    n_target_atoms=n_atoms,
                    beam_particles_per_s=particles_per_s,
                    target_volume_cm3=volume,
                    sigma_E_fn=sigma_E_fn,
                )

                if first_energies is None:
                    first_energies = energies
                    first_dedx = dedx_values

                # Scale by element/isotope weight
                scaled_rate = prate * weight

                # Decay data
                decay = db.get_decay_data(xs.residual_Z, xs.residual_A, xs.state)
                half_life: float | None = None
                if decay is not None:
                    half_life = decay.half_life_s

                # Isotope name
                symbol = db.get_element_symbol(xs.residual_Z)
                state_suffix = xs.state if xs.state else ""
                name = f"{symbol}-{xs.residual_A}{state_suffix}"

                # Bateman activity
                time_grid, activity = bateman_activity(
                    scaled_rate, half_life, irr_time, cool_time,
                )

                # Saturation yield
                sat_yield = saturation_yield(scaled_rate, half_life, current_mA)

                # Activity at end of simulation
                activity_final = float(activity[-1]) if len(activity) > 0 else 0.0

                # Accumulate (same residual from different target isotopes)
                if name in isotope_results:
                    existing = isotope_results[name]
                    combined_rate = existing.production_rate + scaled_rate
                    combined_sat = existing.saturation_yield_Bq_uA + sat_yield

                    # Re-compute Bateman for combined rate
                    combined_tg, combined_act = bateman_activity(
                        combined_rate, half_life, irr_time, cool_time,
                    )
                    isotope_results[name] = IsotopeResult(
                        name=name,
                        Z=xs.residual_Z,
                        A=xs.residual_A,
                        state=xs.state,
                        half_life_s=half_life,
                        production_rate=combined_rate,
                        saturation_yield_Bq_uA=combined_sat,
                        activity_Bq=float(combined_act[-1]),
                        time_grid_s=combined_tg,
                        activity_vs_time_Bq=combined_act,
                    )
                else:
                    isotope_results[name] = IsotopeResult(
                        name=name,
                        Z=xs.residual_Z,
                        A=xs.residual_A,
                        state=xs.state,
                        half_life_s=half_life,
                        production_rate=scaled_rate,
                        saturation_yield_Bq_uA=sat_yield,
                        activity_Bq=activity_final,
                        time_grid_s=time_grid,
                        activity_vs_time_Bq=activity,
                    )

    # --- coupled chain solver ---
    if current_profile is not None and not enable_chains:
        logger.warning(
            "current_profile requires enable_chains=True; "
            "enabling chain solver automatically"
        )
        enable_chains = True
    if enable_chains and isotope_results:
        isotope_results = _apply_chain_solver(
            db, isotope_results, irr_time, cool_time, particles_per_s,
            current_profile=current_profile,
            nominal_current_mA=current_mA,
        )

    # --- depth profile ---
    depth_profile: list[DepthPoint] = []
    if first_energies is not None and first_dedx is not None:
        depths, heat_W_cm3 = generate_depth_profile(
            first_energies, first_dedx,
            current_mA, area, projectile_Z,
        )
        for i in range(len(depths)):
            sig_E_i = sigma_E_fn(float(depths[i])) if sigma_E_fn is not None else 0.0
            depth_profile.append(DepthPoint(
                depth_cm=float(depths[i]),
                energy_MeV=float(first_energies[i]),
                dedx_MeV_cm=float(first_dedx[i]),
                heat_W_cm3=float(heat_W_cm3[i]),
                production_rates={},  # per-point rates omitted for performance
                sigma_E_MeV=sig_E_i,
            ))

    # --- heat ---
    heat_kW = _integrate_heat(depth_profile, area) if depth_profile else 0.0

    delta_E = energy_in - energy_out

    return LayerResult(
        layer=layer,
        energy_in=energy_in,
        energy_out=energy_out,
        delta_E_MeV=delta_E,
        heat_kW=heat_kW,
        depth_profile=depth_profile,
        isotope_results=isotope_results,
        stopping_power_sources=sp_sources,
        sigma_E_in_MeV=sigma_E_initial,
        sigma_E_out_MeV=sigma_E_out,
    )


def _apply_chain_solver(
    db: DatabaseProtocol,
    isotope_results: dict[str, IsotopeResult],
    irr_time: float,
    cool_time: float,
    particles_per_s: float,
    current_profile: CurrentProfile | None = None,
    nominal_current_mA: float = 1.0,
) -> dict[str, IsotopeResult]:
    """Replace independent Bateman results with coupled chain solution.

    Discovers decay chains from directly-produced isotopes, solves the
    coupled ODE, and returns updated IsotopeResult dict with source
    attribution and any daughter isotopes added.
    """
    # Collect direct isotopes for chain discovery
    direct_isotopes: list[tuple[int, int, str, float]] = [
        (iso.Z, iso.A, iso.state, iso.production_rate)
        for iso in isotope_results.values()
    ]

    chain = discover_chains(db, direct_isotopes)
    if len(chain) <= 1:
        # Single isotope or empty — no chain effects, keep independent results
        # but populate source attribution fields
        for name, iso in isotope_results.items():
            isotope_results[name] = IsotopeResult(
                name=iso.name, Z=iso.Z, A=iso.A, state=iso.state,
                half_life_s=iso.half_life_s,
                production_rate=iso.production_rate,
                saturation_yield_Bq_uA=iso.saturation_yield_Bq_uA,
                activity_Bq=iso.activity_Bq,
                time_grid_s=iso.time_grid_s,
                activity_vs_time_Bq=iso.activity_vs_time_Bq,
                source="direct",
                activity_direct_Bq=iso.activity_Bq,
                activity_ingrowth_Bq=0.0,
                activity_direct_vs_time_Bq=iso.activity_vs_time_Bq.copy(),
                activity_ingrowth_vs_time_Bq=np.zeros_like(
                    iso.activity_vs_time_Bq
                ),
            )
        return isotope_results

    solution = solve_chain(
        chain, irr_time, cool_time, particles_per_s,
        current_profile=current_profile,
        nominal_current_mA=nominal_current_mA,
    )

    # Build new isotope_results from chain solution
    new_results: dict[str, IsotopeResult] = {}
    for i, ciso in enumerate(solution.isotopes):
        # Look up symbol
        symbol = db.get_element_symbol(ciso.Z)
        state_suffix = ciso.state if ciso.state else ""
        name = f"{symbol}-{ciso.A}{state_suffix}"

        total_activity = solution.activities[i, :]
        direct_activity = solution.activities_direct[i, :]
        ingrowth_activity = solution.activities_ingrowth[i, :]

        activity_final = float(total_activity[-1]) if len(total_activity) > 0 else 0.0
        direct_final = float(direct_activity[-1]) if len(direct_activity) > 0 else 0.0
        ingrowth_final = float(ingrowth_activity[-1]) if len(ingrowth_activity) > 0 else 0.0

        has_direct = ciso.production_rate > 0
        has_ingrowth = ingrowth_final > 0 or float(np.max(ingrowth_activity)) > 0

        if has_direct and has_ingrowth:
            source = "both"
        elif has_ingrowth:
            source = "daughter"
        else:
            source = "direct"

        # Get existing result for production_rate/sat_yield if available
        existing = isotope_results.get(name)
        prod_rate = existing.production_rate if existing else ciso.production_rate
        sat_yield = existing.saturation_yield_Bq_uA if existing else 0.0

        new_results[name] = IsotopeResult(
            name=name,
            Z=ciso.Z,
            A=ciso.A,
            state=ciso.state,
            half_life_s=ciso.half_life_s,
            production_rate=prod_rate,
            saturation_yield_Bq_uA=sat_yield,
            activity_Bq=activity_final,
            time_grid_s=solution.time_grid_s,
            activity_vs_time_Bq=total_activity,
            source=source,
            activity_direct_Bq=direct_final,
            activity_ingrowth_Bq=ingrowth_final,
            activity_direct_vs_time_Bq=direct_activity,
            activity_ingrowth_vs_time_Bq=ingrowth_activity,
        )

    return new_results


def _integrate_heat(
    profile: list[DepthPoint],
    area_cm2: float,
) -> float:
    """Integrate volumetric heat over depth to get total power [kW].

    P = area × ∫ heat_W_cm3 dx  →  [cm² × W/cm³ × cm] = [W]
    """
    if len(profile) < 2:
        return 0.0

    depths = np.array([dp.depth_cm for dp in profile])
    heat = np.array([dp.heat_W_cm3 for dp in profile])
    power_W = area_cm2 * float(np.trapezoid(heat, depths))
    return power_W * 1e-3  # W -> kW
