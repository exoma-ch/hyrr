"""Export hyrr.sqlite into per-element SQL INSERT chunks for the WASM frontend.

Splits the master database into gzipped SQL files that can be lazy-loaded
in the browser via sql.js. Each chunk contains INSERT statements for one
projectile+element combination.

Output structure:
    output_dir/
    ├── schema.sql                # CREATE TABLE + CREATE INDEX statements
    ├── meta.sql.gz               # abundances + decay data + meta table
    ├── stopping.sql.gz           # PSTAR/ASTAR stopping power tables
    └── xs/
        ├── p_Mo.sql.gz           # proton + Molybdenum cross-sections
        ├── p_O.sql.gz            # proton + Oxygen cross-sections
        ├── a_Bi.sql.gz           # alpha + Bismuth cross-sections
        └── ...                   # one file per projectile+element

Usage:
    python data/build_chunks.py --db data/hyrr.sqlite --output frontend/public/data/
"""

from __future__ import annotations

import argparse
import gzip
import logging
import sqlite3
from pathlib import Path

logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s %(levelname)s %(name)s: %(message)s",
)
logger = logging.getLogger(__name__)

# Element symbols by Z (1-118)
ELEMENT_SYMBOLS: dict[int, str] = {}


def _load_element_symbols(conn: sqlite3.Connection) -> None:
    """Load element symbols from the elements table."""
    global ELEMENT_SYMBOLS
    rows = conn.execute("SELECT Z, symbol FROM elements").fetchall()
    if rows:
        ELEMENT_SYMBOLS = {z: sym for z, sym in rows}
    else:
        # Fallback: derive from natural_abundances table
        rows = conn.execute(
            "SELECT DISTINCT Z, symbol FROM natural_abundances"
        ).fetchall()
        ELEMENT_SYMBOLS = {z: sym for z, sym in rows}


def _escape_sql_value(value: object) -> str:
    """Escape a Python value for SQL INSERT."""
    if value is None:
        return "NULL"
    if isinstance(value, str):
        return "'" + value.replace("'", "''") + "'"
    return str(value)


def _rows_to_inserts(table: str, rows: list[tuple], columns: list[str]) -> str:
    """Convert rows to SQL INSERT statements."""
    lines = []
    for row in rows:
        values = ",".join(_escape_sql_value(v) for v in row)
        lines.append(f"INSERT INTO {table}({','.join(columns)}) VALUES ({values});")
    return "\n".join(lines)


def _write_gzipped(path: Path, content: str) -> int:
    """Write gzipped content to a file. Returns uncompressed size."""
    path.parent.mkdir(parents=True, exist_ok=True)
    with gzip.open(path, "wt", encoding="utf-8") as f:
        f.write(content)
    return len(content)


def export_schema(conn: sqlite3.Connection, output_dir: Path) -> None:
    """Export the CREATE TABLE / CREATE INDEX statements."""
    from build_db import _SCHEMA_SQL

    schema_path = output_dir / "schema.sql"
    schema_path.write_text(_SCHEMA_SQL, encoding="utf-8")
    logger.info("Wrote schema.sql (%d bytes)", len(_SCHEMA_SQL))


def export_meta(conn: sqlite3.Connection, output_dir: Path) -> None:
    """Export abundances + decay data + meta table as a single chunk."""
    parts = []

    # Natural abundances
    rows = conn.execute(
        "SELECT Z, A, symbol, abundance, atomic_mass FROM natural_abundances"
    ).fetchall()
    cols = ["Z", "A", "symbol", "abundance", "atomic_mass"]
    parts.append(f"-- natural_abundances: {len(rows)} rows")
    parts.append(_rows_to_inserts("natural_abundances", rows, cols))

    # Decay data
    rows = conn.execute(
        "SELECT Z, A, state, half_life_s, decay_mode, "
        "daughter_Z, daughter_A, daughter_state, branching FROM decay_data"
    ).fetchall()
    cols = [
        "Z", "A", "state", "half_life_s", "decay_mode",
        "daughter_Z", "daughter_A", "daughter_state", "branching",
    ]
    parts.append(f"-- decay_data: {len(rows)} rows")
    parts.append(_rows_to_inserts("decay_data", rows, cols))

    # Elements (table may not exist in older databases)
    try:
        rows = conn.execute("SELECT Z, symbol FROM elements").fetchall()
        if rows:
            cols = ["Z", "symbol"]
            parts.append(f"-- elements: {len(rows)} rows")
            parts.append(_rows_to_inserts("elements", rows, cols))
    except sqlite3.OperationalError:
        logger.info("No elements table — skipping")

    # Meta table (table may not exist in older databases)
    try:
        rows = conn.execute("SELECT key, value FROM meta").fetchall()
        if rows:
            cols = ["key", "value"]
            parts.append(f"-- meta: {len(rows)} rows")
            parts.append(_rows_to_inserts("meta", rows, cols))
    except sqlite3.OperationalError:
        logger.info("No meta table — skipping")

    content = "\n".join(parts)
    size = _write_gzipped(output_dir / "meta.sql.gz", content)
    logger.info("Wrote meta.sql.gz (%d bytes uncompressed)", size)


def export_stopping(conn: sqlite3.Connection, output_dir: Path) -> None:
    """Export stopping power tables as a single chunk."""
    rows = conn.execute(
        "SELECT source, target_Z, energy_MeV, dedx FROM stopping_power"
    ).fetchall()
    cols = ["source", "target_Z", "energy_MeV", "dedx"]
    content = f"-- stopping_power: {len(rows)} rows\n"
    content += _rows_to_inserts("stopping_power", rows, cols)

    size = _write_gzipped(output_dir / "stopping.sql.gz", content)
    logger.info("Wrote stopping.sql.gz (%d bytes uncompressed, %d rows)", size, len(rows))


def export_cross_sections(conn: sqlite3.Connection, output_dir: Path) -> None:
    """Export cross-sections split by projectile + target element."""
    xs_dir = output_dir / "xs"
    xs_dir.mkdir(parents=True, exist_ok=True)

    # Get all distinct projectile + target_Z combinations
    combos = conn.execute(
        "SELECT DISTINCT projectile, target_Z FROM cross_sections "
        "ORDER BY projectile, target_Z"
    ).fetchall()

    cols = [
        "projectile", "target_Z", "target_A", "residual_Z", "residual_A",
        "state", "energy_MeV", "xs_mb", "source",
    ]

    total_files = 0
    total_rows = 0

    for projectile, target_Z in combos:
        symbol = ELEMENT_SYMBOLS.get(target_Z, f"Z{target_Z}")
        rows = conn.execute(
            "SELECT projectile, target_Z, target_A, residual_Z, residual_A, "
            "state, energy_MeV, xs_mb, source "
            "FROM cross_sections WHERE projectile=? AND target_Z=?",
            (projectile, target_Z),
        ).fetchall()

        if not rows:
            continue

        content = f"-- {projectile} + {symbol} (Z={target_Z}): {len(rows)} rows\n"
        content += _rows_to_inserts("cross_sections", rows, cols)

        chunk_path = xs_dir / f"{projectile}_{symbol}.sql.gz"
        _write_gzipped(chunk_path, content)
        total_files += 1
        total_rows += len(rows)

    logger.info(
        "Wrote %d cross-section chunks (%d total rows)", total_files, total_rows
    )


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Export hyrr.sqlite into SQL chunks for WASM frontend.",
    )
    parser.add_argument(
        "--db",
        type=Path,
        default=Path("data/hyrr.sqlite"),
        help="Input SQLite database path (default: data/hyrr.sqlite)",
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=Path("frontend/public/data"),
        help="Output directory for SQL chunks (default: frontend/public/data/)",
    )
    args = parser.parse_args()

    if not args.db.exists():
        logger.error("Database not found: %s", args.db)
        raise SystemExit(1)

    args.output.mkdir(parents=True, exist_ok=True)

    conn = sqlite3.connect(str(args.db))
    conn.execute("PRAGMA query_only=ON")

    try:
        _load_element_symbols(conn)
        export_schema(conn, args.output)
        export_meta(conn, args.output)
        export_stopping(conn, args.output)
        export_cross_sections(conn, args.output)
    finally:
        conn.close()

    logger.info("Done. Chunks written to %s", args.output)


if __name__ == "__main__":
    main()
