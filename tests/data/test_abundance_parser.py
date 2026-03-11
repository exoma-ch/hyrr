"""Tests for the natural abundance parser."""

from __future__ import annotations

import sqlite3

# The parsers live under data/, not under src/hyrr/.
# We add data/ to sys.path so we can import them.
import sys
from pathlib import Path
from textwrap import dedent

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "data"))

from parsers.abundance import insert_abundances, parse_abundance_file


@pytest.fixture()
def li_abundance_file(tmp_path: Path) -> Path:
    """Create a mock Li abundance file (z003)."""
    content = dedent("""\
        3   6   7.590000  0.040000                                                6Li
        3   7  92.410000  0.040000                                                7Li
    """)
    p = tmp_path / "z003"
    p.write_text(content)
    return p


@pytest.fixture()
def empty_abundance_file(tmp_path: Path) -> Path:
    """Create an empty abundance file (e.g., Tc with no stable isotopes)."""
    p = tmp_path / "z043"
    p.write_text("")
    return p


@pytest.fixture()
def mo_abundance_file(tmp_path: Path) -> Path:
    """Create a mock Mo abundance file with multiple isotopes."""
    content = dedent("""\
          42  92  14.840000  0.350000                                               92Mo
          42  94   9.250000  0.120000                                               94Mo
          42  95  15.920000  0.130000                                               95Mo
          42  96  16.680000  0.020000                                               96Mo
          42  97   9.550000  0.080000                                               97Mo
          42  98  24.130000  0.310000                                               98Mo
          42 100   9.630000  0.230000                                              100Mo
    """)
    p = tmp_path / "z042"
    p.write_text(content)
    return p


class TestParseAbundanceFile:
    """Tests for parse_abundance_file()."""

    def test_li_isotopes(self, li_abundance_file: Path) -> None:
        entries = parse_abundance_file(li_abundance_file)
        assert len(entries) == 2

        li6 = entries[0]
        assert li6.Z == 3
        assert li6.A == 6
        assert li6.symbol == "Li"
        assert li6.abundance == pytest.approx(0.0759, abs=1e-6)
        assert li6.atomic_mass == 6.0

        li7 = entries[1]
        assert li7.Z == 3
        assert li7.A == 7
        assert li7.symbol == "Li"
        assert li7.abundance == pytest.approx(0.9241, abs=1e-6)
        assert li7.atomic_mass == 7.0

    def test_fractional_conversion(self, li_abundance_file: Path) -> None:
        entries = parse_abundance_file(li_abundance_file)
        total = sum(e.abundance for e in entries)
        assert total == pytest.approx(1.0, abs=1e-4)

    def test_empty_file(self, empty_abundance_file: Path) -> None:
        entries = parse_abundance_file(empty_abundance_file)
        assert entries == []

    def test_nonexistent_file(self, tmp_path: Path) -> None:
        entries = parse_abundance_file(tmp_path / "z999")
        assert entries == []

    def test_mo_isotopes(self, mo_abundance_file: Path) -> None:
        entries = parse_abundance_file(mo_abundance_file)
        assert len(entries) == 7
        assert all(e.Z == 42 for e in entries)
        assert all(e.symbol == "Mo" for e in entries)
        total = sum(e.abundance for e in entries)
        assert total == pytest.approx(1.0, abs=0.01)


class TestInsertAbundances:
    """Tests for insert_abundances()."""

    def test_insert_and_count(self, li_abundance_file: Path) -> None:
        entries = parse_abundance_file(li_abundance_file)
        conn = sqlite3.connect(":memory:")
        conn.executescript(
            """
            CREATE TABLE natural_abundances (
                Z INTEGER NOT NULL,
                A INTEGER NOT NULL,
                symbol TEXT NOT NULL,
                abundance REAL NOT NULL,
                atomic_mass REAL NOT NULL,
                PRIMARY KEY (Z, A)
            );
            """
        )
        count = insert_abundances(conn, entries)
        assert count == 2

        rows = conn.execute("SELECT * FROM natural_abundances ORDER BY A").fetchall()
        assert len(rows) == 2
        assert rows[0][0] == 3  # Z
        assert rows[0][1] == 6  # A
        conn.close()

    def test_insert_empty(self) -> None:
        conn = sqlite3.connect(":memory:")
        conn.executescript(
            """
            CREATE TABLE natural_abundances (
                Z INTEGER NOT NULL,
                A INTEGER NOT NULL,
                symbol TEXT NOT NULL,
                abundance REAL NOT NULL,
                atomic_mass REAL NOT NULL,
                PRIMARY KEY (Z, A)
            );
            """
        )
        count = insert_abundances(conn, [])
        assert count == 0
        conn.close()
