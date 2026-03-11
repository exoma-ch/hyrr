"""Tests for hyrr.serialization module."""

from __future__ import annotations

import json

import numpy as np
import pytest

from hyrr.models import (
    Beam,
    DepthPoint,
    Element,
    IsotopeResult,
    Layer,
    LayerResult,
    StackResult,
    TargetStack,
)
from hyrr.serialization import (
    config_from_json,
    config_to_json,
    load_result,
    result_from_json_str,
    result_to_json_str,
    save_result,
    stack_to_config,
)


@pytest.fixture()
def sample_isotope_result() -> IsotopeResult:
    """Synthetic Tc-99m isotope result."""
    time = np.linspace(0, 172800, 200)
    activity = np.concatenate([
        1e10 * (1 - np.exp(-0.001 * time[:100])),
        1e10 * 0.632 * np.exp(-np.log(2) / 21636 * np.linspace(0, 86400, 100)),
    ])
    return IsotopeResult(
        name="Tc-99m",
        Z=43,
        A=99,
        state="m",
        half_life_s=21636.0,
        production_rate=1e10,
        saturation_yield_Bq_uA=6.67e7,
        activity_Bq=float(activity[100]),
        time_grid_s=time,
        activity_vs_time_Bq=activity,
    )


@pytest.fixture()
def sample_stack_result(sample_isotope_result: IsotopeResult) -> StackResult:
    """Synthetic StackResult with one layer and one isotope."""
    elem = Element(symbol="Mo", Z=42, isotopes={100: 1.0})
    layer = Layer(
        density_g_cm3=10.22,
        elements=[(elem, 1.0)],
        thickness_cm=0.02,
    )
    beam = Beam(projectile="p", energy_MeV=16.0, current_mA=0.15)
    stack = TargetStack(
        beam=beam,
        layers=[layer],
        irradiation_time_s=86400.0,
        cooling_time_s=43200.0,
        area_cm2=2.0,
    )

    dp = [
        DepthPoint(
            depth_cm=i * 0.001,
            energy_MeV=16 - i * 0.08,
            dedx_MeV_cm=200.0,
            heat_W_cm3=50.0,
            production_rates={"Tc-99m": 1e8},
        )
        for i in range(20)
    ]
    lr = LayerResult(
        layer=layer,
        energy_in=16.0,
        energy_out=12.0,
        delta_E_MeV=4.0,
        heat_kW=0.6,
        depth_profile=dp,
        isotope_results={"Tc-99m": sample_isotope_result},
    )
    return StackResult(
        stack=stack,
        layer_results=[lr],
        irradiation_time_s=86400.0,
        cooling_time_s=43200.0,
    )


class TestResultRoundTrip:
    """WU3: Round-trip StackResult through JSON."""

    def test_round_trip_preserves_fields(
        self, sample_stack_result: StackResult
    ) -> None:
        json_str = result_to_json_str(sample_stack_result)
        parsed = result_from_json_str(json_str)

        # Config present
        assert "config" in parsed
        assert parsed["config"]["beam"]["projectile"] == "p"
        assert parsed["config"]["beam"]["energy_MeV"] == 16.0

        # Times
        assert parsed["irradiation_time_s"] == 86400.0
        assert parsed["cooling_time_s"] == 43200.0

        # Layer results
        assert len(parsed["layer_results"]) == 1
        lr = parsed["layer_results"][0]
        assert lr["energy_in"] == 16.0
        assert lr["energy_out"] == 12.0
        assert lr["delta_E_MeV"] == 4.0
        assert lr["heat_kW"] == pytest.approx(0.6)

        # Isotope results
        iso = lr["isotope_results"]["Tc-99m"]
        assert iso["name"] == "Tc-99m"
        assert iso["Z"] == 43
        assert iso["A"] == 99
        assert iso["state"] == "m"
        assert iso["half_life_s"] == pytest.approx(21636.0)
        assert len(iso["time_grid_s"]) == 200
        assert len(iso["activity_vs_time_Bq"]) == 200

    def test_nan_inf_handling(self, sample_stack_result: StackResult) -> None:
        """NaN and Inf values should become null in JSON."""
        lr = sample_stack_result.layer_results[0]
        iso = lr.isotope_results["Tc-99m"]
        # Inject NaN/Inf
        object.__setattr__(iso, "production_rate", float("nan"))
        object.__setattr__(iso, "saturation_yield_Bq_uA", float("inf"))

        json_str = result_to_json_str(sample_stack_result)
        parsed = json.loads(json_str)
        iso_parsed = parsed["layer_results"][0]["isotope_results"]["Tc-99m"]
        assert iso_parsed["production_rate"] is None
        assert iso_parsed["saturation_yield_Bq_uA"] is None

    def test_valid_json(self, sample_stack_result: StackResult) -> None:
        """Output must be valid JSON (no NaN literals)."""
        json_str = result_to_json_str(sample_stack_result)
        # json.loads would raise if invalid
        parsed = json.loads(json_str)
        assert isinstance(parsed, dict)


class TestSaveLoadResult:
    """WU3: save_result / load_result file operations."""

    def test_save_load_round_trip(
        self, sample_stack_result: StackResult, tmp_path: object
    ) -> None:
        from pathlib import Path

        path = str(Path(str(tmp_path)) / "result.json")
        save_result(sample_stack_result, path)

        loaded = load_result(path)
        assert loaded["config"]["beam"]["energy_MeV"] == 16.0
        assert len(loaded["layer_results"]) == 1
        assert "Tc-99m" in loaded["layer_results"][0]["isotope_results"]

    def test_file_is_valid_json(
        self, sample_stack_result: StackResult, tmp_path: object
    ) -> None:
        from pathlib import Path

        path = str(Path(str(tmp_path)) / "result.json")
        save_result(sample_stack_result, path)

        with open(path) as f:
            data = json.load(f)
        assert "config" in data


class TestConfigRoundTrip:
    """WU4: Round-trip TargetStack config through JSON."""

    def test_stack_to_config_beam(self, sample_stack_result: StackResult) -> None:
        config = stack_to_config(sample_stack_result.stack)
        assert config["beam"]["projectile"] == "p"
        assert config["beam"]["energy_MeV"] == 16.0
        assert config["beam"]["current_mA"] == 0.15

    def test_stack_to_config_layers(self, sample_stack_result: StackResult) -> None:
        config = stack_to_config(sample_stack_result.stack)
        assert len(config["layers"]) == 1
        layer = config["layers"][0]
        assert layer["density_g_cm3"] == pytest.approx(10.22)
        assert layer["thickness_cm"] == pytest.approx(0.02)

        # Element info
        elem_entry = layer["elements"][0]
        elem_dict, frac = elem_entry
        assert elem_dict["symbol"] == "Mo"
        assert elem_dict["Z"] == 42
        assert 100 in elem_dict["isotopes"]
        assert elem_dict["isotopes"][100] == pytest.approx(1.0)
        assert frac == pytest.approx(1.0)

    def test_stack_to_config_times(self, sample_stack_result: StackResult) -> None:
        config = stack_to_config(sample_stack_result.stack)
        assert config["irradiation_time_s"] == 86400.0
        assert config["cooling_time_s"] == 43200.0
        assert config["area_cm2"] == 2.0

    def test_config_json_round_trip(self, sample_stack_result: StackResult) -> None:
        json_str = config_to_json(sample_stack_result.stack)
        parsed = config_from_json(json_str)
        assert parsed["beam"]["projectile"] == "p"
        assert len(parsed["layers"]) == 1
        assert parsed["irradiation_time_s"] == 86400.0

    def test_no_numpy_in_config(self, sample_stack_result: StackResult) -> None:
        """Config dict must contain only native Python types."""
        config = stack_to_config(sample_stack_result.stack)
        # json.dumps will raise if any numpy types remain
        json_str = json.dumps(config)
        assert isinstance(json_str, str)

    def test_config_enrichment_preserved(self) -> None:
        """Enriched isotopes should round-trip correctly."""
        elem = Element(symbol="Mo", Z=42, isotopes={98: 0.05, 100: 0.95})
        layer = Layer(
            density_g_cm3=10.22,
            elements=[(elem, 1.0)],
            thickness_cm=0.01,
        )
        beam = Beam(projectile="d", energy_MeV=20.0, current_mA=0.1)
        stack = TargetStack(beam=beam, layers=[layer])

        json_str = config_to_json(stack)
        parsed = config_from_json(json_str)

        isotopes = parsed["layers"][0]["elements"][0][0]["isotopes"]
        # JSON keys are strings, values should be close
        assert isotopes["98"] == pytest.approx(0.05)
        assert isotopes["100"] == pytest.approx(0.95)
