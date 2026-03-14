"""Unit tests for hyrr.neutrons — no real nuclear data required."""

from __future__ import annotations

import numpy as np
import pytest

from hyrr.neutrons import (
    CompositeFlux,
    EpithermalFlux,
    MonoenergeticFlux,
    NeutronFlux,
    ThermalFlux,
    WeisskopfFlux,
    flux_averaged_xs,
    neutron_multiplicity,
)

# ---------------------------------------------------------------------------
# neutron_multiplicity
# ---------------------------------------------------------------------------


class TestNeutronMultiplicity:
    """Tests for neutron_multiplicity conservation-law calculation."""

    def test_p_2n_mo100_tc99m(self) -> None:
        """Mo-100(p,2n)Tc-99m: expect 2 neutrons."""
        n = neutron_multiplicity(
            target_Z=42,
            target_A=100,
            projectile_Z=1,
            projectile_A=1,
            residual_Z=43,
            residual_A=99,
        )
        assert n == 2

    def test_p_n_mo100_tc100(self) -> None:
        """Mo-100(p,n)Tc-100: expect 1 neutron."""
        n = neutron_multiplicity(
            target_Z=42,
            target_A=100,
            projectile_Z=1,
            projectile_A=1,
            residual_Z=43,
            residual_A=100,
        )
        assert n == 1

    def test_n_gamma_mo98_mo99(self) -> None:
        """Mo-98(n,gamma)Mo-99: 0 neutrons emitted (capture)."""
        n = neutron_multiplicity(
            target_Z=42,
            target_A=98,
            projectile_Z=0,
            projectile_A=1,
            residual_Z=42,
            residual_A=99,
        )
        assert n == 0

    def test_p_3n(self) -> None:
        """Generic (p,3n) reaction: expect 3 neutrons."""
        # Invent a target A=100, Z=50, residual A=98, Z=51 → emitted A=3, Z=0
        n = neutron_multiplicity(
            target_Z=50,
            target_A=100,
            projectile_Z=1,
            projectile_A=1,
            residual_Z=51,
            residual_A=98,
        )
        assert n == 3

    def test_negative_clamped_to_zero(self) -> None:
        """If the formula gives negative neutrons, clamp to 0."""
        # Contrived case where residual is heavier than compound
        n = neutron_multiplicity(
            target_Z=1,
            target_A=1,
            projectile_Z=0,
            projectile_A=1,
            residual_Z=1,
            residual_A=3,
        )
        assert n == 0

    def test_alpha_xn(self) -> None:
        """Cu-63(alpha,n)Ga-66: expect 1 neutron."""
        n = neutron_multiplicity(
            target_Z=29,
            target_A=63,
            projectile_Z=2,
            projectile_A=4,
            residual_Z=31,
            residual_A=66,
        )
        assert n == 1


# ---------------------------------------------------------------------------
# Flux spectrum models: phi(E) non-negative, reasonable integral
# ---------------------------------------------------------------------------


class TestFluxSpectra:
    """Verify each flux model returns non-negative phi and integrates sensibly."""

    E_GRID = np.geomspace(1e-11, 20.0, 2000)

    def _integrate(self, flux: NeutronFlux) -> float:
        phi = flux.phi(self.E_GRID)
        return float(np.trapezoid(phi, self.E_GRID))

    # -- ThermalFlux --------------------------------------------------------

    def test_thermal_nonnegative(self) -> None:
        flux = ThermalFlux(total_flux=1e12)
        phi = flux.phi(self.E_GRID)
        assert np.all(phi >= 0)

    def test_thermal_integral(self) -> None:
        flux = ThermalFlux(total_flux=1e12)
        integral = self._integrate(flux)
        # Integral should be close to total_flux (within grid resolution)
        np.testing.assert_allclose(integral, 1e12, rtol=0.05)

    # -- WeisskopfFlux ------------------------------------------------------

    def test_weisskopf_nonnegative(self) -> None:
        flux = WeisskopfFlux(total_flux=1e10, temperature_MeV=1.5)
        phi = flux.phi(self.E_GRID)
        assert np.all(phi >= 0)

    def test_weisskopf_integral(self) -> None:
        flux = WeisskopfFlux(total_flux=1e10, temperature_MeV=1.5)
        integral = self._integrate(flux)
        np.testing.assert_allclose(integral, 1e10, rtol=0.05)

    # -- EpithermalFlux -----------------------------------------------------

    def test_epithermal_nonnegative(self) -> None:
        flux = EpithermalFlux(total_flux=1e11)
        phi = flux.phi(self.E_GRID)
        assert np.all(phi >= 0)

    def test_epithermal_integral(self) -> None:
        flux = EpithermalFlux(total_flux=1e11)
        integral = self._integrate(flux)
        np.testing.assert_allclose(integral, 1e11, rtol=0.10)

    # -- MonoenergeticFlux --------------------------------------------------

    def test_monoenergetic_nonnegative(self) -> None:
        flux = MonoenergeticFlux(total_flux=1e9, energy_MeV=14.0)
        phi = flux.phi(self.E_GRID)
        assert np.all(phi >= 0)

    # -- CompositeFlux ------------------------------------------------------

    def test_composite_nonnegative(self) -> None:
        thermal = ThermalFlux(total_flux=1e12)
        epi = EpithermalFlux(total_flux=1e11)
        comp = CompositeFlux(total_flux=0, components=(thermal, epi))
        phi = comp.phi(self.E_GRID)
        assert np.all(phi >= 0)


# ---------------------------------------------------------------------------
# flux_averaged_xs
# ---------------------------------------------------------------------------


class TestFluxAveragedXS:
    """Tests for the flux-averaged cross-section convolution."""

    def test_monoenergetic_returns_xs_at_energy(self) -> None:
        """With a delta-like flux at 14 MeV, <sigma> ~ sigma(14 MeV)."""
        E = np.linspace(0.1, 20.0, 500)
        xs = 100.0 * np.exp(-((E - 14.0) ** 2) / 2.0)  # peaked at 14 MeV
        flux = MonoenergeticFlux(total_flux=1e10, energy_MeV=14.0)
        avg = flux_averaged_xs(E, xs, flux)
        # Should be close to the XS value at 14 MeV = 100 mb
        np.testing.assert_allclose(avg, 100.0, rtol=0.15)

    def test_zero_xs_returns_zero(self) -> None:
        """If sigma(E) = 0 everywhere, <sigma> = 0."""
        E = np.linspace(0.01, 20.0, 200)
        xs = np.zeros_like(E)
        flux = ThermalFlux(total_flux=1e12)
        avg = flux_averaged_xs(E, xs, flux)
        assert avg == pytest.approx(0.0, abs=1e-10)

    def test_constant_xs_returns_constant(self) -> None:
        """If sigma(E) = C everywhere, <sigma> = C."""
        E = np.linspace(1e-8, 20.0, 500)
        C = 42.0
        xs = np.full_like(E, C)
        flux = WeisskopfFlux(total_flux=1e10, temperature_MeV=1.5)
        avg = flux_averaged_xs(E, xs, flux)
        np.testing.assert_allclose(avg, C, rtol=0.05)
