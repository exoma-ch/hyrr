"""Tests for hyrr.compute orchestrator."""

from __future__ import annotations

import numpy as np
import pytest

from hyrr.compute import _layer_composition, compute_stack
from hyrr.models import Beam, Element, Layer, TargetStack
from hyrr.stopping import _interpolator_cache

# ---------------------------------------------------------------------------
# MockDB for orchestrator tests
# ---------------------------------------------------------------------------


class MockDB:
    """Mock database with synthetic data for orchestrator testing.

    Provides PSTAR stopping power for Z=42 (Mo) and Z=29 (Cu),
    a single cross-section channel (p + Mo-100 -> Tc-99m),
    decay data for Tc-99m, and element symbol lookups.
    """

    def __hash__(self) -> int:
        return id(self)

    def __eq__(self, other: object) -> bool:
        return self is other

    def get_stopping_power(
        self, source: str, target_Z: int
    ) -> tuple[np.ndarray, np.ndarray]:
        if source == "PSTAR" and target_Z == 42:
            energies = np.array([1.0, 2.0, 5.0, 10.0, 15.0, 20.0, 50.0, 100.0])
            dedx = np.array([50.0, 40.0, 25.0, 18.0, 15.0, 13.0, 9.0, 7.0])
            return energies, dedx
        if source == "PSTAR" and target_Z == 29:
            energies = np.array([1.0, 2.0, 5.0, 10.0, 15.0, 20.0, 50.0, 100.0])
            dedx = np.array([40.0, 32.0, 20.0, 14.0, 12.0, 10.0, 7.0, 5.5])
            return energies, dedx
        return np.array([]), np.array([])

    def get_cross_sections(
        self, projectile: str, target_Z: int, target_A: int
    ) -> list:
        from hyrr.db import CrossSectionData

        if projectile == "p" and target_Z == 42 and target_A == 100:
            return [
                CrossSectionData(
                    residual_Z=43,
                    residual_A=99,
                    state="m",
                    energies_MeV=np.array(
                        [8.0, 10.0, 12.0, 14.0, 16.0, 18.0, 20.0]
                    ),
                    xs_mb=np.array([0.0, 50.0, 150.0, 200.0, 180.0, 120.0, 60.0]),
                ),
            ]
        return []

    def get_natural_abundances(self, Z: int) -> dict:
        return {}

    def get_decay_data(self, Z: int, A: int, state: str = ""):
        from hyrr.db import DecayData, DecayMode

        if Z == 43 and A == 99 and state == "m":
            return DecayData(
                Z=43,
                A=99,
                state="m",
                half_life_s=21636.0,
                decay_modes=[
                    DecayMode(
                        mode="IT",
                        daughter_Z=43,
                        daughter_A=99,
                        daughter_state="",
                        branching=1.0,
                    )
                ],
            )
        return None

    def get_element_symbol(self, Z: int) -> str:
        symbols = {42: "Mo", 43: "Tc", 29: "Cu"}
        return symbols.get(Z, f"Z{Z}")

    def get_element_Z(self, symbol: str) -> int:
        return {"Mo": 42, "Tc": 43, "Cu": 29}.get(symbol, 0)


# ---------------------------------------------------------------------------
# Fixtures
# ---------------------------------------------------------------------------


@pytest.fixture()
def db() -> MockDB:
    _interpolator_cache.clear()
    return MockDB()


@pytest.fixture()
def mo100_element() -> Element:
    return Element(symbol="Mo", Z=42, isotopes={100: 1.0})


@pytest.fixture()
def cu_element() -> Element:
    return Element(symbol="Cu", Z=29, isotopes={63: 0.6915, 65: 0.3085})


# ---------------------------------------------------------------------------
# Layer composition conversion
# ---------------------------------------------------------------------------


class TestLayerComposition:
    """Tests for _layer_composition helper."""

    def test_single_element_pure(self, mo100_element: Element) -> None:
        """Pure enriched Mo-100 layer -> mass fraction 1.0 for Z=42."""
        layer = Layer(
            density_g_cm3=10.22,
            elements=[(mo100_element, 1.0)],
            energy_out_MeV=12.0,
        )
        comp = _layer_composition(layer)
        assert len(comp) == 1
        assert comp[0][0] == 42
        assert comp[0][1] == pytest.approx(1.0)

    def test_two_elements_normalization(
        self, mo100_element: Element, cu_element: Element
    ) -> None:
        """Two-element layer produces normalized mass fractions."""
        layer = Layer(
            density_g_cm3=8.0,
            elements=[(mo100_element, 0.5), (cu_element, 0.5)],
            thickness_cm=0.01,
        )
        comp = _layer_composition(layer)
        total = sum(w for _, w in comp)
        assert total == pytest.approx(1.0)


# ---------------------------------------------------------------------------
# compute_stack tests
# ---------------------------------------------------------------------------


class TestComputeStackSingleLayer:
    """Tests for compute_stack with a single Mo-100 layer."""

    def test_result_structure(self, db: MockDB, mo100_element: Element) -> None:
        """StackResult has the expected structure."""
        layer = Layer(
            density_g_cm3=10.22,
            elements=[(mo100_element, 1.0)],
            energy_out_MeV=12.0,
        )
        stack = TargetStack(
            beam=Beam(projectile="p", energy_MeV=16.0, current_mA=0.15),
            layers=[layer],
        )
        result = compute_stack(db, stack)

        assert len(result.layer_results) == 1
        lr = result.layer_results[0]
        assert lr.energy_in == pytest.approx(16.0)
        assert lr.energy_out == pytest.approx(12.0, abs=0.5)
        assert lr.delta_E_MeV > 0
        assert lr.layer is layer

    def test_isotope_produced(self, db: MockDB, mo100_element: Element) -> None:
        """Tc-99m should appear in isotope results."""
        layer = Layer(
            density_g_cm3=10.22,
            elements=[(mo100_element, 1.0)],
            energy_out_MeV=12.0,
        )
        stack = TargetStack(
            beam=Beam(projectile="p", energy_MeV=16.0, current_mA=0.15),
            layers=[layer],
        )
        result = compute_stack(db, stack)
        lr = result.layer_results[0]

        assert "Tc-99m" in lr.isotope_results
        tc99m = lr.isotope_results["Tc-99m"]
        assert tc99m.Z == 43
        assert tc99m.A == 99
        assert tc99m.state == "m"
        assert tc99m.production_rate > 0
        assert tc99m.half_life_s == pytest.approx(21636.0)
        assert tc99m.saturation_yield_Bq_uA > 0
        assert len(tc99m.time_grid_s) > 0
        assert len(tc99m.activity_vs_time_Bq) == len(tc99m.time_grid_s)

    def test_depth_profile_exists(self, db: MockDB, mo100_element: Element) -> None:
        """Layer should have a non-empty depth profile."""
        layer = Layer(
            density_g_cm3=10.22,
            elements=[(mo100_element, 1.0)],
            energy_out_MeV=12.0,
        )
        stack = TargetStack(
            beam=Beam(projectile="p", energy_MeV=16.0, current_mA=0.15),
            layers=[layer],
        )
        result = compute_stack(db, stack)
        lr = result.layer_results[0]

        assert len(lr.depth_profile) > 0
        assert lr.depth_profile[0].depth_cm == pytest.approx(0.0)
        assert all(dp.heat_W_cm3 >= 0 for dp in lr.depth_profile)

    def test_layer_computed_fields(self, db: MockDB, mo100_element: Element) -> None:
        """Layer mutable fields are set after compute_stack."""
        layer = Layer(
            density_g_cm3=10.22,
            elements=[(mo100_element, 1.0)],
            energy_out_MeV=12.0,
        )
        stack = TargetStack(
            beam=Beam(projectile="p", energy_MeV=16.0, current_mA=0.15),
            layers=[layer],
        )
        compute_stack(db, stack)

        assert layer._energy_in == pytest.approx(16.0)
        assert layer._energy_out == pytest.approx(12.0, abs=0.5)
        assert layer._thickness > 0

    def test_stopping_power_sources(self, db: MockDB, mo100_element: Element) -> None:
        """Stopping power sources are recorded."""
        layer = Layer(
            density_g_cm3=10.22,
            elements=[(mo100_element, 1.0)],
            energy_out_MeV=12.0,
        )
        stack = TargetStack(
            beam=Beam(projectile="p", energy_MeV=16.0, current_mA=0.15),
            layers=[layer],
        )
        result = compute_stack(db, stack)
        lr = result.layer_results[0]

        assert 42 in lr.stopping_power_sources
        assert lr.stopping_power_sources[42] == "PSTAR"


class TestComputeStackTwoLayers:
    """Tests for energy propagation across two layers."""

    def test_energy_propagation(
        self, db: MockDB, mo100_element: Element, cu_element: Element
    ) -> None:
        """Second layer's energy_in equals first layer's energy_out."""
        layer1 = Layer(
            density_g_cm3=10.22,
            elements=[(mo100_element, 1.0)],
            energy_out_MeV=14.0,
        )
        layer2 = Layer(
            density_g_cm3=8.96,
            elements=[(cu_element, 1.0)],
            thickness_cm=0.01,
        )
        stack = TargetStack(
            beam=Beam(projectile="p", energy_MeV=16.0, current_mA=0.15),
            layers=[layer1, layer2],
        )
        result = compute_stack(db, stack)

        assert len(result.layer_results) == 2
        lr1 = result.layer_results[0]
        lr2 = result.layer_results[1]

        assert lr2.energy_in == pytest.approx(lr1.energy_out)
        assert lr2.energy_out < lr2.energy_in

    def test_total_energy_loss(
        self, db: MockDB, mo100_element: Element, cu_element: Element
    ) -> None:
        """Sum of delta_E across layers equals beam E_in - last E_out."""
        layer1 = Layer(
            density_g_cm3=10.22,
            elements=[(mo100_element, 1.0)],
            energy_out_MeV=14.0,
        )
        layer2 = Layer(
            density_g_cm3=8.96,
            elements=[(cu_element, 1.0)],
            thickness_cm=0.01,
        )
        stack = TargetStack(
            beam=Beam(projectile="p", energy_MeV=16.0, current_mA=0.15),
            layers=[layer1, layer2],
        )
        result = compute_stack(db, stack)

        total_dE = sum(lr.delta_E_MeV for lr in result.layer_results)
        expected = 16.0 - result.layer_results[-1].energy_out
        assert total_dE == pytest.approx(expected, rel=1e-6)


class TestComputeStackThickness:
    """Tests for layers specified by thickness or areal density."""

    def test_thickness_spec(self, db: MockDB, mo100_element: Element) -> None:
        """Layer with thickness_cm spec computes energy_out."""
        layer = Layer(
            density_g_cm3=10.22,
            elements=[(mo100_element, 1.0)],
            thickness_cm=0.02,
        )
        stack = TargetStack(
            beam=Beam(projectile="p", energy_MeV=16.0, current_mA=0.15),
            layers=[layer],
        )
        result = compute_stack(db, stack)
        lr = result.layer_results[0]

        assert lr.energy_out < 16.0
        assert lr.energy_out > 0

    def test_areal_density_spec(self, db: MockDB, mo100_element: Element) -> None:
        """Layer with areal_density_g_cm2 spec computes thickness and E_out."""
        density = 10.22
        thickness = 0.02
        areal = density * thickness

        layer = Layer(
            density_g_cm3=density,
            elements=[(mo100_element, 1.0)],
            areal_density_g_cm2=areal,
        )
        stack = TargetStack(
            beam=Beam(projectile="p", energy_MeV=16.0, current_mA=0.15),
            layers=[layer],
        )
        result = compute_stack(db, stack)
        lr = result.layer_results[0]

        assert layer._thickness == pytest.approx(thickness)
        assert lr.energy_out < 16.0
