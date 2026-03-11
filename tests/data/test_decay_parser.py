"""Tests for the ENDF-6 decay data parser."""

from __future__ import annotations

import sqlite3
import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "data"))

from parsers.decay import (
    _decode_rtyp,
    _parse_endf_float,
    _parse_identification,
    insert_decay_data,
    parse_endf_decay_file,
)

# ---------------------------------------------------------------------------
# Minimal ENDF-6 fixtures
# ---------------------------------------------------------------------------

# A simple beta- decay nuclide (Tc-99): Z=43, A=99, T_half=6.75e12 s,
# one decay mode: beta- with BR=1.0
_TC99_ENDF = """\
 4.309900+4 9.805660+1         -1          0          2          11180 1451    1
 0.000000+0 1.000000+0          0          0          0          61180 1451    2
 0.000000+0 0.000000+0          0          0          4        3111180 1451    3
 0.000000+0 0.000000+0          0          0         18          21180 1451    4
 43-Tc- 99 SAC        EVAL-JUN00 DDEP COLLABORATION               1180 1451    5
                      DIST-NOV07                       261107     1180 1451    6
----JEFF-311          MATERIAL 1180                               1180 1451    7
-----RADIOACTIVE DECAY DATA                                       1180 1451    8
------ENDF-6 FORMAT                                               1180 1451    9
                                1        451         11          01180 1451   10
                                8        457          5          01180 1451   11
 0.000000+0 0.000000+0          0          0          0          01180 1  099999
                                                                  1180 0  0    0
 4.30990+04 9.80566+01          0          0          0          41180 8457    1
 6.75318+12 2.52455+11          0          0          6          01180 8457    2
 8.54016+04 6.00002+02 7.03987-01 1.37411-01 0.00000+00 0.00000+001180 8457    3
 4.50000+00 1.00000+00          0          0          6          11180 8457    4
 1.00000+00 0.00000+00 2.93700+05 1.40000+03 1.00000+00 0.00000+001180 8457    5
                                                                  1180 8  099999
                                                                  1180 0  0    0
                                                                     0 0  0    0
"""

# A stable nuclide (Mo-94): Z=42, A=94, T_half=0.0
_MO94_ENDF = """\
 4.209400+4 9.311370+1         -1          0          2          11134 1451    1
 0.000000+0 1.000000+0          0          0          0          61134 1451    2
 0.000000+0 0.000000+0          0          0          4        3111134 1451    3
 0.000000+0 0.000000+0          0          0         18          21134 1451    4
 42-Mo- 94 CSN+BRC    EVAL-DEC03 G.AUDI, O.BERSILLON, J.BLACHOT + 1134 1451    5
                      DIST-NOV07                       261107     1134 1451    6
----JEFF-311          MATERIAL 1134                               1134 1451    7
-----RADIOACTIVE DECAY DATA                                       1134 1451    8
------ENDF-6 FORMAT                                               1134 1451    9
                                1        451         11          01134 1451   10
                                8        457          5          01134 1451   11
 0.000000+0 0.000000+0          0          0          0          01134 1  099999
                                                                  1134 0  0    0
 4.20940+04 9.31137+01          0          0          0          01134 8457    1
 0.00000+00 0.00000+00          0          0          6          01134 8457    2
 0.00000+00 0.00000+00 0.00000+00 0.00000+00 0.00000+00 0.00000+001134 8457    3
 0.00000+00 1.00000+00          0          0          6          01134 8457    4
 0.00000+00 0.00000+00 0.00000+00 0.00000+00 0.00000+00 0.00000+001134 8457    5
                                                                  1134 8  099999
                                                                  1134 0  0    0
                                                                     0 0  0    0
"""

# A metastable nuclide (Tc-87m) with proton + beta+ decay
_TC87M_ENDF = """\
 4.308700+4 8.618970+1         -1          0          2          11159 1451    1
 0.000000+0 1.000000+0          0          0          0          61159 1451    2
 0.000000+0 0.000000+0          0          0          4        3111159 1451    3
 0.000000+0 0.000000+0          0          0         18          21159 1451    4
 43-Tc- 87MCSN+BRC    EVAL-DEC03 G.AUDI, O.BERSILLON, J.BLACHOT + 1159 1451    5
                      DIST-NOV07                       261107     1159 1451    6
----JEFF-311          MATERIAL 1159                               1159 1451    7
-----RADIOACTIVE DECAY DATA                                       1159 1451    8
------ENDF-6 FORMAT                                               1159 1451    9
                                1        451         11          01159 1451   10
                                8        457          6          01159 1451   11
 0.000000+0 0.000000+0          0          0          0          01159 1  099999
                                                                  1159 0  0    0
 4.30870+04 8.61897+01          0          0          0          01159 8457    1
 2.18000+00 1.60000-01          0          0          6          01159 8457    2
 3.78333+06 0.00000+00 3.78333+06 0.00000+00 0.00000+00 0.00000+001159 8457    3
 0.50000+00-1.00000+00          0          0         12          21159 8457    4
 7.00000+00 0.00000+00 8.50999+05 0.00000+00 3.00000-01 0.00000+001159 8457    5
 2.00000+00 0.00000+00 1.14300+07 0.00000+00 7.00000-01 0.00000+001159 8457    6
                                                                  1159 8  099999
                                                                  1159 0  0    0
                                                                     0 0  0    0
"""

# File with two nuclides concatenated
_TWO_NUCLIDE_FILE = _MO94_ENDF + _TC99_ENDF


class TestParseEndfFloat:
    """Tests for ENDF-6 float parsing."""

    def test_standard_notation(self) -> None:
        assert _parse_endf_float("1.23456E+07") == pytest.approx(1.23456e7)

    def test_compact_notation(self) -> None:
        assert _parse_endf_float("1.23456+07") == pytest.approx(1.23456e7)

    def test_negative_exponent(self) -> None:
        assert _parse_endf_float("6.75318+12") == pytest.approx(6.75318e12)

    def test_zero(self) -> None:
        assert _parse_endf_float("0.00000+00") == 0.0

    def test_empty_string(self) -> None:
        assert _parse_endf_float("") == 0.0

    def test_compact_negative_value(self) -> None:
        assert _parse_endf_float("-1.00000+00") == pytest.approx(-1.0)

    def test_E_format(self) -> None:
        assert _parse_endf_float("2.46600E+04") == pytest.approx(24660.0)


class TestParseIdentification:
    """Tests for nuclide identification line parsing."""

    def test_ground_state(self) -> None:
        result = _parse_identification(
            " 43-Tc- 99 SAC        EVAL-JUN00 DDEP COLLABORATION               "
        )
        assert result == (43, 99, "")

    def test_metastable(self) -> None:
        result = _parse_identification(
            " 43-Tc- 87MCSN+BRC    EVAL-DEC03 G.AUDI, O.BERSILLON"
        )
        assert result == (43, 87, "m")

    def test_mo(self) -> None:
        result = _parse_identification(
            " 42-Mo- 83 CSN+BRC    EVAL-DEC03 G.AUDI, O.BERSILLON, J.BLACHOT + "
        )
        assert result == (42, 83, "")


class TestDecodeRtyp:
    """Tests for RTYP decay mode decoding."""

    def test_beta_minus(self) -> None:
        mode, dz, da = _decode_rtyp(1.0, 43, 99)
        assert mode == "beta-"
        assert dz == 44
        assert da == 99

    def test_beta_plus(self) -> None:
        mode, dz, da = _decode_rtyp(2.0, 43, 85)
        assert mode == "beta+"
        assert dz == 42
        assert da == 85

    def test_alpha(self) -> None:
        mode, dz, da = _decode_rtyp(4.0, 92, 238)
        assert mode == "alpha"
        assert dz == 90
        assert da == 234

    def test_it(self) -> None:
        mode, dz, da = _decode_rtyp(3.0, 43, 99)
        assert mode == "IT"
        assert dz == 43
        assert da == 99

    def test_beta_minus_neutron(self) -> None:
        mode, dz, da = _decode_rtyp(1.5, 43, 85)
        assert mode == "beta-+n"
        assert dz == 44  # +1 from beta-
        assert da == 84  # -1 from neutron

    def test_proton(self) -> None:
        mode, dz, da = _decode_rtyp(7.0, 43, 87)
        assert mode == "p"
        assert dz == 42
        assert da == 86


class TestParseEndfDecayFile:
    """Tests for parsing complete ENDF-6 files."""

    def test_tc99_beta_minus(self, tmp_path: Path) -> None:
        p = tmp_path / "z043"
        p.write_text(_TC99_ENDF)
        entries = parse_endf_decay_file(p)

        assert len(entries) == 1
        e = entries[0]
        assert e.Z == 43
        assert e.A == 99
        assert e.state == ""
        assert e.half_life_s == pytest.approx(6.75318e12, rel=1e-4)
        assert e.decay_mode == "beta-"
        assert e.daughter_Z == 44
        assert e.daughter_A == 99
        assert e.branching == pytest.approx(1.0)

    def test_stable_nuclide(self, tmp_path: Path) -> None:
        p = tmp_path / "z042"
        p.write_text(_MO94_ENDF)
        entries = parse_endf_decay_file(p)

        assert len(entries) == 1
        e = entries[0]
        assert e.Z == 42
        assert e.A == 94
        assert e.state == ""
        assert e.half_life_s is None
        assert e.decay_mode == "stable"
        assert e.daughter_Z is None
        assert e.daughter_A is None
        assert e.branching == 1.0

    def test_metastable_nuclide(self, tmp_path: Path) -> None:
        p = tmp_path / "z043m"
        p.write_text(_TC87M_ENDF)
        entries = parse_endf_decay_file(p)

        assert len(entries) == 2
        assert all(e.Z == 43 for e in entries)
        assert all(e.A == 87 for e in entries)
        assert all(e.state == "m" for e in entries)
        assert entries[0].half_life_s == pytest.approx(2.18, rel=0.01)

        modes = {e.decay_mode for e in entries}
        assert "p" in modes
        assert "beta+" in modes

        # Check branching ratios sum to 1
        total_br = sum(e.branching for e in entries)
        assert total_br == pytest.approx(1.0, abs=0.01)

    def test_two_nuclides_in_one_file(self, tmp_path: Path) -> None:
        p = tmp_path / "z042"
        p.write_text(_TWO_NUCLIDE_FILE)
        entries = parse_endf_decay_file(p)

        assert len(entries) == 2
        z_a_pairs = {(e.Z, e.A) for e in entries}
        assert (42, 94) in z_a_pairs
        assert (43, 99) in z_a_pairs

    def test_nonexistent_file(self, tmp_path: Path) -> None:
        entries = parse_endf_decay_file(tmp_path / "z999")
        assert entries == []

    def test_empty_file(self, tmp_path: Path) -> None:
        p = tmp_path / "z000"
        p.write_text("")
        entries = parse_endf_decay_file(p)
        assert entries == []


class TestInsertDecayData:
    """Tests for insert_decay_data()."""

    def test_insert_and_query(self, tmp_path: Path) -> None:
        p = tmp_path / "z043"
        p.write_text(_TC99_ENDF)
        entries = parse_endf_decay_file(p)

        conn = sqlite3.connect(":memory:")
        conn.executescript(
            """
            CREATE TABLE decay_data (
                Z INTEGER NOT NULL,
                A INTEGER NOT NULL,
                state TEXT NOT NULL DEFAULT '',
                half_life_s REAL,
                decay_mode TEXT NOT NULL,
                daughter_Z INTEGER,
                daughter_A INTEGER,
                daughter_state TEXT DEFAULT '',
                branching REAL DEFAULT 1.0,
                PRIMARY KEY (Z, A, state, decay_mode, daughter_Z, daughter_A, daughter_state)
            );
            """
        )
        count = insert_decay_data(conn, entries)
        assert count == 1

        rows = conn.execute("SELECT * FROM decay_data").fetchall()
        assert len(rows) == 1
        conn.close()

    def test_insert_empty(self) -> None:
        conn = sqlite3.connect(":memory:")
        conn.executescript(
            """
            CREATE TABLE decay_data (
                Z INTEGER NOT NULL,
                A INTEGER NOT NULL,
                state TEXT NOT NULL DEFAULT '',
                half_life_s REAL,
                decay_mode TEXT NOT NULL,
                daughter_Z INTEGER,
                daughter_A INTEGER,
                daughter_state TEXT DEFAULT '',
                branching REAL DEFAULT 1.0,
                PRIMARY KEY (Z, A, state, decay_mode, daughter_Z, daughter_A, daughter_state)
            );
            """
        )
        count = insert_decay_data(conn, [])
        assert count == 0
        conn.close()
