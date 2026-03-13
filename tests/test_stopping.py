"""Tests for hyrr.stopping module."""

from __future__ import annotations

import numpy as np
import pytest

from hyrr.stopping import (
    _get_interpolator,
    _interpolator_cache,
    _make_interpolator,
    compound_dedx,
    compute_thickness_from_energy,
    dedx_MeV_per_cm,
    elemental_dedx,
)

# ---------------------------------------------------------------------------
# Mock database
# ---------------------------------------------------------------------------


class MockDB:
    """Mock database with known PSTAR/ASTAR values for testing.

    Implements the subset of DatabaseProtocol used by stopping.py.
    Must be hashable for lru_cache compatibility.
    """

    def __hash__(self) -> int:
        return id(self)

    def __eq__(self, other: object) -> bool:
        return self is other

    def get_stopping_power(
        self, source: str, target_Z: int
    ) -> tuple[np.ndarray, np.ndarray]:
        """Return synthetic stopping power tables."""
        if source == "PSTAR" and target_Z == 42:  # Molybdenum
            energies = np.array([1.0, 2.0, 5.0, 10.0, 15.0, 20.0, 50.0, 100.0])
            dedx = np.array([50.0, 40.0, 25.0, 18.0, 15.0, 13.0, 9.0, 7.0])
            return energies, dedx
        if source == "PSTAR" and target_Z == 6:  # Carbon
            energies = np.array([1.0, 2.0, 5.0, 10.0, 15.0, 20.0, 50.0, 100.0])
            dedx = np.array([30.0, 24.0, 16.0, 12.0, 10.0, 9.0, 6.0, 5.0])
            return energies, dedx
        if source == "ASTAR" and target_Z == 42:  # Molybdenum
            energies = np.array([1.0, 2.0, 5.0, 10.0, 20.0, 40.0, 100.0, 200.0])
            dedx = np.array([200.0, 160.0, 100.0, 70.0, 50.0, 35.0, 25.0, 20.0])
            return energies, dedx
        if source == "ASTAR" and target_Z == 6:  # Carbon
            energies = np.array([1.0, 2.0, 5.0, 10.0, 20.0, 40.0, 100.0, 200.0])
            dedx = np.array([120.0, 96.0, 64.0, 48.0, 35.0, 25.0, 18.0, 14.0])
            return energies, dedx
        msg = f"MockDB has no data for source={source}, target_Z={target_Z}"
        raise ValueError(msg)

    # Stubs for remaining DatabaseProtocol methods (unused by stopping.py)
    def get_natural_abundances(self, Z: int) -> dict:
        return {}

    def get_cross_sections(self, *a: object) -> list:
        return []

    def get_decay_data(self, *a: object) -> None:
        return None

    def get_element_symbol(self, Z: int) -> str:
        return ""

    def get_element_Z(self, symbol: str) -> int:
        return 0


# ---------------------------------------------------------------------------
# Fixtures
# ---------------------------------------------------------------------------


@pytest.fixture()
def db() -> MockDB:
    """Fresh MockDB instance (new hash each time to avoid cache crosstalk)."""
    # Clear the dict cache to avoid stale entries between tests
    _interpolator_cache.clear()
    return MockDB()


# ---------------------------------------------------------------------------
# Interpolator tests
# ---------------------------------------------------------------------------


class TestMakeInterpolator:
    """Tests for _make_interpolator."""

    def test_exact_data_point(self) -> None:
        """Interpolator returns exact value at a data point."""
        energies = np.array([1.0, 10.0, 100.0])
        dedx = np.array([50.0, 20.0, 10.0])
        interp = _make_interpolator(energies, dedx)
        assert interp(10.0) == pytest.approx(20.0, rel=1e-6)

    def test_interpolation_between_points(self) -> None:
        """Interpolated value lies between neighbors."""
        energies = np.array([1.0, 10.0, 100.0])
        dedx = np.array([50.0, 20.0, 10.0])
        interp = _make_interpolator(energies, dedx)
        val = interp(5.0)
        assert 20.0 < val < 50.0

    def test_monotonic_decrease(self) -> None:
        """Stopping power should decrease with energy for typical data."""
        energies = np.array([1.0, 2.0, 5.0, 10.0, 50.0, 100.0])
        dedx = np.array([50.0, 40.0, 25.0, 18.0, 9.0, 7.0])
        interp = _make_interpolator(energies, dedx)
        vals = [interp(e) for e in [1.0, 5.0, 10.0, 50.0, 100.0]]
        for i in range(len(vals) - 1):
            assert vals[i] > vals[i + 1]


# ---------------------------------------------------------------------------
# elemental_dedx tests
# ---------------------------------------------------------------------------


class TestElementalDedx:
    """Tests for elemental_dedx."""

    def test_proton_direct_lookup(self, db: MockDB) -> None:
        """Proton at 10 MeV should return PSTAR value at 10 MeV."""
        result = elemental_dedx(db, "p", 42, 10.0)
        assert result == pytest.approx(18.0, rel=1e-4)

    def test_deuteron_velocity_scaling(self, db: MockDB) -> None:
        """Deuteron at 20 MeV should look up PSTAR at 10 MeV (E/A = 20/2)."""
        d_val = elemental_dedx(db, "d", 42, 20.0)
        p_val = elemental_dedx(db, "p", 42, 10.0)
        assert d_val == pytest.approx(p_val, rel=1e-6)

    def test_triton_velocity_scaling(self, db: MockDB) -> None:
        """Triton at 30 MeV should look up PSTAR at 10 MeV (E/A = 30/3)."""
        t_val = elemental_dedx(db, "t", 42, 30.0)
        p_val = elemental_dedx(db, "p", 42, 10.0)
        assert t_val == pytest.approx(p_val, rel=1e-6)

    def test_he3_velocity_scaling(self, db: MockDB) -> None:
        """³He at 30 MeV should look up ASTAR at 40 MeV (E × 4/3)."""
        h_val = elemental_dedx(db, "h", 42, 30.0)
        # Build interpolator for ASTAR/Mo and check at 40 MeV
        energies, dedx = db.get_stopping_power("ASTAR", 42)
        interp = _make_interpolator(energies, dedx)
        expected = interp(40.0)
        assert h_val == pytest.approx(expected, rel=1e-6)

    def test_alpha_direct_lookup(self, db: MockDB) -> None:
        """Alpha at 10 MeV should return ASTAR value at 10 MeV."""
        result = elemental_dedx(db, "a", 42, 10.0)
        assert result == pytest.approx(70.0, rel=1e-4)

    def test_invalid_projectile(self, db: MockDB) -> None:
        """Unsupported projectile should raise ValueError."""
        with pytest.raises(ValueError, match="Cannot parse projectile"):
            elemental_dedx(db, "x", 42, 10.0)

    def test_boundary_energy_low(self, db: MockDB) -> None:
        """Lookup near minimum table energy still returns a value."""
        result = elemental_dedx(db, "p", 42, 1.0)
        assert result == pytest.approx(50.0, rel=1e-4)

    def test_boundary_energy_high(self, db: MockDB) -> None:
        """Lookup near maximum table energy still returns a value."""
        result = elemental_dedx(db, "p", 42, 100.0)
        assert result == pytest.approx(7.0, rel=1e-4)


# ---------------------------------------------------------------------------
# compound_dedx tests
# ---------------------------------------------------------------------------


class TestCompoundDedx:
    """Tests for compound_dedx (Bragg additivity)."""

    def test_single_element(self, db: MockDB) -> None:
        """Compound with one element at w=1.0 equals elemental value."""
        composition = [(42, 1.0)]
        result = compound_dedx(db, "p", composition, 10.0)
        expected = elemental_dedx(db, "p", 42, 10.0)
        assert result == pytest.approx(expected, rel=1e-10)

    def test_two_elements_bragg_additivity(self, db: MockDB) -> None:
        """Bragg additivity: S_compound = w1*S1 + w2*S2."""
        w_Mo = 0.6
        w_C = 0.4
        composition = [(42, w_Mo), (6, w_C)]
        result = compound_dedx(db, "p", composition, 10.0)

        s_Mo = elemental_dedx(db, "p", 42, 10.0)
        s_C = elemental_dedx(db, "p", 6, 10.0)
        expected = w_Mo * s_Mo + w_C * s_C
        assert result == pytest.approx(expected, rel=1e-10)

    def test_zero_weight_element_ignored(self, db: MockDB) -> None:
        """Element with zero mass fraction contributes nothing."""
        composition = [(42, 1.0), (6, 0.0)]
        result = compound_dedx(db, "p", composition, 10.0)
        expected = elemental_dedx(db, "p", 42, 10.0)
        assert result == pytest.approx(expected, rel=1e-10)


# ---------------------------------------------------------------------------
# dedx_MeV_per_cm tests
# ---------------------------------------------------------------------------


class TestDedxMeVPerCm:
    """Tests for dedx_MeV_per_cm (linear stopping power)."""

    def test_linear_is_mass_times_density(self, db: MockDB) -> None:
        """dE/dx [MeV/cm] = S [MeV·cm²/g] × ρ [g/cm³]."""
        density = 10.28  # Mo density
        composition = [(42, 1.0)]
        energy = 10.0

        linear = dedx_MeV_per_cm(db, "p", composition, density, energy)
        mass_sp = compound_dedx(db, "p", composition, energy)
        assert linear == pytest.approx(mass_sp * density, rel=1e-10)

    def test_double_density_doubles_linear_sp(self, db: MockDB) -> None:
        """Doubling density should double linear stopping power."""
        composition = [(42, 1.0)]
        s1 = dedx_MeV_per_cm(db, "p", composition, 5.0, 10.0)
        s2 = dedx_MeV_per_cm(db, "p", composition, 10.0, 10.0)
        assert s2 == pytest.approx(2.0 * s1, rel=1e-10)


# ---------------------------------------------------------------------------
# compute_thickness_from_energy tests
# ---------------------------------------------------------------------------


class TestComputeThicknessFromEnergy:
    """Tests for compute_thickness_from_energy."""

    def test_constant_dedx_thickness(self, db: MockDB) -> None:
        """With nearly constant dE/dx, thickness ≈ ΔE / (dE/dx).

        Use a narrow energy window around 10 MeV where dE/dx is roughly
        constant so the numerical integration should be close to the
        analytical result.
        """
        density = 10.28
        composition = [(42, 1.0)]
        e_in = 10.5
        e_out = 9.5
        delta_E = e_in - e_out  # 1.0 MeV

        # dE/dx at midpoint
        dedx_mid = dedx_MeV_per_cm(db, "p", composition, density, 10.0)
        expected_thickness = delta_E / dedx_mid

        result = compute_thickness_from_energy(
            db, "p", composition, density, e_in, e_out, n_points=2000
        )
        assert result == pytest.approx(expected_thickness, rel=1e-3)

    def test_thickness_positive(self, db: MockDB) -> None:
        """Thickness should always be positive when E_in > E_out."""
        result = compute_thickness_from_energy(db, "p", [(42, 1.0)], 10.28, 20.0, 10.0)
        assert result > 0.0

    def test_zero_energy_loss_gives_zero_thickness(self, db: MockDB) -> None:
        """When E_in == E_out, thickness should be ~0."""
        result = compute_thickness_from_energy(
            db, "p", [(42, 1.0)], 10.28, 10.0, 10.0, n_points=100
        )
        assert result == pytest.approx(0.0, abs=1e-10)


# ---------------------------------------------------------------------------
# Caching tests
# ---------------------------------------------------------------------------


class TestCaching:
    """Tests for interpolator caching."""

    def test_cache_returns_same_object(self, db: MockDB) -> None:
        """Repeated calls with same args return the same interpolator."""
        interp1, src1 = _get_interpolator(db, "PSTAR", 42)
        interp2, src2 = _get_interpolator(db, "PSTAR", 42)
        assert interp1 is interp2
        assert src1 == src2

    def test_different_args_different_objects(self, db: MockDB) -> None:
        """Different source/Z combinations get separate interpolators."""
        interp_pstar, _ = _get_interpolator(db, "PSTAR", 42)
        interp_astar, _ = _get_interpolator(db, "ASTAR", 42)
        assert interp_pstar is not interp_astar

    def test_source_label_from_db(self, db: MockDB) -> None:
        """When data exists in DB, source label matches the request."""
        _, source = _get_interpolator(db, "PSTAR", 42)
        assert source == "PSTAR"
