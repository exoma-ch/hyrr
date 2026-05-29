"""Tests for CurrentProfile surface parsers (from_csv, from_parquet, from_values)."""

from __future__ import annotations

import numpy as np
import pytest

from hyrr.models import CurrentProfile

# ---------------------------------------------------------------------------
# from_csv
# ---------------------------------------------------------------------------


class TestFromCSV:
    def test_comma_delimited_with_header(self, tmp_path):
        csv = tmp_path / "profile.csv"
        csv.write_text("time_s,current_mA\n0,0.10\n5,0.15\n10,0.12\n")
        p = CurrentProfile.from_csv(csv)
        assert len(p.times_s) == 3
        assert p.times_s[0] == 0.0
        assert p.currents_mA[1] == pytest.approx(0.15)

    def test_tab_delimited(self, tmp_path):
        csv = tmp_path / "profile.tsv"
        csv.write_text("time_s\tcurrent_mA\n0\t0.10\n5\t0.15\n")
        p = CurrentProfile.from_csv(csv)
        assert len(p.times_s) == 2
        assert p.currents_mA[0] == pytest.approx(0.10)

    def test_no_header(self, tmp_path):
        csv = tmp_path / "profile.csv"
        csv.write_text("0,0.10\n5,0.15\n10,0.12\n")
        p = CurrentProfile.from_csv(csv)
        assert len(p.times_s) == 3

    def test_negative_current_raises(self, tmp_path):
        csv = tmp_path / "profile.csv"
        csv.write_text("time_s,current_mA\n0,0.10\n5,-0.05\n")
        with pytest.raises(ValueError, match="non-negative"):
            CurrentProfile.from_csv(csv)

    def test_non_monotonic_raises(self, tmp_path):
        csv = tmp_path / "profile.csv"
        csv.write_text("time_s,current_mA\n0,0.10\n5,0.15\n3,0.12\n")
        with pytest.raises(ValueError, match="monotonically"):
            CurrentProfile.from_csv(csv)

    def test_empty_file_raises(self, tmp_path):
        csv = tmp_path / "profile.csv"
        csv.write_text("time_s,current_mA\n")
        with pytest.raises(ValueError, match="at least one"):
            CurrentProfile.from_csv(csv)

    def test_skips_blank_lines(self, tmp_path):
        csv = tmp_path / "profile.csv"
        csv.write_text("time_s,current_mA\n0,0.10\n\n5,0.15\n\n")
        p = CurrentProfile.from_csv(csv)
        assert len(p.times_s) == 2


# ---------------------------------------------------------------------------
# from_parquet
# ---------------------------------------------------------------------------


class TestFromParquet:
    def test_round_trip(self, tmp_path):
        polars = pytest.importorskip("polars")
        pq = tmp_path / "profile.parquet"
        df = polars.DataFrame(
            {"time_s": [0.0, 5.0, 10.0], "current_mA": [0.10, 0.15, 0.12]}
        )
        df.write_parquet(pq)

        p = CurrentProfile.from_parquet(pq)
        assert len(p.times_s) == 3
        assert p.currents_mA[2] == pytest.approx(0.12)

    def test_validation_fires(self, tmp_path):
        polars = pytest.importorskip("polars")
        pq = tmp_path / "bad.parquet"
        df = polars.DataFrame(
            {"time_s": [0.0, 5.0, 3.0], "current_mA": [0.10, 0.15, 0.12]}
        )
        df.write_parquet(pq)

        with pytest.raises(ValueError, match="monotonically"):
            CurrentProfile.from_parquet(pq)


# ---------------------------------------------------------------------------
# from_values
# ---------------------------------------------------------------------------


class TestFromValues:
    def test_uniform_spacing(self):
        p = CurrentProfile.from_values([0.10, 0.15, 0.12], dt=5.0)
        np.testing.assert_array_almost_equal(p.times_s, [0.0, 5.0, 10.0])
        assert p.currents_mA[1] == pytest.approx(0.15)

    def test_single_value(self):
        p = CurrentProfile.from_values([0.03], dt=1.0)
        assert len(p.times_s) == 1
        assert p.times_s[0] == 0.0

    def test_negative_current_raises(self):
        with pytest.raises(ValueError, match="non-negative"):
            CurrentProfile.from_values([0.10, -0.05], dt=5.0)
