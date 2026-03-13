"""Tests for hyrr.db — DataStore Parquet/Polars implementation."""

from __future__ import annotations

import numpy as np
import polars as pl
import pytest

from hyrr.db import (
    CrossSectionData,
    DatabaseProtocol,
    DataStore,
    DecayData,
)

# ---------------------------------------------------------------------------
# Fixtures
# ---------------------------------------------------------------------------


@pytest.fixture()
def data_dir(tmp_path):
    """Create a temporary parquet data directory with test data."""
    meta_dir = tmp_path / "meta"
    meta_dir.mkdir()
    stopping_dir = tmp_path / "stopping"
    stopping_dir.mkdir()
    xs_dir = tmp_path / "test-lib" / "xs"
    xs_dir.mkdir(parents=True)

    # -- elements
    pl.DataFrame(
        {
            "Z": [42, 43],
            "symbol": ["Mo", "Tc"],
        }
    ).cast({"Z": pl.Int32}).write_parquet(meta_dir / "elements.parquet")

    # -- abundances: Mo
    pl.DataFrame(
        {
            "Z": [42, 42, 42],
            "A": [92, 94, 100],
            "symbol": ["Mo", "Mo", "Mo"],
            "abundance": [0.1453, 0.0915, 0.0974],
            "atomic_mass": [91.906810, 93.905085, 99.907477],
        }
    ).cast({"Z": pl.Int32, "A": pl.Int32}).write_parquet(
        meta_dir / "abundances.parquet"
    )

    # -- decay: Tc-99m
    pl.DataFrame(
        {
            "Z": [43, 43],
            "A": [99, 99],
            "state": ["m", "m"],
            "half_life_s": [21624.0, 21624.0],
            "decay_mode": ["IT", "beta-"],
            "daughter_Z": [43, 44],
            "daughter_A": [99, 99],
            "daughter_state": ["", ""],
            "branching": [0.885, 0.115],
        }
    ).cast(
        {
            "Z": pl.Int32,
            "A": pl.Int32,
            "daughter_Z": pl.Int32,
            "daughter_A": pl.Int32,
        }
    ).write_parquet(meta_dir / "decay.parquet")

    # -- stopping power: PSTAR for Z=1
    pl.DataFrame(
        {
            "source": ["PSTAR", "PSTAR", "PSTAR"],
            "target_Z": [1, 1, 1],
            "energy_MeV": [0.1, 1.0, 10.0],
            "dedx": [500.0, 200.0, 50.0],
        }
    ).cast({"target_Z": pl.Int32}).write_parquet(stopping_dir / "stopping.parquet")

    # -- cross-sections: p + Mo
    xs_data = pl.DataFrame(
        {
            "target_A": [100, 100, 100, 100, 100],
            "residual_Z": [43, 43, 43, 43, 43],
            "residual_A": [99, 99, 99, 99, 99],
            "state": ["m", "m", "m", "", ""],
            "energy_MeV": [5.0, 10.0, 15.0, 5.0, 10.0],
            "xs_mb": [10.0, 50.0, 30.0, 5.0, 25.0],
        }
    ).cast(
        {
            "target_A": pl.Int32,
            "residual_Z": pl.Int32,
            "residual_A": pl.Int32,
        }
    )
    xs_data.write_parquet(xs_dir / "p_Mo.parquet")

    return tmp_path


@pytest.fixture()
def db(data_dir) -> DataStore:
    """Create a DataStore from temp parquet directory."""
    return DataStore(data_dir, library="test-lib")


# ---------------------------------------------------------------------------
# Tests
# ---------------------------------------------------------------------------


class TestProtocol:
    """Verify DataStore satisfies DatabaseProtocol."""

    def test_isinstance(self, db: DataStore) -> None:
        assert isinstance(db, DatabaseProtocol)


class TestGetCrossSections:
    """Tests for get_cross_sections."""

    def test_groups_by_residual(self, db: DataStore) -> None:
        results = db.get_cross_sections("p", 42, 100)
        assert len(results) == 2
        assert all(isinstance(r, CrossSectionData) for r in results)

    def test_array_shapes_and_values(self, db: DataStore) -> None:
        results = db.get_cross_sections("p", 42, 100)
        by_state = {r.state: r for r in results}

        meta = by_state["m"]
        assert meta.residual_Z == 43
        assert meta.residual_A == 99
        assert meta.energies_MeV.shape == (3,)
        assert meta.xs_mb.shape == (3,)
        np.testing.assert_array_almost_equal(meta.energies_MeV, [5.0, 10.0, 15.0])
        np.testing.assert_array_almost_equal(meta.xs_mb, [10.0, 50.0, 30.0])

        ground = by_state[""]
        assert ground.energies_MeV.shape == (2,)

    def test_empty_result(self, db: DataStore) -> None:
        results = db.get_cross_sections("d", 1, 1)
        assert results == []


class TestGetStoppingPower:
    """Tests for get_stopping_power."""

    def test_sorted_arrays(self, db: DataStore) -> None:
        energies, dedx = db.get_stopping_power("PSTAR", 1)
        assert len(energies) == 3
        np.testing.assert_array_almost_equal(energies, [0.1, 1.0, 10.0])
        np.testing.assert_array_almost_equal(dedx, [500.0, 200.0, 50.0])

    def test_caching_returns_same_object(self, db: DataStore) -> None:
        result1 = db.get_stopping_power("PSTAR", 1)
        result2 = db.get_stopping_power("PSTAR", 1)
        assert result1 is result2


class TestGetNaturalAbundances:
    """Tests for get_natural_abundances."""

    def test_returns_correct_dict(self, db: DataStore) -> None:
        abund = db.get_natural_abundances(42)
        assert set(abund.keys()) == {92, 94, 100}
        a, m = abund[100]
        assert a == pytest.approx(0.0974)
        assert m == pytest.approx(99.907477)

    def test_empty_for_unknown(self, db: DataStore) -> None:
        abund = db.get_natural_abundances(999)
        assert abund == {}


class TestGetDecayData:
    """Tests for get_decay_data."""

    def test_known_nuclide(self, db: DataStore) -> None:
        dd = db.get_decay_data(43, 99, "m")
        assert dd is not None
        assert isinstance(dd, DecayData)
        assert dd.half_life_s == pytest.approx(21624.0)
        assert len(dd.decay_modes) == 2
        modes = {m.mode for m in dd.decay_modes}
        assert modes == {"IT", "beta-"}

    def test_unknown_returns_none(self, db: DataStore) -> None:
        assert db.get_decay_data(999, 999) is None


class TestGetElement:
    """Tests for get_element_symbol and get_element_Z."""

    def test_from_data(self, db: DataStore) -> None:
        assert db.get_element_symbol(42) == "Mo"
        assert db.get_element_Z("Tc") == 43

    def test_fallback_dict(self, db: DataStore) -> None:
        # Z=1 not in elements table, but in fallback
        assert db.get_element_symbol(1) == "H"
        assert db.get_element_Z("H") == 1

    def test_unknown_raises(self, db: DataStore) -> None:
        with pytest.raises(KeyError):
            db.get_element_symbol(999)
        with pytest.raises(KeyError):
            db.get_element_Z("Xx")


class TestContextManager:
    """Tests for context manager lifecycle."""

    def test_enter_exit(self, data_dir) -> None:
        store = DataStore(data_dir, library="test-lib")
        with store as d:
            assert d is store

    def test_nonexistent_dir_raises(self, tmp_path) -> None:
        with pytest.raises(FileNotFoundError):
            DataStore(tmp_path / "nonexistent")
