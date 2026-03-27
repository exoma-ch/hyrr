"""Shared pure-math utilities and physics constants.

Contains functions and constants that are used by plotting.py, neutrons.py,
compute3d.py, and other modules but are not part of the core physics engine
(Rust). These are either pure-math helpers or constants needed by Python
callers that haven't been migrated to Rust yet.
"""

from __future__ import annotations

import math
from collections.abc import Callable

import numpy as np
import numpy.typing as npt
from numpy.polynomial.hermite import hermgauss

# Physical constants
BARN_CM2 = 1e-24  # 1 barn = 1e-24 cm²
MILLIBARN_CM2 = 1e-27  # 1 millibarn = 1e-27 cm²
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
    """
    nodes, weights = hermgauss(n_points)
    weights = weights / np.sqrt(np.pi)

    result = np.zeros_like(E_mean)
    sig_mask = sigma_E > 1.0e-6

    if np.any(~sig_mask):
        result[~sig_mask] = xs_interp_fn(E_mean[~sig_mask])

    if np.any(sig_mask):
        E_m = E_mean[sig_mask]
        s_E = sigma_E[sig_mask]
        acc = np.zeros_like(E_m)
        for k in range(n_points):
            E_k = E_m + math.sqrt(2.0) * s_E * nodes[k]
            E_k = np.maximum(E_k, 0.0)
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

    R = beam_particles/s * (n_atoms / V) * integral(sigma(E) / |dE/dx(E)| dE)

    Used by compute3d.py for per-segment production in mesh transport.
    """
    energies = np.linspace(energy_out_MeV, energy_in_MeV, n_points)

    result = dedx_fn(energies)
    dedx_values = np.broadcast_to(
        np.asarray(result, dtype=np.float64), energies.shape
    ).copy()

    if sigma_E_fn is not None:
        dE = np.abs(energies[1] - energies[0]) if n_points > 1 else 0.0
        depths = np.zeros(n_points)
        for i in range(n_points - 2, -1, -1):
            depths[i] = depths[i + 1] + dE / np.abs(dedx_values[i + 1])

        sigma_E_arr = np.array([sigma_E_fn(float(d)) for d in depths])

        def xs_interp_fn(E: npt.NDArray[np.float64]) -> npt.NDArray[np.float64]:
            return np.interp(E, xs_energies_MeV, xs_mb, left=0.0, right=0.0)

        xs_interp = _gauss_hermite_convolved_xs(
            xs_interp_fn,
            energies,
            sigma_E_arr,
        )
    else:
        xs_interp = np.interp(energies, xs_energies_MeV, xs_mb, left=0.0, right=0.0)

    integrand = xs_interp / np.abs(dedx_values)
    integral = np.trapezoid(integrand, energies)

    number_density = n_target_atoms / target_volume_cm3
    prate = beam_particles_per_s * number_density * integral * MILLIBARN_CM2

    return float(prate), energies, xs_interp, dedx_values
