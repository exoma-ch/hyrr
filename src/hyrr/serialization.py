"""Serialization utilities for HYRR results and configs.

Round-trip JSON serialization for StackResult and TargetStack objects.
"""

from __future__ import annotations

import json
import math
from typing import Any

import numpy as np

from hyrr.models import StackResult, TargetStack


def _safe_float(v: float | None) -> float | None:
    """Convert to JSON-safe float (NaN/Inf -> None)."""
    if v is None:
        return None
    if isinstance(v, (float, np.floating)):
        if math.isnan(v) or math.isinf(v):
            return None
    return float(v)


# ---------------------------------------------------------------------------
# WU3: Result serialization
# ---------------------------------------------------------------------------


def result_to_json_str(result: StackResult) -> str:
    """Serialize a StackResult to a JSON string.

    Includes full config (beam, layers, times) and per-layer results
    (energies, heat, isotopes with time grids). Numpy arrays are
    converted to lists. NaN/Inf values become null.
    """
    data = _result_to_dict(result)
    return json.dumps(data)


def _result_to_dict(result: StackResult) -> dict[str, Any]:
    """Convert StackResult to a JSON-serializable dict."""
    config = stack_to_config(result.stack)

    layers_out: list[dict[str, Any]] = []
    for i, lr in enumerate(result.layer_results):
        isotopes_out: dict[str, Any] = {}
        for name, iso in lr.isotope_results.items():
            isotopes_out[name] = {
                "name": iso.name,
                "Z": iso.Z,
                "A": iso.A,
                "state": iso.state,
                "half_life_s": _safe_float(iso.half_life_s),
                "production_rate": _safe_float(iso.production_rate),
                "saturation_yield_Bq_uA": _safe_float(iso.saturation_yield_Bq_uA),
                "activity_Bq": _safe_float(iso.activity_Bq),
                "time_grid_s": iso.time_grid_s.tolist(),
                "activity_vs_time_Bq": iso.activity_vs_time_Bq.tolist(),
                "source": iso.source,
                "activity_direct_Bq": _safe_float(iso.activity_direct_Bq),
                "activity_ingrowth_Bq": _safe_float(iso.activity_ingrowth_Bq),
                "activity_direct_vs_time_Bq": iso.activity_direct_vs_time_Bq.tolist(),
                "activity_ingrowth_vs_time_Bq": iso.activity_ingrowth_vs_time_Bq.tolist(),
            }

        layers_out.append(
            {
                "layer_index": i,
                "energy_in": _safe_float(lr.energy_in),
                "energy_out": _safe_float(lr.energy_out),
                "delta_E_MeV": _safe_float(lr.delta_E_MeV),
                "heat_kW": _safe_float(lr.heat_kW),
                "isotope_results": isotopes_out,
            }
        )

    return {
        "config": config,
        "layer_results": layers_out,
        "irradiation_time_s": result.irradiation_time_s,
        "cooling_time_s": result.cooling_time_s,
    }


def result_from_json_str(json_str: str) -> dict:
    """Parse a result JSON string back to a dict."""
    return json.loads(json_str)


def save_result(result: StackResult, path: str) -> None:
    """Write a StackResult as JSON to a file."""
    data = _result_to_dict(result)
    with open(path, "w") as f:
        json.dump(data, f)


def load_result(path: str) -> dict:
    """Read a result JSON file back to a dict."""
    with open(path) as f:
        return json.load(f)


# ---------------------------------------------------------------------------
# WU4: Config serialization
# ---------------------------------------------------------------------------


def stack_to_config(stack: TargetStack) -> dict[str, Any]:
    """Extract a pure-dict config from a TargetStack.

    The returned dict contains beam params, layer material info
    (element symbols, isotopic fractions, density), irradiation/cooling
    times, and area.  No numpy objects — all values are native Python.
    """
    beam_dict = {
        "projectile": stack.beam.projectile,
        "energy_MeV": float(stack.beam.energy_MeV),
        "current_mA": float(stack.beam.current_mA),
    }

    layers_list: list[dict[str, Any]] = []
    for layer in stack.layers:
        elements_list: list[list[Any]] = []
        for elem, frac in layer.elements:
            elem_dict = {
                "symbol": elem.symbol,
                "Z": elem.Z,
                "isotopes": {int(a): float(f) for a, f in elem.isotopes.items()},
            }
            elements_list.append([elem_dict, float(frac)])

        layer_dict: dict[str, Any] = {
            "density_g_cm3": float(layer.density_g_cm3),
            "elements": elements_list,
            "is_monitor": layer.is_monitor,
        }
        if layer.thickness_cm is not None:
            layer_dict["thickness_cm"] = float(layer.thickness_cm)
        if layer.areal_density_g_cm2 is not None:
            layer_dict["areal_density_g_cm2"] = float(layer.areal_density_g_cm2)
        if layer.energy_out_MeV is not None:
            layer_dict["energy_out_MeV"] = float(layer.energy_out_MeV)

        layers_list.append(layer_dict)

    return {
        "beam": beam_dict,
        "layers": layers_list,
        "irradiation_time_s": float(stack.irradiation_time_s),
        "cooling_time_s": float(stack.cooling_time_s),
        "area_cm2": float(stack.area_cm2),
    }


def config_to_json(stack: TargetStack) -> str:
    """Serialize a TargetStack config to a JSON string."""
    return json.dumps(stack_to_config(stack))


def config_from_json(json_str: str) -> dict:
    """Parse a config JSON string back to a dict."""
    return json.loads(json_str)
