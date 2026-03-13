"""Plotting utilities for HYRR results.

Energy scans, depth profiles, cooling curves, purity plots.
Supports matplotlib (publication) and plotly (interactive).
"""

from __future__ import annotations

from typing import TYPE_CHECKING, Any, Literal

import numpy as np

if TYPE_CHECKING:
    from hyrr.models import BeamProfile, IsotopeResult, LayerResult

Backend = Literal["matplotlib", "plotly"]


def plot_depth_profile(
    layer_result: LayerResult,
    quantity: Literal["heat", "energy", "dedx"] = "heat",
    backend: Backend = "plotly",
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
    backend: Backend = "plotly",
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
    backend: Backend = "plotly",
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
    backend: Backend = "plotly",
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
    backend: Backend = "plotly",
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


# ---------------------------------------------------------------------------
# 1. Straggling diagnostics
# ---------------------------------------------------------------------------


def plot_straggling_vs_depth(
    layer_results: list[LayerResult],
    backend: Backend = "plotly",
    **kwargs: Any,
) -> Any:
    """Plot sigma_E [MeV] vs depth [mm] across all layers.

    Reads sigma_E_MeV from each DepthPoint. Concatenates across layers
    with cumulative depth offset.

    Args:
        layer_results: List of LayerResult from a stack simulation.
        backend: "matplotlib" or "plotly".
        **kwargs: Additional arguments forwarded to the backend.

    Returns:
        matplotlib Figure or plotly Figure.
    """
    depths_mm: list[float] = []
    sigmas: list[float] = []
    cumulative_depth_cm = 0.0

    for lr in layer_results:
        for dp in lr.depth_profile:
            depths_mm.append((cumulative_depth_cm + dp.depth_cm) * 10.0)
            sigmas.append(dp.sigma_E_MeV)
        if lr.depth_profile:
            cumulative_depth_cm += lr.depth_profile[-1].depth_cm

    title = kwargs.pop("title", "Energy Straggling vs Depth")

    if backend == "matplotlib":
        return _mpl_line_plot(
            depths_mm,
            sigmas,
            xlabel="Depth [mm]",
            ylabel=r"$\sigma_E$ [MeV]",
            title=title,
            **kwargs,
        )
    return _plotly_line_plot(
        depths_mm,
        sigmas,
        xlabel="Depth [mm]",
        ylabel="\u03c3_E [MeV]",
        title=title,
    )


def plot_energy_band(
    layer_results: list[LayerResult],
    n_sigma: float = 2.0,
    backend: Backend = "plotly",
    **kwargs: Any,
) -> Any:
    """Plot E(z) +/- n_sigma * sigma_E(z) as a shaded band.

    Shows mean energy line with filled region for energy spread.
    Layer boundaries as vertical dashed lines.

    Args:
        layer_results: List of LayerResult from a stack simulation.
        n_sigma: Number of standard deviations for the band width.
        backend: "matplotlib" or "plotly".
        **kwargs: Additional arguments forwarded to the backend.

    Returns:
        matplotlib Figure or plotly Figure.
    """
    depths_mm: list[float] = []
    energies: list[float] = []
    sigmas: list[float] = []
    boundaries_mm: list[float] = []
    cumulative_depth_cm = 0.0

    for lr in layer_results:
        for dp in lr.depth_profile:
            depths_mm.append((cumulative_depth_cm + dp.depth_cm) * 10.0)
            energies.append(dp.energy_MeV)
            sigmas.append(dp.sigma_E_MeV)
        if lr.depth_profile:
            cumulative_depth_cm += lr.depth_profile[-1].depth_cm
            boundaries_mm.append(cumulative_depth_cm * 10.0)

    x = np.array(depths_mm)
    y = np.array(energies)
    s = np.array(sigmas)
    y_upper = y + n_sigma * s
    y_lower = y - n_sigma * s

    title = kwargs.pop("title", f"Energy Band (\u00b1{n_sigma}\u03c3)")

    if backend == "matplotlib":
        return _mpl_band_plot(
            x.tolist(),
            y.tolist(),
            y_lower.tolist(),
            y_upper.tolist(),
            vlines=boundaries_mm[:-1] if boundaries_mm else [],
            xlabel="Depth [mm]",
            ylabel="Energy [MeV]",
            title=title,
            band_label=f"\u00b1{n_sigma}\u03c3",
            **kwargs,
        )
    return _plotly_band_plot(
        x.tolist(),
        y.tolist(),
        y_lower.tolist(),
        y_upper.tolist(),
        vlines=boundaries_mm[:-1] if boundaries_mm else [],
        xlabel="Depth [mm]",
        ylabel="Energy [MeV]",
        title=title,
        band_label=f"\u00b1{n_sigma}\u03c3",
    )


def plot_xs_convolution(
    xs_energies_MeV: np.ndarray,
    xs_mb: np.ndarray,
    sigma_E_MeV: float,
    reaction_label: str = "",
    backend: Backend = "plotly",
    **kwargs: Any,
) -> Any:
    """Compare point cross-section vs Gauss-convolved cross-section.

    Two lines: original sigma(E) and <sigma>(E) convolved with Gaussian of
    width sigma_E_MeV.  Uses the _gauss_hermite_convolved_xs function from
    production.py.

    Args:
        xs_energies_MeV: Energy grid [MeV].
        xs_mb: Cross-section values [mb] at each energy.
        sigma_E_MeV: Gaussian energy spread [MeV] for convolution.
        reaction_label: Optional label for the reaction (e.g., "Zn-68(p,n)Ga-68").
        backend: "matplotlib" or "plotly".
        **kwargs: Additional arguments forwarded to the backend.

    Returns:
        matplotlib Figure or plotly Figure.
    """
    from hyrr.production import _gauss_hermite_convolved_xs

    def xs_interp_fn(E: np.ndarray) -> np.ndarray:
        return np.interp(E, xs_energies_MeV, xs_mb, left=0.0, right=0.0)

    sigma_arr = np.full_like(xs_energies_MeV, sigma_E_MeV)
    xs_convolved = _gauss_hermite_convolved_xs(
        xs_interp_fn, xs_energies_MeV, sigma_arr
    )

    label_point = reaction_label or "\u03c3(E)"
    label_conv = f"\u27e8\u03c3\u27e9(E), \u03c3_E={sigma_E_MeV:.2f} MeV"
    traces = [
        (xs_energies_MeV.tolist(), xs_mb.tolist(), label_point),
        (xs_energies_MeV.tolist(), xs_convolved.tolist(), label_conv),
    ]

    title = kwargs.pop("title", f"Cross-Section Convolution{' \u2014 ' + reaction_label if reaction_label else ''}")

    if backend == "matplotlib":
        return _mpl_multi_line_plot(
            traces,
            xlabel="Energy [MeV]",
            ylabel="Cross-section [mb]",
            title=title,
            **kwargs,
        )
    return _plotly_multi_line_plot(
        traces,
        xlabel="Energy [MeV]",
        ylabel="Cross-section [mb]",
        title=title,
    )


# ---------------------------------------------------------------------------
# 2. Production depth diagnostics
# ---------------------------------------------------------------------------


def plot_production_vs_depth(
    layer_result: LayerResult,
    top_n: int = 5,
    backend: Backend = "plotly",
    **kwargs: Any,
) -> Any:
    """Plot local production rate vs depth for top isotopes.

    Uses production_rates dict from each DepthPoint.  If production_rates
    are empty (common for performance when depth-resolved rates are not
    stored), only isotopes with non-zero entries will appear.

    X-axis: depth [mm], Y-axis: production rate [s^-1/cm].
    Multiple lines for different isotopes.

    Args:
        layer_result: Results for one layer.
        top_n: Number of top isotopes to display (by integrated rate).
        backend: "matplotlib" or "plotly".
        **kwargs: Additional arguments forwarded to the backend.

    Returns:
        matplotlib Figure or plotly Figure.
    """
    depths_mm = [dp.depth_cm * 10.0 for dp in layer_result.depth_profile]

    # Collect all isotope names
    all_isotopes: set[str] = set()
    for dp in layer_result.depth_profile:
        all_isotopes.update(dp.production_rates.keys())

    if not all_isotopes:
        # No production rate data — return empty plot
        title = kwargs.pop("title", "Production Rate vs Depth (no data)")
        if backend == "matplotlib":
            return _mpl_line_plot([], [], "Depth [mm]", "Production rate [s\u207b\u00b9/cm]", title, **kwargs)
        return _plotly_line_plot([], [], "Depth [mm]", "Production rate [s\u207b\u00b9/cm]", title)

    # Build rate arrays and rank by integrated rate
    isotope_rates: dict[str, list[float]] = {}
    for iso in all_isotopes:
        isotope_rates[iso] = [dp.production_rates.get(iso, 0.0) for dp in layer_result.depth_profile]

    ranked = sorted(
        isotope_rates.items(),
        key=lambda kv: sum(kv[1]),
        reverse=True,
    )[:top_n]

    traces = [(depths_mm, rates, name) for name, rates in ranked]
    title = kwargs.pop("title", "Production Rate vs Depth")

    if backend == "matplotlib":
        return _mpl_multi_line_plot(
            traces,
            xlabel="Depth [mm]",
            ylabel=r"Production rate [s$^{-1}$/cm]",
            title=title,
            **kwargs,
        )
    return _plotly_multi_line_plot(
        traces,
        xlabel="Depth [mm]",
        ylabel="Production rate [s\u207b\u00b9/cm]",
        title=title,
    )


def plot_cumulative_yield(
    layer_result: LayerResult,
    top_n: int = 5,
    backend: Backend = "plotly",
    **kwargs: Any,
) -> Any:
    """Plot cumulative yield fraction vs depth.

    For each isotope, integrates production rate from entrance to depth z,
    normalized to total. X: depth [mm], Y: cumulative fraction [0-1].

    Args:
        layer_result: Results for one layer.
        top_n: Number of top isotopes to display.
        backend: "matplotlib" or "plotly".
        **kwargs: Additional arguments forwarded to the backend.

    Returns:
        matplotlib Figure or plotly Figure.
    """
    depths_mm = [dp.depth_cm * 10.0 for dp in layer_result.depth_profile]

    all_isotopes: set[str] = set()
    for dp in layer_result.depth_profile:
        all_isotopes.update(dp.production_rates.keys())

    if not all_isotopes:
        title = kwargs.pop("title", "Cumulative Yield vs Depth (no data)")
        if backend == "matplotlib":
            return _mpl_line_plot([], [], "Depth [mm]", "Cumulative fraction", title, **kwargs)
        return _plotly_line_plot([], [], "Depth [mm]", "Cumulative fraction", title)

    isotope_rates: dict[str, np.ndarray] = {}
    for iso in all_isotopes:
        isotope_rates[iso] = np.array(
            [dp.production_rates.get(iso, 0.0) for dp in layer_result.depth_profile]
        )

    ranked = sorted(
        isotope_rates.items(),
        key=lambda kv: float(kv[1].sum()),
        reverse=True,
    )[:top_n]

    traces = []
    for name, rates in ranked:
        cumulative = np.cumsum(rates)
        total = cumulative[-1] if len(cumulative) > 0 and cumulative[-1] > 0 else 1.0
        traces.append((depths_mm, (cumulative / total).tolist(), name))

    title = kwargs.pop("title", "Cumulative Yield vs Depth")

    if backend == "matplotlib":
        return _mpl_multi_line_plot(
            traces,
            xlabel="Depth [mm]",
            ylabel="Cumulative fraction",
            title=title,
            **kwargs,
        )
    return _plotly_multi_line_plot(
        traces,
        xlabel="Depth [mm]",
        ylabel="Cumulative fraction",
        title=title,
    )


def plot_excitation_function(
    xs_energies_MeV: np.ndarray,
    xs_mb: np.ndarray,
    energy_in: float,
    energy_out: float,
    reaction_label: str = "",
    backend: Backend = "plotly",
    **kwargs: Any,
) -> Any:
    """Plot cross-section with shaded energy window.

    Shows sigma(E) curve with vertical lines at E_in and E_out, and a
    shaded region between them indicating the beam's energy range in the
    layer.

    Args:
        xs_energies_MeV: Energy grid [MeV].
        xs_mb: Cross-section values [mb] at each energy.
        energy_in: Beam energy entering the layer [MeV].
        energy_out: Beam energy leaving the layer [MeV].
        reaction_label: Optional reaction label.
        backend: "matplotlib" or "plotly".
        **kwargs: Additional arguments forwarded to the backend.

    Returns:
        matplotlib Figure or plotly Figure.
    """
    title = kwargs.pop(
        "title",
        f"Excitation Function{' \u2014 ' + reaction_label if reaction_label else ''}",
    )

    if backend == "matplotlib":
        return _mpl_excitation_plot(
            xs_energies_MeV.tolist(),
            xs_mb.tolist(),
            energy_in,
            energy_out,
            xlabel="Energy [MeV]",
            ylabel="Cross-section [mb]",
            title=title,
            **kwargs,
        )
    return _plotly_excitation_plot(
        xs_energies_MeV.tolist(),
        xs_mb.tolist(),
        energy_in,
        energy_out,
        xlabel="Energy [MeV]",
        ylabel="Cross-section [mb]",
        title=title,
    )


# ---------------------------------------------------------------------------
# 3. Beam profile diagnostics
# ---------------------------------------------------------------------------


def plot_beam_spot(
    profile: BeamProfile,
    n_sigma: float = 3.0,
    n_points: int = 200,
    backend: Backend = "plotly",
    **kwargs: Any,
) -> Any:
    """Plot 2D Gaussian beam spot intensity.

    Contour/filled-contour plot of the 2D Gaussian intensity
    I(x,y) = exp(-x^2/2*sigma_x^2 - y^2/2*sigma_y^2).
    Axes: x [mm], y [mm]. Shows sigma_x, sigma_y ellipses at 1sigma, 2sigma.

    Args:
        profile: BeamProfile describing the transverse beam shape.
        n_sigma: Number of sigma to extend the plot axes.
        n_points: Grid resolution per axis.
        backend: "matplotlib" or "plotly".
        **kwargs: Additional arguments forwarded to the backend.

    Returns:
        matplotlib Figure or plotly Figure.
    """
    sigma_x_mm = profile.sigma_x_cm * 10.0
    sigma_y_mm = profile.effective_sigma_y_cm * 10.0

    # Avoid zero-size grids for pencil beams
    sx = max(sigma_x_mm, 0.01)
    sy = max(sigma_y_mm, 0.01)

    x = np.linspace(-n_sigma * sx, n_sigma * sx, n_points)
    y = np.linspace(-n_sigma * sy, n_sigma * sy, n_points)
    X, Y = np.meshgrid(x, y)

    if sx > 1e-9 and sy > 1e-9:
        Z = np.exp(-X ** 2 / (2 * sx ** 2) - Y ** 2 / (2 * sy ** 2))
    else:
        Z = np.zeros_like(X)

    title = kwargs.pop(
        "title",
        f"Beam Spot (\u03c3_x={sigma_x_mm:.2f} mm, \u03c3_y={sigma_y_mm:.2f} mm)",
    )

    if backend == "matplotlib":
        return _mpl_contour_plot(
            X,
            Y,
            Z,
            xlabel="x [mm]",
            ylabel="y [mm]",
            title=title,
            sigma_x=sx,
            sigma_y=sy,
            **kwargs,
        )
    return _plotly_contour_plot(
        X,
        Y,
        Z,
        xlabel="x [mm]",
        ylabel="y [mm]",
        title=title,
    )


def plot_phase_space(
    profile: BeamProfile,
    plane: str = "x",
    n_sigma: float = 3.0,
    backend: Backend = "plotly",
    **kwargs: Any,
) -> Any:
    """Plot transverse phase-space ellipse.

    Draws the Twiss ellipse in (x [mm], theta [mrad]) or (y, theta_y) space.
    Uses sigma_x (or sigma_y), divergence, and Twiss alpha to compute the
    ellipse.  If emittance is set, use it; otherwise compute from
    sigma * sigma_theta.

    The ellipse equation: gamma*x^2 + 2*alpha*x*theta + beta*theta^2 = epsilon
    where gamma = (1 + alpha^2) / beta, beta = sigma^2 / epsilon,
    epsilon = sigma * sigma_theta (if uncorrelated).

    Args:
        profile: BeamProfile describing the transverse beam shape.
        plane: "x" or "y" for which transverse plane.
        n_sigma: Number of sigma for the ellipse extent.
        backend: "matplotlib" or "plotly".
        **kwargs: Additional arguments forwarded to the backend.

    Returns:
        matplotlib Figure or plotly Figure.
    """
    if plane == "x":
        sigma_mm = profile.sigma_x_cm * 10.0
        div_mrad = profile.divergence_x_mrad
        emittance = profile.emittance_x_mm_mrad
        alpha = profile.alpha_x
        pos_label = "x [mm]"
        ang_label = r"$\theta_x$ [mrad]"
        ang_label_plotly = "\u03b8_x [mrad]"
    elif plane == "y":
        sigma_mm = profile.effective_sigma_y_cm * 10.0
        div_mrad = profile.effective_divergence_y_mrad
        emittance = profile.emittance_y_mm_mrad
        alpha = profile.alpha_y
        pos_label = "y [mm]"
        ang_label = r"$\theta_y$ [mrad]"
        ang_label_plotly = "\u03b8_y [mrad]"
    else:
        raise ValueError(f"plane must be 'x' or 'y', got {plane!r}")

    # Compute emittance if not provided
    if emittance is not None:
        eps = emittance
    else:
        eps = sigma_mm * div_mrad if sigma_mm > 0 and div_mrad > 0 else 0.01

    # Twiss beta and gamma
    if sigma_mm > 0 and eps > 0:
        beta = sigma_mm ** 2 / eps
    else:
        beta = 1.0
    gamma_tw = (1.0 + alpha ** 2) / beta

    # Parametric ellipse: [x, theta] = R * [cos(t), sin(t)]
    # Sigma matrix: [[beta, -alpha], [-alpha, gamma]] * eps
    # Eigendecomposition for parametric form
    sigma_mat = eps * np.array([[beta, -alpha], [-alpha, gamma_tw]])
    eigvals, eigvecs = np.linalg.eigh(sigma_mat)
    eigvals = np.maximum(eigvals, 0.0)

    t = np.linspace(0, 2 * np.pi, 200)
    circle = np.column_stack([np.cos(t), np.sin(t)])
    # Scale by sqrt(eigenvalues) for 1-sigma, then by n_sigma
    ellipse = circle @ np.diag(np.sqrt(eigvals)) @ eigvecs.T

    x_ell = ellipse[:, 0]
    theta_ell = ellipse[:, 1]

    title = kwargs.pop(
        "title",
        f"Phase Space ({plane}-plane, \u03b5={eps:.3f} mm\u00b7mrad)",
    )

    traces = [(x_ell.tolist(), theta_ell.tolist(), "1\u03c3 ellipse")]
    if n_sigma > 1:
        x_ell_n = x_ell * n_sigma
        theta_ell_n = theta_ell * n_sigma
        traces.append(
            (x_ell_n.tolist(), theta_ell_n.tolist(), f"{n_sigma}\u03c3 ellipse")
        )

    if backend == "matplotlib":
        return _mpl_multi_line_plot(
            traces,
            xlabel=pos_label,
            ylabel=ang_label,
            title=title,
            **kwargs,
        )
    return _plotly_multi_line_plot(
        traces,
        xlabel=pos_label,
        ylabel=ang_label_plotly,
        title=title,
    )


# ---------------------------------------------------------------------------
# 4. 3D geometry cross-section plots
# ---------------------------------------------------------------------------


def plot_mesh_cross_section(
    polygons: list,  # list[MeshSlicePolygon]
    values: dict[int, float] | None = None,  # tet_index -> value
    quantity_label: str = "Value",
    colormap: str = "Plasma",
    log_scale: bool = False,
    show_materials: bool = True,
    materials: dict[int, Any] | None = None,  # material_id -> MaterialInfo
    beam_sigma_cm: float | None = None,
    backend: Backend = "plotly",
    **kwargs: Any,
) -> Any:
    """Plot a 2D cross-section of the mesh colored by a quantity.

    Uses MeshSlicePolygon list (from cut_mesh_with_plane).  Each polygon is
    rendered as a filled patch.  If values is None and show_materials is True,
    color by material_id.

    Args:
        polygons: List of MeshSlicePolygon from cut_mesh_with_plane.
        values: Optional mapping of tet_index to a scalar value for coloring.
        quantity_label: Label for the color bar.
        colormap: Colormap/colorscale name.
        log_scale: If True, apply log10 to values before mapping colors.
        show_materials: If True and values is None, color by material_id.
        materials: Optional mapping of material_id to MaterialInfo (for legend).
        beam_sigma_cm: If set, overlay beam envelope (sigma circles or lines).
        backend: "matplotlib" or "plotly".
        **kwargs: Additional arguments forwarded to the backend.

    Returns:
        matplotlib Figure or plotly Figure.
    """
    title = kwargs.pop("title", "Mesh Cross-Section")

    if backend == "matplotlib":
        return _mpl_mesh_cross_section(
            polygons,
            values=values,
            quantity_label=quantity_label,
            colormap=colormap,
            show_materials=show_materials,
            materials=materials,
            title=title,
            **kwargs,
        )
    return _plotly_mesh_cross_section(
        polygons,
        values=values,
        quantity_label=quantity_label,
        colormap=colormap,
        log_scale=log_scale,
        show_materials=show_materials,
        materials=materials,
        beam_sigma_cm=beam_sigma_cm,
        title=title,
        **kwargs,
    )


def plot_ray_visualization(
    mesh: Any,  # TetrahedralMesh
    ray_segments: list[list],  # list of list[RaySegment]
    backend: Backend = "plotly",
    **kwargs: Any,
) -> Any:
    """Plot mesh wireframe with ray paths overlaid (2D projection).

    Projects the mesh edges and ray entry/exit points onto a 2D plane
    (default: XZ plane for a z-directed beam).
    Shows mesh outline and colored ray lines.

    Args:
        mesh: TetrahedralMesh instance.
        ray_segments: List of ray results, each a list of RaySegments.
        backend: "matplotlib" or "plotly".
        **kwargs: Additional arguments forwarded to the backend.

    Returns:
        matplotlib Figure or plotly Figure.
    """
    title = kwargs.pop("title", "Ray Visualization")

    if backend == "matplotlib":
        return _mpl_ray_visualization(
            mesh,
            ray_segments,
            title=title,
            **kwargs,
        )
    return _plotly_ray_visualization(
        mesh,
        ray_segments,
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


# --- New backend helpers ---


def _mpl_band_plot(
    x: list[float],
    y: list[float],
    y_lower: list[float],
    y_upper: list[float],
    vlines: list[float] | None = None,
    xlabel: str = "",
    ylabel: str = "",
    title: str = "",
    band_label: str = "",
    **kwargs: Any,
) -> Any:
    """Matplotlib line plot with shaded band and optional vertical lines."""
    import matplotlib
    matplotlib.use("Agg")
    import matplotlib.pyplot as plt

    fig, ax = plt.subplots(figsize=kwargs.get("figsize", (10, 6)))
    ax.plot(x, y, label="Mean energy", color="C0")
    ax.fill_between(
        x, y_lower, y_upper, alpha=0.3, color="C0", label=band_label,
    )
    if vlines:
        for vx in vlines:
            ax.axvline(vx, color="gray", linestyle="--", linewidth=0.8)
    ax.set_xlabel(xlabel)
    ax.set_ylabel(ylabel)
    ax.set_title(title)
    ax.legend(loc="best")
    fig.tight_layout()
    return fig


def _plotly_band_plot(
    x: list[float],
    y: list[float],
    y_lower: list[float],
    y_upper: list[float],
    vlines: list[float] | None = None,
    xlabel: str = "",
    ylabel: str = "",
    title: str = "",
    band_label: str = "",
    **kwargs: Any,
) -> Any:
    """Plotly line plot with shaded band and optional vertical lines."""
    import plotly.graph_objects as go

    fig = go.Figure()
    fig.add_trace(go.Scatter(x=x, y=y_upper, mode="lines", line={"width": 0},
                             showlegend=False))
    fig.add_trace(go.Scatter(x=x, y=y_lower, mode="lines", line={"width": 0},
                             fill="tonexty", fillcolor="rgba(31,119,180,0.3)",
                             name=band_label))
    fig.add_trace(go.Scatter(x=x, y=y, mode="lines", name="Mean energy"))
    if vlines:
        for vx in vlines:
            fig.add_vline(x=vx, line_dash="dash", line_color="gray")
    fig.update_layout(title=title, xaxis_title=xlabel, yaxis_title=ylabel)
    return fig


def _mpl_excitation_plot(
    x: list[float],
    y: list[float],
    energy_in: float,
    energy_out: float,
    xlabel: str = "",
    ylabel: str = "",
    title: str = "",
    **kwargs: Any,
) -> Any:
    """Matplotlib cross-section plot with shaded energy window."""
    import matplotlib
    matplotlib.use("Agg")
    import matplotlib.pyplot as plt

    fig, ax = plt.subplots(figsize=kwargs.get("figsize", (10, 6)))
    ax.plot(x, y, color="C0")
    e_lo = min(energy_in, energy_out)
    e_hi = max(energy_in, energy_out)
    ax.axvline(energy_in, color="C1", linestyle="--", label=f"E_in = {energy_in:.1f} MeV")
    ax.axvline(energy_out, color="C2", linestyle="--", label=f"E_out = {energy_out:.1f} MeV")
    ax.axvspan(e_lo, e_hi, alpha=0.15, color="C1", label="Energy window")
    ax.set_xlabel(xlabel)
    ax.set_ylabel(ylabel)
    ax.set_title(title)
    ax.legend(loc="best")
    fig.tight_layout()
    return fig


def _plotly_excitation_plot(
    x: list[float],
    y: list[float],
    energy_in: float,
    energy_out: float,
    xlabel: str = "",
    ylabel: str = "",
    title: str = "",
    **kwargs: Any,
) -> Any:
    """Plotly cross-section plot with shaded energy window."""
    import plotly.graph_objects as go

    fig = go.Figure()
    fig.add_trace(go.Scatter(x=x, y=y, mode="lines", name="\u03c3(E)"))
    e_lo = min(energy_in, energy_out)
    e_hi = max(energy_in, energy_out)
    fig.add_vrect(x0=e_lo, x1=e_hi, fillcolor="orange", opacity=0.15,
                  line_width=0, annotation_text="Energy window")
    fig.add_vline(x=energy_in, line_dash="dash", line_color="orange",
                  annotation_text=f"E_in={energy_in:.1f}")
    fig.add_vline(x=energy_out, line_dash="dash", line_color="green",
                  annotation_text=f"E_out={energy_out:.1f}")
    fig.update_layout(title=title, xaxis_title=xlabel, yaxis_title=ylabel)
    return fig


def _mpl_contour_plot(
    X: np.ndarray,
    Y: np.ndarray,
    Z: np.ndarray,
    xlabel: str = "",
    ylabel: str = "",
    title: str = "",
    sigma_x: float | None = None,
    sigma_y: float | None = None,
    **kwargs: Any,
) -> Any:
    """Matplotlib filled-contour plot with optional sigma ellipses."""
    import matplotlib
    matplotlib.use("Agg")
    import matplotlib.pyplot as plt
    from matplotlib.patches import Ellipse

    fig, ax = plt.subplots(figsize=kwargs.get("figsize", (8, 8)))
    cf = ax.contourf(X, Y, Z, levels=20, cmap="inferno")
    fig.colorbar(cf, ax=ax, label="Relative intensity")

    # Draw sigma ellipses
    if sigma_x is not None and sigma_y is not None:
        for n in (1, 2):
            ell = Ellipse(
                (0, 0),
                width=2 * n * sigma_x,
                height=2 * n * sigma_y,
                fill=False,
                edgecolor="white",
                linestyle="--" if n == 2 else "-",
                linewidth=1.2,
                label=f"{n}\u03c3",
            )
            ax.add_patch(ell)
        ax.legend(loc="upper right", framealpha=0.7)

    ax.set_xlabel(xlabel)
    ax.set_ylabel(ylabel)
    ax.set_title(title)
    ax.set_aspect("equal")
    fig.tight_layout()
    return fig


def _plotly_contour_plot(
    X: np.ndarray,
    Y: np.ndarray,
    Z: np.ndarray,
    xlabel: str = "",
    ylabel: str = "",
    title: str = "",
    **kwargs: Any,
) -> Any:
    """Plotly filled-contour plot."""
    import plotly.graph_objects as go

    fig = go.Figure(
        data=go.Contour(
            x=X[0, :].tolist(),
            y=Y[:, 0].tolist(),
            z=Z.tolist(),
            colorscale="Inferno",
            colorbar={"title": "Relative intensity"},
        )
    )
    fig.update_layout(
        title=title,
        xaxis_title=xlabel,
        yaxis_title=ylabel,
        yaxis_scaleanchor="x",
    )
    return fig


def _mpl_mesh_cross_section(
    polygons: list,  # list[MeshSlicePolygon]
    values: dict[int, float] | None = None,
    quantity_label: str = "Value",
    colormap: str = "viridis",
    show_materials: bool = True,
    materials: dict[int, Any] | None = None,
    title: str = "",
    **kwargs: Any,
) -> Any:
    """Matplotlib mesh cross-section with PatchCollection."""
    import matplotlib
    matplotlib.use("Agg")
    import matplotlib.pyplot as plt
    from matplotlib.collections import PatchCollection
    from matplotlib.patches import Polygon as MplPolygon

    fig, ax = plt.subplots(figsize=kwargs.get("figsize", (10, 8)))

    if not polygons:
        ax.set_title(title + " (no polygons)")
        fig.tight_layout()
        return fig

    patches = []
    face_values: list[float] = []

    for poly in polygons:
        verts = poly.vertices_2d
        patches.append(MplPolygon(verts, closed=True))
        if values is not None:
            face_values.append(values.get(poly.tet_index, 0.0))
        elif show_materials:
            face_values.append(float(poly.material_id))

    pc = PatchCollection(patches, cmap=colormap, edgecolors="k", linewidths=0.3)

    if face_values:
        pc.set_array(np.array(face_values))

    ax.add_collection(pc)
    ax.autoscale_view()
    ax.set_aspect("equal")

    if face_values:
        label = quantity_label if values is not None else "Material ID"
        fig.colorbar(pc, ax=ax, label=label)

    # Add material legend if available
    if materials and values is None and show_materials:
        mat_ids = sorted({p.material_id for p in polygons})
        legend_labels = []
        for mid in mat_ids:
            info = materials.get(mid)
            name = info.name if info and hasattr(info, "name") else str(mid)
            legend_labels.append(f"ID {mid}: {name}")
        # Add text annotation instead of legend patches (simpler)
        ax.annotate(
            "\n".join(legend_labels),
            xy=(0.02, 0.98),
            xycoords="axes fraction",
            verticalalignment="top",
            fontsize=8,
            bbox={"boxstyle": "round", "facecolor": "white", "alpha": 0.8},
        )

    ax.set_xlabel("u [cm]")
    ax.set_ylabel("v [cm]")
    ax.set_title(title)
    fig.tight_layout()
    return fig


def _plotly_mesh_cross_section(
    polygons: list,  # list[MeshSlicePolygon]
    values: dict[int, float] | None = None,
    quantity_label: str = "Value",
    colormap: str = "Plasma",
    log_scale: bool = False,
    show_materials: bool = True,
    materials: dict[int, Any] | None = None,
    beam_sigma_cm: float | None = None,
    title: str = "",
    **kwargs: Any,
) -> Any:
    """Plotly mesh cross-section with colorscale support."""
    import plotly.express as px
    import plotly.graph_objects as go

    fig = go.Figure()

    if not polygons:
        fig.update_layout(title=title + " (no polygons)")
        return fig

    # Pre-compute color mapping when values provided
    if values is not None:
        raw_vals = [values.get(p.tet_index, 0.0) for p in polygons]
        if log_scale:
            mapped = [np.log10(v + 1e-30) for v in raw_vals]
            finite = [v for v in mapped if v > -20]
        else:
            mapped = list(raw_vals)
            finite = [v for v in mapped if v > 0]
        vmin = min(finite) if finite else 0.0
        vmax = max(finite) if finite else 1.0
        span = vmax - vmin if vmax > vmin else 1.0

    # Assign material colors for material-only mode
    mat_color_cycle = px.colors.qualitative.Plotly
    mat_id_list = sorted({p.material_id for p in polygons})
    mat_color_map = {
        mid: mat_color_cycle[i % len(mat_color_cycle)]
        for i, mid in enumerate(mat_id_list)
    }

    added_legend: set[int] = set()
    for i, poly in enumerate(polygons):
        verts = poly.vertices_2d
        xs = list(verts[:, 0]) + [verts[0, 0]]
        ys = list(verts[:, 1]) + [verts[0, 1]]
        info = materials.get(poly.material_id) if materials else None
        mat_name = info.name if info and hasattr(info, "name") else str(poly.material_id)

        if values is not None:
            t_norm = max(0.0, min(1.0, (mapped[i] - vmin) / span))
            color = px.colors.sample_colorscale(colormap, [t_norm])[0]
            fig.add_trace(go.Scatter(
                x=xs, y=ys, mode="lines", fill="toself",
                fillcolor=color,
                line={"color": color, "width": 0},
                opacity=0.9,
                showlegend=False,
                hovertemplate=(
                    f"<b>{mat_name}</b><br>"
                    f"{quantity_label}: {raw_vals[i]:.2e}<br>"
                    f"<extra>tet {poly.tet_index}</extra>"
                ),
            ))
        elif show_materials:
            mid = poly.material_id
            show = mid not in added_legend
            added_legend.add(mid)
            fig.add_trace(go.Scatter(
                x=xs, y=ys, mode="lines", fill="toself",
                fillcolor=mat_color_map[mid],
                line={"color": "gray", "width": 0.3},
                opacity=0.8,
                name=mat_name,
                legendgroup=str(mid),
                showlegend=show,
            ))
        else:
            fig.add_trace(go.Scatter(
                x=xs, y=ys, mode="lines", fill="toself",
                fillcolor="steelblue",
                line={"color": "gray", "width": 0.3},
                showlegend=False,
            ))

    # Colorbar for values mode
    if values is not None:
        cb_vals = np.linspace(vmin, vmax, 50)
        cb_label = f"log₁₀({quantity_label})" if log_scale else quantity_label
        fig.add_trace(go.Scatter(
            x=[None] * len(cb_vals), y=[None] * len(cb_vals),
            mode="markers",
            marker={
                "size": 0, "color": cb_vals, "colorscale": colormap,
                "colorbar": {"title": cb_label, "thickness": 15},
                "showscale": True,
            },
            showlegend=False, hoverinfo="skip",
        ))

    # Beam overlay (sigma circles/lines)
    if beam_sigma_cm is not None:
        sigma = beam_sigma_cm
        # Detect orientation: if y-range >> x-range it's longitudinal, else axial
        all_x = np.concatenate([p.vertices_2d[:, 0] for p in polygons])
        all_y = np.concatenate([p.vertices_2d[:, 1] for p in polygons])
        x_span = float(np.ptp(all_x))
        y_span = float(np.ptp(all_y))

        if x_span > 2 * y_span:
            # Longitudinal slice (wide): horizontal lines for beam envelope
            for n, dash in [(1, "dot"), (2, "dash")]:
                fig.add_hline(y=n * sigma, line={"color": "white", "width": 1, "dash": dash})
                fig.add_hline(y=-n * sigma, line={"color": "white", "width": 1, "dash": dash})
                fig.add_annotation(
                    x=float(np.max(all_x)) + x_span * 0.02, y=n * sigma,
                    text=f"{n}σ", showarrow=False,
                    font={"color": "white", "size": 10},
                )
        else:
            # Axial slice (square-ish): circles for beam spot
            theta = np.linspace(0, 2 * np.pi, 100)
            for n, dash in [(1, "dot"), (2, "dash")]:
                r = n * sigma
                fig.add_trace(go.Scatter(
                    x=r * np.cos(theta), y=r * np.sin(theta),
                    mode="lines",
                    line={"color": "white", "width": 1.5, "dash": dash},
                    name=f"Beam {n}σ ({n * sigma * 10:.1f} mm)",
                ))

    bg = "black" if values is not None else "white"
    fig.update_layout(
        title=title,
        xaxis_title=kwargs.get("xlabel", "u [cm]"),
        yaxis_title=kwargs.get("ylabel", "v [cm]"),
        yaxis_scaleanchor="x",
        plot_bgcolor=bg,
        width=kwargs.get("width"),
        height=kwargs.get("height"),
    )
    return fig


def _mpl_ray_visualization(
    mesh: Any,  # TetrahedralMesh
    ray_segments: list[list],  # list of list[RaySegment]
    title: str = "",
    **kwargs: Any,
) -> Any:
    """Matplotlib 2D projection of mesh wireframe with ray paths (XZ plane)."""
    import matplotlib
    matplotlib.use("Agg")
    import matplotlib.pyplot as plt

    fig, ax = plt.subplots(figsize=kwargs.get("figsize", (10, 8)))

    # Draw mesh edges (unique edges from all tets) projected onto XZ
    edges_drawn: set[tuple[int, int]] = set()
    edge_pairs = [(0, 1), (0, 2), (0, 3), (1, 2), (1, 3), (2, 3)]
    for elem in mesh.elements:
        for i, j in edge_pairs:
            edge = (min(elem[i], elem[j]), max(elem[i], elem[j]))
            if edge not in edges_drawn:
                edges_drawn.add(edge)
                n0 = mesh.nodes[edge[0]]
                n1 = mesh.nodes[edge[1]]
                ax.plot(
                    [n0[0], n1[0]], [n0[2], n1[2]],
                    color="lightgray", linewidth=0.3, zorder=1,
                )

    # Draw ray paths
    colors = plt.cm.tab10(np.linspace(0, 1, max(len(ray_segments), 1)))
    for ray_idx, segments in enumerate(ray_segments):
        color = colors[ray_idx % len(colors)]
        for seg in segments:
            entry = seg.entry_point
            exit_ = seg.exit_point
            ax.plot(
                [entry[0], exit_[0]], [entry[2], exit_[2]],
                color=color, linewidth=1.5, zorder=2,
            )

    ax.set_xlabel("x [cm]")
    ax.set_ylabel("z [cm]")
    ax.set_title(title)
    ax.set_aspect("equal")
    fig.tight_layout()
    return fig


def _plotly_ray_visualization(
    mesh: Any,  # TetrahedralMesh
    ray_segments: list[list],  # list of list[RaySegment]
    title: str = "",
    **kwargs: Any,
) -> Any:
    """Plotly 2D projection of mesh wireframe with ray paths (XZ plane)."""
    import plotly.graph_objects as go

    fig = go.Figure()

    # Mesh edges projected to XZ
    edges_drawn: set[tuple[int, int]] = set()
    edge_pairs = [(0, 1), (0, 2), (0, 3), (1, 2), (1, 3), (2, 3)]
    mesh_x: list[float | None] = []
    mesh_z: list[float | None] = []
    for elem in mesh.elements:
        for i, j in edge_pairs:
            edge = (min(elem[i], elem[j]), max(elem[i], elem[j]))
            if edge not in edges_drawn:
                edges_drawn.add(edge)
                n0 = mesh.nodes[edge[0]]
                n1 = mesh.nodes[edge[1]]
                mesh_x.extend([float(n0[0]), float(n1[0]), None])
                mesh_z.extend([float(n0[2]), float(n1[2]), None])

    fig.add_trace(go.Scatter(
        x=mesh_x, y=mesh_z, mode="lines",
        line={"color": "lightgray", "width": 0.5},
        name="Mesh", showlegend=True,
    ))

    # Ray paths
    for ray_idx, segments in enumerate(ray_segments):
        ray_x: list[float | None] = []
        ray_z: list[float | None] = []
        for seg in segments:
            entry = seg.entry_point
            exit_ = seg.exit_point
            ray_x.extend([float(entry[0]), float(exit_[0]), None])
            ray_z.extend([float(entry[2]), float(exit_[2]), None])
        fig.add_trace(go.Scatter(
            x=ray_x, y=ray_z, mode="lines",
            name=f"Ray {ray_idx}",
        ))

    fig.update_layout(
        title=title,
        xaxis_title="x [cm]",
        yaxis_title="z [cm]",
        yaxis_scaleanchor="x",
    )
    return fig
