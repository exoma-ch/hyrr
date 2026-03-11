"""Tests for hyrr.production with analytically solvable cases."""

from __future__ import annotations

import numpy as np
import pytest

from hyrr.production import (
    activity_to_yield_GBq_per_mAh,
    bateman_activity,
    compute_production_rate,
    daughter_ingrowth,
    generate_depth_profile,
    saturation_yield,
)


class TestProductionRate:
    """Tests for compute_production_rate."""

    def test_constant_xs_and_dedx(self) -> None:
        """With constant sigma=100 mb and constant dE/dx=10 MeV/cm,
        integral = sigma * dE / dEdx = 100 * 4 / 10 = 40 mb*cm
        prate = n_beam * (n_atoms/V) * 40 * 1e-27
        """
        xs_e = np.array([10.0, 20.0])
        xs_mb = np.array([100.0, 100.0])  # constant 100 mb

        def dedx_fn(e: float) -> float:
            return 10.0  # constant 10 MeV/cm

        prate, _, _, _ = compute_production_rate(
            xs_e,
            xs_mb,
            dedx_fn,
            energy_in_MeV=16.0,
            energy_out_MeV=12.0,
            n_target_atoms=1e21,
            beam_particles_per_s=1e15,
            target_volume_cm3=0.02,
            n_points=1000,
        )

        # Expected: 1e15 * (1e21/0.02) * (100 * 4 / 10) * 1e-27
        #         = 1e15 * 5e22 * 40 * 1e-27
        #         = 1e15 * 2e-3 = 2e12
        expected = 1e15 * (1e21 / 0.02) * 40.0 * 1e-27
        np.testing.assert_allclose(prate, expected, rtol=0.01)

    def test_zero_xs_outside_range(self) -> None:
        """Cross-section zero outside energy range gives zero production."""
        xs_e = np.array([100.0, 200.0])  # way above our energy window
        xs_mb = np.array([500.0, 500.0])

        def dedx_fn(e: float) -> float:
            return 10.0

        prate, _, _, _ = compute_production_rate(
            xs_e,
            xs_mb,
            dedx_fn,
            energy_in_MeV=16.0,
            energy_out_MeV=12.0,
            n_target_atoms=1e21,
            beam_particles_per_s=1e15,
            target_volume_cm3=0.02,
        )
        assert prate == pytest.approx(0.0, abs=1e-20)

    def test_returns_correct_shapes(self) -> None:
        """Output arrays should have length n_points."""
        xs_e = np.array([10.0, 20.0])
        xs_mb = np.array([50.0, 50.0])

        def dedx_fn(e: float) -> float:
            return 5.0

        _, energies, xs_out, dedx_out = compute_production_rate(
            xs_e,
            xs_mb,
            dedx_fn,
            energy_in_MeV=15.0,
            energy_out_MeV=11.0,
            n_target_atoms=1e20,
            beam_particles_per_s=1e14,
            target_volume_cm3=0.01,
            n_points=50,
        )
        assert len(energies) == 50
        assert len(xs_out) == 50
        assert len(dedx_out) == 50

    def test_energy_grid_ascending(self) -> None:
        """Energy grid should go from E_out to E_in (ascending)."""
        xs_e = np.array([5.0, 25.0])
        xs_mb = np.array([10.0, 10.0])

        def dedx_fn(e: float) -> float:
            return 8.0

        _, energies, _, _ = compute_production_rate(
            xs_e,
            xs_mb,
            dedx_fn,
            energy_in_MeV=20.0,
            energy_out_MeV=10.0,
            n_target_atoms=1e20,
            beam_particles_per_s=1e14,
            target_volume_cm3=0.01,
        )
        assert energies[0] == pytest.approx(10.0)
        assert energies[-1] == pytest.approx(20.0)
        assert np.all(np.diff(energies) >= 0)


class TestBatemanActivity:
    """Tests for bateman_activity."""

    def test_short_lived_saturation(self) -> None:
        """Very short half-life: activity approaches R at saturation."""
        r = 1e10
        t_half = 1.0  # 1 second
        t_irr = 100.0  # >> half-life

        time, activity = bateman_activity(r, t_half, t_irr, 0.0)
        # At end of irradiation, should be very close to R
        a_eoi = activity[time <= t_irr][-1]
        np.testing.assert_allclose(a_eoi, r, rtol=1e-3)

    def test_pure_decay_cooling(self) -> None:
        """After irradiation, activity decays exponentially."""
        r = 1e10
        t_half = 100.0
        t_irr = 1000.0
        t_cool = 500.0

        time, activity = bateman_activity(r, t_half, t_irr, t_cool)
        a_eoi = r * (1 - np.exp(-np.log(2) / t_half * t_irr))

        # At end of cooling (5 half-lives)
        a_final = activity[-1]
        expected = a_eoi * np.exp(-np.log(2) / t_half * t_cool)
        np.testing.assert_allclose(a_final, expected, rtol=0.05)

    def test_stable_isotope(self) -> None:
        """Stable isotope has zero activity."""
        time, activity = bateman_activity(1e10, None, 86400, 86400)
        assert np.all(activity == 0.0)

    def test_zero_cooling(self) -> None:
        """With zero cooling time, final activity equals EOI activity."""
        r = 1e8
        t_half = 3600.0
        t_irr = 7200.0

        time, activity = bateman_activity(r, t_half, t_irr, 0.0)
        lam = np.log(2) / t_half
        expected_eoi = r * (1 - np.exp(-lam * t_irr))

        # Last irradiation point should match
        a_eoi = activity[time <= t_irr][-1]
        np.testing.assert_allclose(a_eoi, expected_eoi, rtol=1e-4)

    def test_activity_monotone_during_irradiation(self) -> None:
        """Activity should be monotonically increasing during irradiation."""
        _, activity = bateman_activity(1e9, 600.0, 3600.0, 0.0)
        # All points are irradiation (cooling=0)
        assert np.all(np.diff(activity) >= 0)


class TestDaughterIngrowth:
    """Tests for daughter_ingrowth."""

    def test_secular_equilibrium(self) -> None:
        """Long-lived parent, short-lived daughter -> secular equilibrium."""
        a_p_eoi = 1e9  # 1 GBq parent
        t_p = 1e6  # parent half-life
        t_d = 100.0  # daughter half-life
        br = 1.0

        cooling = np.array([1000.0])  # >> daughter half-life
        a_d = daughter_ingrowth(a_p_eoi, t_p, t_d, br, cooling)

        # At secular equilibrium: A_D ~ A_P * exp(-lambda_P * t)
        lambda_p = np.log(2) / t_p
        expected = br * a_p_eoi * np.exp(-lambda_p * 1000.0)
        np.testing.assert_allclose(a_d[0], expected, rtol=0.05)

    def test_stable_daughter_zero(self) -> None:
        """Stable daughter has zero activity."""
        cooling = np.array([100.0, 200.0, 300.0])
        a_d = daughter_ingrowth(1e9, 3600.0, None, 1.0, cooling)
        assert np.all(a_d == 0.0)

    def test_zero_branching(self) -> None:
        """Zero branching ratio gives zero daughter activity."""
        cooling = np.array([100.0, 200.0])
        a_d = daughter_ingrowth(1e9, 3600.0, 600.0, 0.0, cooling)
        np.testing.assert_allclose(a_d, 0.0, atol=1e-30)

    def test_at_t_zero(self) -> None:
        """At t=0 cooling, daughter activity should be zero."""
        cooling = np.array([0.0])
        a_d = daughter_ingrowth(1e9, 3600.0, 600.0, 1.0, cooling)
        assert a_d[0] == pytest.approx(0.0, abs=1e-10)


class TestSaturationYield:
    """Tests for saturation_yield."""

    def test_basic(self) -> None:
        """Y_sat = R / I_uA."""
        r = 1e12
        i_mA = 0.15
        y = saturation_yield(r, 100.0, i_mA)
        expected = r / (i_mA * 1e3)  # = 1e12 / 150
        assert y == pytest.approx(expected)

    def test_stable_zero(self) -> None:
        """Stable isotope has zero saturation yield."""
        assert saturation_yield(1e12, None, 0.15) == 0.0


class TestYieldConversion:
    """Tests for activity_to_yield_GBq_per_mAh."""

    def test_gbq_per_mah(self) -> None:
        """Check unit conversion from Bq to GBq/mAh."""
        activity_bq = 78.61e9  # 78.61 GBq
        i_mA = 0.15
        t_s = 86400.0  # 1 day

        y = activity_to_yield_GBq_per_mAh(activity_bq, i_mA, t_s)
        # charge = 0.15 mA * 24 h = 3.6 mAh
        expected = 78.61 / 3.6
        assert y == pytest.approx(expected, rel=0.01)

    def test_zero_time(self) -> None:
        """Zero irradiation time gives zero yield."""
        assert activity_to_yield_GBq_per_mAh(1e9, 0.15, 0.0) == 0.0


class TestDepthProfile:
    """Tests for generate_depth_profile."""

    def test_constant_dedx_depth(self) -> None:
        """Constant dE/dx: depth increments should be uniform."""
        energies = np.linspace(10.0, 20.0, 11)
        dedx = np.full(11, 5.0)  # 5 MeV/cm everywhere

        depths, _ = generate_depth_profile(energies, dedx, 0.1, 1.0, 1)
        # dE = 1.0 MeV per step, dE/dx = 5 -> dx = 0.2 cm per step
        expected_total = 10 * (1.0 / 5.0)  # 2.0 cm
        np.testing.assert_allclose(depths[-1], expected_total, rtol=1e-10)

    def test_heat_proportional_to_dedx(self) -> None:
        """Heat density should scale with stopping power."""
        energies = np.linspace(10.0, 20.0, 5)
        dedx = np.array([5.0, 10.0, 15.0, 10.0, 5.0])

        _, heat = generate_depth_profile(energies, dedx, 0.1, 1.0, 1)
        # Heat proportional to |dE/dx|
        np.testing.assert_allclose(heat[2] / heat[0], 15.0 / 5.0, rtol=1e-10)
