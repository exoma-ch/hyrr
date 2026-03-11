"""Tests for hyrr.plotting module."""

from __future__ import annotations

import matplotlib

matplotlib.use("Agg")

import matplotlib.pyplot as plt  # noqa: E402
import numpy as np  # noqa: E402
import pytest  # noqa: E402

from hyrr.models import (  # noqa: E402
    DepthPoint,
    Element,
    IsotopeResult,
    Layer,
    LayerResult,
)
from hyrr.plotting import (  # noqa: E402
    plot_activity_vs_time,
    plot_cooling_curve,
    plot_depth_profile,
    plot_energy_scan,
    plot_purity_vs_cooling,
)

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _make_depth_profile(n: int = 50) -> list[DepthPoint]:
    return [
        DepthPoint(
            depth_cm=i * 0.001,
            energy_MeV=16.0 - i * 0.08,
            dedx_MeV_cm=200.0 + i * 2,
            heat_W_cm3=50.0 + i * 0.5,
            production_rates={"Tc-99m": 1e8 * (1 + i * 0.01)},
        )
        for i in range(n)
    ]


def _make_isotope_result(
    name: str, half_life: float, peak_activity: float
) -> IsotopeResult:
    time = np.linspace(0, 172800, 200)  # 2 days
    activity_irr = peak_activity * (1 - np.exp(-0.001 * time[:100]))
    cooling = np.full(100, activity_irr[-1]) * np.exp(
        -np.log(2) / half_life * np.linspace(0, 86400, 100)
    )
    return IsotopeResult(
        name=name,
        Z=43,
        A=99,
        state="m",
        half_life_s=half_life,
        production_rate=1e10,
        saturation_yield_Bq_uA=1e8,
        activity_Bq=float(cooling[0]),
        time_grid_s=time,
        activity_vs_time_Bq=np.concatenate([activity_irr, cooling]),
    )


# ---------------------------------------------------------------------------
# Fixtures
# ---------------------------------------------------------------------------


@pytest.fixture()
def element() -> Element:
    return Element(symbol="Mo", Z=42, isotopes={98: 0.7, 100: 0.3})


@pytest.fixture()
def layer(element: Element) -> Layer:
    return Layer(
        density_g_cm3=10.2,
        elements=[(element, 1.0)],
        thickness_cm=0.05,
    )


@pytest.fixture()
def layer_result(layer: Layer) -> LayerResult:
    profile = _make_depth_profile()
    iso1 = _make_isotope_result("Tc-99m", half_life=21624.0, peak_activity=1e10)
    iso2 = _make_isotope_result("Tc-99g", half_life=2.111e5 * 365.25 * 86400, peak_activity=1e8)
    return LayerResult(
        layer=layer,
        energy_in=16.0,
        energy_out=12.0,
        delta_E_MeV=4.0,
        heat_kW=0.1,
        depth_profile=profile,
        isotope_results={"Tc-99m": iso1, "Tc-99g": iso2},
    )


@pytest.fixture()
def isotope_results_list() -> list[IsotopeResult]:
    return [
        _make_isotope_result("Tc-99m", 21624.0, 1e10),
        _make_isotope_result("Tc-99g", 2.111e5 * 365.25 * 86400, 1e8),
        _make_isotope_result("Mo-99", 237168.0, 5e9),
    ]


@pytest.fixture()
def isotope_results_dict(
    isotope_results_list: list[IsotopeResult],
) -> dict[str, IsotopeResult]:
    return {r.name: r for r in isotope_results_list}


# ---------------------------------------------------------------------------
# Tests: plot_depth_profile
# ---------------------------------------------------------------------------


class TestDepthProfile:
    def test_matplotlib_returns_figure(self, layer_result: LayerResult) -> None:
        fig = plot_depth_profile(layer_result, backend="matplotlib")
        assert isinstance(fig, plt.Figure)
        plt.close(fig)

    def test_heat_quantity(self, layer_result: LayerResult) -> None:
        fig = plot_depth_profile(layer_result, quantity="heat")
        assert isinstance(fig, plt.Figure)
        ax = fig.axes[0]
        assert "Heat" in ax.get_ylabel()
        plt.close(fig)

    def test_energy_quantity(self, layer_result: LayerResult) -> None:
        fig = plot_depth_profile(layer_result, quantity="energy")
        assert isinstance(fig, plt.Figure)
        ax = fig.axes[0]
        assert "Energy" in ax.get_ylabel()
        plt.close(fig)

    def test_dedx_quantity(self, layer_result: LayerResult) -> None:
        fig = plot_depth_profile(layer_result, quantity="dedx")
        assert isinstance(fig, plt.Figure)
        ax = fig.axes[0]
        assert "dE/dx" in ax.get_ylabel()
        plt.close(fig)

    def test_invalid_quantity(self, layer_result: LayerResult) -> None:
        with pytest.raises(ValueError, match="Unknown quantity"):
            plot_depth_profile(layer_result, quantity="invalid")  # type: ignore[arg-type]

    def test_has_one_axis(self, layer_result: LayerResult) -> None:
        fig = plot_depth_profile(layer_result)
        assert len(fig.axes) == 1
        plt.close(fig)


# ---------------------------------------------------------------------------
# Tests: plot_activity_vs_time
# ---------------------------------------------------------------------------


class TestActivityVsTime:
    def test_returns_figure(
        self, isotope_results_list: list[IsotopeResult]
    ) -> None:
        fig = plot_activity_vs_time(isotope_results_list)
        assert isinstance(fig, plt.Figure)
        plt.close(fig)

    def test_accepts_dict(
        self, isotope_results_dict: dict[str, IsotopeResult]
    ) -> None:
        fig = plot_activity_vs_time(isotope_results_dict)
        assert isinstance(fig, plt.Figure)
        plt.close(fig)

    def test_top_n_filtering(
        self, isotope_results_list: list[IsotopeResult]
    ) -> None:
        fig = plot_activity_vs_time(isotope_results_list, top_n=2)
        ax = fig.axes[0]
        assert len(ax.get_lines()) == 2
        plt.close(fig)

    def test_empty_results(self) -> None:
        fig = plot_activity_vs_time([])
        assert isinstance(fig, plt.Figure)
        plt.close(fig)

    def test_min_activity_filter(
        self, isotope_results_list: list[IsotopeResult]
    ) -> None:
        fig = plot_activity_vs_time(isotope_results_list, min_activity_Bq=1e11)
        ax = fig.axes[0]
        # Very high threshold should filter out everything
        assert len(ax.get_lines()) == 0
        plt.close(fig)


# ---------------------------------------------------------------------------
# Tests: plot_cooling_curve
# ---------------------------------------------------------------------------


class TestCoolingCurve:
    def test_returns_figure(
        self, isotope_results_list: list[IsotopeResult]
    ) -> None:
        fig = plot_cooling_curve(isotope_results_list, irradiation_time_s=86400)
        assert isinstance(fig, plt.Figure)
        plt.close(fig)

    def test_log_y_scale(
        self, isotope_results_list: list[IsotopeResult]
    ) -> None:
        fig = plot_cooling_curve(isotope_results_list, irradiation_time_s=86400)
        ax = fig.axes[0]
        assert ax.get_yscale() == "log"
        plt.close(fig)

    def test_accepts_dict(
        self, isotope_results_dict: dict[str, IsotopeResult]
    ) -> None:
        fig = plot_cooling_curve(isotope_results_dict, irradiation_time_s=86400)
        assert isinstance(fig, plt.Figure)
        plt.close(fig)


# ---------------------------------------------------------------------------
# Tests: plot_purity_vs_cooling
# ---------------------------------------------------------------------------


class TestPurityVsCooling:
    def test_returns_figure(
        self, isotope_results_dict: dict[str, IsotopeResult]
    ) -> None:
        fig = plot_purity_vs_cooling(
            isotope_results_dict, "Tc-99m", irradiation_time_s=86400
        )
        assert isinstance(fig, plt.Figure)
        plt.close(fig)

    def test_missing_isotope(
        self, isotope_results_dict: dict[str, IsotopeResult]
    ) -> None:
        with pytest.raises(ValueError, match="not found"):
            plot_purity_vs_cooling(
                isotope_results_dict, "nonexistent", irradiation_time_s=86400
            )

    def test_purity_label(
        self, isotope_results_dict: dict[str, IsotopeResult]
    ) -> None:
        fig = plot_purity_vs_cooling(
            isotope_results_dict, "Tc-99m", irradiation_time_s=86400
        )
        ax = fig.axes[0]
        assert "Purity" in ax.get_ylabel()
        plt.close(fig)


# ---------------------------------------------------------------------------
# Tests: plot_energy_scan
# ---------------------------------------------------------------------------


class TestEnergyScan:
    def test_returns_figure(self) -> None:
        energies = [10.0, 12.0, 14.0, 16.0, 18.0, 20.0]
        activities = {
            "Tc-99m": [0.0, 50.0, 150.0, 260.0, 300.0, 320.0],
            "Tc-99g": [0.0, 10.0, 30.0, 50.0, 60.0, 65.0],
        }
        fig = plot_energy_scan(energies, activities)
        assert isinstance(fig, plt.Figure)
        plt.close(fig)

    def test_traces_count(self) -> None:
        energies = [10.0, 12.0, 14.0]
        activities = {
            "A": [1.0, 2.0, 3.0],
            "B": [4.0, 5.0, 6.0],
            "C": [7.0, 8.0, 9.0],
        }
        fig = plot_energy_scan(energies, activities)
        ax = fig.axes[0]
        assert len(ax.get_lines()) == 3
        plt.close(fig)


# ---------------------------------------------------------------------------
# Tests: plotly backend (skip if not installed)
# ---------------------------------------------------------------------------


_has_plotly = True
try:
    import plotly  # noqa: F401
except ImportError:
    _has_plotly = False


@pytest.mark.skipif(not _has_plotly, reason="plotly not installed")
class TestPlotlyBackend:
    def test_depth_profile(self, layer_result: LayerResult) -> None:
        fig = plot_depth_profile(layer_result, backend="plotly")
        assert hasattr(fig, "data")
        assert len(fig.data) == 1

    def test_activity_vs_time(
        self, isotope_results_list: list[IsotopeResult]
    ) -> None:
        fig = plot_activity_vs_time(isotope_results_list, backend="plotly")
        assert hasattr(fig, "data")
        assert len(fig.data) == 3

    def test_cooling_curve(
        self, isotope_results_list: list[IsotopeResult]
    ) -> None:
        fig = plot_cooling_curve(
            isotope_results_list, irradiation_time_s=86400, backend="plotly"
        )
        assert hasattr(fig, "data")

    def test_purity_vs_cooling(
        self, isotope_results_dict: dict[str, IsotopeResult]
    ) -> None:
        fig = plot_purity_vs_cooling(
            isotope_results_dict, "Tc-99m", 86400, backend="plotly"
        )
        assert hasattr(fig, "data")
        assert len(fig.data) == 1

    def test_energy_scan(self) -> None:
        energies = [10.0, 12.0, 14.0]
        activities = {"A": [1.0, 2.0, 3.0], "B": [4.0, 5.0, 6.0]}
        fig = plot_energy_scan(energies, activities, backend="plotly")
        assert len(fig.data) == 2
