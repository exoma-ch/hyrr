"""Output formatting and reporting for HYRR results.

Converts result dataclasses to DataFrames, text summaries, and Excel files.
Uses Polars for DataFrame operations.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

import numpy as np

if TYPE_CHECKING:
    import pandas as pd
    import polars as pl

    from hyrr.models import IsotopeResult, LayerResult, StackResult


def result_to_polars(result: StackResult) -> pl.DataFrame:
    """Convert a StackResult to a Polars DataFrame.

    One row per isotope per layer with columns:
    - layer_index, isotope, Z, A, state, half_life_s
    - production_rate, activity_Bq, saturation_yield_Bq_uA
    - energy_in_MeV, energy_out_MeV, delta_E_MeV, heat_kW

    Args:
        result: Complete simulation result.

    Returns:
        Polars DataFrame with all isotope results.
    """
    import polars as pl

    rows: list[dict[str, object]] = []
    for i, lr in enumerate(result.layer_results):
        sp_sources = ", ".join(sorted(set(lr.stopping_power_sources.values()))) if lr.stopping_power_sources else ""
        for name, iso in lr.isotope_results.items():
            rows.append({
                "layer_index": i,
                "isotope": name,
                "Z": iso.Z,
                "A": iso.A,
                "state": iso.state,
                "half_life_s": iso.half_life_s,
                "production_rate": iso.production_rate,
                "activity_Bq": iso.activity_Bq,
                "saturation_yield_Bq_uA": iso.saturation_yield_Bq_uA,
                "energy_in_MeV": lr.energy_in,
                "energy_out_MeV": lr.energy_out,
                "delta_E_MeV": lr.delta_E_MeV,
                "heat_kW": lr.heat_kW,
                "stopping_power_source": sp_sources,
                "source": iso.source,
                "activity_direct_Bq": iso.activity_direct_Bq,
                "activity_ingrowth_Bq": iso.activity_ingrowth_Bq,
            })

    if not rows:
        return pl.DataFrame()

    return pl.DataFrame(rows)


def layer_result_to_polars(layer_result: LayerResult) -> pl.DataFrame:
    """Convert a single LayerResult to a Polars DataFrame.

    Same columns as result_to_polars but for a single layer (no layer_index).

    Args:
        layer_result: Single layer simulation result.

    Returns:
        Polars DataFrame with isotope results for one layer.
    """
    import polars as pl

    rows: list[dict[str, object]] = []
    for name, iso in layer_result.isotope_results.items():
        rows.append({
            "isotope": name,
            "Z": iso.Z,
            "A": iso.A,
            "state": iso.state,
            "half_life_s": iso.half_life_s,
            "production_rate": iso.production_rate,
            "activity_Bq": iso.activity_Bq,
            "saturation_yield_Bq_uA": iso.saturation_yield_Bq_uA,
        })

    if not rows:
        return pl.DataFrame()

    return pl.DataFrame(rows)


def depth_profile_to_polars(layer_result: LayerResult) -> pl.DataFrame:
    """Convert depth profile to a Polars DataFrame.

    Columns: depth_cm, energy_MeV, dedx_MeV_cm, heat_W_cm3.

    Args:
        layer_result: Single layer simulation result.

    Returns:
        Polars DataFrame with depth profile data.
    """
    import polars as pl

    if not layer_result.depth_profile:
        return pl.DataFrame()

    return pl.DataFrame({
        "depth_cm": [dp.depth_cm for dp in layer_result.depth_profile],
        "energy_MeV": [dp.energy_MeV for dp in layer_result.depth_profile],
        "dedx_MeV_cm": [dp.dedx_MeV_cm for dp in layer_result.depth_profile],
        "heat_W_cm3": [dp.heat_W_cm3 for dp in layer_result.depth_profile],
    })


def activity_timeseries_to_polars(
    isotope_result: IsotopeResult,
) -> pl.DataFrame:
    """Convert activity time series to a Polars DataFrame.

    Columns: time_s, time_hours, activity_Bq, activity_GBq.

    Args:
        isotope_result: Single isotope simulation result.

    Returns:
        Polars DataFrame with activity time series.
    """
    import polars as pl

    return pl.DataFrame({
        "time_s": isotope_result.time_grid_s,
        "time_hours": isotope_result.time_grid_s / 3600.0,
        "activity_Bq": isotope_result.activity_vs_time_Bq,
        "activity_GBq": isotope_result.activity_vs_time_Bq * 1e-9,
    })


def result_summary(result: StackResult) -> str:
    """Generate a text summary similar to ISOTOPIA output format.

    Includes beam parameters, layer info, and isotope production table.

    Args:
        result: Complete simulation result.

    Returns:
        Formatted text string.
    """
    lines: list[str] = []
    stack = result.stack
    beam = stack.beam

    lines.append("=" * 72)
    lines.append("HYRR — Hierarchical Yield and Radionuclide Rates")
    lines.append("=" * 72)
    lines.append("")
    lines.append(f"Projectile:         {beam.projectile}")
    lines.append(f"Beam energy:        {beam.energy_MeV:.3f} MeV")
    lines.append(f"Beam current:       {beam.current_mA:.3f} mA")
    lines.append(f"Irradiation time:   {_format_time(result.irradiation_time_s)}")
    lines.append(f"Cooling time:       {_format_time(result.cooling_time_s)}")
    lines.append(f"Particles/s:        {beam.particles_per_second:.6E}")
    lines.append("")

    for i, lr in enumerate(result.layer_results):
        lines.append(f"--- Layer {i + 1} ---")
        lines.append(f"  E_in:      {lr.energy_in:.3f} MeV")
        lines.append(f"  E_out:     {lr.energy_out:.3f} MeV")
        lines.append(f"  dE:        {lr.delta_E_MeV:.3f} MeV")
        lines.append(f"  Heat:      {lr.heat_kW:.3f} kW")
        if lr.stopping_power_sources:
            sources = set(lr.stopping_power_sources.values())
            lines.append(f"  Stopping:  {', '.join(sorted(sources))}")
        lines.append("")

        if lr.isotope_results:
            header = (
                f"  {'Isotope':<12} {'Source':<8} {'Prod. rate [1/s]':>18} "
                f"{'Activity [GBq]':>16} {'Yield [GBq/mAh]':>17} "
                f"{'Half-life':>20}"
            )
            lines.append(header)
            lines.append("  " + "-" * 95)

            sorted_isos = sorted(
                lr.isotope_results.values(),
                key=lambda r: r.activity_Bq,
                reverse=True,
            )

            for iso in sorted_isos:
                activity_gbq = iso.activity_Bq * 1e-9
                hl = (
                    _format_halflife(iso.half_life_s)
                    if iso.half_life_s
                    else "stable"
                )

                time_h = result.irradiation_time_s / 3600.0
                charge_mah = beam.current_mA * time_h
                yield_val = (
                    activity_gbq / charge_mah if charge_mah > 0 else 0.0
                )

                lines.append(
                    f"  {iso.name:<12} {iso.source:<8} {iso.production_rate:>18.6E} "
                    f"{activity_gbq:>16.4E} {yield_val:>17.6E} {hl:>20}"
                )

                # Show direct/ingrowth breakdown for "both" isotopes
                if iso.source == "both":
                    direct_gbq = iso.activity_direct_Bq * 1e-9
                    ingrowth_gbq = iso.activity_ingrowth_Bq * 1e-9
                    lines.append(
                        f"  {'':12} {'':8} {'  direct:':>18} "
                        f"{direct_gbq:>16.4E}"
                    )
                    lines.append(
                        f"  {'':12} {'':8} {'  ingrowth:':>18} "
                        f"{ingrowth_gbq:>16.4E}"
                    )
            lines.append("")

    lines.append("=" * 72)
    return "\n".join(lines)


def purity_at(
    result: StackResult | LayerResult,
    cooling_time_s: float,
    isotope: str,
) -> float:
    """Calculate radionuclidic purity at a specific cooling time.

    Purity = A_target / A_total at the specified cooling time.

    Args:
        result: StackResult or LayerResult containing isotope data.
        cooling_time_s: Cooling time after end of irradiation [s].
        isotope: Name of target isotope.

    Returns:
        Purity as fraction (0.0 to 1.0).

    Raises:
        ValueError: If the isotope is not found in the results.
    """
    from hyrr.models import StackResult

    if isinstance(result, StackResult):
        all_isotopes: dict[str, IsotopeResult] = {}
        for lr in result.layer_results:
            all_isotopes.update(lr.isotope_results)
        irr_time = result.irradiation_time_s
    else:
        all_isotopes = result.isotope_results
        # For LayerResult, estimate irradiation time from the time grid
        irr_time = _estimate_irradiation_time(result)

    if isotope not in all_isotopes:
        msg = f"Isotope {isotope!r} not found in results"
        raise ValueError(msg)

    target_time = irr_time + cooling_time_s

    target_activity = _interpolate_activity(all_isotopes[isotope], target_time)
    total_activity = sum(
        _interpolate_activity(iso, target_time) for iso in all_isotopes.values()
    )

    if total_activity <= 0:
        return 0.0
    return target_activity / total_activity


def result_to_pandas(result: StackResult) -> pd.DataFrame:
    """Convert a StackResult to a pandas DataFrame.

    One row per isotope per layer with columns:
    - layer_index, isotope, Z, A, state, half_life_s
    - production_rate, activity_Bq, saturation_yield_Bq_uA
    - energy_in_MeV, energy_out_MeV, delta_E_MeV, heat_kW
    - stopping_power_source

    Args:
        result: Complete simulation result.

    Returns:
        pandas DataFrame with all isotope results.
    """
    import pandas as pd

    rows: list[dict[str, object]] = []
    for i, lr in enumerate(result.layer_results):
        sp_sources = ", ".join(sorted(set(lr.stopping_power_sources.values()))) if lr.stopping_power_sources else ""
        for name, iso in lr.isotope_results.items():
            rows.append({
                "layer_index": i,
                "isotope": name,
                "Z": iso.Z,
                "A": iso.A,
                "state": iso.state,
                "half_life_s": iso.half_life_s,
                "production_rate": iso.production_rate,
                "activity_Bq": iso.activity_Bq,
                "saturation_yield_Bq_uA": iso.saturation_yield_Bq_uA,
                "energy_in_MeV": lr.energy_in,
                "energy_out_MeV": lr.energy_out,
                "delta_E_MeV": lr.delta_E_MeV,
                "heat_kW": lr.heat_kW,
                "stopping_power_source": sp_sources,
                "source": iso.source,
                "activity_direct_Bq": iso.activity_direct_Bq,
                "activity_ingrowth_Bq": iso.activity_ingrowth_Bq,
            })

    if not rows:
        return pd.DataFrame()

    return pd.DataFrame(rows)


def result_to_excel(result: StackResult, path: str) -> None:
    """Export results to an Excel file.

    Writes the main isotope table to the file. Requires the ``xlsxwriter``
    package (optional dependency of Polars).

    Args:
        result: Complete simulation result.
        path: Output file path (.xlsx).
    """
    df = result_to_polars(result)
    if not df.is_empty():
        df.write_excel(path)


def result_to_csv_bundle(result: StackResult, path: str) -> None:
    """Export results to a zip file containing CSV files.

    The archive contains:
    - ``summary.csv`` — one row per isotope per layer (same columns as
      :func:`result_to_polars`).
    - ``layer_{i}_depth_profile.csv`` — depth profile for each layer.
    - ``layer_{i}_isotopes.csv`` — isotope details for each layer.

    Uses only the standard library (no Polars dependency).

    Args:
        result: Complete simulation result.
        path: Output file path (.zip).
    """
    import csv
    import io
    import zipfile

    summary_columns = [
        "layer_index", "isotope", "Z", "A", "state", "half_life_s",
        "production_rate", "activity_Bq", "saturation_yield_Bq_uA",
        "energy_in_MeV", "energy_out_MeV", "delta_E_MeV", "heat_kW",
        "stopping_power_source", "source",
        "activity_direct_Bq", "activity_ingrowth_Bq",
    ]
    depth_columns = ["depth_cm", "energy_MeV", "dedx_MeV_cm", "heat_W_cm3"]
    isotope_columns = [
        "isotope", "Z", "A", "state", "half_life_s",
        "production_rate", "activity_Bq", "saturation_yield_Bq_uA",
        "source", "activity_direct_Bq", "activity_ingrowth_Bq",
    ]

    with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as zf:
        # summary.csv
        buf = io.StringIO()
        writer = csv.writer(buf)
        writer.writerow(summary_columns)
        for i, lr in enumerate(result.layer_results):
            sp_src = (
                ", ".join(sorted(set(lr.stopping_power_sources.values())))
                if lr.stopping_power_sources
                else ""
            )
            for _name, iso in lr.isotope_results.items():
                writer.writerow([
                    i, iso.name, iso.Z, iso.A, iso.state, iso.half_life_s,
                    iso.production_rate, iso.activity_Bq,
                    iso.saturation_yield_Bq_uA, lr.energy_in, lr.energy_out,
                    lr.delta_E_MeV, lr.heat_kW, sp_src,
                    iso.source, iso.activity_direct_Bq,
                    iso.activity_ingrowth_Bq,
                ])
        zf.writestr("summary.csv", buf.getvalue())

        # Per-layer files
        for i, lr in enumerate(result.layer_results):
            # depth profile
            buf = io.StringIO()
            writer = csv.writer(buf)
            writer.writerow(depth_columns)
            for dp in lr.depth_profile:
                writer.writerow([
                    dp.depth_cm, dp.energy_MeV, dp.dedx_MeV_cm, dp.heat_W_cm3,
                ])
            zf.writestr(f"layer_{i}_depth_profile.csv", buf.getvalue())

            # isotopes
            buf = io.StringIO()
            writer = csv.writer(buf)
            writer.writerow(isotope_columns)
            for _name, iso in lr.isotope_results.items():
                writer.writerow([
                    iso.name, iso.Z, iso.A, iso.state, iso.half_life_s,
                    iso.production_rate, iso.activity_Bq,
                    iso.saturation_yield_Bq_uA,
                    iso.source, iso.activity_direct_Bq,
                    iso.activity_ingrowth_Bq,
                ])
            zf.writestr(f"layer_{i}_isotopes.csv", buf.getvalue())


# ---------------------------------------------------------------------------
# Private helpers
# ---------------------------------------------------------------------------


def _estimate_irradiation_time(layer_result: LayerResult) -> float:
    """Estimate irradiation time from isotope time grids."""
    for iso in layer_result.isotope_results.values():
        return float(iso.time_grid_s[len(iso.time_grid_s) // 2])
    return 0.0


def _interpolate_activity(iso: IsotopeResult, time_s: float) -> float:
    """Interpolate activity at a specific time."""
    return float(np.interp(time_s, iso.time_grid_s, iso.activity_vs_time_Bq))


def _format_time(seconds: float) -> str:
    """Format seconds into human-readable time string."""
    days = int(seconds // 86400)
    hours = int((seconds % 86400) // 3600)
    mins = int((seconds % 3600) // 60)
    secs = int(seconds % 60)
    parts: list[str] = []
    if days:
        parts.append(f"{days}d")
    if hours or days:
        parts.append(f"{hours}h")
    if mins or hours or days:
        parts.append(f"{mins}m")
    parts.append(f"{secs}s")
    return " ".join(parts)


def _format_halflife(seconds: float) -> str:
    """Format half-life into appropriate units."""
    if seconds < 60:
        return f"{seconds:.1f} s"
    if seconds < 3600:
        return f"{seconds / 60:.1f} min"
    if seconds < 86400:
        return f"{seconds / 3600:.1f} h"
    if seconds < 365.25 * 86400:
        return f"{seconds / 86400:.1f} d"
    return f"{seconds / (365.25 * 86400):.1f} y"
