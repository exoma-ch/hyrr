"""Data models for HYRR.

Beam, Element, Layer, TargetStack, DepthPoint, and result types.
"""

from __future__ import annotations

import math
from dataclasses import dataclass, field
from typing import Literal

import numpy as np
import numpy.typing as npt

from hyrr.projectile import Projectile, resolve_projectile

ProjectileType = Literal["p", "d", "t", "h", "a"]

# Deprecated: kept for backward compatibility with light ions
PROJECTILE_A: dict[str, int] = {"p": 1, "d": 2, "t": 3, "h": 3, "a": 4}
PROJECTILE_Z: dict[str, int] = {"p": 1, "d": 1, "t": 1, "h": 2, "a": 2}


@dataclass(frozen=True)
class Beam:
    """Incident beam specification."""

    projectile: str
    energy_MeV: float
    current_mA: float

    def __post_init__(self) -> None:
        if self.energy_MeV <= 0:
            raise ValueError(f"energy_MeV must be positive, got {self.energy_MeV}")
        if self.current_mA <= 0:
            raise ValueError(f"current_mA must be positive, got {self.current_mA}")
        resolve_projectile(self.projectile)  # validates; raises ValueError if invalid

    @property
    def projectile_obj(self) -> Projectile:
        """Resolved projectile definition."""
        return resolve_projectile(self.projectile)

    @property
    def particles_per_second(self) -> float:
        """Number of incident particles per second."""
        charge_e = 1.602176634e-19  # elementary charge [C]
        return (self.current_mA * 1e-3) / (self.projectile_obj.charge_state * charge_e)


@dataclass
class Element:
    """Element with isotopic composition."""

    symbol: str
    Z: int
    isotopes: dict[int, float]  # A -> fractional abundance

    def __post_init__(self) -> None:
        if self.Z < 1:
            raise ValueError(f"Z must be >= 1, got {self.Z}")
        if not self.isotopes:
            raise ValueError("isotopes dict must not be empty")
        total = sum(self.isotopes.values())
        if not math.isclose(total, 1.0, rel_tol=1e-3):
            raise ValueError(f"isotope abundances must sum to ~1.0, got {total}")


@dataclass
class Layer:
    """Single target layer specification.

    Exactly one of thickness_cm, areal_density_g_cm2, or energy_out_MeV must
    be set.
    """

    density_g_cm3: float
    elements: list[tuple[Element, float]]  # (Element, atom_fraction) pairs

    # Exactly one of these must be set:
    thickness_cm: float | None = None
    areal_density_g_cm2: float | None = None
    energy_out_MeV: float | None = None

    # Computed after stack validation:
    _energy_in: float = field(init=False, default=0.0, repr=False)
    _energy_out: float = field(init=False, default=0.0, repr=False)
    _thickness: float = field(init=False, default=0.0, repr=False)

    is_monitor: bool = False

    def __post_init__(self) -> None:
        specs = [self.thickness_cm, self.areal_density_g_cm2, self.energy_out_MeV]
        n_set = sum(1 for s in specs if s is not None)
        if n_set != 1:
            raise ValueError(
                f"Exactly one of thickness_cm, areal_density_g_cm2, or "
                f"energy_out_MeV must be set, got {n_set}"
            )
        if self.density_g_cm3 <= 0:
            raise ValueError(f"density must be positive, got {self.density_g_cm3}")
        if not self.elements:
            raise ValueError("elements list must not be empty")

    @property
    def average_atomic_mass(self) -> float:
        """Weighted average atomic mass [u]."""
        total = 0.0
        for elem, frac in self.elements:
            elem_mass = sum(A * ab for A, ab in elem.isotopes.items())
            total += frac * elem_mass
        return total


@dataclass
class CurrentProfile:
    """Time-varying beam current during irradiation.

    Piecewise-constant interpretation: ``currents_mA[i]`` applies from
    ``times_s[i]`` to ``times_s[i+1]``.  The last entry applies until
    the end of irradiation.

    Attributes:
        times_s: Monotonically increasing time points starting at 0 [s].
        currents_mA: Beam current at each time point [mA].
    """

    times_s: npt.NDArray[np.float64]
    currents_mA: npt.NDArray[np.float64]

    def __post_init__(self) -> None:
        if len(self.times_s) != len(self.currents_mA):
            raise ValueError(
                f"times_s and currents_mA must have the same length, "
                f"got {len(self.times_s)} and {len(self.currents_mA)}"
            )
        if len(self.times_s) < 1:
            raise ValueError("CurrentProfile must have at least one entry")
        if np.any(np.diff(self.times_s) < 0):
            raise ValueError("times_s must be monotonically increasing")
        if np.any(self.currents_mA < 0):
            raise ValueError("currents_mA must be non-negative")

    def current_at(self, t: float) -> float:
        """Return beam current at time t (piecewise-constant lookup)."""
        idx = int(np.searchsorted(self.times_s, t, side="right")) - 1
        idx = max(0, min(idx, len(self.currents_mA) - 1))
        return float(self.currents_mA[idx])

    def intervals(
        self, t_end: float,
    ) -> list[tuple[float, float, float]]:
        """Return list of (t_start, t_end, current_mA) intervals.

        Covers [0, t_end] using the piecewise-constant profile.
        """
        result: list[tuple[float, float, float]] = []
        times = self.times_s
        currents = self.currents_mA

        for i in range(len(times)):
            t_start_i = float(times[i])
            if t_start_i >= t_end:
                break
            t_end_i = float(times[i + 1]) if i + 1 < len(times) else t_end
            t_end_i = min(t_end_i, t_end)
            result.append((t_start_i, t_end_i, float(currents[i])))

        # If profile starts after 0, prepend with first current
        if result and result[0][0] > 0:
            result.insert(0, (0.0, result[0][0], float(currents[0])))
        elif not result:
            result.append((0.0, t_end, float(currents[0])))

        return result


@dataclass
class TargetStack:
    """Ordered stack of target layers traversed by the beam."""

    beam: Beam
    layers: list[Layer]
    irradiation_time_s: float = 86400.0  # default 1 day
    cooling_time_s: float = 86400.0  # default 1 day
    area_cm2: float = 1.0
    current_profile: CurrentProfile | None = None


@dataclass(frozen=True)
class DepthPoint:
    """Single point in a depth profile."""

    depth_cm: float
    energy_MeV: float
    dedx_MeV_cm: float
    heat_W_cm3: float
    production_rates: dict[str, float]  # isotope_name -> local rate [s^-1 cm^-1]


@dataclass
class IsotopeResult:
    """Production result for a single isotope."""

    name: str  # e.g., "Tc-99m"
    Z: int
    A: int
    state: str  # '', 'g', 'm'
    half_life_s: float | None  # None for stable
    production_rate: float  # [s^-1]
    saturation_yield_Bq_uA: float
    activity_Bq: float  # at end of irradiation+cooling
    time_grid_s: npt.NDArray[np.float64]  # time points
    activity_vs_time_Bq: npt.NDArray[np.float64]  # activity at each time point

    # Source attribution (chain solver)
    source: str = "direct"  # "direct", "daughter", "both"
    activity_direct_Bq: float = 0.0
    activity_ingrowth_Bq: float = 0.0
    activity_direct_vs_time_Bq: npt.NDArray[np.float64] = field(
        default_factory=lambda: np.array([], dtype=np.float64)
    )
    activity_ingrowth_vs_time_Bq: npt.NDArray[np.float64] = field(
        default_factory=lambda: np.array([], dtype=np.float64)
    )


@dataclass
class LayerResult:
    """Full result for a single layer."""

    layer: Layer
    energy_in: float
    energy_out: float
    delta_E_MeV: float
    heat_kW: float
    depth_profile: list[DepthPoint]
    isotope_results: dict[str, IsotopeResult]  # keyed by isotope name
    stopping_power_sources: dict[int, str] = field(default_factory=dict)
    # Z -> source label ('PSTAR', 'ASTAR', 'pycatima/SRIM')


@dataclass
class StackResult:
    """Full result for all layers in a stack."""

    stack: TargetStack
    layer_results: list[LayerResult]
    irradiation_time_s: float
    cooling_time_s: float
