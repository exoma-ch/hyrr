"""JSON API bridge for hyrr simulations.

Converts JSON SimulationConfig → Python models → compute_stack → JSON result.
Used by the FastAPI server and direct API usage.
"""

from __future__ import annotations

import json
import math
import time
from typing import Any

import numpy as np

from hyrr.db import DatabaseProtocol
from hyrr.materials import (
    SYMBOL_TO_Z,
    formula_to_mass_fractions,
    resolve_element,
    resolve_isotopics,
)
from hyrr.models import Beam, BeamProfile, Layer, StackResult, TargetStack


def _resolve_material(
    db: DatabaseProtocol,
    material: str,
    enrichment: dict[str, dict[int, float]] | None,
) -> tuple[list[tuple[Any, float]], float]:
    """Resolve a material identifier to (elements, density).

    Tries py-mat first for named materials (e.g. "havar", "stainless.s316L"),
    then falls back to formula parsing (e.g. "Cu", "MoO3", "Mo-100").

    Returns:
        (elements_with_atom_fractions, density_g_cm3)
    """
    # Try py-mat lookup first
    try:
        import pymat

        all_mats = pymat.load_all()

        # Handle dotted paths (e.g., "stainless.s316L")
        parts = material.split(".")
        mat = all_mats.get(parts[0])
        for part in parts[1:]:
            if mat is None:
                break
            mat = getattr(mat, "_children", {}).get(part)

        if mat is not None:
            density = getattr(mat, "density", None)
            if density is None:
                mech = getattr(getattr(mat, "properties", None), "mechanical", None)
                if mech:
                    density = getattr(mech, "density", None) or getattr(
                        mech, "density_value", None
                    )

            composition = getattr(mat, "composition", None)
            formula = getattr(mat, "formula", None)

            if density is not None and (composition or formula):
                if composition:
                    elements = resolve_isotopics(
                        db,
                        composition,
                        is_atom_fraction=False,
                        overrides=enrichment,
                    )
                else:
                    mass_fracs = formula_to_mass_fractions(formula)
                    elements = resolve_isotopics(
                        db,
                        mass_fracs,
                        is_atom_fraction=False,
                        overrides=enrichment,
                    )
                return elements, float(density)
    except ImportError:
        pass

    # Check for isotope notation: "Mo-100" -> enriched single isotope
    if "-" in material:
        sym, mass_str = material.rsplit("-", 1)
        if sym in SYMBOL_TO_Z and mass_str.isdigit():
            A = int(mass_str)
            # Treat as 100% enriched single isotope
            iso_enrichment = enrichment or {}
            if sym not in iso_enrichment:
                iso_enrichment[sym] = {A: 1.0}
            element = resolve_element(db, sym, iso_enrichment.get(sym))
            # Density: use natural element density as approximation
            return [(element, 1.0)], _element_density(sym)

    # Fallback: treat as chemical formula
    if material in SYMBOL_TO_Z:
        element = resolve_element(db, material, (enrichment or {}).get(material))
        return [(element, 1.0)], _element_density(material)

    # Compound formula (e.g., "MoO3")
    mass_fracs = formula_to_mass_fractions(material)
    elements = resolve_isotopics(
        db,
        mass_fracs,
        is_atom_fraction=False,
        overrides=enrichment,
    )
    # For compounds, density must be provided or we use a placeholder
    return elements, _compound_density(material)


# Densities for common pure elements (g/cm³)
_ELEMENT_DENSITIES: dict[str, float] = {
    "H": 0.00009,
    "He": 0.000164,
    "Li": 0.534,
    "Be": 1.85,
    "B": 2.34,
    "C": 2.267,
    "N": 0.00125,
    "O": 0.00143,
    "F": 0.0017,
    "Na": 0.971,
    "Mg": 1.738,
    "Al": 2.70,
    "Si": 2.33,
    "P": 1.82,
    "S": 2.07,
    "K": 0.862,
    "Ca": 1.55,
    "Ti": 4.506,
    "V": 6.11,
    "Cr": 7.15,
    "Mn": 7.44,
    "Fe": 7.874,
    "Co": 8.90,
    "Ni": 8.908,
    "Cu": 8.96,
    "Zn": 7.134,
    "Ga": 5.91,
    "Ge": 5.323,
    "As": 5.776,
    "Se": 4.81,
    "Rb": 1.532,
    "Sr": 2.64,
    "Y": 4.469,
    "Zr": 6.52,
    "Nb": 8.57,
    "Mo": 10.28,
    "Ru": 12.37,
    "Rh": 12.41,
    "Pd": 12.02,
    "Ag": 10.49,
    "Cd": 8.69,
    "In": 7.31,
    "Sn": 7.287,
    "Sb": 6.685,
    "Te": 6.24,
    "I": 4.93,
    "Cs": 1.873,
    "Ba": 3.594,
    "La": 6.145,
    "Ce": 6.77,
    "Nd": 7.01,
    "Sm": 7.52,
    "Eu": 5.244,
    "Gd": 7.90,
    "Tb": 8.23,
    "Dy": 8.55,
    "Ho": 8.80,
    "Er": 9.07,
    "Tm": 9.32,
    "Yb": 6.90,
    "Lu": 9.84,
    "Hf": 13.31,
    "Ta": 16.69,
    "W": 19.25,
    "Re": 21.02,
    "Os": 22.59,
    "Ir": 22.56,
    "Pt": 21.45,
    "Au": 19.32,
    "Hg": 13.534,
    "Tl": 11.85,
    "Pb": 11.34,
    "Bi": 9.78,
    "Th": 11.72,
    "U": 19.10,
    "Ra": 5.5,
}


def _element_density(symbol: str) -> float:
    return _ELEMENT_DENSITIES.get(symbol, 7.0)  # fallback 7 g/cm³


def _compound_density(formula: str) -> float:
    """Rough density estimate for compounds. Should be overridden by py-mat."""
    return 5.0  # conservative default


def _parse_enrichment(
    enrichment_json: dict[str, dict[str, float]] | None,
) -> dict[str, dict[int, float]] | None:
    """Convert JSON enrichment (string keys) to Python (int keys)."""
    if not enrichment_json:
        return None
    result: dict[str, dict[int, float]] = {}
    for symbol, isotopes in enrichment_json.items():
        result[symbol] = {int(a): frac for a, frac in isotopes.items()}
    return result


def config_to_stack(
    db: DatabaseProtocol,
    config: dict,
) -> TargetStack:
    """Convert a JSON config dict to a TargetStack.

    Args:
        db: Database for isotopic resolution
        config: Dict matching frontend SimulationConfig shape

    Returns:
        TargetStack ready for compute_stack()
    """
    beam_cfg = config["beam"]

    # Parse optional beam profile
    profile: BeamProfile | None = None
    profile_cfg = beam_cfg.get("profile")
    if profile_cfg:
        profile = BeamProfile(
            sigma_x_cm=profile_cfg.get("sigma_x_cm", 0.0),
            sigma_y_cm=profile_cfg.get("sigma_y_cm"),
            divergence_x_mrad=profile_cfg.get("divergence_x_mrad", 0.0),
            divergence_y_mrad=profile_cfg.get("divergence_y_mrad"),
            emittance_x_mm_mrad=profile_cfg.get("emittance_x_mm_mrad"),
            emittance_y_mm_mrad=profile_cfg.get("emittance_y_mm_mrad"),
            alpha_x=profile_cfg.get("alpha_x", 0.0),
            alpha_y=profile_cfg.get("alpha_y", 0.0),
        )

    # Parse optional 3D pose
    position = beam_cfg.get("position")
    if position is not None:
        position = tuple(position)
    direction = beam_cfg.get("direction")
    if direction is not None:
        direction = tuple(direction)

    beam = Beam(
        projectile=beam_cfg["projectile"],
        energy_MeV=beam_cfg["energy_MeV"],
        current_mA=beam_cfg["current_mA"],
        energy_spread_MeV=beam_cfg.get("energy_spread_MeV", 0.0),
        profile=profile,
        position=position,
        direction=direction,
    )

    layers: list[Layer] = []
    for layer_cfg in config["layers"]:
        enrichment = _parse_enrichment(layer_cfg.get("enrichment"))
        elements, density = _resolve_material(
            db,
            layer_cfg["material"],
            enrichment,
        )

        layer = Layer(
            density_g_cm3=density,
            elements=elements,
            thickness_cm=layer_cfg.get("thickness_cm"),
            areal_density_g_cm2=layer_cfg.get("areal_density_g_cm2"),
            energy_out_MeV=layer_cfg.get("energy_out_MeV"),
            is_monitor=layer_cfg.get("is_monitor", False),
        )
        layers.append(layer)

    return TargetStack(
        beam=beam,
        layers=layers,
        irradiation_time_s=config["irradiation_s"],
        cooling_time_s=config["cooling_s"],
    )


def _convert_rust_result(rust: dict, config: dict) -> dict:
    """Convert Rust StackResult JSON to the Python API SimulationResult format.

    Rust serde produces snake_case keys (layer_results, isotope_results, etc.).
    The Python API uses a different shape (layers, isotopes as list, etc.).
    """
    layers_out = []
    for i, lr in enumerate(rust.get("layer_results", [])):
        isotopes_out = []
        iso_data = lr.get("isotope_results", {})
        for name, iso in iso_data.items():
            isotopes_out.append({
                "name": iso.get("name", name),
                "Z": iso.get("z", 0),
                "A": iso.get("a", 0),
                "state": iso.get("state", ""),
                "half_life_s": iso.get("half_life_s"),
                "production_rate": iso.get("production_rate", 0),
                "saturation_yield_Bq_uA": iso.get("saturation_yield_bq_ua", 0),
                "activity_Bq": iso.get("activity_bq", 0),
                "source": iso.get("source", "direct"),
                "activity_direct_Bq": iso.get("activity_direct_bq", 0),
                "activity_ingrowth_Bq": iso.get("activity_ingrowth_bq", 0),
            })

        layers_out.append({
            "layer_index": i,
            "energy_in": lr.get("energy_in", 0),
            "energy_out": lr.get("energy_out", 0),
            "delta_E_MeV": lr.get("delta_e_mev", 0),
            "heat_kW": lr.get("heat_kw", 0),
            "isotopes": isotopes_out,
        })

    return {
        "config": config,
        "layers": layers_out,
        "timestamp": time.time(),
    }


def _safe_float(v: float) -> float | None:
    """Convert to JSON-safe float (NaN/Inf → None)."""
    if isinstance(v, (float, np.floating)):
        if math.isnan(v) or math.isinf(v):
            return None
    return float(v)


def result_to_json(result: StackResult, config: dict) -> dict:
    """Convert a StackResult to JSON-serializable dict matching frontend types.

    Args:
        result: Compute result
        config: Original config dict (echoed back in response)

    Returns:
        Dict matching frontend SimulationResult shape
    """
    layers_out = []
    for i, lr in enumerate(result.layer_results):
        isotopes_out = []
        for _name, iso in lr.isotope_results.items():
            isotopes_out.append(
                {
                    "name": iso.name,
                    "Z": iso.Z,
                    "A": iso.A,
                    "state": iso.state,
                    "half_life_s": _safe_float(iso.half_life_s)
                    if iso.half_life_s is not None
                    else None,
                    "production_rate": _safe_float(iso.production_rate),
                    "saturation_yield_Bq_uA": _safe_float(iso.saturation_yield_Bq_uA),
                    "activity_Bq": _safe_float(iso.activity_Bq),
                    "source": iso.source,
                    "activity_direct_Bq": _safe_float(iso.activity_direct_Bq),
                    "activity_ingrowth_Bq": _safe_float(iso.activity_ingrowth_Bq),
                }
            )

        layers_out.append(
            {
                "layer_index": i,
                "energy_in": _safe_float(lr.energy_in),
                "energy_out": _safe_float(lr.energy_out),
                "delta_E_MeV": _safe_float(lr.delta_E_MeV),
                "heat_kW": _safe_float(lr.heat_kW),
                "isotopes": isotopes_out,
            }
        )

    return {
        "config": config,
        "layers": layers_out,
        "timestamp": time.time(),
    }


def run_simulation_from_json(
    config_json: str,
    data_dir: str,
    library: str | None = None,
) -> dict:
    """Run a full simulation from JSON config and a data directory path.

    When the native Rust extension is available, routes through hyrr-core
    for significantly faster compute. Falls back to pure-Python transparently.

    Args:
        config_json: JSON string of SimulationConfig
        data_dir: Path to the nucl-parquet data directory
        library: Cross-section library name (default from config or DEFAULT_LIBRARY)

    Returns:
        Dict matching frontend SimulationResult shape
    """
    from hyrr._native_bridge import HAS_NATIVE
    from hyrr.db import DEFAULT_LIBRARY

    config = json.loads(config_json)
    lib = library or config.get("library", DEFAULT_LIBRARY)

    from hyrr._native_bridge import _native_compute_stack_json

    if not HAS_NATIVE:
        raise RuntimeError(
            "hyrr._native extension not available. "
            "Install with: maturin develop -m py/Cargo.toml"
        )

    result_json = _native_compute_stack_json(data_dir, lib, config_json)
    rust_result = json.loads(result_json)
    return _convert_rust_result(rust_result, config)


def run_simulation(
    db: DatabaseProtocol,
    config: dict,
) -> dict:
    """Run simulation with an existing database connection.

    Routes through the Rust compute engine via JSON serialization.
    Used by the FastAPI server where the database is kept open.

    Args:
        db: Open database connection
        config: Dict matching frontend SimulationConfig shape

    Returns:
        Dict matching frontend SimulationResult shape
    """
    config_json = json.dumps(config)
    return run_simulation_from_json(
        config_json,
        data_dir=str(db.data_dir),
        library=db.library,
    )
