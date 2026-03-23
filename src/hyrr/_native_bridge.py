"""Bridge to native Rust extension (hyrr._native via PyO3/maturin).

When the native extension is available, physics compute is routed through
hyrr-core (Rust). The native extension is required — there is no
pure-Python fallback for physics computation.
"""

from __future__ import annotations

import json
import logging

import numpy as np
import numpy.typing as npt

logger = logging.getLogger(__name__)

try:
    from hyrr._native import (  # type: ignore[import-not-found]
        PyDataStore as NativeDataStore,
        compute_stack_json as _native_compute_stack_json,
        py_bateman_activity as _native_bateman_activity,
        py_compute_energy_out as _native_compute_energy_out,
        py_compute_thickness as _native_compute_thickness,
        py_dedx_mev_per_cm as _native_dedx_mev_per_cm,
        py_parse_formula as native_parse_formula,
        py_saturation_yield as _native_saturation_yield,
        resolve_material_json as _native_resolve_material_json,
    )

    HAS_NATIVE = True
    logger.debug("hyrr._native loaded — using Rust compute backend")
except ImportError:
    HAS_NATIVE = False
    NativeDataStore = None  # type: ignore[assignment,misc]
    _native_compute_stack_json = None  # type: ignore[assignment]
    _native_bateman_activity = None  # type: ignore[assignment]
    _native_compute_energy_out = None  # type: ignore[assignment]
    _native_compute_thickness = None  # type: ignore[assignment]
    _native_dedx_mev_per_cm = None  # type: ignore[assignment]
    native_parse_formula = None  # type: ignore[assignment]
    _native_saturation_yield = None  # type: ignore[assignment]
    _native_resolve_material_json = None  # type: ignore[assignment]
    logger.debug("hyrr._native not available — Rust extension required")


# ---------------------------------------------------------------------------
# Python-friendly wrappers for low-level Rust primitives
# ---------------------------------------------------------------------------


def bateman_activity(
    production_rate: float,
    half_life_s: float | None,
    irradiation_time_s: float,
    cooling_time_s: float,
    n_time_points: int = 200,
) -> tuple[npt.NDArray[np.float64], npt.NDArray[np.float64]]:
    """Compute time-dependent activity via Bateman equations (Rust)."""
    result_json = _native_bateman_activity(
        production_rate,
        half_life_s,
        irradiation_time_s,
        cooling_time_s,
        n_time_points,
    )
    data = json.loads(result_json)
    return np.array(data["time_grid"]), np.array(data["activity"])


def saturation_yield(
    production_rate: float,
    half_life_s: float | None,
    beam_current_mA: float,
) -> float:
    """Compute saturation yield [Bq/µA] (Rust)."""
    return _native_saturation_yield(production_rate, half_life_s, beam_current_mA)


def dedx_MeV_per_cm(
    db: object,
    projectile: str,
    composition: list[tuple[int, float]],
    density_g_cm3: float,
    energy_MeV: float | npt.NDArray[np.float64],
) -> float | npt.NDArray[np.float64]:
    """Compute linear stopping power [MeV/cm] via Rust.

    Accepts the same signature as the old stopping.dedx_MeV_per_cm
    for backwards compatibility with compute3d.py.
    """
    composition_json = json.dumps(composition)
    scalar = np.ndim(energy_MeV) == 0
    energies = [float(energy_MeV)] if scalar else list(np.asarray(energy_MeV))
    result = _native_dedx_mev_per_cm(
        str(db.data_dir),  # type: ignore[attr-defined]
        db.library,  # type: ignore[attr-defined]
        projectile,
        composition_json,
        density_g_cm3,
        energies,
    )
    if scalar:
        return result[0]
    return np.array(result)


__all__ = [
    "HAS_NATIVE",
    "NativeDataStore",
    "_native_compute_stack_json",
    "bateman_activity",
    "dedx_MeV_per_cm",
    "native_parse_formula",
    "saturation_yield",
]
