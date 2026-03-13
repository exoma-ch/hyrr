"""Integration tests for hyrr.api JSON bridge (require nuclear data)."""

from __future__ import annotations

import json

import pytest

from hyrr.api import (
    _resolve_material,
    config_to_stack,
    run_simulation,
    run_simulation_from_json,
)


class TestResolveMaterial:
    def test_element_symbol(self, database):
        elements, density = _resolve_material(database, "Cu", None)
        assert len(elements) > 0
        assert density == pytest.approx(8.96, abs=0.01)

    def test_isotope_notation(self, database):
        elements, density = _resolve_material(database, "Mo-100", None)
        assert len(elements) == 1
        elem, frac = elements[0]
        assert elem.symbol == "Mo"
        assert 100 in elem.isotopes
        assert elem.isotopes[100] == pytest.approx(1.0)

    def test_enriched_isotope(self, database):
        enrichment = {"Mo": {100: 0.995, 98: 0.005}}
        elements, density = _resolve_material(database, "Mo-100", enrichment)
        elem, _ = elements[0]
        assert elem.isotopes[100] == pytest.approx(0.995)
        assert elem.isotopes[98] == pytest.approx(0.005)


class TestConfigToStack:
    def test_simple_config(self, database):
        config = {
            "beam": {"projectile": "p", "energy_MeV": 16, "current_mA": 0.15},
            "layers": [{"material": "Cu", "thickness_cm": 0.1}],
            "irradiation_s": 3600,
            "cooling_s": 3600,
        }
        stack = config_to_stack(database, config)
        assert stack.beam.projectile == "p"
        assert stack.beam.energy_MeV == 16
        assert len(stack.layers) == 1
        assert stack.layers[0].thickness_cm == 0.1

    def test_multi_layer_config(self, database):
        config = {
            "beam": {"projectile": "p", "energy_MeV": 16, "current_mA": 0.15},
            "layers": [
                {"material": "Cu", "thickness_cm": 0.0025},
                {
                    "material": "Mo-100",
                    "energy_out_MeV": 12,
                    "enrichment": {"Mo": {"100": 0.995, "98": 0.005}},
                },
            ],
            "irradiation_s": 86400,
            "cooling_s": 86400,
        }
        stack = config_to_stack(database, config)
        assert len(stack.layers) == 2
        assert stack.layers[1].energy_out_MeV == 12


class TestRunSimulation:
    def test_run_simulation(self, database):
        config = {
            "beam": {"projectile": "p", "energy_MeV": 16, "current_mA": 0.15},
            "layers": [{"material": "Cu", "thickness_cm": 0.1}],
            "irradiation_s": 3600,
            "cooling_s": 3600,
        }
        result = run_simulation(database, config)
        assert "config" in result
        assert "layers" in result
        assert "timestamp" in result
        assert len(result["layers"]) == 1
        assert len(result["layers"][0]["isotopes"]) > 0
        # Verify JSON-serializable (no NaN)
        serialized = json.dumps(result)
        assert "NaN" not in serialized

    def test_run_from_json(self, data_path):
        config = {
            "beam": {"projectile": "p", "energy_MeV": 16, "current_mA": 0.15},
            "layers": [{"material": "Cu", "thickness_cm": 0.1}],
            "irradiation_s": 3600,
            "cooling_s": 3600,
        }
        result = run_simulation_from_json(json.dumps(config), str(data_path))
        assert len(result["layers"]) == 1
        serialized = json.dumps(result)
        assert "NaN" not in serialized
