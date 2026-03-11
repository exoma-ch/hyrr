"""Parser for TENDL/IAEA cross-section files in YANDF-0.2 format.

Parses files from the isotopia.libs directory tree and inserts
cross-section data into the hyrr SQLite database.
"""

from __future__ import annotations

import logging
import math
import re
import sqlite3
from collections.abc import Iterable, Iterator
from pathlib import Path
from typing import NamedTuple

logger = logging.getLogger(__name__)

# Projectile directories expected under the TENDL base path.
PROJECTILE_DIRS = ("p", "d", "t", "h", "a")

# Regex to extract the state suffix from a residual nuclide name like Tc99m.
_NUCLIDE_STATE_RE = re.compile(r"^[A-Z][a-z]?\d+(g|m)$")


class CrossSectionEntry(NamedTuple):
    """Single parsed cross-section file entry."""

    projectile: str
    target_Z: int
    target_A: int
    residual_Z: int
    residual_A: int
    state: str  # '', 'g', 'm'
    energies_MeV: list[float]
    xs_mb: list[float]
    source: str


def _extract_state(nuclide: str, isomer_level: int | None) -> str:
    """Determine state from nuclide name and isomer level.

    Rules:
    - Nuclide ending with 'm' -> 'm' (metastable)
    - Nuclide ending with 'g' -> 'g' (ground state)
    - isomer level == 1 -> 'm'
    - Otherwise -> '' (total cross-section)
    """
    if _NUCLIDE_STATE_RE.match(nuclide):
        return nuclide[-1]
    if isomer_level is not None and isomer_level >= 1:
        return "m"
    return ""


def parse_yandf_file(path: Path) -> CrossSectionEntry | None:
    """Parse a single YANDF-0.2 cross-section file.

    Returns None if the file cannot be parsed (e.g., empty or malformed).
    """
    try:
        text = path.read_text(encoding="utf-8")
    except OSError:
        logger.warning("Cannot read file: %s", path)
        return None

    lines = text.splitlines()
    if not lines:
        return None

    # --- Extract header values ---
    target_z: int | None = None
    target_a: int | None = None
    residual_z: int | None = None
    residual_a: int | None = None
    residual_nuclide: str = ""
    isomer_level: int | None = None
    source: str = "iaea.2024"

    # Track the current header section for context-aware parsing.
    section = ""
    subsection = ""

    for line in lines:
        if not line.startswith("#"):
            break

        # Detect section headers like "# target:" or "# residual:"
        section_match = re.match(r"^#\s+(\w+):\s*$", line)
        if section_match:
            section = section_match.group(1)
            subsection = ""
            continue

        # Detect subsection headers like "#   level:"
        subsection_match = re.match(r"^#\s{3,}(\w+):\s*$", line)
        if subsection_match:
            subsection = subsection_match.group(1)
            continue

        # Key-value pairs like "#   Z: 42"
        kv_match = re.match(r"^#\s+(\w+):\s+(.+)$", line)
        if not kv_match:
            continue

        key = kv_match.group(1).strip()
        value = kv_match.group(2).strip()

        if section == "target":
            if key == "Z":
                target_z = int(value)
            elif key == "A":
                target_a = int(value)
        elif section == "residual":
            if subsection == "level" and key == "isomer":
                isomer_level = int(value)
            elif key == "Z":
                residual_z = int(value)
            elif key == "A":
                residual_a = int(value)
            elif key == "nuclide":
                residual_nuclide = value
        elif section == "endf":
            if key == "library":
                source = value.split("+")[0].strip()
        elif section == "header":
            if key == "format" and value != "YANDF-0.2":
                logger.warning("Unexpected format %s in %s", value, path)

    # Validate required header fields.
    if any(v is None for v in (target_z, target_a, residual_z, residual_a)):
        logger.warning("Missing header fields in %s", path)
        return None

    assert target_z is not None
    assert target_a is not None
    assert residual_z is not None
    assert residual_a is not None

    # Determine state.
    state = _extract_state(residual_nuclide, isomer_level)

    # Determine projectile from directory structure: .../p/Target/...
    projectile = _projectile_from_path(path)

    # Determine source from directory structure if not in header.
    source_from_dir = _source_from_path(path)
    if source_from_dir:
        source = source_from_dir

    # --- Parse data rows ---
    energies: list[float] = []
    xs_values: list[float] = []

    for line in lines:
        if line.startswith("#"):
            continue
        stripped = line.strip()
        if not stripped:
            continue
        parts = stripped.split()
        if len(parts) < 2:
            continue
        try:
            energy = float(parts[0])
            xs = float(parts[1])
        except ValueError:
            continue
        energies.append(energy)
        xs_values.append(xs)

    if not energies:
        return None

    return CrossSectionEntry(
        projectile=projectile,
        target_Z=target_z,
        target_A=target_a,
        residual_Z=residual_z,
        residual_A=residual_a,
        state=state,
        energies_MeV=energies,
        xs_mb=xs_values,
        source=source,
    )


def _projectile_from_path(path: Path) -> str:
    """Extract the projectile letter from the file path.

    Looks for a parent directory named one of {p, d, t, h, a}.
    Falls back to the first character of the filename.
    """
    for parent in path.parents:
        if parent.name in PROJECTILE_DIRS:
            return parent.name
    # Fallback: extract from filename like "p-Mo100-..."
    return path.name.split("-")[0] if "-" in path.name else "p"


def _source_from_path(path: Path) -> str | None:
    """Extract the source identifier from the directory path.

    Looks for a parent directory like 'iaea.2024' or 'tendl.2023'.
    """
    for parent in path.parents:
        if re.match(r"^(iaea|tendl)\.\d{4}$", parent.name):
            return parent.name
    return None


def walk_tendl_directory(base_path: Path) -> Iterator[CrossSectionEntry]:
    """Walk the TENDL directory tree and yield parsed cross-section entries.

    Expected structure:
        base_path/{p,d,t,h,a}/{Target}/iaea.2024/tables/residual/*
    """
    base = Path(base_path)
    if not base.is_dir():
        logger.error("TENDL base path does not exist: %s", base)
        return

    for proj_dir in PROJECTILE_DIRS:
        proj_path = base / proj_dir
        if not proj_path.is_dir():
            logger.debug("Projectile directory not found: %s", proj_path)
            continue

        for target_dir in sorted(proj_path.iterdir()):
            if not target_dir.is_dir():
                continue

            residual_dir = target_dir / "iaea.2024" / "tables" / "residual"
            if not residual_dir.is_dir():
                logger.debug("No residual dir: %s", residual_dir)
                continue

            for data_file in sorted(residual_dir.iterdir()):
                if data_file.is_dir():
                    continue
                entry = parse_yandf_file(data_file)
                if entry is not None:
                    yield entry


def insert_cross_sections(
    conn: sqlite3.Connection,
    entries: Iterable[CrossSectionEntry],
    batch_size: int = 10000,
) -> int:
    """Insert cross-section entries into the database.

    Uses transactions and batch inserts for performance.
    Returns total number of rows inserted.
    """
    sql = (
        "INSERT OR REPLACE INTO cross_sections "
        "(projectile, target_Z, target_A, residual_Z, residual_A, "
        "state, energy_MeV, xs_mb, source) "
        "VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"
    )
    total = 0
    batch: list[tuple[str, int, int, int, int, str, float, float, str]] = []

    for entry in entries:
        for energy, xs in zip(entry.energies_MeV, entry.xs_mb, strict=True):
            # Skip NaN/inf values from malformed data files
            if not (math.isfinite(energy) and math.isfinite(xs)):
                continue
            batch.append((
                entry.projectile,
                entry.target_Z,
                entry.target_A,
                entry.residual_Z,
                entry.residual_A,
                entry.state,
                energy,
                xs,
                entry.source,
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
