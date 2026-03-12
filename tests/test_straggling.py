"""Tests for energy straggling convolution (Bohr formula + Gauss-Hermite)."""

from __future__ import annotations

import math

import numpy as np
import pytest

from hyrr.models import Beam, BeamProfile
from hyrr.production import _gauss_hermite_convolved_xs, compute_production_rate
from hyrr.stopping import bohr_straggling_variance_per_cm, cumulative_straggling_sigma


# ---------------------------------------------------------------------------
# Bohr straggling unit tests
# ---------------------------------------------------------------------------


class TestBohrStraggling:
    """Tests for bohr_straggling_variance_per_cm."""

    def test_proton_in_copper(self) -> None:
        """Hand-calc: proton in Cu (Z=29, A=63.546, ρ=8.96 g/cm³).

        dσ²/dz = 4π e⁴ Z_proj² Σ Z_i n_i
        n_Cu = ρ N_A / A = 8.96 * 6.022e23 / 63.546 = 8.489e22 atoms/cm³
        e² = 1.4399764e-13 MeV·cm
        e⁴ = (1.4399764e-13)² = 2.07353...e-26 MeV²·cm²
        dσ²/dz = 4π * e⁴ * 1² * 29 * 8.489e22
        """
        Z_proj = 1
        composition = [(29, 1.0)]  # pure Cu
        density = 8.96
        atomic_masses = {29: 63.546}

        result = bohr_straggling_variance_per_cm(
            Z_proj, composition, density, atomic_masses,
        )

        # Compute expected
        e2 = 1.4399764e-13  # MeV·cm
        NA = 6.02214076e23
        n_Cu = density * NA * 1.0 / 63.546
        expected = 4.0 * math.pi * e2**2 * Z_proj**2 * 29 * n_Cu

        assert result == pytest.approx(expected, rel=1e-6)
        # Sanity: should be on order of 1e-3 to 1e-1 MeV²/cm
        assert 1e-4 < result < 1.0

    def test_zero_density_gives_zero(self) -> None:
        """Zero density means zero straggling."""
        result = bohr_straggling_variance_per_cm(
            1, [(29, 1.0)], 0.0, {29: 63.546},
        )
        assert result == 0.0

    def test_scales_with_projectile_Z_squared(self) -> None:
        """Straggling scales as Z_proj²."""
        composition = [(29, 1.0)]
        density = 8.96
        am = {29: 63.546}

        s1 = bohr_straggling_variance_per_cm(1, composition, density, am)
        s2 = bohr_straggling_variance_per_cm(2, composition, density, am)

        assert s2 == pytest.approx(4.0 * s1, rel=1e-10)

    def test_compound_material(self) -> None:
        """Two-element compound: contributions from both elements."""
        # 60% Mo (Z=42, A=95.95), 40% O (Z=8, A=16.0) by mass
        composition = [(42, 0.6), (8, 0.4)]
        density = 5.0
        am = {42: 95.95, 8: 16.0}

        result = bohr_straggling_variance_per_cm(1, composition, density, am)
        assert result > 0


class TestCumulativeStraggling:
    """Tests for cumulative_straggling_sigma."""

    def test_zero_thickness_returns_initial(self) -> None:
        """σ_E after zero thickness equals initial σ_E."""
        result = cumulative_straggling_sigma(
            0.3, 1, [(29, 1.0)], 8.96, {29: 63.546}, 0.0,
        )
        assert result == pytest.approx(0.3, rel=1e-10)

    def test_zero_initial_with_thickness(self) -> None:
        """Starting from zero spread, grows as sqrt(dσ²/dz × Δz)."""
        am = {29: 63.546}
        dsig2 = bohr_straggling_variance_per_cm(1, [(29, 1.0)], 8.96, am)
        thickness = 0.1  # cm

        result = cumulative_straggling_sigma(
            0.0, 1, [(29, 1.0)], 8.96, am, thickness,
        )
        expected = math.sqrt(dsig2 * thickness)
        assert result == pytest.approx(expected, rel=1e-10)

    def test_quadrature_addition(self) -> None:
        """σ = sqrt(σ₀² + dσ²/dz × Δz)."""
        am = {29: 63.546}
        sigma0 = 0.5
        thickness = 0.2

        result = cumulative_straggling_sigma(
            sigma0, 1, [(29, 1.0)], 8.96, am, thickness,
        )
        dsig2 = bohr_straggling_variance_per_cm(1, [(29, 1.0)], 8.96, am)
        expected = math.sqrt(sigma0**2 + dsig2 * thickness)
        assert result == pytest.approx(expected, rel=1e-10)


# ---------------------------------------------------------------------------
# Gauss-Hermite convolution tests
# ---------------------------------------------------------------------------


class TestGaussHermiteConvolution:
    """Tests for _gauss_hermite_convolved_xs."""

    def test_constant_xs_unchanged(self) -> None:
        """Convolving a constant cross-section returns the same constant."""
        def xs_fn(E: np.ndarray) -> np.ndarray:
            return np.full_like(E, 100.0)

        E_mean = np.array([10.0, 15.0, 20.0])
        sigma_E = np.array([0.5, 1.0, 2.0])

        result = _gauss_hermite_convolved_xs(xs_fn, E_mean, sigma_E)
        np.testing.assert_allclose(result, 100.0, rtol=1e-10)

    def test_zero_spread_returns_direct(self) -> None:
        """With σ_E ≈ 0, returns σ(E_mean) directly."""
        xs_data_E = np.array([5.0, 10.0, 15.0, 20.0, 25.0])
        xs_data_mb = np.array([0.0, 50.0, 100.0, 50.0, 0.0])

        def xs_fn(E: np.ndarray) -> np.ndarray:
            return np.interp(E, xs_data_E, xs_data_mb, left=0.0, right=0.0)

        E_mean = np.array([15.0])
        sigma_E = np.array([0.0])

        result = _gauss_hermite_convolved_xs(xs_fn, E_mean, sigma_E)
        assert result[0] == pytest.approx(100.0, rel=1e-6)

    def test_peaked_xs_spread_reduces_peak(self) -> None:
        """Gaussian convolution of a peaked xs should reduce the peak value."""
        xs_data_E = np.linspace(0, 30, 300)
        # Sharp peak at 15 MeV
        xs_data_mb = 100.0 * np.exp(-((xs_data_E - 15.0) ** 2) / (2 * 0.5**2))

        def xs_fn(E: np.ndarray) -> np.ndarray:
            return np.interp(E, xs_data_E, xs_data_mb, left=0.0, right=0.0)

        E_mean = np.array([15.0])
        # No spread
        no_spread = _gauss_hermite_convolved_xs(xs_fn, E_mean, np.array([1e-9]))
        # With spread
        with_spread = _gauss_hermite_convolved_xs(xs_fn, E_mean, np.array([2.0]))

        # Peak should be reduced by convolution
        assert with_spread[0] < no_spread[0]
        # But still positive
        assert with_spread[0] > 0

    def test_below_threshold_with_spread(self) -> None:
        """Cross-section with threshold: spread enables sub-threshold production."""
        # Threshold reaction: σ = 0 below 14 MeV, σ = 100 above
        xs_data_E = np.array([0.0, 13.99, 14.0, 30.0])
        xs_data_mb = np.array([0.0, 0.0, 100.0, 100.0])

        def xs_fn(E: np.ndarray) -> np.ndarray:
            return np.interp(E, xs_data_E, xs_data_mb, left=0.0, right=0.0)

        # Mean energy just below threshold
        E_mean = np.array([13.5])

        no_spread = _gauss_hermite_convolved_xs(xs_fn, E_mean, np.array([1e-9]))
        with_spread = _gauss_hermite_convolved_xs(xs_fn, E_mean, np.array([1.0]))

        assert no_spread[0] == pytest.approx(0.0, abs=1e-6)
        assert with_spread[0] > 0  # tail reaches above threshold


# ---------------------------------------------------------------------------
# Integration: compute_production_rate with straggling
# ---------------------------------------------------------------------------


class TestProductionRateWithStraggling:
    """Tests for compute_production_rate with sigma_E_fn."""

    def test_sigma_E_fn_none_backward_compat(self) -> None:
        """sigma_E_fn=None produces identical results to the original code."""
        xs_e = np.array([10.0, 20.0])
        xs_mb = np.array([100.0, 100.0])

        def dedx_fn(e):
            return 10.0

        kwargs = dict(
            xs_energies_MeV=xs_e,
            xs_mb=xs_mb,
            dedx_fn=dedx_fn,
            energy_in_MeV=16.0,
            energy_out_MeV=12.0,
            n_target_atoms=1e21,
            beam_particles_per_s=1e15,
            target_volume_cm3=0.02,
            n_points=200,
        )

        prate_no_fn, _, _, _ = compute_production_rate(**kwargs, sigma_E_fn=None)
        prate_zero_fn, _, _, _ = compute_production_rate(
            **kwargs, sigma_E_fn=lambda z: 0.0,
        )

        # Both should match closely (zero spread = no convolution effect)
        np.testing.assert_allclose(prate_no_fn, prate_zero_fn, rtol=1e-4)

    def test_near_threshold_straggling_increases_yield(self) -> None:
        """Below a threshold, energy straggling enables production.

        Threshold at 14 MeV; beam at 13.8 → 12.8 MeV (entirely below).
        Without straggling: zero yield.
        With straggling: high-energy tail reaches above threshold.
        """
        xs_e = np.array([0.0, 13.99, 14.0, 20.0])
        xs_mb = np.array([0.0, 0.0, 100.0, 100.0])

        def dedx_fn(e):
            return np.full_like(np.asarray(e), 10.0)

        kwargs = dict(
            xs_energies_MeV=xs_e,
            xs_mb=xs_mb,
            dedx_fn=dedx_fn,
            energy_in_MeV=13.8,
            energy_out_MeV=12.8,
            n_target_atoms=1e21,
            beam_particles_per_s=1e15,
            target_volume_cm3=0.02,
            n_points=200,
        )

        prate_no, _, _, _ = compute_production_rate(**kwargs, sigma_E_fn=None)
        prate_strag, _, _, _ = compute_production_rate(
            **kwargs, sigma_E_fn=lambda z: 0.5,
        )

        # Without straggling: zero (entirely below threshold)
        assert prate_no == pytest.approx(0.0, abs=1e-10)
        # With straggling: positive (tail reaches above threshold)
        assert prate_strag > 0


# ---------------------------------------------------------------------------
# Beam model tests
# ---------------------------------------------------------------------------


class TestBeamEnergySpread:
    """Tests for Beam.energy_spread_MeV."""

    def test_default_zero(self) -> None:
        beam = Beam(projectile="p", energy_MeV=18.0, current_mA=0.15)
        assert beam.energy_spread_MeV == 0.0

    def test_positive_value(self) -> None:
        beam = Beam(
            projectile="p", energy_MeV=18.0, current_mA=0.15,
            energy_spread_MeV=0.3,
        )
        assert beam.energy_spread_MeV == 0.3

    def test_negative_raises(self) -> None:
        with pytest.raises(ValueError, match="energy_spread_MeV"):
            Beam(
                projectile="p", energy_MeV=18.0, current_mA=0.15,
                energy_spread_MeV=-0.1,
            )


# ---------------------------------------------------------------------------
# BeamProfile tests
# ---------------------------------------------------------------------------


class TestBeamProfile:
    """Tests for BeamProfile and Beam integration."""

    def test_default_pencil(self) -> None:
        """Default profile is a zero-size pencil beam."""
        p = BeamProfile()
        assert p.sigma_x_cm == 0.0
        assert p.effective_sigma_y_cm == 0.0
        assert p.divergence_x_mrad == 0.0
        assert p.is_pencil is True
        assert p.spot_radius_cm == 0.0

    def test_circular_beam(self) -> None:
        """Setting only σ_x gives circular beam (σ_y = σ_x)."""
        p = BeamProfile(sigma_x_cm=0.3)
        assert p.effective_sigma_y_cm == 0.3
        assert p.is_pencil is False
        assert p.spot_radius_cm == pytest.approx(0.3)

    def test_elliptical_beam(self) -> None:
        """Explicit σ_x and σ_y for elliptical beam."""
        p = BeamProfile(sigma_x_cm=0.3, sigma_y_cm=0.5)
        assert p.effective_sigma_y_cm == 0.5
        assert p.spot_radius_cm == pytest.approx(0.4)  # (0.3+0.5)/2

    def test_divergence_circular_default(self) -> None:
        """Setting only divergence_x gives circular divergence."""
        p = BeamProfile(divergence_x_mrad=3.0)
        assert p.effective_divergence_y_mrad == 3.0

    def test_divergence_asymmetric(self) -> None:
        """Explicit x and y divergences."""
        p = BeamProfile(divergence_x_mrad=3.0, divergence_y_mrad=5.0)
        assert p.effective_divergence_y_mrad == 5.0

    def test_emittance_optional(self) -> None:
        """Emittance fields default to None."""
        p = BeamProfile(sigma_x_cm=0.2, divergence_x_mrad=3.0)
        assert p.emittance_x_mm_mrad is None

    def test_emittance_set(self) -> None:
        """Emittance can be set explicitly."""
        p = BeamProfile(emittance_x_mm_mrad=5.0)
        assert p.emittance_x_mm_mrad == 5.0

    def test_twiss_alpha(self) -> None:
        """Twiss α defaults to 0 (waist)."""
        p = BeamProfile()
        assert p.alpha_x == 0.0
        assert p.alpha_y == 0.0
        p2 = BeamProfile(alpha_x=-1.5)
        assert p2.alpha_x == -1.5

    def test_negative_sigma_raises(self) -> None:
        with pytest.raises(ValueError, match="sigma_x_cm"):
            BeamProfile(sigma_x_cm=-0.1)

    def test_negative_sigma_y_raises(self) -> None:
        with pytest.raises(ValueError, match="sigma_y_cm"):
            BeamProfile(sigma_y_cm=-0.1)

    def test_negative_divergence_raises(self) -> None:
        with pytest.raises(ValueError, match="divergence_x_mrad"):
            BeamProfile(divergence_x_mrad=-1.0)

    def test_negative_emittance_raises(self) -> None:
        with pytest.raises(ValueError, match="emittance_x_mm_mrad"):
            BeamProfile(emittance_x_mm_mrad=-1.0)

    def test_beam_with_profile_none(self) -> None:
        """Beam with no profile (1D mode) — backward compat."""
        beam = Beam(projectile="p", energy_MeV=18.0, current_mA=0.15)
        assert beam.profile is None

    def test_beam_with_profile(self) -> None:
        """Beam with full profile."""
        prof = BeamProfile(sigma_x_cm=0.2, divergence_x_mrad=3.0)
        beam = Beam(
            projectile="p", energy_MeV=18.0, current_mA=0.15,
            energy_spread_MeV=0.3, profile=prof,
        )
        assert beam.profile is not None
        assert beam.profile.sigma_x_cm == 0.2

    def test_beam_position_direction(self) -> None:
        """Beam with explicit 3D pose."""
        beam = Beam(
            projectile="p", energy_MeV=18.0, current_mA=0.15,
            position=(1.0, 2.0, 0.0),
            direction=(0.0, 0.0, 1.0),
        )
        np.testing.assert_array_equal(beam.position_array, [1.0, 2.0, 0.0])
        np.testing.assert_allclose(beam.direction_array, [0.0, 0.0, 1.0])

    def test_beam_direction_normalized(self) -> None:
        """Direction property returns a unit vector."""
        beam = Beam(
            projectile="p", energy_MeV=18.0, current_mA=0.15,
            direction=(3.0, 0.0, 4.0),
        )
        d = beam.direction_array
        np.testing.assert_allclose(np.linalg.norm(d), 1.0)
        np.testing.assert_allclose(d, [0.6, 0.0, 0.8])

    def test_beam_defaults_position_direction(self) -> None:
        """Default position = origin, direction = +z."""
        beam = Beam(projectile="p", energy_MeV=18.0, current_mA=0.15)
        np.testing.assert_array_equal(beam.position_array, [0.0, 0.0, 0.0])
        np.testing.assert_array_equal(beam.direction_array, [0.0, 0.0, 1.0])

    def test_beam_zero_direction_raises(self) -> None:
        """Zero-length direction vector raises."""
        with pytest.raises(ValueError, match="direction"):
            Beam(
                projectile="p", energy_MeV=18.0, current_mA=0.15,
                direction=(0.0, 0.0, 0.0),
            )
