"""Tests for hyrr.sweep module."""

from __future__ import annotations

import numpy as np
import pytest

from hyrr.models import Beam, Element, Layer, TargetStack
from hyrr.stopping import _interpolator_cache
from hyrr.sweep import _set_param, sweep


class MockDB:
    """Mock database for sweep tests."""

    def __hash__(self) -> int:
        return id(self)

    def __eq__(self, other: object) -> bool:
        return self is other

    def get_stopping_power(self, source, target_Z):
        if source == "PSTAR" and target_Z == 42:
            energies = np.array([1.0, 2.0, 5.0, 10.0, 15.0, 20.0, 50.0, 100.0])
            dedx = np.array([50.0, 40.0, 25.0, 18.0, 15.0, 13.0, 9.0, 7.0])
            return energies, dedx
        return np.array([]), np.array([])

    def get_cross_sections(self, projectile, target_Z, target_A):
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
                )
            ]
        return []

    def get_natural_abundances(self, Z):
        return {}

    def get_decay_data(self, Z, A, state=""):
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

    def get_element_symbol(self, Z):
        return {42: "Mo", 43: "Tc"}.get(Z, f"Z{Z}")

    def get_element_Z(self, symbol):
        return {"Mo": 42, "Tc": 43}.get(symbol, 0)


@pytest.fixture()
def db():
    _interpolator_cache.clear()
    return MockDB()


@pytest.fixture()
def base_stack():
    elem = Element(symbol="Mo", Z=42, isotopes={100: 1.0})
    layer = Layer(
        density_g_cm3=10.22,
        elements=[(elem, 1.0)],
        energy_out_MeV=12.0,
    )
    return TargetStack(
        beam=Beam(projectile="p", energy_MeV=16.0, current_mA=0.15),
        layers=[layer],
    )


class TestSweepEnergy:
    def test_shape(self, db, base_stack):
        _interpolator_cache.clear()
        df = sweep(db, base_stack, "beam.energy_MeV", [14.0, 16.0, 18.0])
        assert len(df) == 3
        assert "param_value" in df.columns

    def test_isotope_column(self, db, base_stack):
        _interpolator_cache.clear()
        df = sweep(db, base_stack, "beam.energy_MeV", [14.0, 16.0, 18.0])
        assert "Tc-99m_activity_Bq" in df.columns

    def test_energy_out_column(self, db, base_stack):
        _interpolator_cache.clear()
        df = sweep(db, base_stack, "beam.energy_MeV", [14.0, 16.0, 18.0])
        assert "energy_out_MeV" in df.columns
        vals = df["energy_out_MeV"].to_list()
        for v in vals:
            assert v == pytest.approx(12.0, abs=0.5)


class TestSetParam:
    def test_beam_energy(self, base_stack):
        modified = _set_param(base_stack, "beam.energy_MeV", 20.0)
        assert modified.beam.energy_MeV == 20.0
        assert base_stack.beam.energy_MeV == 16.0  # original unchanged

    def test_irradiation_time(self, base_stack):
        modified = _set_param(base_stack, "irradiation_time_s", 3600.0)
        assert modified.irradiation_time_s == 3600.0

    def test_invalid_param(self, base_stack):
        with pytest.raises(ValueError, match="Unsupported"):
            _set_param(base_stack, "nonexistent.param", 1.0)

    def test_layer_index_out_of_range(self, base_stack):
        with pytest.raises(IndexError):
            _set_param(base_stack, "layers[5].thickness_cm", 0.01)
