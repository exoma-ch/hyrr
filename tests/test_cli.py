"""Tests for the HYRR CLI."""

from __future__ import annotations

import json
from pathlib import Path

import pytest

from hyrr.cli import _extract_isotopes, _pct_diff, _toml_to_config, main


class TestTomlToConfig:
    def test_basic_conversion(self) -> None:
        """Test TOML to config dict conversion."""
        toml_data = {
            "beam": {"projectile": "p", "energy_MeV": 16.0, "current_mA": 0.15},
            "layers": [
                {
                    "material": "Mo",
                    "thickness_cm": 0.02,
                    "enrichment": {"Mo": {100: 0.999, 98: 0.001}},
                }
            ],
            "irradiation_s": 86400,
            "cooling_s": 86400,
        }
        config = _toml_to_config(toml_data)
        assert config["beam"]["projectile"] == "p"
        assert len(config["layers"]) == 1
        assert config["layers"][0]["material"] == "Mo"
        assert config["layers"][0]["thickness_cm"] == 0.02
        # Enrichment keys should be strings
        assert "100" in config["layers"][0]["enrichment"]["Mo"]

    def test_energy_out_layer(self) -> None:
        toml_data = {
            "beam": {"projectile": "p", "energy_MeV": 16.0, "current_mA": 0.15},
            "layers": [{"material": "Cu", "energy_out_MeV": 12.0}],
            "irradiation_s": 3600,
            "cooling_s": 3600,
        }
        config = _toml_to_config(toml_data)
        assert config["layers"][0]["energy_out_MeV"] == 12.0
        assert "thickness_cm" not in config["layers"][0]

    def test_areal_density_layer(self) -> None:
        toml_data = {
            "beam": {"projectile": "p", "energy_MeV": 16.0, "current_mA": 0.15},
            "layers": [{"material": "Cu", "areal_density_g_cm2": 0.5}],
            "irradiation_s": 3600,
            "cooling_s": 3600,
        }
        config = _toml_to_config(toml_data)
        assert config["layers"][0]["areal_density_g_cm2"] == 0.5

    def test_is_monitor_flag(self) -> None:
        toml_data = {
            "beam": {"projectile": "p", "energy_MeV": 16.0, "current_mA": 0.15},
            "layers": [{"material": "Cu", "thickness_cm": 0.01, "is_monitor": True}],
            "irradiation_s": 3600,
            "cooling_s": 3600,
        }
        config = _toml_to_config(toml_data)
        assert config["layers"][0]["is_monitor"] is True

    def test_multiple_layers(self) -> None:
        toml_data = {
            "beam": {"projectile": "p", "energy_MeV": 16.0, "current_mA": 0.15},
            "layers": [
                {"material": "Mo", "thickness_cm": 0.02},
                {"material": "Cu", "thickness_cm": 0.01},
            ],
            "irradiation_s": 3600,
            "cooling_s": 3600,
        }
        config = _toml_to_config(toml_data)
        assert len(config["layers"]) == 2
        assert config["layers"][0]["material"] == "Mo"
        assert config["layers"][1]["material"] == "Cu"


class TestCLIHelp:
    def test_no_args_shows_help(self, capsys):
        """No arguments prints help and exits 0."""
        result = main([])
        assert result == 0
        captured = capsys.readouterr()
        assert "HYRR" in captured.out

    def test_version(self, capsys):
        """--version shows version."""
        with pytest.raises(SystemExit) as exc_info:
            main(["--version"])
        assert exc_info.value.code == 0

    def test_help_flag(self):
        """--help exits with 0."""
        with pytest.raises(SystemExit) as exc_info:
            main(["--help"])
        assert exc_info.value.code == 0

    def test_info_help(self):
        with pytest.raises(SystemExit) as exc_info:
            main(["info", "--help"])
        assert exc_info.value.code == 0

    def test_run_help(self):
        with pytest.raises(SystemExit) as exc_info:
            main(["run", "--help"])
        assert exc_info.value.code == 0

    def test_generate_xs_help(self):
        with pytest.raises(SystemExit) as exc_info:
            main(["generate-xs", "--help"])
        assert exc_info.value.code == 0


class TestCLIInfo:
    def test_info_missing_dir(self, tmp_path):
        """info with nonexistent data dir returns error."""
        result = main(["info", "--data-dir", str(tmp_path / "nonexistent")])
        assert result == 1

    def test_info_with_data(self, tmp_path):
        """info with a valid parquet data dir shows stats."""
        import polars as pl

        # Build minimal parquet data dir
        meta = tmp_path / "meta"
        meta.mkdir()
        stopping = tmp_path / "stopping"
        stopping.mkdir()
        xs = tmp_path / "tendl-2024" / "xs"
        xs.mkdir(parents=True)

        pl.DataFrame({"Z": [42], "symbol": ["Mo"]}).cast(
            {"Z": pl.Int32}
        ).write_parquet(meta / "elements.parquet")
        pl.DataFrame({
            "Z": [42], "A": [100], "symbol": ["Mo"],
            "abundance": [0.0974], "atomic_mass": [99.907],
        }).cast({"Z": pl.Int32, "A": pl.Int32}).write_parquet(
            meta / "abundances.parquet"
        )
        pl.DataFrame({
            "Z": [43], "A": [99], "state": ["m"], "half_life_s": [21624.0],
            "decay_mode": ["IT"], "daughter_Z": [43], "daughter_A": [99],
            "daughter_state": [""], "branching": [1.0],
        }).cast({
            "Z": pl.Int32, "A": pl.Int32,
            "daughter_Z": pl.Int32, "daughter_A": pl.Int32,
        }).write_parquet(meta / "decay.parquet")
        pl.DataFrame({
            "source": ["PSTAR"], "target_Z": [1],
            "energy_MeV": [10.0], "dedx": [50.0],
        }).cast({"target_Z": pl.Int32}).write_parquet(
            stopping / "stopping.parquet"
        )
        pl.DataFrame({
            "target_A": [100], "residual_Z": [43], "residual_A": [99],
            "state": ["m"], "energy_MeV": [10.0], "xs_mb": [100.0],
        }).cast({
            "target_A": pl.Int32, "residual_Z": pl.Int32,
            "residual_A": pl.Int32,
        }).write_parquet(xs / "p_Mo.parquet")

        result = main(["info", "--data-dir", str(tmp_path)])
        assert result == 0


class TestCLIRun:
    def test_run_missing_input(self, tmp_path):
        result = main(["run", str(tmp_path / "nonexistent.toml")])
        assert result == 1


class TestPctDiff:
    def test_normal(self) -> None:
        assert _pct_diff(100.0, 110.0) == pytest.approx(10.0)

    def test_zero_base(self) -> None:
        assert _pct_diff(0.0, 0.0) == 0.0

    def test_zero_base_nonzero(self) -> None:
        assert _pct_diff(0.0, 5.0) == float("inf")


class TestExtractIsotopes:
    def test_basic(self) -> None:
        data = {
            "layers": [{
                "layer_index": 0,
                "isotopes": [
                    {"name": "Tc-99m", "activity_Bq": 5e9, "saturation_yield_Bq_uA": 6.67e7},
                    {"name": "Mo-99", "activity_Bq": 1e8, "saturation_yield_Bq_uA": 1e6},
                ],
            }]
        }
        isos = _extract_isotopes(data)
        assert "Tc-99m" in isos
        assert "Mo-99" in isos

    def test_layer_filter(self) -> None:
        data = {
            "layers": [
                {"layer_index": 0, "isotopes": [{"name": "Tc-99m", "activity_Bq": 5e9}]},
                {"layer_index": 1, "isotopes": [{"name": "Cu-64", "activity_Bq": 1e8}]},
            ]
        }
        isos = _extract_isotopes(data, layer_filter=1)
        assert "Cu-64" in isos
        assert "Tc-99m" not in isos


class TestCompareCommand:
    def test_compare_two_files(self, tmp_path: Path) -> None:
        data1 = {
            "config": {},
            "layers": [{"layer_index": 0, "isotopes": [
                {"name": "Tc-99m", "Z": 43, "A": 99, "state": "m",
                 "activity_Bq": 5e9, "saturation_yield_Bq_uA": 6.67e7},
            ]}],
            "timestamp": 0,
        }
        data2 = {
            "config": {},
            "layers": [{"layer_index": 0, "isotopes": [
                {"name": "Tc-99m", "Z": 43, "A": 99, "state": "m",
                 "activity_Bq": 5.5e9, "saturation_yield_Bq_uA": 7.0e7},
            ]}],
            "timestamp": 0,
        }
        f1 = tmp_path / "result1.json"
        f2 = tmp_path / "result2.json"
        f1.write_text(json.dumps(data1))
        f2.write_text(json.dumps(data2))

        result = main(["compare", str(f1), str(f2)])
        assert result == 0

    def test_compare_missing_file(self, tmp_path: Path) -> None:
        f1 = tmp_path / "missing.json"
        f2 = tmp_path / "also_missing.json"
        result = main(["compare", str(f1), str(f2)])
        assert result == 1

    def test_compare_with_isotope_filter(self, tmp_path: Path) -> None:
        data = {
            "config": {},
            "layers": [{"layer_index": 0, "isotopes": [
                {"name": "Tc-99m", "activity_Bq": 5e9, "saturation_yield_Bq_uA": 6.67e7},
                {"name": "Mo-99", "activity_Bq": 1e8, "saturation_yield_Bq_uA": 1e6},
            ]}],
            "timestamp": 0,
        }
        f1 = tmp_path / "r1.json"
        f2 = tmp_path / "r2.json"
        f1.write_text(json.dumps(data))
        f2.write_text(json.dumps(data))

        result = main(["compare", str(f1), str(f2), "--isotope", "Tc-99m"])
        assert result == 0


class TestImportSmoke:
    def test_import_hyrr(self):
        import hyrr

        assert hasattr(hyrr, "__version__")

    def test_import_cli(self):
        from hyrr.cli import main

        assert callable(main)
