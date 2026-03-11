"""Parser for ENDF-6 decay data files.

Parses z??? files from the isotopia decay directory. Each file contains
multiple nuclide records in ENDF-6 format with sections MF=1/MT=451
(identification) and MF=8/MT=457 (decay data).
"""

from __future__ import annotations

import logging
import re
import sqlite3
from collections.abc import Iterable, Iterator
from pathlib import Path
from typing import NamedTuple

logger = logging.getLogger(__name__)

# ENDF-6 decay mode codes mapped to human-readable strings and
# (delta_Z, delta_A) for computing daughter nuclide.
_RTYP_MAP: dict[int, tuple[str, int, int]] = {
    1: ("beta-", +1, 0),
    2: ("beta+", -1, 0),
    3: ("IT", 0, 0),
    4: ("alpha", -2, -4),
    5: ("n", 0, -1),
    6: ("SF", 0, 0),
    7: ("p", -1, -1),
}

# Regex to parse the nuclide identification line (1451 line 5).
# Examples: " 43-Tc- 85 ", " 43-Tc- 99 ", " 42-Mo- 89M", " 43-Tc- 87MCSN"
_IDENT_RE = re.compile(
    r"\s*(\d+)-([A-Z][a-z]?)\s*-\s*(\d+)\s*(M?)",
)


class DecayEntry(NamedTuple):
    """Single parsed decay record."""

    Z: int
    A: int
    state: str  # '' or 'm'
    half_life_s: float | None  # None for stable
    decay_mode: str
    daughter_Z: int | None
    daughter_A: int | None
    daughter_state: str
    branching: float


def _parse_endf_float(s: str) -> float:
    """Parse an ENDF-6 floating-point field.

    ENDF-6 uses a compact notation where the 'E' may be omitted:
    ``1.23456+07`` means ``1.23456E+07``.  Standard notation like
    ``1.23456E+07`` is also accepted.
    """
    s = s.strip()
    if not s:
        return 0.0
    # Already has 'E' or 'e' — standard float
    if "E" in s or "e" in s:
        return float(s)
    # Look for sign character that isn't at position 0 (not the leading sign).
    # Pattern: digits/dot followed by +/- then digits.
    m = re.search(r"(?<=[0-9])([+-])", s)
    if m:
        idx = m.start()
        return float(s[:idx] + "E" + s[idx:])
    return float(s)


def _parse_endf_fields(line: str) -> list[float]:
    """Extract up to 6 numeric fields from an ENDF-6 data line.

    Each field occupies 11 characters in the first 66 columns.
    """
    fields: list[float] = []
    for i in range(6):
        start = i * 11
        end = start + 11
        if end > len(line):
            break
        raw = line[start:end]
        try:
            fields.append(_parse_endf_float(raw))
        except ValueError:
            fields.append(0.0)
    return fields


def _parse_endf_ints(line: str) -> list[int]:
    """Extract integer fields from an ENDF-6 line.

    Integer fields are in the same 11-char positions but formatted as I11.
    """
    ints: list[int] = []
    for i in range(6):
        start = i * 11
        end = start + 11
        if end > len(line):
            break
        raw = line[start:end].strip()
        try:
            ints.append(int(raw))
        except ValueError:
            ints.append(0)
    return ints


def _get_mf_mt(line: str) -> tuple[int, int]:
    """Extract MF and MT from an ENDF-6 line (columns 71-72 and 73-75)."""
    if len(line) < 75:
        return 0, 0
    try:
        mf = int(line[70:72].strip() or "0")
        mt = int(line[72:75].strip() or "0")
    except ValueError:
        return 0, 0
    return mf, mt


def _get_mat(line: str) -> int:
    """Extract material number (MAT) from columns 67-70."""
    if len(line) < 70:
        return 0
    try:
        return int(line[66:70].strip() or "0")
    except ValueError:
        return 0


def _decode_rtyp(rtyp_val: float, parent_z: int, parent_a: int) -> tuple[str, int, int]:
    """Decode an RTYP value into (mode_string, daughter_Z, daughter_A).

    Handles compound modes like 1.5 (beta- + neutron).
    """
    primary = int(rtyp_val)
    secondary = round((rtyp_val - primary) * 10)

    if primary not in _RTYP_MAP:
        return f"unknown({rtyp_val})", parent_z, parent_a

    mode_str, dz, da = _RTYP_MAP[primary]
    daughter_z = parent_z + dz
    daughter_a = parent_a + da

    if secondary and secondary in _RTYP_MAP:
        sec_str, dz2, da2 = _RTYP_MAP[secondary]
        mode_str = f"{mode_str}+{sec_str}"
        daughter_z += dz2
        daughter_a += da2

    return mode_str, daughter_z, daughter_a


def _parse_identification(line: str) -> tuple[int, int, str] | None:
    """Parse nuclide identification from 1451 line 5.

    Returns (Z, A, state) or None if unparseable.
    """
    m = _IDENT_RE.match(line)
    if not m:
        return None
    z = int(m.group(1))
    a = int(m.group(3))
    state = "m" if m.group(4) == "M" else ""
    return z, a, state


def parse_endf_decay_file(path: Path) -> list[DecayEntry]:
    """Parse a single z??? ENDF-6 decay data file.

    Each file contains multiple nuclide records separated by MAT=0 markers.
    """
    try:
        text = path.read_text(encoding="utf-8")
    except OSError:
        logger.warning("Cannot read file: %s", path)
        return []

    lines = text.splitlines()
    entries: list[DecayEntry] = []

    # State tracking
    current_z: int | None = None
    current_a: int | None = None
    current_state: str = ""
    current_mat: int = 0

    # Track which line number within a given MF/MT section
    section_line: int = 0
    in_1451: bool = False
    in_8457: bool = False
    ident_line_count: int = 0

    # Decay data accumulators
    half_life: float | None = None
    ndk: int = 0
    decay_lines_remaining: int = 0

    for line in lines:
        if len(line) < 75:
            continue

        mat = _get_mat(line)
        mf, mt = _get_mf_mt(line)

        # MAT=0 marks end of nuclide
        if mat == 0:
            current_z = None
            current_a = None
            current_state = ""
            current_mat = 0
            in_1451 = False
            in_8457 = False
            section_line = 0
            ident_line_count = 0
            continue

        # Track material transitions
        if mat != current_mat:
            current_mat = mat
            in_1451 = False
            in_8457 = False
            section_line = 0
            ident_line_count = 0

        # Section MF=1, MT=451: identification
        if mf == 1 and mt == 451:
            if not in_1451:
                in_1451 = True
                in_8457 = False
                ident_line_count = 0
            ident_line_count += 1
            if ident_line_count == 5:
                ident = _parse_identification(line)
                if ident:
                    current_z, current_a, current_state = ident
            continue

        # End of 1451 section (MF=1, MT=99999 or different section starts)
        if in_1451 and (mf != 1 or mt != 451):
            in_1451 = False

        # Section MF=8, MT=457: decay data
        if mf == 8 and mt == 457:
            if not in_8457:
                in_8457 = True
                section_line = 0

            section_line += 1

            if current_z is None or current_a is None:
                continue

            if section_line == 2:
                # Half-life line: T_half, dT_half, then integers
                fields = _parse_endf_fields(line)
                half_life = fields[0] if fields else 0.0
                # Half-life of 0.0 means stable
                if half_life == 0.0:
                    half_life = None

            elif section_line == 4:
                # SPI, PAR, then 4 integers — last useful integer is NDK
                ints = _parse_endf_ints(line)
                ndk = ints[5] if len(ints) >= 6 else 0
                decay_lines_remaining = ndk

                if half_life is None:
                    # Stable nuclide
                    entries.append(
                        DecayEntry(
                            Z=current_z,
                            A=current_a,
                            state=current_state,
                            half_life_s=None,
                            decay_mode="stable",
                            daughter_Z=None,
                            daughter_A=None,
                            daughter_state="",
                            branching=1.0,
                        )
                    )

                if ndk == 0 and half_life is not None:
                    # Unstable but no decay modes listed — record as-is
                    entries.append(
                        DecayEntry(
                            Z=current_z,
                            A=current_a,
                            state=current_state,
                            half_life_s=half_life,
                            decay_mode="unknown",
                            daughter_Z=None,
                            daughter_A=None,
                            daughter_state="",
                            branching=1.0,
                        )
                    )

            elif section_line >= 5 and decay_lines_remaining > 0:
                # Decay mode record: RTYP, RFS, Q, dQ, BR, dBR
                fields = _parse_endf_fields(line)
                if len(fields) >= 5:
                    rtyp = fields[0]
                    rfs = fields[1]
                    branching = fields[4]

                    mode_str, daughter_z, daughter_a = _decode_rtyp(
                        rtyp, current_z, current_a
                    )

                    # RFS=0 → ground state, RFS=1 → metastable
                    daughter_state = "m" if rfs >= 1.0 else ""
                    # IT always produces ground state of same nuclide
                    if int(rtyp) == 3:
                        daughter_state = ""

                    entries.append(
                        DecayEntry(
                            Z=current_z,
                            A=current_a,
                            state=current_state,
                            half_life_s=half_life,
                            decay_mode=mode_str,
                            daughter_Z=daughter_z,
                            daughter_A=daughter_a,
                            daughter_state=daughter_state,
                            branching=branching,
                        )
                    )
                decay_lines_remaining -= 1

            continue

        # Leaving 8457 section
        if in_8457 and (mf != 8 or mt != 457):
            in_8457 = False
            section_line = 0

    return entries


def walk_decay_directory(base_path: Path) -> Iterator[DecayEntry]:
    """Walk decay directory and yield all entries.

    Expects files named z000 through z118 in *base_path*.
    """
    base = Path(base_path)
    if not base.is_dir():
        logger.error("Decay directory does not exist: %s", base)
        return

    for child in sorted(base.iterdir()):
        if not child.is_file():
            continue
        if not child.name.startswith("z"):
            continue
        yield from parse_endf_decay_file(child)


def insert_decay_data(
    conn: sqlite3.Connection,
    entries: Iterable[DecayEntry],
    batch_size: int = 5000,
) -> int:
    """Insert decay entries into database. Returns row count."""
    sql = (
        "INSERT OR REPLACE INTO decay_data "
        "(Z, A, state, half_life_s, decay_mode, daughter_Z, daughter_A, "
        "daughter_state, branching) "
        "VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"
    )
    total = 0
    batch: list[tuple[int, int, str, float | None, str, int | None, int | None, str, float]] = []

    for entry in entries:
        batch.append((
            entry.Z,
            entry.A,
            entry.state,
            entry.half_life_s,
            entry.decay_mode,
            entry.daughter_Z,
            entry.daughter_A,
            entry.daughter_state,
            entry.branching,
        ))
        if len(batch) >= batch_size:
            conn.executemany(sql, batch)
            conn.commit()
            total += len(batch)
            batch.clear()

    if batch:
        conn.executemany(sql, batch)
        conn.commit()
        total += len(batch)

    return total
