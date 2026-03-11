"""Parser for natural isotopic abundance files.

Parses z??? files from the isotopia abundance directory. Each file contains
abundance data for isotopes of one or more elements, with columns:
Z, A, abundance_percent, uncertainty_percent, (padding), symbol.
"""

from __future__ import annotations

import logging
import sqlite3
from collections.abc import Iterable, Iterator
from pathlib import Path
from typing import NamedTuple

logger = logging.getLogger(__name__)


class AbundanceEntry(NamedTuple):
    """Single parsed abundance record."""

    Z: int
    A: int
    symbol: str
    abundance: float  # fractional (0-1)
    atomic_mass: float  # approximated as float(A)


def parse_abundance_file(path: Path) -> list[AbundanceEntry]:
    """Parse a single z??? abundance file.

    Returns an empty list if the file is empty or cannot be read.
    """
    try:
        text = path.read_text(encoding="utf-8")
    except OSError:
        logger.warning("Cannot read file: %s", path)
        return []

    entries: list[AbundanceEntry] = []
    for line in text.splitlines():
        stripped = line.strip()
        if not stripped:
            continue
        parts = stripped.split()
        if len(parts) < 5:
            logger.debug("Skipping short line in %s: %r", path, line)
            continue
        try:
            z = int(parts[0])
            a = int(parts[1])
            abundance_pct = float(parts[2])
            # parts[3] is uncertainty — we skip it
            symbol = parts[-1]  # last token, e.g. "92Mo"
        except (ValueError, IndexError):
            logger.debug("Skipping malformed line in %s: %r", path, line)
            continue

        # Strip leading digits from symbol to get the element name.
        # Symbol field looks like "6Li", "92Mo", "100Mo", etc.
        sym = ""
        for ch in symbol:
            if ch.isalpha():
                sym += ch
        if not sym:
            sym = symbol

        entries.append(
            AbundanceEntry(
                Z=z,
                A=a,
                symbol=sym,
                abundance=abundance_pct / 100.0,
                atomic_mass=float(a),
            )
        )
    return entries


def walk_abundance_directory(base_path: Path) -> Iterator[AbundanceEntry]:
    """Walk abundance directory and yield all entries.

    Expects files named z001 through z118 in *base_path*.
    """
    base = Path(base_path)
    if not base.is_dir():
        logger.error("Abundance directory does not exist: %s", base)
        return

    for child in sorted(base.iterdir()):
        if not child.is_file():
            continue
        if not child.name.startswith("z"):
            continue
        yield from parse_abundance_file(child)


def insert_abundances(
    conn: sqlite3.Connection,
    entries: Iterable[AbundanceEntry],
    batch_size: int = 5000,
) -> int:
    """Insert abundance entries into database. Returns row count."""
    sql = (
        "INSERT OR REPLACE INTO natural_abundances "
        "(Z, A, symbol, abundance, atomic_mass) "
        "VALUES (?, ?, ?, ?, ?)"
    )
    total = 0
    batch: list[tuple[int, int, str, float, float]] = []

    for entry in entries:
        batch.append((entry.Z, entry.A, entry.symbol, entry.abundance, entry.atomic_mass))
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
