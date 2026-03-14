"""Integration tests for neutron activation with real TENDL-2025 data.

Requires nucl-parquet data directory with tendl-2025 library.
All tests are skipped if the data is not available.
"""

from __future__ import annotations

from pathlib import Path

import numpy as np
import pytest

from tests.integration.conftest import requires_db

pytestmark = [pytest.mark.integration, requires_db]


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _make_db(data_path: Path):
    """Create a DataStore with tendl-2025 library."""
    from hyrr.db import DataStore

    return DataStore(data_path, library="tendl-2025")


def _mo_element():
    """Natural Mo element (simplified: Mo-98 and Mo-100 dominate)."""
    from hyrr.models import Element

    return Element(
        symbol="Mo",
        Z=42,
        isotopes={
            92: 0.1453,
            94: 0.0915,
            95: 0.1584,
            96: 0.1667,
            97: 0.0960,
            98: 0.2439,
            100: 0.0982,
        },
    )


def _cu_element():
    """Natural Cu element."""
    from hyrr.models import Element

    return Element(
        symbol="Cu",
        Z=29,
        isotopes={
            63: 0.6915,
            65: 0.3085,
        },
    )


def _make_layer(element, density: float, thickness_cm: float):
    """Create a Layer with the given element at 100% composition."""
    from hyrr.models import Layer

    layer = Layer(
        density_g_cm3=density,
        elements=[(element, 1.0)],
        thickness_cm=thickness_cm,
    )
    # Set internal _thickness for neutron code
    layer._thickness = thickness_cm
    return layer


# ---------------------------------------------------------------------------
# Tests
# ---------------------------------------------------------------------------


class TestMo98NgammaMo99:
    """Mo-98(n,gamma)Mo-99 thermal neutron activation."""

    def test_production_rate_positive(self, data_path: Path) -> None:
        """Thermal neutron activation of Mo should produce isotopes."""
        from hyrr.neutrons import ThermalFlux, compute_neutron_activation

        db = _make_db(data_path)
        mo = _mo_element()
        layer = _make_layer(mo, density=10.28, thickness_cm=0.1)

        flux = ThermalFlux(total_flux=1e14, kT_eV=0.0253)
        result = compute_neutron_activation(
            db,
            layer,
            flux,
            irradiation_time_s=3600.0,
            cooling_time_s=0.0,
        )

        # Should have at least one isotope produced
        assert len(result.isotope_results) > 0
        # At least one channel should have non-zero production
        total_rate = sum(iso.production_rate for iso in result.isotope_results.values())
        assert total_rate > 0, (
            f"No production from thermal Mo activation; "
            f"products: {list(result.isotope_results.keys())}"
        )

    def test_transmission_between_zero_and_one(self, data_path: Path) -> None:
        """Transmission should be in (0, 1) for a finite target."""
        from hyrr.neutrons import ThermalFlux, compute_neutron_activation

        db = _make_db(data_path)
        mo = _mo_element()
        layer = _make_layer(mo, density=10.28, thickness_cm=0.1)
        flux = ThermalFlux(total_flux=1e14, kT_eV=0.0253)

        result = compute_neutron_activation(
            db,
            layer,
            flux,
            irradiation_time_s=3600.0,
            cooling_time_s=0.0,
        )

        assert 0.0 < result.transmission < 1.0


class TestThinVsThick:
    """Thin target should transmit more neutrons than thick target."""

    def test_thin_higher_transmission(self, data_path: Path) -> None:
        from hyrr.neutrons import ThermalFlux, compute_neutron_activation

        db = _make_db(data_path)
        mo = _mo_element()
        flux = ThermalFlux(total_flux=1e14, kT_eV=0.0253)

        thin_layer = _make_layer(mo, density=10.28, thickness_cm=0.001)
        thick_layer = _make_layer(mo, density=10.28, thickness_cm=1.0)

        thin_result = compute_neutron_activation(
            db,
            thin_layer,
            flux,
            irradiation_time_s=3600.0,
            cooling_time_s=0.0,
        )
        thick_result = compute_neutron_activation(
            db,
            thick_layer,
            flux,
            irradiation_time_s=3600.0,
            cooling_time_s=0.0,
        )

        assert thin_result.transmission > thick_result.transmission
        # Thin target transmission should be close to 1
        assert thin_result.transmission > 0.99

    def test_thick_target_more_production(self, data_path: Path) -> None:
        """Thick target should produce more total activity than thin."""
        from hyrr.neutrons import ThermalFlux, compute_neutron_activation

        db = _make_db(data_path)
        mo = _mo_element()
        flux = ThermalFlux(total_flux=1e14, kT_eV=0.0253)

        thin_layer = _make_layer(mo, density=10.28, thickness_cm=0.001)
        thick_layer = _make_layer(mo, density=10.28, thickness_cm=1.0)

        thin_result = compute_neutron_activation(
            db,
            thin_layer,
            flux,
            irradiation_time_s=3600.0,
            cooling_time_s=0.0,
        )
        thick_result = compute_neutron_activation(
            db,
            thick_layer,
            flux,
            irradiation_time_s=3600.0,
            cooling_time_s=0.0,
        )

        thin_total = sum(
            iso.production_rate for iso in thin_result.isotope_results.values()
        )
        thick_total = sum(
            iso.production_rate for iso in thick_result.isotope_results.values()
        )
        assert thick_total > thin_total


class TestCuWeisskopf:
    """Copper activation with Weisskopf evaporation spectrum."""

    def test_cu_activation_produces_isotopes(self, data_path: Path) -> None:
        from hyrr.neutrons import WeisskopfFlux, compute_neutron_activation

        db = _make_db(data_path)
        cu = _cu_element()
        layer = _make_layer(cu, density=8.96, thickness_cm=0.1)

        flux = WeisskopfFlux(total_flux=1e12, temperature_MeV=1.5)
        result = compute_neutron_activation(
            db,
            layer,
            flux,
            irradiation_time_s=3600.0,
            cooling_time_s=0.0,
        )

        assert len(result.isotope_results) > 0
        total_rate = sum(iso.production_rate for iso in result.isotope_results.values())
        assert total_rate > 0


class TestSecondaryNeutronActivation:
    """Test compute_secondary_neutron_activation with a mock LayerResult."""

    def test_secondary_from_p_mo100(self, data_path: Path) -> None:
        """Secondary neutron activation from proton bombardment of Mo."""
        from hyrr.models import IsotopeResult, LayerResult
        from hyrr.neutrons import (
            compute_secondary_neutron_activation,
        )

        db = _make_db(data_path)
        mo = _mo_element()
        layer = _make_layer(mo, density=10.28, thickness_cm=0.5)

        # Build a minimal LayerResult with one (p,2n) channel
        tc99_result = IsotopeResult(
            name="Tc-99m",
            Z=43,
            A=99,
            state="m",
            half_life_s=21624.0,
            production_rate=1e10,
            saturation_yield_Bq_uA=1e10,
            activity_Bq=5e9,
            time_grid_s=np.array([0.0, 3600.0]),
            activity_vs_time_Bq=np.array([0.0, 5e9]),
            source="direct",
        )

        layer_result = LayerResult(
            layer=layer,
            energy_in=30.0,
            energy_out=10.0,
            delta_E_MeV=20.0,
            heat_kW=0.02,
            depth_profile=[],
            isotope_results={"Tc-99m": tc99_result},
        )

        n_result = compute_secondary_neutron_activation(
            db,
            layer_result,
            projectile_Z=1,
            projectile_A=1,
            irradiation_time_s=3600.0,
            cooling_time_s=0.0,
        )

        # Should produce a result (Mo target has neutron capture channels)
        assert n_result is not None
        assert n_result.transmission > 0
        assert len(n_result.isotope_results) > 0

    def test_neutron_multiplicity_used_correctly(self) -> None:
        """Verify Mo-100(p,2n)Tc-99m gives 2 neutrons (no data needed)."""
        from hyrr.neutrons import neutron_multiplicity

        n = neutron_multiplicity(
            target_Z=42,
            target_A=100,
            projectile_Z=1,
            projectile_A=1,
            residual_Z=43,
            residual_A=99,
        )
        assert n == 2
