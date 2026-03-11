"""Tests for hyrr.output module."""

from __future__ import annotations

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
from hyrr.output import (
    _format_halflife,
    _format_time,
    activity_timeseries_to_polars,
    depth_profile_to_polars,
    layer_result_to_polars,
    purity_at,
    result_summary,
    result_to_csv_bundle,
    result_to_excel,
    result_to_pandas,
    result_to_polars,
)


@pytest.fixture()
def sample_isotope_result() -> IsotopeResult:
    """Synthetic Tc-99m isotope result with irradiation + decay curve."""
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
    stack = TargetStack(beam=beam, layers=[layer])

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
        irradiation_time_s=86400,
        cooling_time_s=86400,
    )


class TestResultToPolars:
    def test_columns(self, sample_stack_result: StackResult) -> None:
        import polars as pl

        df = result_to_polars(sample_stack_result)
        assert isinstance(df, pl.DataFrame)
        assert "isotope" in df.columns
        assert "production_rate" in df.columns
        assert "layer_index" in df.columns
        assert len(df) == 1

    def test_empty_result(self) -> None:

        elem = Element(symbol="Mo", Z=42, isotopes={100: 1.0})
        layer = Layer(
            density_g_cm3=10.22,
            elements=[(elem, 1.0)],
            thickness_cm=0.02,
        )
        beam = Beam(projectile="p", energy_MeV=16.0, current_mA=0.15)
        stack = TargetStack(beam=beam, layers=[layer])
        lr = LayerResult(
            layer=layer,
            energy_in=16,
            energy_out=12,
            delta_E_MeV=4,
            heat_kW=0.6,
            depth_profile=[],
            isotope_results={},
        )
        result = StackResult(
            stack=stack,
            layer_results=[lr],
            irradiation_time_s=86400,
            cooling_time_s=86400,
        )
        df = result_to_polars(result)
        assert df.is_empty()


class TestLayerResultToPolars:
    def test_columns(self, sample_stack_result: StackResult) -> None:
        import polars as pl

        df = layer_result_to_polars(sample_stack_result.layer_results[0])
        assert isinstance(df, pl.DataFrame)
        assert "isotope" in df.columns
        assert "layer_index" not in df.columns
        assert len(df) == 1


class TestDepthProfileToPolars:
    def test_columns(self, sample_stack_result: StackResult) -> None:
        df = depth_profile_to_polars(sample_stack_result.layer_results[0])
        assert "depth_cm" in df.columns
        assert "energy_MeV" in df.columns
        assert len(df) == 20

    def test_empty_profile(self) -> None:
        elem = Element(symbol="Mo", Z=42, isotopes={100: 1.0})
        layer = Layer(
            density_g_cm3=10.22,
            elements=[(elem, 1.0)],
            thickness_cm=0.02,
        )
        lr = LayerResult(
            layer=layer,
            energy_in=16,
            energy_out=12,
            delta_E_MeV=4,
            heat_kW=0.6,
            depth_profile=[],
            isotope_results={},
        )
        df = depth_profile_to_polars(lr)
        assert df.is_empty()


class TestActivityTimeseries:
    def test_columns(self, sample_isotope_result: IsotopeResult) -> None:
        df = activity_timeseries_to_polars(sample_isotope_result)
        assert "time_s" in df.columns
        assert "activity_GBq" in df.columns
        assert len(df) == 200


class TestResultToPandas:
    def test_columns(self, sample_stack_result: StackResult) -> None:
        import pandas as pd

        df = result_to_pandas(sample_stack_result)
        assert isinstance(df, pd.DataFrame)
        assert "isotope" in df.columns
        assert "production_rate" in df.columns
        assert "layer_index" in df.columns
        assert "stopping_power_source" in df.columns
        assert len(df) == 1

    def test_empty_result(self) -> None:
        import pandas as pd

        elem = Element(symbol="Mo", Z=42, isotopes={100: 1.0})
        layer = Layer(
            density_g_cm3=10.22,
            elements=[(elem, 1.0)],
            thickness_cm=0.02,
        )
        beam = Beam(projectile="p", energy_MeV=16.0, current_mA=0.15)
        stack = TargetStack(beam=beam, layers=[layer])
        lr = LayerResult(
            layer=layer,
            energy_in=16,
            energy_out=12,
            delta_E_MeV=4,
            heat_kW=0.6,
            depth_profile=[],
            isotope_results={},
        )
        result = StackResult(
            stack=stack,
            layer_results=[lr],
            irradiation_time_s=86400,
            cooling_time_s=86400,
        )
        df = result_to_pandas(result)
        assert isinstance(df, pd.DataFrame)
        assert len(df) == 0

    def test_values(self, sample_stack_result: StackResult) -> None:
        df = result_to_pandas(sample_stack_result)
        row = df.iloc[0]
        assert row["isotope"] == "Tc-99m"
        assert row["Z"] == 43
        assert row["A"] == 99
        assert row["energy_in_MeV"] == 16.0
        assert row["energy_out_MeV"] == 12.0


class TestResultSummary:
    def test_contains_beam_info(self, sample_stack_result: StackResult) -> None:
        text = result_summary(sample_stack_result)
        assert "HYRR" in text
        assert "16.000" in text
        assert "0.150" in text
        assert "Tc-99m" in text

    def test_contains_layer_info(self, sample_stack_result: StackResult) -> None:
        text = result_summary(sample_stack_result)
        assert "Layer 1" in text
        assert "12.000" in text


class TestPurityAt:
    def test_single_isotope(self, sample_stack_result: StackResult) -> None:
        """Single isotope should give 100% purity."""
        p = purity_at(sample_stack_result, 0.0, "Tc-99m")
        assert p == pytest.approx(1.0, abs=0.01)

    def test_missing_isotope(self, sample_stack_result: StackResult) -> None:
        with pytest.raises(ValueError, match="not found"):
            purity_at(sample_stack_result, 0.0, "nonexistent")

    def test_layer_result(self, sample_stack_result: StackResult) -> None:
        """purity_at should also work with a LayerResult."""
        lr = sample_stack_result.layer_results[0]
        p = purity_at(lr, 0.0, "Tc-99m")
        assert p == pytest.approx(1.0, abs=0.01)


class TestFormatHelpers:
    def test_format_time(self) -> None:
        assert "1d" in _format_time(86400)
        assert "1h" in _format_time(3600)

    def test_format_halflife(self) -> None:
        assert "s" in _format_halflife(30.0)
        assert "min" in _format_halflife(600.0)
        assert "h" in _format_halflife(21636.0)
        assert "d" in _format_halflife(200000.0)
        assert "y" in _format_halflife(1e9)


class TestExcelExport:
    def test_creates_file(
        self,
        sample_stack_result: StackResult,
        tmp_path: object,
    ) -> None:
        import os
        from pathlib import Path

        path = str(Path(str(tmp_path)) / "test.xlsx")
        result_to_excel(sample_stack_result, path)
        assert os.path.exists(path)
        assert os.path.getsize(path) > 0


class TestCsvBundle:
    def test_creates_zip_with_expected_files(
        self,
        sample_stack_result: StackResult,
        tmp_path: object,
    ) -> None:
        import zipfile
        from pathlib import Path

        path = str(Path(str(tmp_path)) / "bundle.zip")
        result_to_csv_bundle(sample_stack_result, path)

        with zipfile.ZipFile(path) as zf:
            names = zf.namelist()
            assert "summary.csv" in names
            assert "layer_0_depth_profile.csv" in names
            assert "layer_0_isotopes.csv" in names

    def test_summary_csv_headers(
        self,
        sample_stack_result: StackResult,
        tmp_path: object,
    ) -> None:
        import csv
        import io
        import zipfile
        from pathlib import Path

        path = str(Path(str(tmp_path)) / "bundle.zip")
        result_to_csv_bundle(sample_stack_result, path)

        with zipfile.ZipFile(path) as zf:
            with zf.open("summary.csv") as f:
                reader = csv.reader(io.TextIOWrapper(f))
                header = next(reader)
        expected = [
            "layer_index", "isotope", "Z", "A", "state", "half_life_s",
            "production_rate", "activity_Bq", "saturation_yield_Bq_uA",
            "energy_in_MeV", "energy_out_MeV", "delta_E_MeV", "heat_kW",
            "stopping_power_source", "source",
            "activity_direct_Bq", "activity_ingrowth_Bq",
        ]
        assert header == expected

    def test_depth_profile_csv_headers(
        self,
        sample_stack_result: StackResult,
        tmp_path: object,
    ) -> None:
        import csv
        import io
        import zipfile
        from pathlib import Path

        path = str(Path(str(tmp_path)) / "bundle.zip")
        result_to_csv_bundle(sample_stack_result, path)

        with zipfile.ZipFile(path) as zf:
            with zf.open("layer_0_depth_profile.csv") as f:
                reader = csv.reader(io.TextIOWrapper(f))
                header = next(reader)
        assert header == ["depth_cm", "energy_MeV", "dedx_MeV_cm", "heat_W_cm3"]

    def test_isotopes_csv_headers(
        self,
        sample_stack_result: StackResult,
        tmp_path: object,
    ) -> None:
        import csv
        import io
        import zipfile
        from pathlib import Path

        path = str(Path(str(tmp_path)) / "bundle.zip")
        result_to_csv_bundle(sample_stack_result, path)

        with zipfile.ZipFile(path) as zf:
            with zf.open("layer_0_isotopes.csv") as f:
                reader = csv.reader(io.TextIOWrapper(f))
                header = next(reader)
        expected = [
            "isotope", "Z", "A", "state", "half_life_s",
            "production_rate", "activity_Bq", "saturation_yield_Bq_uA",
            "source", "activity_direct_Bq", "activity_ingrowth_Bq",
        ]
        assert header == expected

    def test_summary_csv_data_row(
        self,
        sample_stack_result: StackResult,
        tmp_path: object,
    ) -> None:
        import csv
        import io
        import zipfile
        from pathlib import Path

        path = str(Path(str(tmp_path)) / "bundle.zip")
        result_to_csv_bundle(sample_stack_result, path)

        with zipfile.ZipFile(path) as zf:
            with zf.open("summary.csv") as f:
                reader = csv.reader(io.TextIOWrapper(f))
                _header = next(reader)
                row = next(reader)
        assert row[0] == "0"  # layer_index
        assert row[1] == "Tc-99m"  # isotope
