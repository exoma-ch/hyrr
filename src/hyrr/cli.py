"""Command-line interface for HYRR."""

from __future__ import annotations

import argparse
import sys
from pathlib import Path


def main(argv: list[str] | None = None) -> int:
    """Entry point for the hyrr CLI."""
    parser = argparse.ArgumentParser(
        prog="hyrr",
        description="HYRR — Hierarchical Yield and Radionuclide Rates",
    )
    parser.add_argument(
        "--version", action="version", version=f"%(prog)s {_get_version()}"
    )

    subparsers = parser.add_subparsers(dest="command", help="Available commands")

    # hyrr build-db
    build_parser = subparsers.add_parser(
        "build-db", help="Build SQLite database from raw data"
    )
    build_parser.add_argument(
        "--tendl-path", type=Path, help="Path to isotopia.libs/ directory"
    )
    build_parser.add_argument(
        "--abundance-path", type=Path, help="Path to abundance/ directory"
    )
    build_parser.add_argument(
        "--decay-path", type=Path, help="Path to decay/ directory"
    )
    build_parser.add_argument("--pstar-file", type=Path, help="Path to PSTAR data file")
    build_parser.add_argument("--astar-file", type=Path, help="Path to ASTAR data file")
    build_parser.add_argument(
        "--output", "-o", type=Path, default=Path("data/hyrr.sqlite")
    )
    build_parser.add_argument("--verbose", "-v", action="store_true")

    # hyrr info
    info_parser = subparsers.add_parser("info", help="Show data store statistics")
    info_parser.add_argument(
        "--data-dir", type=Path, default=None, help="Path to parquet data directory"
    )

    # hyrr run
    run_parser = subparsers.add_parser("run", help="Run simulation from TOML input")
    run_parser.add_argument("input_file", type=Path, help="TOML input file")
    run_parser.add_argument(
        "--data-dir", type=Path, default=None, help="Path to parquet data directory"
    )
    run_parser.add_argument(
        "--output-dir", type=Path, default=None, help="Output directory"
    )
    run_parser.add_argument(
        "--format", choices=["text", "excel", "both"], default="text"
    )

    # hyrr compare
    compare_parser = subparsers.add_parser(
        "compare", help="Compare two simulation result files"
    )
    compare_parser.add_argument("file1", type=Path, help="First result JSON file")
    compare_parser.add_argument("file2", type=Path, help="Second result JSON file")
    compare_parser.add_argument(
        "--isotope", type=str, default=None, help="Filter to specific isotope"
    )
    compare_parser.add_argument(
        "--layer", type=int, default=None, help="Filter to specific layer index"
    )

    # hyrr download-data
    dl_parser = subparsers.add_parser(
        "download-data", help="Download pre-built database from GitHub Releases"
    )
    dl_parser.add_argument(
        "--output-dir",
        type=Path,
        default=Path.home() / ".hyrr",
        help="Directory to store the database (default: ~/.hyrr/)",
    )
    dl_parser.add_argument(
        "--force", action="store_true", help="Overwrite existing database"
    )

    # hyrr generate-xs
    genxs_parser = subparsers.add_parser(
        "generate-xs", help="Generate cross-section data using TALYS"
    )
    genxs_parser.add_argument(
        "--projectile", required=True,
        help="Projectile (e.g., C-12, O-16, p)"
    )
    genxs_parser.add_argument(
        "--target", required=True,
        help="Target element symbol (e.g., Mo, Cu)"
    )
    genxs_parser.add_argument(
        "--energy-range", type=str, default="5-50",
        help="Energy range in MeV (e.g., 5-50)"
    )
    genxs_parser.add_argument(
        "--energy-step", type=float, default=0.5,
        help="Energy step in MeV (default: 0.5)"
    )
    genxs_parser.add_argument(
        "--data-dir", type=Path, default=None,
        help="Path to parquet data directory"
    )
    genxs_parser.add_argument(
        "--work-dir", type=Path, default=Path("talys_work"),
        help="Working directory for TALYS runs"
    )
    genxs_parser.add_argument(
        "--force", action="store_true",
        help="Overwrite existing parquet files"
    )

    args = parser.parse_args(argv)

    if args.command is None:
        parser.print_help()
        return 0

    if args.command == "build-db":
        return _cmd_build_db(args)
    elif args.command == "info":
        return _cmd_info(args)
    elif args.command == "run":
        return _cmd_run(args)
    elif args.command == "compare":
        return _cmd_compare(args)
    elif args.command == "download-data":
        return _cmd_download_data(args)
    elif args.command == "generate-xs":
        return _cmd_generate_xs(args)

    return 0


def _get_version() -> str:
    """Get package version."""
    from hyrr import __version__

    return __version__


def _find_data_dir(data_dir_arg: Path | None) -> Path:
    """Find the parquet data directory.

    Search order:
    1. Explicit --data-dir argument
    2. HYRR_DATA environment variable
    3. data/parquet/ relative to package
    4. ~/.hyrr/parquet/
    """
    import os

    if data_dir_arg is not None:
        return data_dir_arg

    env_path = os.environ.get("HYRR_DATA")
    if env_path:
        return Path(env_path)

    # Relative to package
    pkg_dir = Path(__file__).parent.parent.parent / "data" / "parquet"
    if pkg_dir.is_dir():
        return pkg_dir

    # User home
    home_dir = Path.home() / ".hyrr" / "parquet"
    if home_dir.is_dir():
        return home_dir

    return pkg_dir  # Return default even if doesn't exist (will error later)


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
    PRIMARY KEY (projectile, target_Z, target_A, residual_Z, residual_A, state, energy_MeV, source)
);
CREATE INDEX IF NOT EXISTS idx_reaction ON cross_sections(projectile, target_Z, target_A);
CREATE INDEX IF NOT EXISTS idx_product ON cross_sections(residual_Z, residual_A, state);

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
CREATE INDEX IF NOT EXISTS idx_parent ON decay_data(Z, A, state);
CREATE INDEX IF NOT EXISTS idx_daughter ON decay_data(daughter_Z, daughter_A, daughter_state);

CREATE TABLE IF NOT EXISTS elements (
    Z       INTEGER PRIMARY KEY,
    symbol  TEXT    NOT NULL UNIQUE
);
"""


def _cmd_build_db(args: argparse.Namespace) -> int:
    """Build the SQLite database from raw data sources."""
    import sqlite3

    output = args.output
    output.parent.mkdir(parents=True, exist_ok=True)

    conn = sqlite3.connect(str(output))
    conn.executescript(_SCHEMA_SQL)

    total = 0

    if args.tendl_path:
        if not args.tendl_path.exists():
            print(f"Error: TENDL path not found: {args.tendl_path}", file=sys.stderr)
            return 1
        # Lazy import parser
        sys.path.insert(0, str(Path(__file__).parent.parent.parent / "data"))
        from parsers.tendl import (
            insert_cross_sections,
            walk_tendl_directory,
        )

        n = insert_cross_sections(conn, walk_tendl_directory(args.tendl_path))
        if args.verbose:
            print(f"Cross-sections: {n} rows")
        total += n

    if args.abundance_path:
        if not args.abundance_path.exists():
            print(
                f"Error: Abundance path not found: {args.abundance_path}",
                file=sys.stderr,
            )
            return 1
        sys.path.insert(0, str(Path(__file__).parent.parent.parent / "data"))
        from parsers.abundance import (
            insert_abundances,
            walk_abundance_directory,
        )

        n = insert_abundances(conn, walk_abundance_directory(args.abundance_path))
        if args.verbose:
            print(f"Abundances: {n} rows")
        total += n

    if args.decay_path:
        if not args.decay_path.exists():
            print(f"Error: Decay path not found: {args.decay_path}", file=sys.stderr)
            return 1
        sys.path.insert(0, str(Path(__file__).parent.parent.parent / "data"))
        from parsers.decay import (
            insert_decay_data,
            walk_decay_directory,
        )

        n = insert_decay_data(conn, walk_decay_directory(args.decay_path))
        if args.verbose:
            print(f"Decay data: {n} rows")
        total += n

    if args.pstar_file:
        sys.path.insert(0, str(Path(__file__).parent.parent.parent / "data"))
        from parsers.stopping import (
            insert_stopping_power,
            parse_stopping_file,
        )

        n = insert_stopping_power(conn, parse_stopping_file(args.pstar_file, "PSTAR"))
        if args.verbose:
            print(f"PSTAR: {n} rows")
        total += n

    if args.astar_file:
        sys.path.insert(0, str(Path(__file__).parent.parent.parent / "data"))
        from parsers.stopping import (
            insert_stopping_power,
            parse_stopping_file,
        )

        n = insert_stopping_power(conn, parse_stopping_file(args.astar_file, "ASTAR"))
        if args.verbose:
            print(f"ASTAR: {n} rows")
        total += n

    conn.close()
    print(f"Database built: {output} ({total} total rows)")
    return 0


def _cmd_info(args: argparse.Namespace) -> int:
    """Show data store statistics."""
    import polars as pl

    data_dir = _find_data_dir(args.data_dir)

    if not data_dir.is_dir():
        print(f"Data directory not found: {data_dir}", file=sys.stderr)
        print("Set HYRR_DATA or place data in data/parquet/.", file=sys.stderr)
        return 1

    print(f"Data directory: {data_dir}")

    # Calculate total size
    total_bytes = sum(f.stat().st_size for f in data_dir.rglob("*.parquet"))
    print(f"Total size: {total_bytes / 1e6:.1f} MB")
    print()

    tables = {
        "meta/abundances.parquet": "Natural abundances",
        "meta/decay.parquet": "Decay data",
        "meta/elements.parquet": "Elements",
        "stopping/stopping.parquet": "Stopping power",
    }

    for path, label in tables.items():
        full = data_dir / path
        if full.exists():
            df = pl.read_parquet(full)
            print(f"  {label}: {len(df):,} rows")
        else:
            print(f"  {label}: file not found")

    # Cross-section file summary
    xs_dir = data_dir / "xs"
    if xs_dir.is_dir():
        xs_files = list(xs_dir.glob("*.parquet"))
        print(f"\n  Cross-section files: {len(xs_files)}")

        # Count by projectile
        projectiles: dict[str, int] = {}
        for f in xs_files:
            proj = f.stem.split("_")[0]
            projectiles[proj] = projectiles.get(proj, 0) + 1
        for proj, count in sorted(projectiles.items()):
            print(f"    {proj}: {count} targets")

    return 0


def _toml_to_config(toml_data: dict) -> dict:
    """Convert TOML data structure to config dict for config_to_stack."""
    config: dict = {
        "beam": toml_data["beam"],
        "irradiation_s": toml_data["irradiation_s"],
        "cooling_s": toml_data["cooling_s"],
    }

    layers = []
    for layer_toml in toml_data["layers"]:
        layer: dict = {"material": layer_toml["material"]}

        if "thickness_cm" in layer_toml:
            layer["thickness_cm"] = layer_toml["thickness_cm"]
        elif "energy_out_MeV" in layer_toml:
            layer["energy_out_MeV"] = layer_toml["energy_out_MeV"]
        elif "areal_density_g_cm2" in layer_toml:
            layer["areal_density_g_cm2"] = layer_toml["areal_density_g_cm2"]

        if "enrichment" in layer_toml:
            # TOML integer keys -> string keys for JSON compatibility
            enrichment = {}
            for symbol, isotopes in layer_toml["enrichment"].items():
                enrichment[symbol] = {str(a): frac for a, frac in isotopes.items()}
            layer["enrichment"] = enrichment

        if "is_monitor" in layer_toml:
            layer["is_monitor"] = layer_toml["is_monitor"]

        layers.append(layer)

    config["layers"] = layers
    return config


def _cmd_run(args: argparse.Namespace) -> int:
    """Run simulation from TOML input file."""
    import tomllib

    if not args.input_file.exists():
        print(f"Input file not found: {args.input_file}", file=sys.stderr)
        return 1

    data_dir = _find_data_dir(args.data_dir)
    if not data_dir.is_dir():
        print(f"Data directory not found: {data_dir}", file=sys.stderr)
        return 1

    # Parse TOML
    with open(args.input_file, "rb") as f:
        toml_config = tomllib.load(f)

    # Convert TOML to the config dict format expected by config_to_stack
    config = _toml_to_config(toml_config)

    # Open data store and run simulation
    from hyrr.api import config_to_stack
    from hyrr.compute import compute_stack
    from hyrr.db import DataStore
    from hyrr.output import result_summary, result_to_excel

    db = DataStore(data_dir)
    stack = config_to_stack(db, config)
    result = compute_stack(db, stack)

    # Output results
    summary = result_summary(result)

    if args.output_dir:
        args.output_dir.mkdir(parents=True, exist_ok=True)

        if args.format in ("text", "both"):
            text_path = args.output_dir / "summary.txt"
            text_path.write_text(summary)
            print(f"Summary written to {text_path}")

        if args.format in ("excel", "both"):
            excel_path = args.output_dir / "results.xlsx"
            result_to_excel(result, str(excel_path))
            print(f"Excel written to {excel_path}")

        if args.format == "text":
            print(summary)
    else:
        print(summary)

    return 0


def _extract_isotopes(data: dict, layer_filter: int | None = None) -> dict[str, dict]:
    """Extract isotope dict from result JSON, keyed by name."""
    result: dict[str, dict] = {}
    layers = data.get("layers", [])
    for layer_data in layers:
        if layer_filter is not None and layer_data.get("layer_index") != layer_filter:
            continue
        for iso in layer_data.get("isotopes", []):
            name = iso.get("name", "")
            if name:
                result[name] = iso
    return result


def _pct_diff(a: float, b: float) -> float:
    """Percentage difference: (b - a) / a * 100. Returns 0 if a is zero."""
    if a == 0:
        return 0.0 if b == 0 else float("inf")
    return (b - a) / a * 100.0


def _cmd_compare(args: argparse.Namespace) -> int:
    """Compare two simulation result JSON files."""
    import json

    for path in (args.file1, args.file2):
        if not path.exists():
            print(f"File not found: {path}", file=sys.stderr)
            return 1

    with open(args.file1) as f:
        data1 = json.load(f)
    with open(args.file2) as f:
        data2 = json.load(f)

    # Extract isotope data from both results
    isos1 = _extract_isotopes(data1, args.layer)
    isos2 = _extract_isotopes(data2, args.layer)

    if args.isotope:
        isos1 = {k: v for k, v in isos1.items() if k == args.isotope}
        isos2 = {k: v for k, v in isos2.items() if k == args.isotope}

    all_names = sorted(set(isos1) | set(isos2))
    if not all_names:
        print("No isotopes to compare.")
        return 0

    # Print comparison table
    header = (
        f"{'Isotope':<14} {'Activity_1':>14} {'Activity_2':>14} {'Diff %':>10}"
        f" {'Yield_1':>14} {'Yield_2':>14} {'Diff %':>10}"
    )
    print(header)
    print("-" * len(header))

    for name in all_names:
        d1 = isos1.get(name, {})
        d2 = isos2.get(name, {})

        a1 = d1.get("activity_Bq", 0.0)
        a2 = d2.get("activity_Bq", 0.0)
        y1 = d1.get("saturation_yield_Bq_uA", 0.0)
        y2 = d2.get("saturation_yield_Bq_uA", 0.0)

        a_diff = _pct_diff(a1, a2)
        y_diff = _pct_diff(y1, y2)

        print(
            f"{name:<14} {a1:>14.4E} {a2:>14.4E} {a_diff:>9.2f}% "
            f"{y1:>14.4E} {y2:>14.4E} {y_diff:>9.2f}%"
        )

    return 0


def _cmd_download_data(args: argparse.Namespace) -> int:
    """Download pre-built parquet data from GitHub Releases."""
    import subprocess
    import urllib.request

    output_dir = args.output_dir
    parquet_dir = output_dir / "parquet"

    if parquet_dir.is_dir() and not args.force:
        print(f"Data already exists: {parquet_dir}")
        print("Use --force to overwrite.")
        return 0

    output_dir.mkdir(parents=True, exist_ok=True)

    # Latest data release URL
    release_url = (
        "https://github.com/MorePET/hyrr/releases/latest/download/parquet.tar.zst"
    )

    print(f"Downloading data from {release_url} ...")
    print("(This may take a moment for the ~60 MB file)")

    try:
        archive_path = output_dir / "parquet.tar.zst"
        urllib.request.urlretrieve(release_url, str(archive_path))

        # Decompress and extract
        try:
            subprocess.run(
                ["tar", "--zstd", "-xf", str(archive_path), "-C", str(output_dir)],
                check=True,
            )
            archive_path.unlink()
            print(f"Data extracted to: {parquet_dir}")
        except (FileNotFoundError, subprocess.CalledProcessError):
            print(f"Downloaded archive: {archive_path}")
            print("Extract manually with: tar --zstd -xf parquet.tar.zst")
    except Exception as e:
        print(f"Download failed: {e}", file=sys.stderr)
        print(
            "You can build the data manually with 'hyrr build-db'.",
            file=sys.stderr,
        )
        return 1

    return 0


def _cmd_generate_xs(args: argparse.Namespace) -> int:
    """Generate cross-section data using TALYS."""
    import shutil
    import subprocess

    from hyrr.projectile import resolve_projectile

    # Validate projectile
    try:
        proj = resolve_projectile(args.projectile)
    except ValueError as e:
        print(f"Error: {e}", file=sys.stderr)
        return 1

    # Check TALYS is installed
    if shutil.which("talys") is None:
        print("Error: TALYS not found in PATH. Install TALYS first.", file=sys.stderr)
        return 1

    # Resolve target
    from hyrr.db import ELEMENT_SYMBOLS, _SYMBOL_TO_Z

    target_symbol = args.target
    target_Z = _SYMBOL_TO_Z.get(target_symbol)
    if target_Z is None:
        print(f"Error: Unknown target element: {target_symbol}", file=sys.stderr)
        return 1

    # Parse energy range
    try:
        parts = args.energy_range.split("-")
        e_min, e_max = float(parts[0]), float(parts[1])
    except (ValueError, IndexError):
        print(f"Error: Invalid energy range: {args.energy_range}. Use format: MIN-MAX", file=sys.stderr)
        return 1

    # Find data directory for output
    data_dir = _find_data_dir(args.data_dir)

    # Get natural isotopes for the target element
    from hyrr.db import DataStore

    try:
        db = DataStore(data_dir)
    except FileNotFoundError:
        print(f"Data directory not found: {data_dir}", file=sys.stderr)
        print("Set HYRR_DATA or use --data-dir", file=sys.stderr)
        return 1

    abundances = db.get_natural_abundances(target_Z)
    if not abundances:
        print(f"No natural abundance data for {target_symbol} (Z={target_Z})", file=sys.stderr)
        return 1

    # Check if output already exists
    xs_dir = data_dir / "xs"
    output_file = xs_dir / f"{args.projectile}_{target_symbol}.parquet"
    if output_file.exists() and not args.force:
        print(f"Cross-section file already exists: {output_file}")
        print("Use --force to overwrite.")
        return 0

    print(f"Generating cross-sections for {args.projectile} + {target_symbol}")
    print(f"Energy range: {e_min}-{e_max} MeV, step: {args.energy_step} MeV")
    print(f"Target isotopes: {', '.join(f'{target_symbol}-{A}' for A in sorted(abundances))}")

    # Import TALYS parser
    sys.path.insert(0, str(Path(__file__).parent.parent.parent / "data"))
    from parsers.talys import generate_talys_input, parse_talys_residual, write_xs_parquet

    all_entries = []

    for target_A in sorted(abundances):
        # Determine projectile symbol for TALYS input
        if proj.Z <= 2:
            talys_proj_symbol = args.projectile
        else:
            talys_proj_symbol = args.projectile.split("-")[0]

        print(f"  Running TALYS: {args.projectile} + {target_symbol}-{target_A} ...")

        work_dir = generate_talys_input(
            projectile_symbol=talys_proj_symbol,
            target_Z=target_Z,
            target_A=target_A,
            target_symbol=target_symbol,
            energy_range_MeV=(e_min, e_max),
            energy_step_MeV=args.energy_step,
            output_dir=args.work_dir,
        )

        try:
            result = subprocess.run(
                ["talys"],
                cwd=work_dir,
                stdin=subprocess.DEVNULL,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                timeout=3600,
            )
            if result.returncode != 0:
                print(f"    TALYS failed for {target_symbol}-{target_A} (exit code {result.returncode})", file=sys.stderr)
                if result.stderr:
                    print(f"    stderr: {result.stderr.decode()[:200]}", file=sys.stderr)
                continue
        except subprocess.TimeoutExpired:
            print(f"    TALYS timed out for {target_symbol}-{target_A}", file=sys.stderr)
            continue
        except FileNotFoundError:
            print("Error: TALYS executable not found", file=sys.stderr)
            return 1

        entries = parse_talys_residual(work_dir, args.projectile, target_Z, target_A)
        print(f"    Parsed {len(entries)} residual channels")
        all_entries.extend(entries)

    if not all_entries:
        print("No cross-section data generated.", file=sys.stderr)
        return 1

    write_xs_parquet(all_entries, output_file)
    print(f"Wrote {output_file}")

    return 0
