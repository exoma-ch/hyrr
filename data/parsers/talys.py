"""Parser for TALYS output and input file generator.

Parses TALYS residual production cross-section files (rp*.tot, rp*.L01)
and generates TALYS input files for arbitrary projectile-target combinations.
"""

from __future__ import annotations

import logging
import re
from pathlib import Path

import polars as pl

from parsers.tendl import CrossSectionEntry

logger = logging.getLogger(__name__)

_RP_FILE_RE = re.compile(r"^rp(\d{3})(\d{3})\.(tot|L\d{2})$")


def parse_talys_residual(
    output_dir: Path,
    projectile: str,
    target_Z: int,
    target_A: int,
) -> list[CrossSectionEntry]:
    """Parse TALYS rp*.tot and rp*.L01 files into CrossSectionEntry list."""
    entries: list[CrossSectionEntry] = []

    if not output_dir.is_dir():
        logger.warning("TALYS output directory not found: %s", output_dir)
        return entries

    for path in sorted(output_dir.iterdir()):
        m = _RP_FILE_RE.match(path.name)
        if not m:
            continue

        residual_Z = int(m.group(1))
        residual_A = int(m.group(2))
        suffix = m.group(3)

        if suffix == "tot":
            state = ""
        elif suffix.startswith("L"):
            level = int(suffix[1:])
            state = "m" if level == 1 else f"m{level}"
        else:
            continue

        energies, xs_values = _parse_rp_file(path)
        if not energies:
            continue

        entries.append(CrossSectionEntry(
            projectile=projectile,
            target_Z=target_Z,
            target_A=target_A,
            residual_Z=residual_Z,
            residual_A=residual_A,
            state=state,
            energies_MeV=energies,
            xs_mb=xs_values,
            source="talys",
        ))

    return entries


def _parse_rp_file(path: Path) -> tuple[list[float], list[float]]:
    """Parse a single TALYS rp*.tot or rp*.L* file."""
    energies: list[float] = []
    xs_values: list[float] = []

    try:
        text = path.read_text(encoding="utf-8")
    except OSError:
        logger.warning("Cannot read file: %s", path)
        return energies, xs_values

    for line in text.splitlines():
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        parts = line.split()
        if len(parts) < 2:
            continue
        try:
            energy = float(parts[0])
            xs = float(parts[1])
        except ValueError:
            continue
        energies.append(energy)
        xs_values.append(xs)

    return energies, xs_values


def write_xs_parquet(
    entries: list[CrossSectionEntry],
    output_path: Path,
) -> None:
    """Write cross-section entries to parquet file.

    Output schema matches xs/{proj}_{elem}.parquet:
    target_A (Int32), residual_Z (Int32), residual_A (Int32),
    state (Utf8), energy_MeV (Float64), xs_mb (Float64)
    """
    rows: list[dict] = []
    for entry in entries:
        for energy, xs in zip(entry.energies_MeV, entry.xs_mb):
            rows.append({
                "target_A": entry.target_A,
                "residual_Z": entry.residual_Z,
                "residual_A": entry.residual_A,
                "state": entry.state,
                "energy_MeV": energy,
                "xs_mb": xs,
            })

    if not rows:
        logger.warning("No cross-section data to write")
        return

    df = pl.DataFrame(rows).cast({
        "target_A": pl.Int32,
        "residual_Z": pl.Int32,
        "residual_A": pl.Int32,
    })

    output_path.parent.mkdir(parents=True, exist_ok=True)
    df.write_parquet(output_path)
    logger.info("Wrote %d rows to %s", len(df), output_path)


def generate_talys_input(
    projectile_symbol: str,
    target_Z: int,
    target_A: int,
    target_symbol: str,
    energy_range_MeV: tuple[float, float],
    energy_step_MeV: float = 0.5,
    output_dir: Path = Path("talys_work"),
) -> Path:
    """Generate TALYS input file for a projectile-target combination.

    Returns path to the working directory containing the 'input' file.
    """
    work_dir = output_dir / f"{projectile_symbol}_{target_symbol}{target_A}"
    work_dir.mkdir(parents=True, exist_ok=True)

    # Generate energy grid file
    e_min, e_max = energy_range_MeV
    energies = []
    e = e_min
    while e <= e_max + 1e-9:
        energies.append(e)
        e += energy_step_MeV

    energy_file = work_dir / "energies.dat"
    energy_file.write_text("\n".join(f"{e:.3f}" for e in energies) + "\n")

    proj_keyword = projectile_symbol.lower()

    # maxz/maxa: generous bounds for residual production
    maxz = max(target_Z + 2, target_Z)
    maxa = target_A + 10

    input_lines = [
        f"projectile {proj_keyword}",
        f"element {target_symbol.lower()}",
        f"mass {target_A}",
        "energy energies.dat",
        "rpevap y",
        "rprecoil y",
        "filespecs y",
        "channels y",
        f"maxz {maxz}",
        f"maxa {maxa}",
    ]

    input_file = work_dir / "input"
    input_file.write_text("\n".join(input_lines) + "\n")

    return work_dir


def get_element_symbol_for_z(Z: int) -> str:
    """Get element symbol from Z, using the db module's table."""
    from hyrr.db import ELEMENT_SYMBOLS
    sym = ELEMENT_SYMBOLS.get(Z)
    if sym is None:
        msg = f"Unknown element Z={Z}"
        raise ValueError(msg)
    return sym
