"""Tests for hyrr.db — DataStore backed by nucl-parquet DuckDB client (#257).

These tests verify the adapter layer between hyrr's DatabaseProtocol and
the nucl-parquet Python client. They create a minimal nucl-parquet data
directory with test parquet files and exercise every Protocol method.

Catching regressions on nucl-parquet upgrades is a primary goal — if the
upstream client changes column names, view schemas, or loader behavior,
these tests break before physics code does.
"""

from __future__ import annotations

import json

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
# Fixtures — create a minimal nucl-parquet data directory
# ---------------------------------------------------------------------------


@pytest.fixture()
def data_dir(tmp_path):
    """Create a temporary nucl-parquet data directory with test data."""
    meta_dir = tmp_path / "meta"
    meta_dir.mkdir()
    stopping_dir = tmp_path / "stopping"
    stopping_dir.mkdir()
    xs_dir = tmp_path / "test-lib" / "xs"
    xs_dir.mkdir(parents=True)

    # -- catalog.json (SSoT for file discovery)
    catalog = {
        "version": 2,
        "data_version": "test",
        "libraries": {
            "test-lib": {
                "name": "Test Library",
                "path": "test-lib/xs/",
                "projectiles": ["p"],
                "data_type": "cross_sections",
            }
        },
        "shared": {
            "meta": {"path": "meta/", "files": {}},
            "stopping": {"path": "stopping/", "sources": ["PSTAR"]},
        },
    }
    (tmp_path / "catalog.json").write_text(json.dumps(catalog))

    # -- elements
    pl.DataFrame(
        {"Z": [20, 21, 42, 43], "symbol": ["Ca", "Sc", "Mo", "Tc"]}
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

    # -- decay: Tc-99m + Sc-44 ground
    pl.DataFrame(
        {
            "Z": [43, 43, 21],
            "A": [99, 99, 44],
            "state": ["m", "m", ""],
            "half_life_s": [21624.0, 21624.0, 14551.0],
            "decay_mode": ["IT", "beta-", "beta+"],
            "daughter_Z": [43, 44, 20],
            "daughter_A": [99, 99, 44],
            "daughter_state": ["", "", ""],
            "branching": [0.885, 0.115, 1.0],
        }
    ).cast(
        {"Z": pl.Int32, "A": pl.Int32, "daughter_Z": pl.Int32, "daughter_A": pl.Int32}
    ).write_parquet(meta_dir / "decay.parquet")

    # -- dose_constants
    pl.DataFrame(
        {
            "Z": [43, 21],
            "A": [99, 44],
            "state": ["m", ""],
            "k_uSv_m2_MBq_h": [0.0141, 0.153],
            "source": ["ensdf", "ensdf"],
        }
    ).cast({"Z": pl.Int32, "A": pl.Int32}).write_parquet(
        meta_dir / "dose_constants.parquet"
    )

    # -- stopping power
    pl.DataFrame(
        {
            "source": ["PSTAR", "PSTAR", "PSTAR"],
            "target_Z": [1, 1, 1],
            "energy_MeV": [0.1, 1.0, 10.0],
            "dedx": [500.0, 200.0, 50.0],
        }
    ).cast({"target_Z": pl.Int32}).write_parquet(stopping_dir / "PSTAR.parquet")

    # -- cross-sections: p + Mo (with state-resolved + total)
    pl.DataFrame(
        {
            "target_A": [100, 100, 100, 100, 100],
            "residual_Z": [43, 43, 43, 43, 43],
            "residual_A": [99, 99, 99, 99, 99],
            "state": ["m", "m", "m", "", ""],
            "energy_MeV": [5.0, 10.0, 15.0, 5.0, 10.0],
            "xs_mb": [10.0, 50.0, 30.0, 5.0, 25.0],
        }
    ).cast(
        {"target_A": pl.Int32, "residual_Z": pl.Int32, "residual_A": pl.Int32}
    ).write_parquet(xs_dir / "p_Mo.parquet")

    # -- cross-sections: p + Ca (Sc-44 with g/m/total split)
    pl.DataFrame(
        {
            "target_A": [44, 44, 44, 44, 44, 44],
            "residual_Z": [21, 21, 21, 21, 21, 21],
            "residual_A": [44, 44, 44, 44, 44, 44],
            "state": ["", "", "g", "g", "m", "m"],
            "energy_MeV": [5.0, 10.0, 5.0, 10.0, 5.0, 10.0],
            "xs_mb": [600.0, 611.0, 560.0, 576.0, 20.0, 34.0],
        }
    ).cast(
        {"target_A": pl.Int32, "residual_Z": pl.Int32, "residual_A": pl.Int32}
    ).write_parquet(xs_dir / "p_Ca.parquet")

    return tmp_path


@pytest.fixture()
def db(data_dir) -> DataStore:
    """Create a DataStore from temp data directory."""
    return DataStore(data_dir, library="test-lib")


# ---------------------------------------------------------------------------
# Protocol conformance
# ---------------------------------------------------------------------------


class TestProtocol:
    """Verify DataStore satisfies DatabaseProtocol."""

    def test_isinstance(self, db: DataStore) -> None:
        assert isinstance(db, DatabaseProtocol)


# ---------------------------------------------------------------------------
# Cross-sections — dedup + state resolution
# ---------------------------------------------------------------------------


class TestGetCrossSections:

    def test_prefers_state_resolved_over_total(self, db: DataStore) -> None:
        """When both total and resolved exist, only resolved is returned (#254)."""
        results = db.get_cross_sections("p", 42, 100)
        assert len(results) == 1
        assert results[0].state == "m"
        assert all(isinstance(r, CrossSectionData) for r in results)

    def test_array_shapes_and_values(self, db: DataStore) -> None:
        results = db.get_cross_sections("p", 42, 100)
        meta = results[0]
        assert meta.residual_Z == 43
        assert meta.residual_A == 99
        assert meta.energies_MeV.shape == (3,)
        np.testing.assert_array_almost_equal(meta.energies_MeV, [5.0, 10.0, 15.0])
        np.testing.assert_array_almost_equal(meta.xs_mb, [10.0, 50.0, 30.0])

    def test_sc44_g_m_split(self, db: DataStore) -> None:
        """Ca-44(p,n)Sc-44: total dropped, g+m kept (#252)."""
        results = db.get_cross_sections("p", 20, 44)
        states = {r.state for r in results}
        assert states == {"g", "m"}
        assert len(results) == 2

    def test_empty_result(self, db: DataStore) -> None:
        results = db.get_cross_sections("d", 1, 1)
        assert results == []


# ---------------------------------------------------------------------------
# Stopping power
# ---------------------------------------------------------------------------


class TestGetStoppingPower:

    def test_sorted_arrays(self, db: DataStore) -> None:
        energies, dedx = db.get_stopping_power("PSTAR", 1)
        assert len(energies) == 3
        np.testing.assert_array_almost_equal(energies, [0.1, 1.0, 10.0])
        np.testing.assert_array_almost_equal(dedx, [500.0, 200.0, 50.0])

    def test_caching(self, db: DataStore) -> None:
        result1 = db.get_stopping_power("PSTAR", 1)
        result2 = db.get_stopping_power("PSTAR", 1)
        assert result1 is result2


# ---------------------------------------------------------------------------
# Abundances
# ---------------------------------------------------------------------------


class TestGetNaturalAbundances:

    def test_returns_correct_dict(self, db: DataStore) -> None:
        abund = db.get_natural_abundances(42)
        assert set(abund.keys()) == {92, 94, 100}
        a, m = abund[100]
        assert a == pytest.approx(0.0974)
        assert m == pytest.approx(99.907477)

    def test_empty_for_unknown(self, db: DataStore) -> None:
        abund = db.get_natural_abundances(999)
        assert abund == {}


# ---------------------------------------------------------------------------
# Decay data — state normalization
# ---------------------------------------------------------------------------


class TestGetDecayData:

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

    def test_normalizes_g_to_empty(self, db: DataStore) -> None:
        """state='g' from xs data should find ground state stored as '' (#254)."""
        dd = db.get_decay_data(21, 44, "g")
        assert dd is not None
        assert dd.half_life_s == pytest.approx(14551.0)
        # Direct "" also works
        assert db.get_decay_data(21, 44, "").half_life_s == pytest.approx(14551.0)
        # "m" should not match ground
        assert db.get_decay_data(21, 44, "m") is None


# ---------------------------------------------------------------------------
# Dose constants (#257 — new Python capability)
# ---------------------------------------------------------------------------


class TestGetDoseConstant:

    def test_known_nuclide(self, db: DataStore) -> None:
        result = db.get_dose_constant(43, 99, "m")
        assert result is not None
        k, source = result
        assert k == pytest.approx(0.0141)
        assert source == "ensdf"

    def test_normalizes_g_to_empty(self, db: DataStore) -> None:
        """state='g' should find ground state stored as '' (#254)."""
        result = db.get_dose_constant(21, 44, "g")
        assert result is not None
        assert result[0] == pytest.approx(0.153)

    def test_unknown_returns_none(self, db: DataStore) -> None:
        assert db.get_dose_constant(999, 999) is None


# ---------------------------------------------------------------------------
# Element lookups
# ---------------------------------------------------------------------------


class TestGetElement:

    def test_from_data(self, db: DataStore) -> None:
        assert db.get_element_symbol(42) == "Mo"
        assert db.get_element_Z("Tc") == 43

    def test_fallback_dict(self, db: DataStore) -> None:
        assert db.get_element_symbol(1) == "H"
        assert db.get_element_Z("H") == 1

    def test_unknown_raises(self, db: DataStore) -> None:
        with pytest.raises(KeyError):
            db.get_element_symbol(999)
        with pytest.raises(KeyError):
            db.get_element_Z("Xx")


# ---------------------------------------------------------------------------
# Context manager + DuckDB exposure
# ---------------------------------------------------------------------------


class TestContextManager:

    def test_enter_exit(self, data_dir) -> None:
        with DataStore(data_dir, library="test-lib") as d:
            assert d.get_element_symbol(42) == "Mo"

    def test_nonexistent_dir_raises(self, tmp_path) -> None:
        with pytest.raises(FileNotFoundError):
            DataStore(tmp_path / "nonexistent")


class TestDuckDbExposed:
    """Verify the DuckDB connection is accessible for advanced queries."""

    def test_db_property(self, db: DataStore) -> None:
        result = db.db.execute("SELECT COUNT(*) FROM abundances").fetchone()
        assert result is not None
        assert result[0] == 3

    def test_dose_constants_view_exists(self, db: DataStore) -> None:
        """dose_constants view should be queryable via the exposed db."""
        result = db.db.execute(
            "SELECT k_uSv_m2_MBq_h FROM dose_constants WHERE Z=43 AND A=99 AND state='m'"
        ).fetchone()
        assert result is not None
        assert result[0] == pytest.approx(0.0141)
