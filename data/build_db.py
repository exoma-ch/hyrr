"""One-shot script to build hyrr.sqlite from raw data sources.

Parses:
- TENDL cross-sections from isotopia.libs/{p,d,t,h,a}/*/iaea.2024/tables/residual/*
- PSTAR/ASTAR stopping power tables from libdEdx data files
- Natural isotopic abundances (IUPAC)
- Decay data from ISOTOPIA decay files

Usage:
    python data/build_db.py \
        --tendl-path ../curie/isotopia.libs/ \
        --abundance-path ../curie/isotopia/files/abundance/ \
        --decay-path ../curie/isotopia/files/decay/ \
        --output data/hyrr.sqlite
"""

from __future__ import annotations

import argparse
import logging
import sqlite3
import sys
from pathlib import Path

logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s %(levelname)s %(name)s: %(message)s",
)
logger = logging.getLogger(__name__)

# Schema DDL (must match src/hyrr/db.py exactly).
_SCHEMA_SQL = """\
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
CREATE INDEX IF NOT EXISTS idx_reaction
    ON cross_sections(projectile, target_Z, target_A);
CREATE INDEX IF NOT EXISTS idx_product
    ON cross_sections(residual_Z, residual_A, state);

CREATE TABLE IF NOT EXISTS stopping_power (
    source      TEXT    NOT NULL,
    target_Z    INTEGER NOT NULL,
    energy_MeV  REAL    NOT NULL,
    dedx        REAL    NOT NULL,
    PRIMARY KEY (source, target_Z, energy_MeV)
);

CREATE TABLE IF NOT EXISTS natural_abundances (
    Z            INTEGER NOT NULL,
    A            INTEGER NOT NULL,
    symbol       TEXT    NOT NULL,
    abundance    REAL    NOT NULL,
    atomic_mass  REAL    NOT NULL,
    PRIMARY KEY (Z, A)
);

CREATE TABLE IF NOT EXISTS decay_data (
    Z              INTEGER NOT NULL,
    A              INTEGER NOT NULL,
    state          TEXT    NOT NULL DEFAULT '',
    half_life_s    REAL,
    decay_mode     TEXT    NOT NULL,
    daughter_Z     INTEGER,
    daughter_A     INTEGER,
    daughter_state TEXT    DEFAULT '',
    branching      REAL    DEFAULT 1.0,
    PRIMARY KEY (Z, A, state, decay_mode, daughter_Z, daughter_A, daughter_state)
);
CREATE INDEX IF NOT EXISTS idx_parent
    ON decay_data(Z, A, state);
CREATE INDEX IF NOT EXISTS idx_daughter
    ON decay_data(daughter_Z, daughter_A, daughter_state);

CREATE TABLE IF NOT EXISTS elements (
    Z       INTEGER PRIMARY KEY,
    symbol  TEXT    NOT NULL UNIQUE
);
"""


def _init_database(db_path: Path) -> sqlite3.Connection:
    """Open (or create) the SQLite database and ensure the schema exists."""
    conn = sqlite3.connect(str(db_path))
    conn.execute("PRAGMA journal_mode=WAL")
    conn.execute("PRAGMA synchronous=NORMAL")
    conn.executescript(_SCHEMA_SQL)
    return conn


def build_cross_sections(conn: sqlite3.Connection, tendl_path: Path) -> int:
    """Parse and insert TENDL cross-sections."""
    from parsers.tendl import insert_cross_sections, walk_tendl_directory

    entries = walk_tendl_directory(tendl_path)
    return insert_cross_sections(conn, entries)


def build_abundances(conn: sqlite3.Connection, abundance_path: Path) -> int:
    """Parse and insert natural isotopic abundances."""
    from parsers.abundance import insert_abundances, walk_abundance_directory

    entries = walk_abundance_directory(abundance_path)
    return insert_abundances(conn, entries)


def build_decay_data(conn: sqlite3.Connection, decay_path: Path) -> int:
    """Parse and insert decay data from ENDF-6 files."""
    from parsers.decay import insert_decay_data, walk_decay_directory

    entries = walk_decay_directory(decay_path)
    return insert_decay_data(conn, entries)


def build_stopping_power(conn: sqlite3.Connection, path: Path, source: str) -> int:
    """Parse and insert stopping power data."""
    from parsers.stopping import insert_stopping_power, parse_stopping_file

    entries = parse_stopping_file(path, source)
    return insert_stopping_power(conn, entries)


def build_stopping_from_libdedx(conn: sqlite3.Connection, stopping_dir: Path) -> int:
    """Parse and insert PSTAR/ASTAR data from libdEdx directory.

    Expects files: PSTAR.dat, pstarEng.dat, ASTAR.dat, astarEng.dat.
    """
    from parsers.stopping import insert_stopping_power, parse_libdedx_pair

    total = 0
    pstar_data = stopping_dir / "PSTAR.dat"
    pstar_eng = stopping_dir / "pstarEng.dat"
    if pstar_data.exists() and pstar_eng.exists():
        entries = parse_libdedx_pair(pstar_data, pstar_eng, "PSTAR")
        n = insert_stopping_power(conn, entries)
        logger.info("Inserted %d PSTAR rows.", n)
        total += n

    astar_data = stopping_dir / "ASTAR.dat"
    astar_eng = stopping_dir / "astarEng.dat"
    if astar_data.exists() and astar_eng.exists():
        entries = parse_libdedx_pair(astar_data, astar_eng, "ASTAR")
        n = insert_stopping_power(conn, entries)
        logger.info("Inserted %d ASTAR rows.", n)
        total += n

    return total


def main() -> None:
    """CLI entry point for building the hyrr.sqlite database."""
    parser = argparse.ArgumentParser(
        description="Build hyrr.sqlite database from raw data sources.",
    )
    parser.add_argument(
        "--tendl-path",
        type=Path,
        help="Path to isotopia.libs/ directory with TENDL cross-section files",
    )
    parser.add_argument(
        "--abundance-path",
        type=Path,
        help="Path to abundance/ directory with z??? files",
    )
    parser.add_argument(
        "--decay-path",
        type=Path,
        help="Path to decay/ directory with ENDF-6 z??? files",
    )
    parser.add_argument(
        "--stopping-path",
        type=Path,
        help="Path to directory with libdEdx PSTAR/ASTAR data files",
    )
    parser.add_argument(
        "--pstar-file",
        type=Path,
        help="Path to PSTAR stopping power data file (legacy format)",
    )
    parser.add_argument(
        "--astar-file",
        type=Path,
        help="Path to ASTAR stopping power data file (legacy format)",
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=Path("data/hyrr.sqlite"),
        help="Output SQLite database path (default: data/hyrr.sqlite)",
    )
    args = parser.parse_args()

    has_any = args.tendl_path or args.abundance_path or args.decay_path or args.stopping_path or args.pstar_file or args.astar_file
    if not has_any:
        parser.print_help()
        sys.exit(1)

    logger.info("Building database: %s", args.output)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    conn = _init_database(args.output)

    try:
        if args.tendl_path:
            logger.info("Parsing TENDL cross-sections from %s ...", args.tendl_path)
            n = build_cross_sections(conn, args.tendl_path)
            logger.info("Inserted %d cross-section rows.", n)

        if args.abundance_path:
            logger.info("Parsing abundances from %s ...", args.abundance_path)
            n = build_abundances(conn, args.abundance_path)
            logger.info("Inserted %d abundance rows.", n)

        if args.decay_path:
            logger.info("Parsing decay data from %s ...", args.decay_path)
            n = build_decay_data(conn, args.decay_path)
            logger.info("Inserted %d decay data rows.", n)

        if args.stopping_path:
            logger.info("Parsing PSTAR/ASTAR from %s ...", args.stopping_path)
            n = build_stopping_from_libdedx(conn, args.stopping_path)
            logger.info("Inserted %d stopping power rows total.", n)

        if args.pstar_file:
            logger.info("Parsing PSTAR data from %s ...", args.pstar_file)
            n = build_stopping_power(conn, args.pstar_file, "PSTAR")
            logger.info("Inserted %d PSTAR rows.", n)

        if args.astar_file:
            logger.info("Parsing ASTAR data from %s ...", args.astar_file)
            n = build_stopping_power(conn, args.astar_file, "ASTAR")
            logger.info("Inserted %d ASTAR rows.", n)
    finally:
        conn.close()

    logger.info("Done.")


if __name__ == "__main__":
    main()
