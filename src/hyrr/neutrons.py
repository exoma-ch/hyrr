"""Neutron activation module.

Computes neutron-induced isotope production for three scenarios:

1. **Reactor irradiation** — thermal/epithermal flux with Westcott convention
2. **Fast neutron irradiation** — mono-energetic or spectrum-averaged
3. **Secondary neutron activation** — neutrons produced by charged-particle
   reactions (e.g. (p,xn)) activating surrounding material

Unlike charged particles, neutrons have no Coulomb interaction:
- No continuous energy loss (no stopping power)
- Exponential flux attenuation: phi(x) = phi_0 * exp(-Sigma_t * x)
- Production rate: R = N * integral(sigma(E) * phi(E) dE)

The energy convolution (integral of sigma * phi dE) reuses the same
numerical pattern as the Gauss-Hermite straggling convolution in
production.py, with the flux spectrum as the kernel instead of a Gaussian.
"""

from __future__ import annotations

import math
from dataclasses import dataclass
from typing import TYPE_CHECKING

import numpy as np
import numpy.typing as npt

from hyrr.production import (
    AVOGADRO,
    MILLIBARN_CM2,
    bateman_activity,
)

if TYPE_CHECKING:
    from hyrr.db import DatabaseProtocol
    from hyrr.models import IsotopeResult, Layer, LayerResult


# ---------------------------------------------------------------------------
# Neutron flux spectrum models
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class NeutronFlux:
    """Base class for neutron flux spectra.

    Subclasses define phi(E) [n/cm^2/s/MeV] as a function of energy.
    """

    total_flux: float  # integrated flux [n/cm^2/s]

    def phi(self, E_MeV: npt.NDArray[np.float64]) -> npt.NDArray[np.float64]:
        """Differential flux phi(E) [n/cm^2/s/MeV]."""
        raise NotImplementedError


@dataclass(frozen=True)
class ThermalFlux(NeutronFlux):
    """Maxwellian thermal neutron flux.

    phi(E) = phi_th * (2/sqrt(pi)) * sqrt(E/kT) * exp(-E/kT) / kT

    Default temperature: 293.15 K (room temp, kT = 0.0253 eV).
    """

    kT_eV: float = 0.0253  # thermal energy [eV]

    def phi(self, E_MeV: npt.NDArray[np.float64]) -> npt.NDArray[np.float64]:
        kT_MeV = self.kT_eV * 1e-6
        x = E_MeV / kT_MeV
        return (
            self.total_flux
            * (2.0 / math.sqrt(math.pi))
            * np.sqrt(x)
            * np.exp(-x)
            / kT_MeV
        )


@dataclass(frozen=True)
class EpithermalFlux(NeutronFlux):
    """1/E epithermal flux between E_min and E_max.

    phi(E) = phi_epi / (E * ln(E_max/E_min))  for E_min <= E <= E_max
    """

    E_min_eV: float = 0.5  # cadmium cutoff
    E_max_eV: float = 1.0e6  # 1 MeV upper bound

    def phi(self, E_MeV: npt.NDArray[np.float64]) -> npt.NDArray[np.float64]:
        E_min_MeV = self.E_min_eV * 1e-6
        E_max_MeV = self.E_max_eV * 1e-6
        ln_ratio = math.log(E_max_MeV / E_min_MeV)
        result = np.zeros_like(E_MeV)
        mask = (E_MeV >= E_min_MeV) & (E_MeV <= E_max_MeV)
        result[mask] = self.total_flux / (E_MeV[mask] * ln_ratio)
        return result


@dataclass(frozen=True)
class WeisskopfFlux(NeutronFlux):
    """Weisskopf evaporation spectrum for secondary neutrons.

    phi(E) = phi_0 * (E / T^2) * exp(-E / T)

    T ~ 1.0-2.0 MeV for evaporation neutrons from (p,xn) reactions.
    Normalized so integral from 0 to inf = total_flux.
    """

    temperature_MeV: float = 1.5

    def phi(self, E_MeV: npt.NDArray[np.float64]) -> npt.NDArray[np.float64]:
        T = self.temperature_MeV
        return self.total_flux * (E_MeV / T**2) * np.exp(-E_MeV / T)


@dataclass(frozen=True)
class MonoenergeticFlux(NeutronFlux):
    """Delta-function flux at a single energy.

    For flux_averaged_xs, this reduces to sigma(E_0).
    """

    energy_MeV: float = 14.0  # e.g. D-T fusion neutrons

    def phi(self, E_MeV: npt.NDArray[np.float64]) -> npt.NDArray[np.float64]:
        sigma = 0.01
        return (
            self.total_flux
            / (sigma * math.sqrt(2.0 * math.pi))
            * np.exp(-0.5 * ((E_MeV - self.energy_MeV) / sigma) ** 2)
        )


@dataclass(frozen=True)
class CompositeFlux(NeutronFlux):
    """Sum of multiple flux components (e.g. thermal + epithermal + fast)."""

    components: tuple[NeutronFlux, ...] = ()

    def phi(self, E_MeV: npt.NDArray[np.float64]) -> npt.NDArray[np.float64]:
        result = np.zeros_like(E_MeV)
        for comp in self.components:
            result += comp.phi(E_MeV)
        return result


# ---------------------------------------------------------------------------
# Neutron source from charged-particle reactions
# ---------------------------------------------------------------------------


@dataclass
class NeutronSource:
    """Secondary neutron source from charged-particle reactions."""

    total_neutrons_per_s: float
    spectrum: NeutronFlux
    source_layer_index: int = 0


def neutron_multiplicity(
    target_Z: int,
    target_A: int,
    projectile_Z: int,
    projectile_A: int,
    residual_Z: int,
    residual_A: int,
) -> int:
    """Compute number of emitted neutrons from conservation laws.

    n_neutrons = (A_target + A_projectile - A_residual)
                 - (Z_target + Z_projectile - Z_residual)
    """
    total_emitted_A = target_A + projectile_A - residual_A
    total_emitted_Z = target_Z + projectile_Z - residual_Z
    n_neutrons = total_emitted_A - total_emitted_Z
    return max(0, n_neutrons)


def compute_neutron_source(
    layer_result: LayerResult,
    projectile_Z: int,
    projectile_A: int,
    temperature_MeV: float = 1.5,
) -> NeutronSource:
    """Compute secondary neutron source from a charged-particle layer result.

    Sums over all isotope production channels, weighted by neutron
    multiplicity. The spectrum is modeled as a Weisskopf evaporation.
    """
    total_n_rate = 0.0

    for iso in layer_result.isotope_results.values():
        if iso.source == "daughter":
            continue
        for elem, _ in layer_result.layer.elements:
            for A_target in elem.isotopes:
                n_emitted = neutron_multiplicity(
                    elem.Z, A_target,
                    projectile_Z, projectile_A,
                    iso.Z, iso.A,
                )
                if n_emitted > 0:
                    total_n_rate += n_emitted * iso.production_rate

    spectrum = WeisskopfFlux(
        total_flux=total_n_rate,
        temperature_MeV=temperature_MeV,
    )

    return NeutronSource(
        total_neutrons_per_s=total_n_rate,
        spectrum=spectrum,
    )


# ---------------------------------------------------------------------------
# Flux-averaged cross-section (energy convolution)
# ---------------------------------------------------------------------------


def flux_averaged_xs(
    xs_energies_MeV: npt.NDArray[np.float64],
    xs_mb: npt.NDArray[np.float64],
    flux: NeutronFlux,
    n_points: int = 500,
    E_min_MeV: float = 1e-11,
    E_max_MeV: float = 20.0,
) -> float:
    """Compute flux-averaged cross-section: <sigma> = integral(sigma * phi dE) / integral(phi dE).

    This is the neutron analogue of the Gauss-Hermite straggling convolution.
    """
    E_grid = np.geomspace(E_min_MeV, E_max_MeV, n_points)
    sigma_interp = np.interp(E_grid, xs_energies_MeV, xs_mb, left=0.0, right=0.0)
    phi_values = flux.phi(E_grid)
    numerator = np.trapezoid(sigma_interp * phi_values, E_grid)
    denominator = np.trapezoid(phi_values, E_grid)
    if denominator <= 0:
        return 0.0
    return float(numerator / denominator)


# ---------------------------------------------------------------------------
# Macroscopic cross-section and attenuation
# ---------------------------------------------------------------------------


def macroscopic_xs(
    db: DatabaseProtocol,
    layer: Layer,
    flux: NeutronFlux,
) -> float:
    """Compute macroscopic total cross-section Sigma_t [1/cm] for neutron attenuation.

    Sigma_t = sum_i (N_i * <sigma_t,i>)
    """
    density = layer.density_g_cm3
    avg_A = layer.average_atomic_mass
    N_total = density * AVOGADRO / avg_A

    sigma_t = 0.0
    for elem, atom_frac in layer.elements:
        for A, abundance in elem.isotopes.items():
            weight = atom_frac * abundance
            if weight <= 0:
                continue
            xs_list = db.get_cross_sections("n", elem.Z, A)
            total_sigma_mb = 0.0
            for xs in xs_list:
                avg = flux_averaged_xs(xs.energies_MeV, xs.xs_mb, flux)
                total_sigma_mb += avg
            sigma_t += weight * N_total * total_sigma_mb * MILLIBARN_CM2

    return sigma_t


def neutron_flux_at_depth(
    flux_0: float,
    sigma_t: float,
    depth_cm: float,
) -> float:
    """Compute neutron flux at depth x: phi(x) = phi_0 * exp(-Sigma_t * x)."""
    return flux_0 * math.exp(-sigma_t * depth_cm)


# ---------------------------------------------------------------------------
# Neutron activation computation
# ---------------------------------------------------------------------------


@dataclass
class NeutronActivationResult:
    """Result of neutron activation for a single layer."""

    layer: Layer
    flux: NeutronFlux
    sigma_t: float
    thickness_cm: float
    transmission: float
    isotope_results: dict[str, IsotopeResult]


def compute_neutron_activation(
    db: DatabaseProtocol,
    layer: Layer,
    flux: NeutronFlux,
    irradiation_time_s: float,
    cooling_time_s: float,
    thickness_cm: float | None = None,
    n_depth_points: int = 50,
) -> NeutronActivationResult:
    """Compute neutron activation for a single layer."""
    if thickness_cm is None:
        if layer.thickness_cm is not None:
            thickness_cm = layer.thickness_cm
        elif hasattr(layer, "_thickness") and layer._thickness > 0:
            thickness_cm = layer._thickness
        else:
            msg = "Layer thickness must be specified for neutron activation"
            raise ValueError(msg)

    density = layer.density_g_cm3
    avg_A = layer.average_atomic_mass

    sigma_t = macroscopic_xs(db, layer, flux)
    transmission = math.exp(-sigma_t * thickness_cm)

    N_total = density * AVOGADRO / avg_A

    isotope_results: dict[str, IsotopeResult] = {}

    for elem, atom_frac in layer.elements:
        for A, abundance in elem.isotopes.items():
            weight = atom_frac * abundance
            if weight <= 0:
                continue

            xs_list = db.get_cross_sections("n", elem.Z, A)

            for xs in xs_list:
                avg_xs = flux_averaged_xs(xs.energies_MeV, xs.xs_mb, flux)
                if avg_xs <= 0:
                    continue

                if sigma_t > 1e-30:
                    flux_integral = flux.total_flux * (1.0 - transmission) / sigma_t
                else:
                    flux_integral = flux.total_flux * thickness_cm

                N_isotope = N_total * weight
                prate = N_isotope * avg_xs * MILLIBARN_CM2 * flux_integral

                decay = db.get_decay_data(xs.residual_Z, xs.residual_A, xs.state)
                half_life: float | None = None
                if decay is not None:
                    half_life = decay.half_life_s

                symbol = db.get_element_symbol(xs.residual_Z)
                state_suffix = xs.state if xs.state else ""
                name = f"{symbol}-{xs.residual_A}{state_suffix}"

                time_grid, activity = bateman_activity(
                    prate, half_life, irradiation_time_s, cooling_time_s,
                )

                sat_yield = prate

                if name in isotope_results:
                    existing = isotope_results[name]
                    combined_rate = existing.production_rate + prate
                    combined_tg, combined_act = bateman_activity(
                        combined_rate, half_life,
                        irradiation_time_s, cooling_time_s,
                    )
                    isotope_results[name] = _make_isotope_result(
                        name, xs.residual_Z, xs.residual_A, xs.state,
                        half_life, combined_rate,
                        existing.saturation_yield_Bq_uA + sat_yield,
                        combined_tg, combined_act,
                    )
                else:
                    isotope_results[name] = _make_isotope_result(
                        name, xs.residual_Z, xs.residual_A, xs.state,
                        half_life, prate, sat_yield,
                        time_grid, activity,
                    )

    return NeutronActivationResult(
        layer=layer,
        flux=flux,
        sigma_t=sigma_t,
        thickness_cm=thickness_cm,
        transmission=transmission,
        isotope_results=isotope_results,
    )


def _make_isotope_result(
    name: str,
    Z: int,
    A: int,
    state: str,
    half_life_s: float | None,
    production_rate: float,
    sat_yield: float,
    time_grid: npt.NDArray[np.float64],
    activity: npt.NDArray[np.float64],
) -> IsotopeResult:
    from hyrr.models import IsotopeResult as IR

    return IR(
        name=name,
        Z=Z,
        A=A,
        state=state,
        half_life_s=half_life_s,
        production_rate=production_rate,
        saturation_yield_Bq_uA=sat_yield,
        activity_Bq=float(activity[-1]) if len(activity) > 0 else 0.0,
        time_grid_s=time_grid,
        activity_vs_time_Bq=activity,
        source="direct",
    )


# ---------------------------------------------------------------------------
# Two-pass orchestrator: charged particles + secondary neutrons
# ---------------------------------------------------------------------------


def compute_secondary_neutron_activation(
    db: DatabaseProtocol,
    layer_result: LayerResult,
    projectile_Z: int,
    projectile_A: int,
    irradiation_time_s: float,
    cooling_time_s: float,
    neutron_library: str = "tendl-2025",
    temperature_MeV: float = 1.5,
) -> NeutronActivationResult | None:
    """Compute secondary neutron activation from a charged-particle layer."""
    n_source = compute_neutron_source(
        layer_result, projectile_Z, projectile_A,
        temperature_MeV=temperature_MeV,
    )

    if n_source.total_neutrons_per_s <= 0:
        return None

    thickness = layer_result.layer._thickness
    if thickness <= 0:
        return None

    area = 1.0
    effective_flux = n_source.total_neutrons_per_s / (2.0 * area)

    flux = WeisskopfFlux(
        total_flux=effective_flux,
        temperature_MeV=temperature_MeV,
    )

    return compute_neutron_activation(
        db,
        layer_result.layer,
        flux,
        irradiation_time_s,
        cooling_time_s,
        thickness_cm=thickness,
    )
