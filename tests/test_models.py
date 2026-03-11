"""Tests for hyrr.models and hyrr.db data types."""

from __future__ import annotations

import math
from typing import Any

import numpy as np
import pytest

from hyrr.db import CrossSectionData, DatabaseProtocol, DecayData, DecayMode
from hyrr.models import (
    PROJECTILE_Z,
    Beam,
    DepthPoint,
    Element,
    IsotopeResult,
    Layer,
    LayerResult,
    StackResult,
    TargetStack,
)

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def _cu_element() -> Element:
    """Natural copper element fixture."""
    return Element(symbol="Cu", Z=29, isotopes={63: 0.6915, 65: 0.3085})


def _ni_element() -> Element:
    """Natural nickel (simplified two-isotope)."""
    return Element(symbol="Ni", Z=28, isotopes={58: 0.68, 60: 0.32})


def _make_layer(**kwargs: object) -> Layer:
    """Create a layer with sensible defaults, overridable via kwargs."""
    defaults: dict[str, object] = {
        "density_g_cm3": 8.96,
        "elements": [(_cu_element(), 1.0)],
        "thickness_cm": 0.1,
    }
    defaults.update(kwargs)
    return Layer(**defaults)  # type: ignore[arg-type]


# ---------------------------------------------------------------------------
# Beam
# ---------------------------------------------------------------------------

class TestBeam:
    """Tests for the Beam dataclass."""

    def test_valid_proton_beam(self) -> None:
        b = Beam(projectile="p", energy_MeV=18.0, current_mA=0.1)
        assert b.projectile == "p"
        assert b.energy_MeV == 18.0
        assert b.current_mA == 0.1

    def test_valid_alpha_beam(self) -> None:
        b = Beam(projectile="a", energy_MeV=30.0, current_mA=0.05)
        assert b.projectile == "a"

    @pytest.mark.parametrize("proj", ["p", "d", "t", "h", "a"])
    def test_all_projectile_types(self, proj: str) -> None:
        Beam(projectile=proj, energy_MeV=10.0, current_mA=0.1)  # type: ignore[arg-type]

    def test_invalid_projectile(self) -> None:
        with pytest.raises(ValueError, match="Cannot parse projectile"):
            Beam(projectile="x", energy_MeV=10.0, current_mA=0.1)  # type: ignore[arg-type]

    def test_negative_energy(self) -> None:
        with pytest.raises(ValueError, match="energy_MeV must be positive"):
            Beam(projectile="p", energy_MeV=-1.0, current_mA=0.1)

    def test_zero_energy(self) -> None:
        with pytest.raises(ValueError, match="energy_MeV must be positive"):
            Beam(projectile="p", energy_MeV=0.0, current_mA=0.1)

    def test_negative_current(self) -> None:
        with pytest.raises(ValueError, match="current_mA must be positive"):
            Beam(projectile="p", energy_MeV=10.0, current_mA=-0.5)

    def test_zero_current(self) -> None:
        with pytest.raises(ValueError, match="current_mA must be positive"):
            Beam(projectile="p", energy_MeV=10.0, current_mA=0.0)

    def test_particles_per_second_proton(self) -> None:
        b = Beam(projectile="p", energy_MeV=18.0, current_mA=1.0)
        # 1 mA / (1 * e) = 1e-3 / 1.602176634e-19
        expected = 1e-3 / 1.602176634e-19
        assert math.isclose(b.particles_per_second, expected, rel_tol=1e-9)

    def test_particles_per_second_alpha(self) -> None:
        b = Beam(projectile="a", energy_MeV=30.0, current_mA=1.0)
        # alpha has Z=2, so 1e-3 / (2 * e)
        expected = 1e-3 / (2 * 1.602176634e-19)
        assert math.isclose(b.particles_per_second, expected, rel_tol=1e-9)

    def test_particles_per_second_helion(self) -> None:
        b = Beam(projectile="h", energy_MeV=20.0, current_mA=0.5)
        expected = 0.5e-3 / (PROJECTILE_Z["h"] * 1.602176634e-19)
        assert math.isclose(b.particles_per_second, expected, rel_tol=1e-9)

    def test_heavy_ion_beam(self) -> None:
        b = Beam(projectile="C-12", energy_MeV=60.0, current_mA=0.01)
        assert b.projectile == "C-12"
        assert b.projectile_obj.Z == 6
        assert b.projectile_obj.A == 12

    def test_heavy_ion_particles_per_second(self) -> None:
        b = Beam(projectile="C-12", energy_MeV=60.0, current_mA=1.0)
        # C-12 fully stripped: charge_state = 6
        expected = 1e-3 / (6 * 1.602176634e-19)
        assert math.isclose(b.particles_per_second, expected, rel_tol=1e-9)

    def test_frozen(self) -> None:
        b = Beam(projectile="p", energy_MeV=18.0, current_mA=0.1)
        with pytest.raises(AttributeError):
            b.energy_MeV = 20.0  # type: ignore[misc]


# ---------------------------------------------------------------------------
# Element
# ---------------------------------------------------------------------------

class TestElement:
    """Tests for the Element dataclass."""

    def test_valid_element(self) -> None:
        e = _cu_element()
        assert e.symbol == "Cu"
        assert e.Z == 29
        assert len(e.isotopes) == 2

    def test_zero_Z(self) -> None:
        with pytest.raises(ValueError, match="Z must be >= 1"):
            Element(symbol="X", Z=0, isotopes={1: 1.0})

    def test_negative_Z(self) -> None:
        with pytest.raises(ValueError, match="Z must be >= 1"):
            Element(symbol="X", Z=-1, isotopes={1: 1.0})

    def test_empty_isotopes(self) -> None:
        with pytest.raises(ValueError, match="isotopes dict must not be empty"):
            Element(symbol="X", Z=1, isotopes={})

    def test_bad_abundance_sum(self) -> None:
        with pytest.raises(ValueError, match="isotope abundances must sum to"):
            Element(symbol="Cu", Z=29, isotopes={63: 0.5, 65: 0.3})

    def test_abundance_tolerance(self) -> None:
        # Should pass within rel_tol=1e-3
        Element(symbol="Cu", Z=29, isotopes={63: 0.6920, 65: 0.3085})


# ---------------------------------------------------------------------------
# Layer
# ---------------------------------------------------------------------------

class TestLayer:
    """Tests for the Layer dataclass."""

    def test_valid_thickness(self) -> None:
        layer = _make_layer(thickness_cm=0.05)
        assert layer.thickness_cm == 0.05

    def test_valid_areal_density(self) -> None:
        layer = _make_layer(
            thickness_cm=None, areal_density_g_cm2=0.5,
        )
        assert layer.areal_density_g_cm2 == 0.5

    def test_valid_energy_out(self) -> None:
        layer = _make_layer(
            thickness_cm=None, energy_out_MeV=10.0,
        )
        assert layer.energy_out_MeV == 10.0

    def test_zero_specs_raises(self) -> None:
        with pytest.raises(ValueError, match="Exactly one"):
            Layer(
                density_g_cm3=8.96,
                elements=[(_cu_element(), 1.0)],
            )

    def test_two_specs_raises(self) -> None:
        with pytest.raises(ValueError, match="Exactly one"):
            Layer(
                density_g_cm3=8.96,
                elements=[(_cu_element(), 1.0)],
                thickness_cm=0.1,
                areal_density_g_cm2=0.5,
            )

    def test_three_specs_raises(self) -> None:
        with pytest.raises(ValueError, match="Exactly one"):
            Layer(
                density_g_cm3=8.96,
                elements=[(_cu_element(), 1.0)],
                thickness_cm=0.1,
                areal_density_g_cm2=0.5,
                energy_out_MeV=10.0,
            )

    def test_negative_density(self) -> None:
        with pytest.raises(ValueError, match="density must be positive"):
            Layer(
                density_g_cm3=-1.0,
                elements=[(_cu_element(), 1.0)],
                thickness_cm=0.1,
            )

    def test_empty_elements(self) -> None:
        with pytest.raises(ValueError, match="elements list must not be empty"):
            Layer(
                density_g_cm3=8.96,
                elements=[],
                thickness_cm=0.1,
            )

    def test_average_atomic_mass_single(self) -> None:
        cu = _cu_element()
        layer = _make_layer(elements=[(cu, 1.0)])
        expected = 63 * 0.6915 + 65 * 0.3085
        assert math.isclose(layer.average_atomic_mass, expected, rel_tol=1e-9)

    def test_average_atomic_mass_compound(self) -> None:
        cu = _cu_element()
        ni = _ni_element()
        layer = _make_layer(elements=[(cu, 0.5), (ni, 0.5)])
        cu_mass = 63 * 0.6915 + 65 * 0.3085
        ni_mass = 58 * 0.68 + 60 * 0.32
        expected = 0.5 * cu_mass + 0.5 * ni_mass
        assert math.isclose(layer.average_atomic_mass, expected, rel_tol=1e-9)

    def test_is_monitor_default(self) -> None:
        layer = _make_layer()
        assert layer.is_monitor is False

    def test_is_monitor_set(self) -> None:
        layer = _make_layer(is_monitor=True)
        assert layer.is_monitor is True


# ---------------------------------------------------------------------------
# TargetStack
# ---------------------------------------------------------------------------

class TestTargetStack:
    """Tests for the TargetStack dataclass."""

    def test_basic_construction(self) -> None:
        beam = Beam(projectile="p", energy_MeV=18.0, current_mA=0.1)
        layers = [_make_layer()]
        stack = TargetStack(beam=beam, layers=layers)
        assert stack.beam is beam
        assert len(stack.layers) == 1
        assert stack.irradiation_time_s == 86400.0
        assert stack.cooling_time_s == 86400.0
        assert stack.area_cm2 == 1.0

    def test_custom_times(self) -> None:
        beam = Beam(projectile="d", energy_MeV=15.0, current_mA=0.2)
        stack = TargetStack(
            beam=beam,
            layers=[_make_layer()],
            irradiation_time_s=3600.0,
            cooling_time_s=7200.0,
            area_cm2=0.5,
        )
        assert stack.irradiation_time_s == 3600.0
        assert stack.cooling_time_s == 7200.0
        assert stack.area_cm2 == 0.5


# ---------------------------------------------------------------------------
# DepthPoint
# ---------------------------------------------------------------------------

class TestDepthPoint:
    """Tests for the DepthPoint dataclass."""

    def test_construction(self) -> None:
        dp = DepthPoint(
            depth_cm=0.01,
            energy_MeV=17.5,
            dedx_MeV_cm=50.0,
            heat_W_cm3=1.0,
            production_rates={"Tc-99m": 1e6},
        )
        assert dp.depth_cm == 0.01
        assert dp.production_rates["Tc-99m"] == 1e6

    def test_frozen(self) -> None:
        dp = DepthPoint(
            depth_cm=0.0, energy_MeV=18.0, dedx_MeV_cm=0.0,
            heat_W_cm3=0.0, production_rates={},
        )
        with pytest.raises(AttributeError):
            dp.depth_cm = 1.0  # type: ignore[misc]


# ---------------------------------------------------------------------------
# IsotopeResult
# ---------------------------------------------------------------------------

class TestIsotopeResult:
    """Tests for the IsotopeResult dataclass."""

    def test_construction(self) -> None:
        ir = IsotopeResult(
            name="Tc-99m",
            Z=43,
            A=99,
            state="m",
            half_life_s=21624.0,
            production_rate=1e10,
            saturation_yield_Bq_uA=1e12,
            activity_Bq=5e9,
            time_grid_s=np.array([0.0, 3600.0]),
            activity_vs_time_Bq=np.array([5e9, 4e9]),
        )
        assert ir.name == "Tc-99m"
        assert ir.Z == 43
        assert len(ir.time_grid_s) == 2


# ---------------------------------------------------------------------------
# LayerResult / StackResult
# ---------------------------------------------------------------------------

class TestLayerResult:
    """Tests for the LayerResult dataclass."""

    def test_construction(self) -> None:
        layer = _make_layer()
        lr = LayerResult(
            layer=layer,
            energy_in=18.0,
            energy_out=15.0,
            delta_E_MeV=3.0,
            heat_kW=0.3,
            depth_profile=[],
            isotope_results={},
        )
        assert lr.delta_E_MeV == 3.0
        assert lr.isotope_results == {}


class TestStackResult:
    """Tests for the StackResult dataclass."""

    def test_construction(self) -> None:
        beam = Beam(projectile="p", energy_MeV=18.0, current_mA=0.1)
        stack = TargetStack(beam=beam, layers=[_make_layer()])
        sr = StackResult(
            stack=stack,
            layer_results=[],
            irradiation_time_s=86400.0,
            cooling_time_s=86400.0,
        )
        assert sr.stack is stack
        assert sr.layer_results == []


# ---------------------------------------------------------------------------
# DatabaseProtocol
# ---------------------------------------------------------------------------

class TestDatabaseProtocol:
    """Tests for the DatabaseProtocol."""

    def test_runtime_checkable(self) -> None:
        """A class implementing all methods should pass isinstance check."""

        class MockDB:
            def get_cross_sections(
                self, projectile: str, target_Z: int, target_A: int,
            ) -> list[CrossSectionData]:
                return []

            def get_stopping_power(
                self, source: str, target_Z: int,
            ) -> tuple[np.ndarray[Any, np.dtype[np.float64]], np.ndarray[Any, np.dtype[np.float64]]]:
                return np.array([]), np.array([])

            def get_natural_abundances(
                self, Z: int,
            ) -> dict[int, tuple[float, float]]:
                return {}

            def get_decay_data(
                self, Z: int, A: int, state: str = "",
            ) -> DecayData | None:
                return None

            def get_element_symbol(self, Z: int) -> str:
                return ""

            def get_element_Z(self, symbol: str) -> int:
                return 0

        assert isinstance(MockDB(), DatabaseProtocol)

    def test_non_conforming_class_fails(self) -> None:
        """A class missing methods should not satisfy the protocol."""

        class NotADB:
            pass

        assert not isinstance(NotADB(), DatabaseProtocol)


# ---------------------------------------------------------------------------
# DB data structures
# ---------------------------------------------------------------------------

class TestCrossSectionData:
    """Tests for the CrossSectionData dataclass."""

    def test_construction(self) -> None:
        xs = CrossSectionData(
            residual_Z=43,
            residual_A=99,
            state="m",
            energies_MeV=np.array([10.0, 15.0, 20.0]),
            xs_mb=np.array([100.0, 500.0, 300.0]),
        )
        assert xs.residual_Z == 43
        assert len(xs.energies_MeV) == 3


class TestDecayData:
    """Tests for the DecayData dataclass."""

    def test_construction(self) -> None:
        dd = DecayData(
            Z=43, A=99, state="m",
            half_life_s=21624.0,
            decay_modes=[
                DecayMode(
                    mode="IT",
                    daughter_Z=43,
                    daughter_A=99,
                    daughter_state="",
                    branching=0.88,
                ),
            ],
        )
        assert dd.half_life_s == 21624.0
        assert len(dd.decay_modes) == 1

    def test_stable_nuclide(self) -> None:
        dd = DecayData(
            Z=29, A=63, state="",
            half_life_s=None,
            decay_modes=[
                DecayMode(
                    mode="stable",
                    daughter_Z=None,
                    daughter_A=None,
                    daughter_state="",
                    branching=1.0,
                ),
            ],
        )
        assert dd.half_life_s is None


class TestDecayMode:
    """Tests for the DecayMode dataclass."""

    def test_construction(self) -> None:
        dm = DecayMode(
            mode="beta-",
            daughter_Z=44,
            daughter_A=99,
            daughter_state="",
            branching=1.0,
        )
        assert dm.mode == "beta-"
        assert dm.branching == 1.0

    def test_frozen(self) -> None:
        dm = DecayMode(
            mode="EC", daughter_Z=42, daughter_A=99,
            daughter_state="", branching=0.12,
        )
        with pytest.raises(AttributeError):
            dm.mode = "beta+"  # type: ignore[misc]
