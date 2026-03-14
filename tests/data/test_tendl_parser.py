"""Tests for the TENDL/YANDF-0.2 cross-section parser."""

from __future__ import annotations

import sqlite3

# The parser lives in data/parsers/ which is NOT a package installed via pip.
# We add it to sys.path so pytest can import it.
import sys
import textwrap
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "data"))

from parsers.tendl import (
    CrossSectionEntry,
    _extract_state,
    insert_cross_sections,
    parse_yandf_file,
    walk_tendl_directory,
)

# ---------------------------------------------------------------------------
# Sample YANDF file contents as string literals
# ---------------------------------------------------------------------------

SAMPLE_3COL_METASTABLE = textwrap.dedent("""\
    # header:
    #   title: Mo100(p,x)Tc99m cross section
    #   source: IAEA medical isotope consortium
    #   user: Arjan Koning
    #   date: 2024-10-06
    #   format: YANDF-0.2
    # endf:
    #   library: iaea.2024 + TENDL-2023
    #   author: IAEA medical isotope consortium
    #   year: 2024
    # target:
    #   Z: 42
    #   A: 100
    #   nuclide: Mo100
    # reaction:
    #   type: (p,x)
    #   ENDF_MF: 6
    #   ENDF_MT: 5
    # residual:
    #   Z: 43
    #   A: 99
    #   nuclide: Tc99m
    #   level:
    #     isomer: 1
    # datablock:
    #   quantity: cross section
    #   columns: 3
    #   entries: 4
    ##       E             xs            dxs
    ##     [MeV]          [mb]           [mb]
       6.000000E+00   1.100000E+00   1.100000E+00
       6.100000E+00   9.000000E-01   9.000000E-01
       6.200000E+00   8.000000E-01   8.000000E-01
       6.300000E+00   8.000000E-01   8.000000E-01
""")

SAMPLE_2COL_TOTAL = textwrap.dedent("""\
    # header:
    #   title: Mo100(p,x)Se76 cross section
    #   source: ENDF
    #   user: Arjan Koning
    #   date: 2024-10-07
    #   format: YANDF-0.2
    # endf:
    #   library: tendl.2023
    #   author: A.J. Koning
    #   year: 2023
    # target:
    #   Z: 42
    #   A: 100
    #   nuclide: Mo100
    # reaction:
    #   type: (p,x)
    #   ENDF_MF: 6
    #   ENDF_MT: 5
    # residual:
    #   Z: 34
    #   A: 76
    #   nuclide: Se76
    # datablock:
    #   quantity: cross section
    #   columns: 2
    #   entries: 3
    ##       E             xs
    ##     [MeV]          [mb]
       1.300000E+02   5.243020E-11
       1.400000E+02   2.853684E-07
       1.500000E+02   4.512653E-05
""")

SAMPLE_2COL_GROUND = textwrap.dedent("""\
    # header:
    #   title: Mo100(p,x)Se77g cross section
    #   source: ENDF
    #   user: Arjan Koning
    #   date: 2024-10-07
    #   format: YANDF-0.2
    # endf:
    #   library: tendl.2023
    #   author: A.J. Koning
    #   year: 2023
    # target:
    #   Z: 42
    #   A: 100
    #   nuclide: Mo100
    # reaction:
    #   type: (p,x)
    #   ENDF_MF: 6
    #   ENDF_MT: 5
    # residual:
    #   Z: 34
    #   A: 77
    #   nuclide: Se77g
    #   level:
    #     isomer: 0
    # datablock:
    #   quantity: cross section
    #   columns: 2
    #   entries: 2
    ##       E             xs
    ##     [MeV]          [mb]
       1.200000E+02   4.005038E-12
       1.300000E+02   6.972603E-08
""")


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _write_sample(tmp_path: Path, content: str, filename: str) -> Path:
    """Write sample content to a file in a TENDL-like directory structure."""
    # e.g. tmp_path/p/Mo100/iaea.2024/tables/residual/filename
    proj_dir = tmp_path / "p" / "Mo100" / "iaea.2024" / "tables" / "residual"
    proj_dir.mkdir(parents=True, exist_ok=True)
    fpath = proj_dir / filename
    fpath.write_text(content)
    return fpath


def _create_schema(conn: sqlite3.Connection) -> None:
    """Create the cross_sections table in an in-memory database."""
    conn.executescript("""\
        CREATE TABLE IF NOT EXISTS cross_sections (
            projectile  TEXT    NOT NULL,
            target_Z    INTEGER NOT NULL,
            target_A    INTEGER NOT NULL,
            residual_Z  INTEGER NOT NULL,
            residual_A  INTEGER NOT NULL,
            state       TEXT    NOT NULL DEFAULT '',
            energy_MeV  REAL    NOT NULL,
            xs_mb       REAL    NOT NULL,
            source      TEXT    NOT NULL DEFAULT 'iaea.2024',
            PRIMARY KEY (projectile, target_Z, target_A,
                         residual_Z, residual_A, state, energy_MeV, source)
        );
    """)


# ---------------------------------------------------------------------------
# Tests: parse_yandf_file
# ---------------------------------------------------------------------------


class TestParseYandfFile:
    """Tests for parse_yandf_file."""

    def test_3col_metastable(self, tmp_path: Path) -> None:
        """Parse a 3-column file with metastable state."""
        fpath = _write_sample(
            tmp_path, SAMPLE_3COL_METASTABLE, "p-Mo100-rp043099m.iaea.2024"
        )
        entry = parse_yandf_file(fpath)

        assert entry is not None
        assert entry.projectile == "p"
        assert entry.target_Z == 42
        assert entry.target_A == 100
        assert entry.residual_Z == 43
        assert entry.residual_A == 99
        assert entry.state == "m"
        assert len(entry.energies_MeV) == 4
        assert len(entry.xs_mb) == 4
        assert entry.energies_MeV[0] == pytest.approx(6.0)
        assert entry.xs_mb[0] == pytest.approx(1.1)

    def test_2col_total(self, tmp_path: Path) -> None:
        """Parse a 2-column file with total (no state suffix)."""
        fpath = _write_sample(tmp_path, SAMPLE_2COL_TOTAL, "p-Mo100-rp034076.iaea.2024")
        entry = parse_yandf_file(fpath)

        assert entry is not None
        assert entry.state == ""
        assert entry.target_Z == 42
        assert entry.residual_Z == 34
        assert entry.residual_A == 76
        assert len(entry.energies_MeV) == 3
        assert entry.energies_MeV[0] == pytest.approx(130.0)
        assert entry.xs_mb[0] == pytest.approx(5.243020e-11)

    def test_2col_ground_state(self, tmp_path: Path) -> None:
        """Parse a 2-column file with explicit ground state."""
        fpath = _write_sample(
            tmp_path, SAMPLE_2COL_GROUND, "p-Mo100-rp034077g.iaea.2024"
        )
        entry = parse_yandf_file(fpath)

        assert entry is not None
        assert entry.state == "g"
        assert entry.residual_Z == 34
        assert entry.residual_A == 77
        assert len(entry.energies_MeV) == 2

    def test_header_extraction(self, tmp_path: Path) -> None:
        """Verify all header values are correctly extracted."""
        fpath = _write_sample(
            tmp_path, SAMPLE_3COL_METASTABLE, "p-Mo100-rp043099m.iaea.2024"
        )
        entry = parse_yandf_file(fpath)

        assert entry is not None
        assert entry.target_Z == 42
        assert entry.target_A == 100
        assert entry.residual_Z == 43
        assert entry.residual_A == 99

    def test_data_row_count(self, tmp_path: Path) -> None:
        """Parsed data rows should match the entries count in the header."""
        fpath = _write_sample(
            tmp_path, SAMPLE_3COL_METASTABLE, "p-Mo100-rp043099m.iaea.2024"
        )
        entry = parse_yandf_file(fpath)

        assert entry is not None
        # The sample has entries: 4 in the header
        assert len(entry.energies_MeV) == 4
        assert len(entry.xs_mb) == 4

    def test_empty_file_returns_none(self, tmp_path: Path) -> None:
        """An empty file should return None."""
        fpath = _write_sample(tmp_path, "", "p-Mo100-rp000000.iaea.2024")
        assert parse_yandf_file(fpath) is None

    def test_header_only_returns_none(self, tmp_path: Path) -> None:
        """A file with only header lines and no data should return None."""
        header_only = textwrap.dedent("""\
            # header:
            #   title: empty
            #   format: YANDF-0.2
            # target:
            #   Z: 42
            #   A: 100
            # residual:
            #   Z: 34
            #   A: 76
            #   nuclide: Se76
            # datablock:
            #   columns: 2
            #   entries: 0
        """)
        fpath = _write_sample(tmp_path, header_only, "p-Mo100-rp034076.iaea.2024")
        assert parse_yandf_file(fpath) is None

    def test_malformed_file_returns_none(self, tmp_path: Path) -> None:
        """A file with no recognizable header should return None."""
        fpath = _write_sample(
            tmp_path, "this is not a YANDF file\n", "p-Mo100-rp000000.iaea.2024"
        )
        assert parse_yandf_file(fpath) is None

    def test_nonexistent_file_returns_none(self, tmp_path: Path) -> None:
        """A path that does not exist should return None."""
        assert parse_yandf_file(tmp_path / "nonexistent.txt") is None


# ---------------------------------------------------------------------------
# Tests: _extract_state
# ---------------------------------------------------------------------------


class TestExtractState:
    """Tests for the state extraction helper."""

    def test_metastable_suffix(self) -> None:
        assert _extract_state("Tc99m", None) == "m"

    def test_ground_suffix(self) -> None:
        assert _extract_state("Se77g", None) == "g"

    def test_no_suffix(self) -> None:
        assert _extract_state("Se76", None) == ""

    def test_isomer_level_overrides(self) -> None:
        """When nuclide has no suffix but isomer=1, state should be 'm'."""
        assert _extract_state("Tc99", 1) == "m"

    def test_isomer_level_zero(self) -> None:
        """isomer=0 with no suffix should give empty state."""
        assert _extract_state("Se76", 0) == ""


# ---------------------------------------------------------------------------
# Tests: insert_cross_sections
# ---------------------------------------------------------------------------


class TestInsertCrossSections:
    """Tests for inserting parsed data into SQLite."""

    def test_insert_and_query(self) -> None:
        """Insert entries and verify they are queryable."""
        conn = sqlite3.connect(":memory:")
        _create_schema(conn)

        entry = CrossSectionEntry(
            projectile="p",
            target_Z=42,
            target_A=100,
            residual_Z=43,
            residual_A=99,
            state="m",
            energies_MeV=[6.0, 6.1, 6.2],
            xs_mb=[1.1, 0.9, 0.8],
            source="iaea.2024",
        )

        count = insert_cross_sections(conn, [entry])
        assert count == 3

        rows = conn.execute(
            "SELECT energy_MeV, xs_mb FROM cross_sections "
            "WHERE residual_Z = 43 AND residual_A = 99 AND state = 'm' "
            "ORDER BY energy_MeV",
        ).fetchall()
        assert len(rows) == 3
        assert rows[0][0] == pytest.approx(6.0)
        assert rows[0][1] == pytest.approx(1.1)

    def test_insert_multiple_entries(self) -> None:
        """Insert multiple entries in one call."""
        conn = sqlite3.connect(":memory:")
        _create_schema(conn)

        entries = [
            CrossSectionEntry("p", 42, 100, 43, 99, "m", [6.0], [1.1], "iaea.2024"),
            CrossSectionEntry(
                "p", 42, 100, 34, 76, "", [130.0], [5.2e-11], "tendl.2023"
            ),
        ]

        count = insert_cross_sections(conn, entries)
        assert count == 2

    def test_insert_empty_iterable(self) -> None:
        """Inserting no entries should return 0."""
        conn = sqlite3.connect(":memory:")
        _create_schema(conn)
        assert insert_cross_sections(conn, []) == 0

    def test_batch_size_boundary(self) -> None:
        """Verify batching works with a small batch_size."""
        conn = sqlite3.connect(":memory:")
        _create_schema(conn)

        entry = CrossSectionEntry(
            projectile="p",
            target_Z=42,
            target_A=100,
            residual_Z=43,
            residual_A=99,
            state="",
            energies_MeV=[float(i) for i in range(15)],
            xs_mb=[float(i) * 0.1 for i in range(15)],
            source="iaea.2024",
        )

        count = insert_cross_sections(conn, [entry], batch_size=4)
        assert count == 15

        row_count = conn.execute("SELECT COUNT(*) FROM cross_sections").fetchone()[0]
        assert row_count == 15


# ---------------------------------------------------------------------------
# Tests: walk_tendl_directory
# ---------------------------------------------------------------------------


class TestWalkTendlDirectory:
    """Tests for walking the TENDL directory tree."""

    def test_walk_mini_structure(self, tmp_path: Path) -> None:
        """Walk a minimal directory structure with two files."""
        _write_sample(tmp_path, SAMPLE_3COL_METASTABLE, "p-Mo100-rp043099m.iaea.2024")
        _write_sample(tmp_path, SAMPLE_2COL_TOTAL, "p-Mo100-rp034076.iaea.2024")

        entries = list(walk_tendl_directory(tmp_path))
        assert len(entries) == 2

        states = {e.state for e in entries}
        assert "m" in states
        assert "" in states

    def test_walk_nonexistent_directory(self, tmp_path: Path) -> None:
        """Walking a nonexistent path should yield no entries."""
        entries = list(walk_tendl_directory(tmp_path / "nonexistent"))
        assert entries == []

    def test_walk_empty_directory(self, tmp_path: Path) -> None:
        """Walking an empty directory should yield no entries."""
        entries = list(walk_tendl_directory(tmp_path))
        assert entries == []

    def test_walk_multiple_projectiles(self, tmp_path: Path) -> None:
        """Walk a structure with multiple projectile directories."""
        # Create p/Mo100/... and d/Mo100/...
        _write_sample(tmp_path, SAMPLE_2COL_TOTAL, "p-Mo100-rp034076.iaea.2024")

        # Create d directory manually
        d_dir = tmp_path / "d" / "Mo100" / "iaea.2024" / "tables" / "residual"
        d_dir.mkdir(parents=True)
        content = SAMPLE_2COL_TOTAL.replace("(p,x)", "(d,x)")
        (d_dir / "d-Mo100-rp034076.iaea.2024").write_text(content)

        entries = list(walk_tendl_directory(tmp_path))
        assert len(entries) == 2
        projectiles = {e.projectile for e in entries}
        assert projectiles == {"p", "d"}
