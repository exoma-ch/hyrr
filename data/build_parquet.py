"""Export hyrr.sqlite into Parquet files for both Python and browser consumption.

Splits the master database into per-projectile+element Parquet chunks plus
shared meta/stopping files. These become the SSoT for all runtimes.

Output structure:
    output_dir/
    ├── meta/
    │   ├── abundances.parquet      # natural isotopic abundances
    │   ├── decay.parquet           # decay data (modes, half-lives)
    │   └── elements.parquet        # Z ↔ symbol mapping
    ├── stopping/
    │   └── stopping.parquet        # PSTAR/ASTAR stopping power tables
    └── xs/
        ├── p_Cu.parquet            # proton + Copper cross-sections
        ├── p_Mo.parquet            # proton + Molybdenum
        ├── a_Bi.parquet            # alpha + Bismuth
        └── ...                     # one file per projectile+element

Usage:
    python data/build_parquet.py --db data/hyrr.sqlite --output data/parquet/
"""

from __future__ import annotations

import argparse
import logging
import sqlite3
from pathlib import Path

import polars as pl

logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s %(levelname)s %(name)s: %(message)s",
)
logger = logging.getLogger(__name__)

COMPRESSION = "zstd"


def export_abundances(conn: sqlite3.Connection, output_dir: Path) -> None:
    """Export natural isotopic abundances."""
    rows = conn.execute(
        "SELECT Z, A, symbol, abundance, atomic_mass FROM natural_abundances"
    ).fetchall()
    df = pl.DataFrame(
        {
            "Z": [r[0] for r in rows],
            "A": [r[1] for r in rows],
            "symbol": [r[2] for r in rows],
            "abundance": [r[3] for r in rows],
            "atomic_mass": [r[4] for r in rows],
        },
        schema={
            "Z": pl.Int32,
            "A": pl.Int32,
            "symbol": pl.Utf8,
            "abundance": pl.Float64,
            "atomic_mass": pl.Float64,
        },
    )
    path = output_dir / "meta" / "abundances.parquet"
    path.parent.mkdir(parents=True, exist_ok=True)
    df.write_parquet(path, compression=COMPRESSION)
    logger.info("Wrote %s (%d rows, %d KB)", path, len(df), path.stat().st_size // 1024)


def export_decay(conn: sqlite3.Connection, output_dir: Path) -> None:
    """Export decay data."""
    rows = conn.execute(
        "SELECT Z, A, state, half_life_s, decay_mode, "
        "daughter_Z, daughter_A, daughter_state, branching FROM decay_data"
    ).fetchall()
    df = pl.DataFrame(
        {
            "Z": [r[0] for r in rows],
            "A": [r[1] for r in rows],
            "state": [r[2] for r in rows],
            "half_life_s": [r[3] for r in rows],
            "decay_mode": [r[4] for r in rows],
            "daughter_Z": [r[5] for r in rows],
            "daughter_A": [r[6] for r in rows],
            "daughter_state": [r[7] for r in rows],
            "branching": [r[8] for r in rows],
        },
        schema={
            "Z": pl.Int32,
            "A": pl.Int32,
            "state": pl.Utf8,
            "half_life_s": pl.Float64,
            "decay_mode": pl.Utf8,
            "daughter_Z": pl.Int32,
            "daughter_A": pl.Int32,
            "daughter_state": pl.Utf8,
            "branching": pl.Float64,
        },
    )
    path = output_dir / "meta" / "decay.parquet"
    df.write_parquet(path, compression=COMPRESSION)
    logger.info("Wrote %s (%d rows, %d KB)", path, len(df), path.stat().st_size // 1024)


def export_elements(conn: sqlite3.Connection, output_dir: Path) -> None:
    """Export Z ↔ symbol mapping."""
    # Derive from natural_abundances if elements table is missing or empty
    rows = []
    try:
        rows = conn.execute("SELECT Z, symbol FROM elements").fetchall()
    except sqlite3.OperationalError:
        pass
    if not rows:
        rows = conn.execute(
            "SELECT DISTINCT Z, symbol FROM natural_abundances ORDER BY Z"
        ).fetchall()

    df = pl.DataFrame(
        {"Z": [r[0] for r in rows], "symbol": [r[1] for r in rows]},
        schema={"Z": pl.Int32, "symbol": pl.Utf8},
    )
    path = output_dir / "meta" / "elements.parquet"
    df.write_parquet(path, compression=COMPRESSION)
    logger.info("Wrote %s (%d rows)", path, len(df))


def export_stopping(conn: sqlite3.Connection, output_dir: Path) -> None:
    """Export stopping power tables."""
    rows = conn.execute(
        "SELECT source, target_Z, energy_MeV, dedx FROM stopping_power"
    ).fetchall()
    df = pl.DataFrame(
        {
            "source": [r[0] for r in rows],
            "target_Z": [r[1] for r in rows],
            "energy_MeV": [r[2] for r in rows],
            "dedx": [r[3] for r in rows],
        },
        schema={
            "source": pl.Utf8,
            "target_Z": pl.Int32,
            "energy_MeV": pl.Float64,
            "dedx": pl.Float64,
        },
    )
    path = output_dir / "stopping" / "stopping.parquet"
    path.parent.mkdir(parents=True, exist_ok=True)
    df.write_parquet(path, compression=COMPRESSION)
    logger.info(
        "Wrote %s (%d rows, %d KB)", path, len(df), path.stat().st_size // 1024
    )


def export_cross_sections(conn: sqlite3.Connection, output_dir: Path) -> None:
    """Export cross-sections split by projectile + target element."""
    xs_dir = output_dir / "xs"
    xs_dir.mkdir(parents=True, exist_ok=True)

    # Build Z → symbol mapping
    z_to_sym: dict[int, str] = {}
    try:
        for z, sym in conn.execute("SELECT Z, symbol FROM elements"):
            z_to_sym[z] = sym
    except sqlite3.OperationalError:
        pass
    if not z_to_sym:
        for z, sym in conn.execute(
            "SELECT DISTINCT Z, symbol FROM natural_abundances"
        ):
            z_to_sym[z] = sym

    # Get all distinct projectile + target_Z combinations
    combos = conn.execute(
        "SELECT DISTINCT projectile, target_Z FROM cross_sections "
        "ORDER BY projectile, target_Z"
    ).fetchall()

    total_files = 0
    total_rows = 0

    for projectile, target_Z in combos:
        rows = conn.execute(
            "SELECT target_A, residual_Z, residual_A, state, energy_MeV, xs_mb "
            "FROM cross_sections WHERE projectile=? AND target_Z=?",
            (projectile, target_Z),
        ).fetchall()

        if not rows:
            continue

        df = pl.DataFrame(
            {
                "target_A": [r[0] for r in rows],
                "residual_Z": [r[1] for r in rows],
                "residual_A": [r[2] for r in rows],
                "state": [r[3] for r in rows],
                "energy_MeV": [r[4] for r in rows],
                "xs_mb": [r[5] for r in rows],
            },
            schema={
                "target_A": pl.Int32,
                "residual_Z": pl.Int32,
                "residual_A": pl.Int32,
                "state": pl.Utf8,
                "energy_MeV": pl.Float64,
                "xs_mb": pl.Float64,
            },
        )

        symbol = z_to_sym.get(target_Z, f"Z{target_Z}")
        chunk_path = xs_dir / f"{projectile}_{symbol}.parquet"
        df.write_parquet(chunk_path, compression=COMPRESSION)
        total_files += 1
        total_rows += len(df)

    logger.info(
        "Wrote %d cross-section chunks (%d total rows)", total_files, total_rows
    )


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Export hyrr.sqlite into Parquet files.",
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
        default=Path("data/parquet"),
        help="Output directory for Parquet files (default: data/parquet/)",
    )
    args = parser.parse_args()

    if not args.db.exists():
        logger.error("Database not found: %s", args.db)
        raise SystemExit(1)

    args.output.mkdir(parents=True, exist_ok=True)

    conn = sqlite3.connect(str(args.db))
    conn.execute("PRAGMA query_only=ON")

    try:
        export_abundances(conn, args.output)
        export_decay(conn, args.output)
        export_elements(conn, args.output)
        export_stopping(conn, args.output)
        export_cross_sections(conn, args.output)
    finally:
        conn.close()

    # Print total size
    total_bytes = sum(f.stat().st_size for f in args.output.rglob("*.parquet"))
    logger.info(
        "Done. %d files, %.1f MB total → %s",
        len(list(args.output.rglob("*.parquet"))),
        total_bytes / 1024 / 1024,
        args.output,
    )


if __name__ == "__main__":
    main()
