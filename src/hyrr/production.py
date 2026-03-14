"""Production rate integration and Bateman equations.

Computes energy-integrated production rates (integral of sigma/dEdx dE),
time-dependent yield and activity via Bateman equations,
and depth-resolved profiles for heat and activity.

This module takes stopping power as a callable ``dedx_fn`` -- it does NOT
import stopping.py.  This enables independent testing and dependency injection.
"""

from __future__ import annotations

import math
from collections.abc import Callable

import numpy as np
import numpy.typing as npt
from numpy.polynomial.hermite import hermgauss

# Physical constants
BARN_CM2 = 1e-24  # 1 barn = 1e-24 cm^2
MILLIBARN_CM2 = 1e-27  # 1 millibarn = 1e-27 cm^2
AVOGADRO = 6.02214076e23
LN2 = math.log(2)
ELEMENTARY_CHARGE = 1.602176634e-19  # C
MEV_TO_JOULE = 1.602176634e-13  # J/MeV


def _gauss_hermite_convolved_xs(
    xs_interp_fn: Callable[[npt.NDArray[np.float64]], npt.NDArray[np.float64]],
    E_mean: npt.NDArray[np.float64],
    sigma_E: npt.NDArray[np.float64],
    n_points: int = 12,
) -> npt.NDArray[np.float64]:
    """Gauss-Hermite quadrature convolution of cross-section with energy spread.

    ⟨σ⟩ = Σ_k w_k σ(E_mean + √2 σ_E x_k)

    where x_k, w_k are Gauss-Hermite nodes/weights (normalized by √π).

    When σ_E < 1e-6 MeV, returns σ(E_mean) directly to avoid unnecessary work.

    Args:
        xs_interp_fn: Interpolated cross-section function σ(E) [mb].
        E_mean: Mean energy array [MeV].
        sigma_E: Energy spread σ_E array [MeV] (same shape as E_mean).
        n_points: Number of Gauss-Hermite quadrature points.

    Returns:
        Convolved cross-section ⟨σ⟩ [mb], same shape as E_mean.
    """
    nodes, weights = hermgauss(n_points)
    # Normalize weights: ∫ f(x) exp(-x²) dx → (1/√π) Σ w_k f(x_k)
    weights = weights / np.sqrt(np.pi)

    result = np.zeros_like(E_mean)
    # Mask: where sigma_E is significant
    sig_mask = sigma_E > 1.0e-6

    # Points with negligible spread: direct evaluation
    if np.any(~sig_mask):
        result[~sig_mask] = xs_interp_fn(E_mean[~sig_mask])

    # Points with significant spread: Gauss-Hermite quadrature
    if np.any(sig_mask):
        E_m = E_mean[sig_mask]
        s_E = sigma_E[sig_mask]
        acc = np.zeros_like(E_m)
        for k in range(n_points):
            E_k = E_m + math.sqrt(2.0) * s_E * nodes[k]
            E_k = np.maximum(E_k, 0.0)  # no negative energies
            acc += weights[k] * xs_interp_fn(E_k)
        result[sig_mask] = acc

    return result


def compute_production_rate(
    xs_energies_MeV: npt.NDArray[np.float64],
    xs_mb: npt.NDArray[np.float64],
    dedx_fn: Callable[[float], float],
    energy_in_MeV: float,
    energy_out_MeV: float,
    n_target_atoms: float,
    beam_particles_per_s: float,
    target_volume_cm3: float,
    n_points: int = 100,
    sigma_E_fn: Callable[[float], float] | None = None,
) -> tuple[
    float,
    npt.NDArray[np.float64],
    npt.NDArray[np.float64],
    npt.NDArray[np.float64],
]:
    """Compute energy-integrated production rate for one isotope.

    The production rate is:

        R = beam_particles/s * (n_atoms / V) * integral(sigma(E) / |dE/dx(E)| dE)

    where sigma is converted from millibarns to cm^2 via the factor 1e-27.

    Args:
        xs_energies_MeV: Cross-section energy grid [MeV].
        xs_mb: Cross-sections [mb] at each energy.
        dedx_fn: Function returning LINEAR stopping power [MeV/cm] at given energy.
        energy_in_MeV: Beam entry energy for this layer.
        energy_out_MeV: Beam exit energy for this layer.
        n_target_atoms: Total number of target atoms in the layer.
        beam_particles_per_s: Number of beam particles per second.
        target_volume_cm3: Volume of the target layer [cm^3].
        n_points: Number of quadrature points (default 100).
        sigma_E_fn: Optional function σ_E(depth_cm) → MeV giving the energy
            spread at each depth in the layer. When provided, cross-sections
            are convolved with a Gaussian energy distribution via Gauss-Hermite
            quadrature.

    Returns:
        Tuple of (production_rate [s^-1], energies, xs_at_points, dedx_at_points).
    """
    # Energy grid from E_out to E_in (ascending)
    energies = np.linspace(energy_out_MeV, energy_in_MeV, n_points)

    # Evaluate stopping power — try vectorized call first, fall back to scalar loop
    result = dedx_fn(energies)
    dedx_values = np.broadcast_to(
        np.asarray(result, dtype=np.float64), energies.shape
    ).copy()

    if sigma_E_fn is not None:
        # Compute depth at each energy point (cumulative from entrance)
        # energies go E_out → E_in (ascending), but beam enters at E_in
        # Reverse to go from E_in downward, compute depths, then reverse back
        dE = np.abs(energies[1] - energies[0]) if n_points > 1 else 0.0
        depths = np.zeros(n_points)
        for i in range(n_points - 2, -1, -1):
            depths[i] = depths[i + 1] + dE / np.abs(dedx_values[i + 1])

        # Get sigma_E at each depth
        sigma_E_arr = np.array([sigma_E_fn(float(d)) for d in depths])

        # Build interpolation function for cross-sections
        def xs_interp_fn(E: npt.NDArray[np.float64]) -> npt.NDArray[np.float64]:
            return np.interp(E, xs_energies_MeV, xs_mb, left=0.0, right=0.0)

        # Convolved cross-sections
        xs_interp = _gauss_hermite_convolved_xs(
            xs_interp_fn,
            energies,
            sigma_E_arr,
        )
    else:
        # Original path: point-energy cross-section lookup
        xs_interp = np.interp(energies, xs_energies_MeV, xs_mb, left=0.0, right=0.0)

    # Trapezoidal integration: integral of sigma(E) / |dE/dx(E)| dE
    integrand = xs_interp / np.abs(dedx_values)
    integral = np.trapezoid(integrand, energies)  # [mb * cm / MeV * MeV] = [mb * cm]

    # Production rate
    number_density = n_target_atoms / target_volume_cm3  # [atoms/cm^3]
    prate = beam_particles_per_s * number_density * integral * MILLIBARN_CM2
    # Units: [1/s] * [1/cm^3] * [mb*cm] * [cm^2/mb] = [1/s]

    return float(prate), energies, xs_interp, dedx_values


def bateman_activity(
    production_rate: float,
    half_life_s: float | None,
    irradiation_time_s: float,
    cooling_time_s: float,
    n_time_points: int = 200,
) -> tuple[npt.NDArray[np.float64], npt.NDArray[np.float64]]:
    """Compute time-dependent activity using Bateman equations.

    During irradiation (0 <= t <= T_irr)::

        A(t) = R * (1 - exp(-lambda * t))

    During cooling (t > T_irr)::

        A(t) = A(T_irr) * exp(-lambda * (t - T_irr))

    For stable isotopes (half_life is None), activity is always zero.

    Args:
        production_rate: R [s^-1] (reactions per second).
        half_life_s: Half-life in seconds (None for stable).
        irradiation_time_s: Irradiation duration [s].
        cooling_time_s: Cooling duration [s].
        n_time_points: Number of points in time grid.

    Returns:
        Tuple of (time_grid_s, activity_Bq) arrays.
    """
    # Build time grid: half for irradiation, half for cooling
    n_irr = n_time_points // 2
    n_cool = n_time_points - n_irr

    t_irr = np.linspace(0, irradiation_time_s, n_irr)
    # Exclude the duplicate point at irradiation_time_s
    t_cool = np.linspace(
        irradiation_time_s, irradiation_time_s + cooling_time_s, n_cool + 1
    )[1:]
    time_grid = np.concatenate([t_irr, t_cool])

    activity = np.zeros_like(time_grid)

    if half_life_s is None or half_life_s <= 0:
        return time_grid, activity

    decay_constant = LN2 / half_life_s

    # Irradiation phase
    mask_irr = time_grid <= irradiation_time_s
    activity[mask_irr] = production_rate * (
        1.0 - np.exp(-decay_constant * time_grid[mask_irr])
    )

    # Activity at end of irradiation
    a_eoi = production_rate * (1.0 - np.exp(-decay_constant * irradiation_time_s))

    # Cooling phase
    mask_cool = time_grid > irradiation_time_s
    dt_cool = time_grid[mask_cool] - irradiation_time_s
    activity[mask_cool] = a_eoi * np.exp(-decay_constant * dt_cool)

    return time_grid, activity


def daughter_ingrowth(
    parent_activity_eoi_Bq: float,
    parent_half_life_s: float,
    daughter_half_life_s: float | None,
    branching_ratio: float,
    cooling_times_s: npt.NDArray[np.float64],
) -> npt.NDArray[np.float64]:
    """Compute daughter activity from parent decay during cooling.

    Uses the Bateman ingrowth formula::

        A_D(dt) = BR * A_P(EOI) * lambda_D / (lambda_D - lambda_P)
                  * [exp(-lambda_P * dt) - exp(-lambda_D * dt)]

    Args:
        parent_activity_eoi_Bq: Parent activity at end of irradiation [Bq].
        parent_half_life_s: Parent half-life [s].
        daughter_half_life_s: Daughter half-life [s] (None for stable).
        branching_ratio: Branching ratio for this decay mode.
        cooling_times_s: Array of cooling times (relative to EOI) [s].

    Returns:
        Daughter activity [Bq] at each cooling time.
    """
    if daughter_half_life_s is None or daughter_half_life_s <= 0:
        return np.zeros_like(cooling_times_s)

    lambda_p = LN2 / parent_half_life_s
    lambda_d = LN2 / daughter_half_life_s

    # Degenerate case: lambda_P approx lambda_D
    if abs(lambda_d - lambda_p) < 1e-30 * max(lambda_d, lambda_p):
        return (
            branching_ratio
            * parent_activity_eoi_Bq
            * lambda_d
            * cooling_times_s
            * np.exp(-lambda_d * cooling_times_s)
        )

    coeff = branching_ratio * parent_activity_eoi_Bq * lambda_d / (lambda_d - lambda_p)
    return coeff * (
        np.exp(-lambda_p * cooling_times_s) - np.exp(-lambda_d * cooling_times_s)
    )


def saturation_yield(
    production_rate: float,
    half_life_s: float | None,
    beam_current_mA: float,
) -> float:
    """Compute saturation yield [Bq/uA].

    At saturation the activity equals the production rate, so::

        Y_sat = R / I_uA

    Since R is proportional to beam current, the ratio is
    current-independent.

    Args:
        production_rate: R [s^-1].
        half_life_s: Half-life in seconds (None for stable).
        beam_current_mA: Beam current [mA].

    Returns:
        Saturation yield [Bq/uA].
    """
    if half_life_s is None or half_life_s <= 0:
        return 0.0
    current_uA = beam_current_mA * 1e3
    return production_rate / current_uA


def activity_to_yield_GBq_per_mAh(
    activity_Bq: float,
    beam_current_mA: float,
    irradiation_time_s: float,
) -> float:
    """Convert activity to yield [GBq/mAh].

    Args:
        activity_Bq: Activity [Bq].
        beam_current_mA: Beam current [mA].
        irradiation_time_s: Irradiation duration [s].

    Returns:
        Yield in GBq/mAh.
    """
    time_h = irradiation_time_s / 3600.0
    charge_mAh = beam_current_mA * time_h
    if charge_mAh <= 0:
        return 0.0
    return activity_Bq * 1e-9 / charge_mAh


def generate_depth_profile(
    energies: npt.NDArray[np.float64],
    dedx_values: npt.NDArray[np.float64],
    beam_current_mA: float,
    area_cm2: float,
    projectile_Z: int,
) -> tuple[npt.NDArray[np.float64], npt.NDArray[np.float64]]:
    """Generate depth profile from integration points.

    Computes cumulative depth from the energy/stopping-power grid and the
    volumetric heat deposition at each point.

    Args:
        energies: Energy values at each integration point [MeV].
        dedx_values: Stopping power at each point [MeV/cm].
        beam_current_mA: Beam current [mA].
        area_cm2: Beam spot area [cm^2].
        projectile_Z: Charge number of the projectile.

    Returns:
        Tuple of (depths_cm, heat_W_cm3) arrays.
    """
    n = len(energies)
    depths = np.zeros(n)
    dE = np.abs(energies[1] - energies[0]) if n > 1 else 0.0

    for i in range(1, n):
        depths[i] = depths[i - 1] + dE / np.abs(dedx_values[i - 1])

    # Heat density: P = flux * |dE/dx| * MeV_to_J
    beam_particles_per_s = (beam_current_mA * 1e-3) / (projectile_Z * ELEMENTARY_CHARGE)
    flux = beam_particles_per_s / area_cm2  # [particles/s/cm^2]
    heat = flux * np.abs(dedx_values) * MEV_TO_JOULE  # [W/cm^3]

    return depths, heat
