"""Tests for hyrr.sweep module."""

from __future__ import annotations

import pytest

from hyrr.models import Beam, Element, Layer, TargetStack
from hyrr.sweep import _set_param


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
