"""Tests for data/parsers/talys.py module."""

from __future__ import annotations

import sys
from pathlib import Path

import polars as pl
import pytest

# Add data/ to path so we can import parsers.talys
sys.path.insert(0, str(Path(__file__).parent.parent / "data"))

from parsers.talys import (
    _parse_rp_file,
    generate_talys_input,
    parse_talys_residual,
    write_xs_parquet,
)
from parsers.tendl import CrossSectionEntry


class TestParseRpFile:
    """Tests for _parse_rp_file."""

    def test_basic_file(self, tmp_path: Path) -> None:
        rp_file = tmp_path / "rp043099.tot"
        rp_file.write_text(
            "# header line\n"
            "# another header\n"
            "  5.000  0.000E+00\n"
            "  6.000  1.234E+01\n"
            " 10.000  5.678E+02\n"
        )
        energies, xs = _parse_rp_file(rp_file)
        assert len(energies) == 3
        assert energies[0] == pytest.approx(5.0)
        assert energies[1] == pytest.approx(6.0)
        assert xs[1] == pytest.approx(12.34)
        assert xs[2] == pytest.approx(567.8)

    def test_empty_file(self, tmp_path: Path) -> None:
        rp_file = tmp_path / "rp001001.tot"
        rp_file.write_text("")
        energies, xs = _parse_rp_file(rp_file)
        assert energies == []
        assert xs == []

    def test_comments_only(self, tmp_path: Path) -> None:
        rp_file = tmp_path / "rp001001.tot"
        rp_file.write_text("# only comments\n# no data\n")
        energies, xs = _parse_rp_file(rp_file)
        assert energies == []
        assert xs == []


class TestParseTalysResidual:
    """Tests for parse_talys_residual."""

    def test_multiple_residuals(self, tmp_path: Path) -> None:
        # Create two rp files
        (tmp_path / "rp043099.tot").write_text(
            "  8.0  0.0\n 10.0  50.0\n 12.0  150.0\n"
        )
        (tmp_path / "rp042098.tot").write_text(
            "  8.0  10.0\n 10.0  30.0\n"
        )

        entries = parse_talys_residual(tmp_path, "C-12", 42, 100)
        assert len(entries) == 2

        # Sorted by filename, so rp042098 comes before rp043099
        assert entries[0].residual_Z == 42
        assert entries[0].residual_A == 98
        assert entries[0].state == ""
        assert entries[0].projectile == "C-12"
        assert entries[0].target_Z == 42
        assert entries[0].target_A == 100
        assert entries[0].source == "talys"

        assert entries[1].residual_Z == 43
        assert entries[1].residual_A == 99

    def test_metastable_state(self, tmp_path: Path) -> None:
        (tmp_path / "rp043099.tot").write_text("  10.0  100.0\n")
        (tmp_path / "rp043099.L01").write_text("  10.0  40.0\n")

        entries = parse_talys_residual(tmp_path, "p", 42, 100)
        assert len(entries) == 2

        states = {e.state for e in entries}
        assert states == {"", "m"}

    def test_nonexistent_dir(self, tmp_path: Path) -> None:
        entries = parse_talys_residual(tmp_path / "nonexistent", "p", 42, 100)
        assert entries == []

    def test_ignores_non_rp_files(self, tmp_path: Path) -> None:
        (tmp_path / "output.log").write_text("some log data")
        (tmp_path / "total.tot").write_text("10.0 50.0")
        entries = parse_talys_residual(tmp_path, "p", 42, 100)
        assert entries == []


class TestWriteXsParquet:
    """Tests for write_xs_parquet."""

    def test_write_and_read(self, tmp_path: Path) -> None:
        entries = [
            CrossSectionEntry(
                projectile="C-12",
                target_Z=42,
                target_A=100,
                residual_Z=43,
                residual_A=99,
                state="m",
                energies_MeV=[10.0, 15.0, 20.0],
                xs_mb=[50.0, 150.0, 100.0],
                source="talys",
            ),
        ]
        output_path = tmp_path / "xs" / "C-12_Mo.parquet"
        write_xs_parquet(entries, output_path)

        assert output_path.exists()
        df = pl.read_parquet(output_path)
        assert len(df) == 3
        assert set(df.columns) == {
            "target_A", "residual_Z", "residual_A", "state", "energy_MeV", "xs_mb"
        }
        assert df["target_A"].dtype == pl.Int32
        assert df["residual_Z"].dtype == pl.Int32
        assert df["residual_A"].dtype == pl.Int32

    def test_empty_entries(self, tmp_path: Path) -> None:
        output_path = tmp_path / "empty.parquet"
        write_xs_parquet([], output_path)
        assert not output_path.exists()


class TestGenerateTalysInput:
    """Tests for generate_talys_input."""

    def test_creates_input_file(self, tmp_path: Path) -> None:
        work_dir = generate_talys_input(
            projectile_symbol="C",
            target_Z=42,
            target_A=100,
            target_symbol="Mo",
            energy_range_MeV=(5.0, 10.0),
            energy_step_MeV=1.0,
            output_dir=tmp_path,
        )

        input_file = work_dir / "input"
        assert input_file.exists()

        content = input_file.read_text()
        assert "projectile c" in content
        assert "element mo" in content
        assert "mass 100" in content
        assert "energy energies.dat" in content
        assert "rpevap y" in content

    def test_creates_energy_file(self, tmp_path: Path) -> None:
        work_dir = generate_talys_input(
            projectile_symbol="C",
            target_Z=42,
            target_A=100,
            target_symbol="Mo",
            energy_range_MeV=(5.0, 7.0),
            energy_step_MeV=1.0,
            output_dir=tmp_path,
        )

        energy_file = work_dir / "energies.dat"
        assert energy_file.exists()
        lines = energy_file.read_text().strip().split("\n")
        assert len(lines) == 3  # 5.0, 6.0, 7.0
        assert lines[0] == "5.000"
        assert lines[-1] == "7.000"

    def test_work_dir_naming(self, tmp_path: Path) -> None:
        work_dir = generate_talys_input(
            projectile_symbol="O",
            target_Z=29,
            target_A=63,
            target_symbol="Cu",
            energy_range_MeV=(5.0, 10.0),
            output_dir=tmp_path,
        )
        assert work_dir.name == "O_Cu63"
