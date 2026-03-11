"""Plotting utilities for HYRR results.

Energy scans, depth profiles, cooling curves, purity plots.
Supports matplotlib (publication) and plotly (interactive).
"""

from __future__ import annotations

from typing import TYPE_CHECKING, Any, Literal

import numpy as np

if TYPE_CHECKING:
    from hyrr.models import IsotopeResult, LayerResult

Backend = Literal["matplotlib", "plotly"]


def plot_depth_profile(
    layer_result: LayerResult,
    quantity: Literal["heat", "energy", "dedx"] = "heat",
    backend: Backend = "matplotlib",
    **kwargs: Any,
) -> Any:
    """Plot depth profile for a single layer.

    Args:
        layer_result: Results for one layer.
        quantity: What to plot on y-axis.
        backend: "matplotlib" or "plotly".
        **kwargs: Additional arguments passed to plotting function.

    Returns:
        matplotlib Figure or plotly Figure.
    """
    depths = [dp.depth_cm for dp in layer_result.depth_profile]

    if quantity == "heat":
        values = [dp.heat_W_cm3 for dp in layer_result.depth_profile]
        ylabel = "Heat deposition [W/cm\u00b3]"
    elif quantity == "energy":
        values = [dp.energy_MeV for dp in layer_result.depth_profile]
        ylabel = "Energy [MeV]"
    elif quantity == "dedx":
        values = [dp.dedx_MeV_cm for dp in layer_result.depth_profile]
        ylabel = "dE/dx [MeV/cm]"
    else:
        raise ValueError(f"Unknown quantity: {quantity}")

    title = kwargs.pop("title", f"Depth Profile \u2014 {quantity}")

    if backend == "matplotlib":
        return _mpl_line_plot(
            depths,
            values,
            xlabel="Depth [cm]",
            ylabel=ylabel,
            title=title,
            **kwargs,
        )
    return _plotly_line_plot(
        depths,
        values,
        xlabel="Depth [cm]",
        ylabel=ylabel,
        title=title,
    )


def plot_activity_vs_time(
    isotope_results: dict[str, IsotopeResult] | list[IsotopeResult],
    top_n: int | None = 10,
    min_activity_Bq: float = 0.0,
    backend: Backend = "matplotlib",
    **kwargs: Any,
) -> Any:
    """Plot activity vs time for multiple isotopes.

    Args:
        isotope_results: Dict or list of IsotopeResult.
        top_n: Show only the top N isotopes by peak activity (None = all).
        min_activity_Bq: Filter isotopes below this threshold.
        backend: "matplotlib" or "plotly".
        **kwargs: Additional arguments forwarded to the backend.

    Returns:
        matplotlib Figure or plotly Figure.
    """
    if isinstance(isotope_results, dict):
        results = list(isotope_results.values())
    else:
        results = list(isotope_results)

    # Filter and sort by peak activity
    filtered = [
        r for r in results if np.max(r.activity_vs_time_Bq) > min_activity_Bq
    ]
    filtered.sort(key=lambda r: float(np.max(r.activity_vs_time_Bq)), reverse=True)

    if top_n is not None:
        filtered = filtered[:top_n]

    traces = [
        (r.time_grid_s / 3600, r.activity_vs_time_Bq * 1e-9, r.name)
        for r in filtered
    ]
    title = kwargs.pop("title", "Activity vs Time")

    if backend == "matplotlib":
        return _mpl_multi_line_plot(
            traces,
            xlabel="Time [hours]",
            ylabel="Activity [GBq]",
            title=title,
            **kwargs,
        )
    return _plotly_multi_line_plot(
        traces,
        xlabel="Time [hours]",
        ylabel="Activity [GBq]",
        title=title,
    )


def plot_cooling_curve(
    isotope_results: dict[str, IsotopeResult] | list[IsotopeResult],
    irradiation_time_s: float,
    top_n: int | None = 10,
    backend: Backend = "matplotlib",
    **kwargs: Any,
) -> Any:
    """Plot activity during cooling phase only.

    Args:
        isotope_results: Dict or list of IsotopeResult.
        irradiation_time_s: Irradiation time to identify cooling start.
        top_n: Show only top N isotopes.
        backend: "matplotlib" or "plotly".
        **kwargs: Additional arguments forwarded to the backend.

    Returns:
        Figure object.
    """
    if isinstance(isotope_results, dict):
        results = list(isotope_results.values())
    else:
        results = list(isotope_results)

    # Filter to cooling phase and sort by EOI activity
    traces: list[tuple[Any, Any, str]] = []
    for r in results:
        mask = r.time_grid_s >= irradiation_time_s
        if not np.any(mask):
            continue
        cooling_t = (r.time_grid_s[mask] - irradiation_time_s) / 3600
        cooling_a = r.activity_vs_time_Bq[mask] * 1e-9
        if np.max(cooling_a) > 0:
            traces.append((cooling_t, cooling_a, r.name))

    traces.sort(key=lambda t: float(np.max(t[1])), reverse=True)
    if top_n is not None:
        traces = traces[:top_n]

    title = kwargs.pop("title", "Cooling Curve")

    if backend == "matplotlib":
        return _mpl_multi_line_plot(
            traces,
            xlabel="Cooling time [hours]",
            ylabel="Activity [GBq]",
            title=title,
            log_y=True,
            **kwargs,
        )
    return _plotly_multi_line_plot(
        traces,
        xlabel="Cooling time [hours]",
        ylabel="Activity [GBq]",
        title=title,
        log_y=True,
    )


def plot_purity_vs_cooling(
    isotope_results: dict[str, IsotopeResult],
    target_isotope: str,
    irradiation_time_s: float,
    backend: Backend = "matplotlib",
    **kwargs: Any,
) -> Any:
    """Plot radionuclidic purity of target isotope vs cooling time.

    Purity = A_target / A_total at each cooling time point.

    Args:
        isotope_results: Dict of isotope name to IsotopeResult.
        target_isotope: Name of the desired isotope (e.g., "Tc-99m").
        irradiation_time_s: Irradiation time in seconds.
        backend: "matplotlib" or "plotly".
        **kwargs: Additional arguments forwarded to the backend.

    Returns:
        Figure object.
    """
    target = isotope_results.get(target_isotope)
    if target is None:
        raise ValueError(f"Isotope {target_isotope!r} not found in results")

    mask = target.time_grid_s >= irradiation_time_s
    cooling_t = (target.time_grid_s[mask] - irradiation_time_s) / 3600

    # Total activity at each time point (sum all isotopes on same time grid)
    total_activity = np.zeros_like(cooling_t)
    for r in isotope_results.values():
        r_mask = r.time_grid_s >= irradiation_time_s
        if np.any(r_mask):
            r_cooling = r.activity_vs_time_Bq[r_mask]
            if len(r_cooling) == len(total_activity):
                total_activity += r_cooling

    target_activity = target.activity_vs_time_Bq[mask]
    purity = np.where(
        total_activity > 0, target_activity / total_activity * 100, 0.0
    )

    title = kwargs.pop("title", f"Radionuclidic Purity \u2014 {target_isotope}")

    if backend == "matplotlib":
        return _mpl_line_plot(
            cooling_t.tolist(),
            purity.tolist(),
            xlabel="Cooling time [hours]",
            ylabel=f"Purity of {target_isotope} [%]",
            title=title,
            **kwargs,
        )
    return _plotly_line_plot(
        cooling_t.tolist(),
        purity.tolist(),
        xlabel="Cooling time [hours]",
        ylabel=f"Purity of {target_isotope} [%]",
        title=title,
    )


def plot_energy_scan(
    energies_MeV: list[float],
    activities: dict[str, list[float]],
    backend: Backend = "matplotlib",
    **kwargs: Any,
) -> Any:
    """Plot activity vs beam energy for multiple isotopes.

    Args:
        energies_MeV: List of beam energies scanned.
        activities: Dict of isotope name to list of activities [GBq] at each energy.
        backend: "matplotlib" or "plotly".
        **kwargs: Additional arguments forwarded to the backend.

    Returns:
        Figure object.
    """
    traces = [
        (energies_MeV, acts, name) for name, acts in activities.items()
    ]
    title = kwargs.pop("title", "Energy Scan")

    if backend == "matplotlib":
        return _mpl_multi_line_plot(
            traces,
            xlabel="Beam energy [MeV]",
            ylabel="Activity [GBq]",
            title=title,
            **kwargs,
        )
    return _plotly_multi_line_plot(
        traces,
        xlabel="Beam energy [MeV]",
        ylabel="Activity [GBq]",
        title=title,
    )


# --- Backend implementations ---


def _mpl_line_plot(
    x: list[float],
    y: list[float],
    xlabel: str,
    ylabel: str,
    title: str,
    log_y: bool = False,
    **kwargs: Any,
) -> Any:
    """Create a single-line matplotlib plot."""
    import matplotlib
    matplotlib.use("Agg")
    import matplotlib.pyplot as plt

    fig, ax = plt.subplots(figsize=kwargs.get("figsize", (8, 5)))
    ax.plot(x, y)
    ax.set_xlabel(xlabel)
    ax.set_ylabel(ylabel)
    ax.set_title(title)
    if log_y:
        ax.set_yscale("log")
    fig.tight_layout()
    return fig


def _mpl_multi_line_plot(
    traces: list[tuple[Any, Any, str]],
    xlabel: str,
    ylabel: str,
    title: str,
    log_y: bool = False,
    **kwargs: Any,
) -> Any:
    """Create a multi-line matplotlib plot."""
    import matplotlib
    matplotlib.use("Agg")
    import matplotlib.pyplot as plt

    fig, ax = plt.subplots(figsize=kwargs.get("figsize", (10, 6)))
    for x, y, label in traces:
        ax.plot(x, y, label=label)
    ax.set_xlabel(xlabel)
    ax.set_ylabel(ylabel)
    ax.set_title(title)
    if log_y:
        ax.set_yscale("log")
    if traces:
        ax.legend(loc="best")
    fig.tight_layout()
    return fig


def _plotly_line_plot(
    x: list[float],
    y: list[float],
    xlabel: str,
    ylabel: str,
    title: str,
    **kwargs: Any,
) -> Any:
    """Create a single-line plotly plot."""
    import plotly.graph_objects as go

    fig = go.Figure()
    fig.add_trace(go.Scatter(x=x, y=y, mode="lines"))
    fig.update_layout(
        title=title,
        xaxis_title=xlabel,
        yaxis_title=ylabel,
    )
    return fig


def _plotly_multi_line_plot(
    traces: list[tuple[Any, Any, str]],
    xlabel: str,
    ylabel: str,
    title: str,
    log_y: bool = False,
    **kwargs: Any,
) -> Any:
    """Create a multi-line plotly plot."""
    import plotly.graph_objects as go

    fig = go.Figure()
    for x, y, label in traces:
        fig.add_trace(go.Scatter(x=x, y=y, mode="lines", name=label))
    fig.update_layout(
        title=title,
        xaxis_title=xlabel,
        yaxis_title=ylabel,
    )
    if log_y:
        fig.update_yaxes(type="log")
    return fig
