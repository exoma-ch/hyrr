"""Parser for libdEdx stopping power files (PSTAR/ASTAR format).

Supports two formats:

1. **Two-column format** (legacy): ``#Z:{Z}:NPTS:{n}`` header followed
   by *n* lines of ``energy_MeV  dedx_MeV_cm2_g``.

2. **libdEdx split-file format** (from github.com/APTG/libdedx):
   A separate energy grid file (``pstarEng.dat`` / ``astarEng.dat``)
   plus a data file (``PSTAR.dat`` / ``ASTAR.dat``) where each block
   starts with ``#Z:A:NPTS`` and contains *NPTS* dedx values (one per line).
"""

from __future__ import annotations

import logging
import re
import sqlite3
from collections.abc import Iterable, Iterator
from pathlib import Path
from typing import NamedTuple

logger = logging.getLogger(__name__)

# Matches both "#Z:42:NPTS:133" (legacy) and "#42:1:133" (libdEdx)
_HEADER_LEGACY_RE = re.compile(r"#Z:(\d+):NPTS:(\d+)")
_HEADER_LIBDEDX_RE = re.compile(r"#(\d+):\d+:(\d+)")


class StoppingPowerEntry(NamedTuple):
    """Single parsed stopping power data point."""

    source: str  # 'PSTAR' or 'ASTAR'
    target_Z: int
    energy_MeV: float
    dedx: float  # MeV cm^2 / g


def _parse_energy_grid(path: Path) -> list[float]:
    """Parse a libdEdx energy grid file (e.g. pstarEng.dat).

    Format: first line = number of points, then one energy per line [MeV].
    """
    lines = path.read_text(encoding="utf-8").strip().split("\n")
    n = int(lines[0])
    return [float(lines[i + 1]) for i in range(n)]


def parse_stopping_file(
    path: Path,
    source: str,
    energy_grid: list[float] | None = None,
) -> list[StoppingPowerEntry]:
    """Parse a stopping power data file.

    If *energy_grid* is provided, the file is treated as libdEdx format
    (one dedx value per line, energies from the grid). Otherwise, each
    data line must contain ``energy  dedx`` columns (legacy format).
    """
    try:
        text = path.read_text(encoding="utf-8")
    except OSError:
        logger.warning("Cannot read file: %s", path)
        return []

    entries: list[StoppingPowerEntry] = []
    current_z: int | None = None
    npts: int = 0
    data_index: int = 0

    for line in text.splitlines():
        stripped = line.strip()
        if not stripped:
            continue

        # Check for block header (try both formats)
        m = _HEADER_LEGACY_RE.match(stripped) or _HEADER_LIBDEDX_RE.match(stripped)
        if m:
            current_z = int(m.group(1))
            npts = int(m.group(2))
            data_index = 0
            continue

        # Skip other comment lines
        if stripped.startswith("#"):
            continue

        # Data line
        if current_z is not None and data_index < npts:
            parts = stripped.split()
            try:
                if energy_grid is not None:
                    # libdEdx format: single dedx value per line
                    dedx = float(parts[0])
                    energy = energy_grid[data_index]
                elif len(parts) >= 2:
                    # Legacy two-column format
                    energy = float(parts[0])
                    dedx = float(parts[1])
                else:
                    data_index += 1
                    continue

                entries.append(
                    StoppingPowerEntry(
                        source=source,
                        target_Z=current_z,
                        energy_MeV=energy,
                        dedx=dedx,
                    )
                )
            except (ValueError, IndexError):
                logger.debug("Skipping malformed data line: %r", line)
            data_index += 1

    return entries


def parse_libdedx_pair(
    data_path: Path,
    energy_path: Path,
    source: str,
) -> list[StoppingPowerEntry]:
    """Parse a libdEdx data file + energy grid file pair.

    Args:
        data_path: Path to PSTAR.dat or ASTAR.dat.
        energy_path: Path to pstarEng.dat or astarEng.dat.
        source: Source label ('PSTAR' or 'ASTAR').

    Returns:
        List of stopping power entries for all elements in the file.
    """
    energy_grid = _parse_energy_grid(energy_path)
    return parse_stopping_file(data_path, source, energy_grid=energy_grid)


def walk_stopping_directory(
    base_path: Path,
    source: str,
) -> Iterator[StoppingPowerEntry]:
    """Walk a directory of stopping power files and yield all entries.

    Each file in *base_path* is parsed as a stopping power file.
    """
    base = Path(base_path)
    if not base.is_dir():
        logger.error("Stopping power directory does not exist: %s", base)
        return

    for child in sorted(base.iterdir()):
        if not child.is_file():
            continue
        yield from parse_stopping_file(child, source)


def insert_stopping_power(
    conn: sqlite3.Connection,
    entries: Iterable[StoppingPowerEntry],
    batch_size: int = 10000,
) -> int:
    """Insert stopping power entries into database. Returns row count."""
    sql = (
        "INSERT OR REPLACE INTO stopping_power "
        "(source, target_Z, energy_MeV, dedx) "
        "VALUES (?, ?, ?, ?)"
    )
    total = 0
    batch: list[tuple[str, int, float, float]] = []

    for entry in entries:
        batch.append((entry.source, entry.target_Z, entry.energy_MeV, entry.dedx))
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
