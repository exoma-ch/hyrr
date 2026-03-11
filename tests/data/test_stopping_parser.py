"""Tests for the stopping power parser."""

from __future__ import annotations

import sqlite3
import sys
from pathlib import Path
from textwrap import dedent

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "data"))

from parsers.stopping import insert_stopping_power, parse_stopping_file


@pytest.fixture()
def pstar_file(tmp_path: Path) -> Path:
    """Create a minimal PSTAR stopping power file."""
    content = dedent("""\
        #Z:1:NPTS:3
        1.000E-03  2.032E+02
        1.500E-03  2.444E+02
        2.000E-03  2.789E+02
        #Z:2:NPTS:2
        1.000E-03  1.756E+02
        1.500E-03  2.112E+02
    """)
    p = tmp_path / "pstar_data.txt"
    p.write_text(content)
    return p


@pytest.fixture()
def empty_stopping_file(tmp_path: Path) -> Path:
    """Create an empty stopping power file."""
    p = tmp_path / "empty.txt"
    p.write_text("")
    return p


class TestParseStoppingFile:
    """Tests for parse_stopping_file()."""

    def test_basic_parsing(self, pstar_file: Path) -> None:
        entries = parse_stopping_file(pstar_file, "PSTAR")
        assert len(entries) == 5

    def test_source_tag(self, pstar_file: Path) -> None:
        entries = parse_stopping_file(pstar_file, "PSTAR")
        assert all(e.source == "PSTAR" for e in entries)

    def test_z_values(self, pstar_file: Path) -> None:
        entries = parse_stopping_file(pstar_file, "PSTAR")
        z1_entries = [e for e in entries if e.target_Z == 1]
        z2_entries = [e for e in entries if e.target_Z == 2]
        assert len(z1_entries) == 3
        assert len(z2_entries) == 2

    def test_energy_values(self, pstar_file: Path) -> None:
        entries = parse_stopping_file(pstar_file, "PSTAR")
        z1_entries = [e for e in entries if e.target_Z == 1]
        assert z1_entries[0].energy_MeV == pytest.approx(1.0e-3)
        assert z1_entries[0].dedx == pytest.approx(2.032e2)

    def test_empty_file(self, empty_stopping_file: Path) -> None:
        entries = parse_stopping_file(empty_stopping_file, "PSTAR")
        assert entries == []

    def test_nonexistent_file(self, tmp_path: Path) -> None:
        entries = parse_stopping_file(tmp_path / "no_such_file.txt", "ASTAR")
        assert entries == []

    def test_astar_source(self, pstar_file: Path) -> None:
        entries = parse_stopping_file(pstar_file, "ASTAR")
        assert all(e.source == "ASTAR" for e in entries)


class TestInsertStoppingPower:
    """Tests for insert_stopping_power()."""

    def test_insert_and_count(self, pstar_file: Path) -> None:
        entries = parse_stopping_file(pstar_file, "PSTAR")
        conn = sqlite3.connect(":memory:")
        conn.executescript(
            """
            CREATE TABLE stopping_power (
                source TEXT NOT NULL,
                target_Z INTEGER NOT NULL,
                energy_MeV REAL NOT NULL,
                dedx REAL NOT NULL,
                PRIMARY KEY (source, target_Z, energy_MeV)
            );
            """
        )
        count = insert_stopping_power(conn, entries)
        assert count == 5

        rows = conn.execute(
            "SELECT * FROM stopping_power WHERE target_Z = 1 ORDER BY energy_MeV"
        ).fetchall()
        assert len(rows) == 3
        conn.close()

    def test_insert_empty(self) -> None:
        conn = sqlite3.connect(":memory:")
        conn.executescript(
            """
            CREATE TABLE stopping_power (
                source TEXT NOT NULL,
                target_Z INTEGER NOT NULL,
                energy_MeV REAL NOT NULL,
                dedx REAL NOT NULL,
                PRIMARY KEY (source, target_Z, energy_MeV)
            );
            """
        )
        count = insert_stopping_power(conn, [])
        assert count == 0
        conn.close()
