"""Energy straggling (Bohr formula).

Pure NumPy implementation — not yet in Rust core.
Used by compute3d.py for pencil-beam transport through meshes.
"""

from __future__ import annotations

import math

# e² = 1.4399764 MeV·fm (Coulomb constant × e²)
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
    """
    e2_MeV_cm = _E2_MEV_FM * 1.0e-13

    sum_Zn = 0.0
    for Z_i, w_i in composition:
        if w_i <= 0:
            continue
        A_i = atomic_masses[Z_i]
        n_i = density_g_cm3 * _NA * w_i / A_i
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
    """
    dsigma2_dz = bohr_straggling_variance_per_cm(
        projectile_Z,
        composition,
        density_g_cm3,
        atomic_masses,
    )
    return math.sqrt(sigma_E0_MeV**2 + dsigma2_dz * thickness_cm)
