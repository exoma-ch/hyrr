"""Tests for hyrr.neutrons — neutron activation module."""

from __future__ import annotations

import math

import numpy as np
import pytest

from hyrr.neutrons import (
    CompositeFlux,
    EpithermalFlux,
    MonoenergeticFlux,
    NeutronSource,
    ThermalFlux,
    WeisskopfFlux,
    flux_averaged_xs,
    neutron_multiplicity,
)


# ---------------------------------------------------------------------------
# Neutron multiplicity
# ---------------------------------------------------------------------------


class TestNeutronMultiplicity:
    def test_p2n(self) -> None:
        # Mo-100(p,2n)Tc-99m: target(42,100) + proj(1,1) -> res(43,99)
        # emitted nucleons: 100+1-99 = 2, emitted protons: 42+1-43 = 0
        assert neutron_multiplicity(42, 100, 1, 1, 43, 99) == 2

    def test_pn(self) -> None:
        # Cu-65(p,n)Zn-65: target(29,65) + proj(1,1) -> res(30,65)
        # emitted: 65+1-65=1 nucleons, 29+1-30=0 protons -> 1 neutron
        assert neutron_multiplicity(29, 65, 1, 1, 30, 65) == 1

    def test_p_gamma(self) -> None:
        # (p,gamma): no emitted nucleons -> 0 neutrons
        # e.g. target(29,63) + proj(1,1) -> res(30,64)
        assert neutron_multiplicity(29, 63, 1, 1, 30, 64) == 0

    def test_p_alpha(self) -> None:
        # (p,alpha): emits He-4, no neutrons
        # target(28,58) + proj(1,1) -> res(27,55) + alpha
        # emitted A: 58+1-55=4, emitted Z: 28+1-27=2 -> 4-2=2 neutrons? No.
        # Actually (p,alpha) means residual is (Z_t+1-2, A_t+1-4) = (27,55)
        # emitted: 4 nucleons, 2 protons -> 2 neutrons (from the alpha!)
        # But alpha is He-4 (2p+2n), so 2 neutrons are bound in alpha, not free
        # This is a known limitation: we count emitted neutrons including
        # those in composite ejectiles. For secondary activation this is
        # conservative (overestimates).
        assert neutron_multiplicity(28, 58, 1, 1, 27, 55) == 2

    def test_negative_clamped(self) -> None:
        # Pathological case: residual heavier than target+projectile
        assert neutron_multiplicity(1, 1, 1, 1, 2, 3) == 0


# ---------------------------------------------------------------------------
# Flux spectra
# ---------------------------------------------------------------------------


class TestThermalFlux:
    def test_normalization(self) -> None:
        """Integral of Maxwellian should approximate total_flux."""
        flux = ThermalFlux(total_flux=1e12, kT_eV=0.0253)
        E = np.geomspace(1e-11, 1e-3, 10000)  # MeV
        phi = flux.phi(E)
        integral = np.trapezoid(phi, E)
        assert integral == pytest.approx(1e12, rel=0.05)

    def test_peak_location(self) -> None:
        """Maxwellian peaks at E = kT/2."""
        flux = ThermalFlux(total_flux=1e12, kT_eV=0.0253)
        E = np.geomspace(1e-11, 1e-3, 100000)
        phi = flux.phi(E)
        peak_E = E[np.argmax(phi)]
        expected_peak = 0.0253e-6 / 2  # kT/2 in MeV
        assert peak_E == pytest.approx(expected_peak, rel=0.1)


class TestWeisskopfFlux:
    def test_normalization(self) -> None:
        """Integral of Weisskopf spectrum should approximate total_flux."""
        flux = WeisskopfFlux(total_flux=1e8, temperature_MeV=1.5)
        E = np.linspace(0.001, 30, 5000)
        phi = flux.phi(E)
        integral = np.trapezoid(phi, E)
        assert integral == pytest.approx(1e8, rel=0.02)

    def test_peak_at_T(self) -> None:
        """Weisskopf peaks at E = T."""
        flux = WeisskopfFlux(total_flux=1e8, temperature_MeV=1.5)
        E = np.linspace(0.01, 10, 10000)
        phi = flux.phi(E)
        peak_E = E[np.argmax(phi)]
        assert peak_E == pytest.approx(1.5, abs=0.05)


class TestEpithermalFlux:
    def test_1_over_E(self) -> None:
        """phi(E) * E should be constant in the epithermal range."""
        flux = EpithermalFlux(total_flux=1e10, E_min_eV=0.5, E_max_eV=1e6)
        E = np.geomspace(1e-6, 0.5, 100)  # MeV (0.5 eV to 0.5 MeV)
        phi = flux.phi(E)
        phi_E = phi * E
        # Should be constant where phi > 0
        nonzero = phi > 0
        if np.any(nonzero):
            vals = phi_E[nonzero]
            assert np.std(vals) / np.mean(vals) < 0.01  # <1% variation

    def test_zero_outside_range(self) -> None:
        flux = EpithermalFlux(total_flux=1e10, E_min_eV=0.5, E_max_eV=1e6)
        E_below = np.array([1e-8])  # well below E_min
        E_above = np.array([10.0])  # well above E_max
        assert flux.phi(E_below)[0] == 0.0
        assert flux.phi(E_above)[0] == 0.0


class TestMonoenergeticFlux:
    def test_integral(self) -> None:
        """Delta-function flux integrates to total_flux."""
        flux = MonoenergeticFlux(total_flux=1e10, energy_MeV=14.0)
        E = np.linspace(0.01, 30, 10000)
        phi = flux.phi(E)
        integral = np.trapezoid(phi, E)
        assert integral == pytest.approx(1e10, rel=0.05)


class TestCompositeFlux:
    def test_sum(self) -> None:
        th = ThermalFlux(total_flux=1e12)
        epi = EpithermalFlux(total_flux=1e10)
        comp = CompositeFlux(total_flux=1.01e12, components=(th, epi))
        E = np.array([1e-8])  # thermal energy in MeV
        assert comp.phi(E)[0] == pytest.approx(
            th.phi(E)[0] + epi.phi(E)[0]
        )


# ---------------------------------------------------------------------------
# Flux-averaged cross-section
# ---------------------------------------------------------------------------


class TestFluxAveragedXS:
    def test_monoenergetic(self) -> None:
        """For delta flux at E0, <sigma> should equal sigma(E0)."""
        E_xs = np.linspace(0, 20, 200)
        # Simple triangular XS peaking at 10 MeV
        xs = np.where(E_xs < 10, E_xs * 10, (20 - E_xs) * 10)
        xs = np.maximum(xs, 0)

        flux = MonoenergeticFlux(total_flux=1e10, energy_MeV=10.0)
        avg = flux_averaged_xs(E_xs, xs, flux, n_points=2000)
        # sigma at E=10 MeV should be 100 mb
        assert avg == pytest.approx(100.0, rel=0.1)

    def test_zero_xs(self) -> None:
        """Zero cross-section gives zero average."""
        E_xs = np.linspace(0, 20, 100)
        xs = np.zeros_like(E_xs)
        flux = ThermalFlux(total_flux=1e12)
        avg = flux_averaged_xs(E_xs, xs, flux)
        assert avg == 0.0

    def test_constant_xs(self) -> None:
        """Constant sigma(E) = C gives <sigma> = C."""
        E_xs = np.linspace(0, 20, 200)
        xs = np.full_like(E_xs, 50.0)  # 50 mb everywhere
        flux = WeisskopfFlux(total_flux=1e8, temperature_MeV=1.5)
        avg = flux_averaged_xs(E_xs, xs, flux)
        assert avg == pytest.approx(50.0, rel=0.05)
