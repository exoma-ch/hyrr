"""Tests for hyrr.api JSON bridge."""

import json

import pytest

from hyrr.api import (
    _resolve_material,
    _safe_float,
    config_to_stack,
    run_simulation,
    run_simulation_from_json,
)
from pathlib import Path

from hyrr.db import DataStore


def _find_data_dir() -> str:
    candidates = [
        Path(__file__).parent.parent.parent / "nucl-parquet",
        Path(__file__).parent.parent / "data" / "parquet",
    ]
    for p in candidates:
        if p.is_dir() and (p / "meta").is_dir():
            return str(p)
    return str(candidates[0])


DATA_DIR = _find_data_dir()


@pytest.fixture(scope="module")
def db():
    d = DataStore(DATA_DIR)
    yield d


class TestSafeFloat:
    def test_normal(self):
        assert _safe_float(3.14) == 3.14

    def test_nan(self):
        assert _safe_float(float("nan")) is None

    def test_inf(self):
        assert _safe_float(float("inf")) is None

    def test_neg_inf(self):
        assert _safe_float(float("-inf")) is None

    def test_zero(self):
        assert _safe_float(0.0) == 0.0


class TestResolveMaterial:
    def test_element_symbol(self, db):
        elements, density = _resolve_material(db, "Cu", None)
        assert len(elements) > 0
        assert density == pytest.approx(8.96, abs=0.01)

    def test_isotope_notation(self, db):
        elements, density = _resolve_material(db, "Mo-100", None)
        assert len(elements) == 1
        elem, frac = elements[0]
        assert elem.symbol == "Mo"
        assert 100 in elem.isotopes
        assert elem.isotopes[100] == pytest.approx(1.0)

    def test_enriched_isotope(self, db):
        enrichment = {"Mo": {100: 0.995, 98: 0.005}}
        elements, density = _resolve_material(db, "Mo-100", enrichment)
        elem, _ = elements[0]
        assert elem.isotopes[100] == pytest.approx(0.995)
        assert elem.isotopes[98] == pytest.approx(0.005)


class TestConfigToStack:
    def test_simple_config(self, db):
        config = {
            "beam": {"projectile": "p", "energy_MeV": 16, "current_mA": 0.15},
            "layers": [{"material": "Cu", "thickness_cm": 0.1}],
            "irradiation_s": 3600,
            "cooling_s": 3600,
        }
        stack = config_to_stack(db, config)
        assert stack.beam.projectile == "p"
        assert stack.beam.energy_MeV == 16
        assert len(stack.layers) == 1
        assert stack.layers[0].thickness_cm == 0.1

    def test_multi_layer_config(self, db):
        config = {
            "beam": {"projectile": "p", "energy_MeV": 16, "current_mA": 0.15},
            "layers": [
                {"material": "Cu", "thickness_cm": 0.0025},
                {"material": "Mo-100", "energy_out_MeV": 12,
                 "enrichment": {"Mo": {"100": 0.995, "98": 0.005}}},
            ],
            "irradiation_s": 86400,
            "cooling_s": 86400,
        }
        stack = config_to_stack(db, config)
        assert len(stack.layers) == 2
        assert stack.layers[1].energy_out_MeV == 12


@pytest.mark.integration
class TestRunSimulation:
    def test_run_simulation(self, db):
        config = {
            "beam": {"projectile": "p", "energy_MeV": 16, "current_mA": 0.15},
            "layers": [{"material": "Cu", "thickness_cm": 0.1}],
            "irradiation_s": 3600,
            "cooling_s": 3600,
        }
        result = run_simulation(db, config)
        assert "config" in result
        assert "layers" in result
        assert "timestamp" in result
        assert len(result["layers"]) == 1
        assert len(result["layers"][0]["isotopes"]) > 0
        # Verify JSON-serializable (no NaN)
        serialized = json.dumps(result)
        assert "NaN" not in serialized

    def test_run_from_json(self):
        config = {
            "beam": {"projectile": "p", "energy_MeV": 16, "current_mA": 0.15},
            "layers": [{"material": "Cu", "thickness_cm": 0.1}],
            "irradiation_s": 3600,
            "cooling_s": 3600,
        }
        result = run_simulation_from_json(json.dumps(config), DATA_DIR)
        assert len(result["layers"]) == 1
        serialized = json.dumps(result)
        assert "NaN" not in serialized
