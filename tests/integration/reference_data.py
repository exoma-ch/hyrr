"""Reference values from ISOTOPIA output for validation.

Source: curie/samples/p-Mo100-Tc099m/org/isotopia.out
ISOTOPIA-2.1, Version: August 7, 2025

NOTE: ISOTOPIA reports "production rate [s^-1]" as a *per-atom* rate,
i.e. the integral sigma/dEdx * I_beam.  To get the total production rate
R [reactions/s] multiply by N_target_atoms:

    R_total = isotopia_rate * N_atoms

HYRR computes R_total directly (beam_particles/s * N/V * integral).
"""

from __future__ import annotations

from dataclasses import dataclass


@dataclass(frozen=True)
class ReferenceIsotope:
    """Reference isotope production data from ISOTOPIA."""

    name: str
    Z: int
    A: int
    state: str
    isotopia_rate: float  # ISOTOPIA "production rate [s^-1]" (per-atom)
    activity_eoi_GBq: float  # at end of irradiation
    activity_cooled_GBq: float | None  # at end of cooling (1 day), None if not checked
    half_life_s: float | None

    @property
    def production_rate(self) -> float:
        """Total production rate [reactions/s] = isotopia_rate × N_atoms."""
        return self.isotopia_rate * P_MO100_N_ATOMS


# p + Mo-100 reference case
# 16 -> 12 MeV, 0.15 mA, 1 day irradiation, 1 day cooling
P_MO100_BEAM = {
    "projectile": "p",
    "energy_MeV": 16.0,
    "current_mA": 0.15,
}

P_MO100_TARGET = {
    "Z": 42,
    "A": 100,
    "density_g_cm3": 10.22,
    "energy_out_MeV": 12.0,
}

P_MO100_PARAMS = {
    "irradiation_time_s": 86400.0,  # 1 day
    "cooling_time_s": 86400.0,  # 1 day
    "area_cm2": 1.0,
}

# Reference values from ISOTOPIA
P_MO100_THICKNESS_CM = 2.097696e-02
P_MO100_HEAT_KW = 0.6
P_MO100_N_ATOMS = 1.291054e21
P_MO100_PARTICLES_PER_S = 9.362264e14

P_MO100_ISOTOPES = [
    # activity_eoi from ISOTOPIA table, activity_cooled from .act file at t=48h
    ReferenceIsotope("Tc-101", 43, 101, "", 9.115971e-13, 1.176902, None, 852.0),
    ReferenceIsotope("Tc-100", 43, 100, "", 6.088759e-11, 78.60903, None, 15.56),
    ReferenceIsotope("Tc-99", 43, 99, "", 7.742053e-10, 261.6367, None, None),  # sum
    ReferenceIsotope("Tc-99g", 43, 99, "g", 5.875618e-10, 6.726835e-06, None, 6.749e9),
    ReferenceIsotope("Tc-99m", 43, 99, "m", 2.162432e-10, 261.7423, 16.43454, 21636.0),
    ReferenceIsotope("Mo-99", 42, 99, "", 2.299199e-11, 6.618484, None, 237384.0),
    ReferenceIsotope("Nb-97", 41, 97, "", 4.043358e-12, 5.219816, None, 4326.0),
    ReferenceIsotope("Nb-97g", 41, 97, "g", 3.277366e-12, 4.230950, None, 4326.0),
    ReferenceIsotope("Nb-96", 41, 96, "", 2.951210e-13, 0.1941402, None, 84060.0),
]
