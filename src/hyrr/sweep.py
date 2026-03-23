"""Parameter sweep utilities for HYRR.

Run a simulation across a range of parameter values and collect results
into a Polars DataFrame for analysis.
"""

from __future__ import annotations

import copy
import json
import re
from collections.abc import Sequence
from dataclasses import replace
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    import polars as pl

    from hyrr.db import DatabaseProtocol
    from hyrr.models import TargetStack


def sweep(
    db: DatabaseProtocol,
    stack: TargetStack,
    param: str,
    values: Sequence[float],
) -> pl.DataFrame:
    """Run compute_stack for each parameter value and collect results.

    Routes through the Rust compute engine via JSON serialization.

    Args:
        db: Nuclear data provider.
        stack: Base target stack configuration.
        param: Dot-path parameter to vary (e.g. "beam.energy_MeV",
            "layers[0].thickness_cm", "irradiation_time_s").
        values: Sequence of parameter values to sweep.

    Returns:
        Polars DataFrame with one row per value. Columns include
        param_value, per-isotope activity, total_heat_kW, and
        energy_out_MeV for the last layer.
    """
    import polars as pl

    from hyrr.api import run_simulation_from_json
    from hyrr.serialization import stack_to_config

    rows: list[dict[str, object]] = []

    for val in values:
        modified = _set_param(stack, param, val)
        config = stack_to_config(modified)
        config_json = json.dumps(config)
        result_dict = run_simulation_from_json(
            config_json, str(db.data_dir), db.library
        )

        row: dict[str, object] = {"param_value": val}

        for lr in result_dict.get("layers", []):
            for iso in lr.get("isotopes", []):
                name = iso.get("name", "")
                col = f"{name}_activity_Bq"
                row[col] = float(row.get(col, 0.0)) + iso.get("activity_Bq", 0.0)

        row["total_heat_kW"] = sum(
            lr.get("heat_kW", 0.0) for lr in result_dict.get("layers", [])
        )

        layers = result_dict.get("layers", [])
        if layers:
            row["energy_out_MeV"] = layers[-1].get("energy_out", 0.0)

        rows.append(row)

    if not rows:
        return pl.DataFrame()

    return pl.DataFrame(rows)


def _set_param(stack: TargetStack, param: str, value: float) -> TargetStack:
    """Create a modified copy of stack with the given parameter set."""
    new_stack = copy.deepcopy(stack)

    if param == "beam.energy_MeV":
        new_stack.beam = replace(new_stack.beam, energy_MeV=value)
    elif param == "beam.current_mA":
        new_stack.beam = replace(new_stack.beam, current_mA=value)
    elif param == "irradiation_time_s":
        new_stack.irradiation_time_s = value
    elif param == "cooling_time_s":
        new_stack.cooling_time_s = value
    else:
        # layers[N].attr pattern
        match = re.match(r"layers\[(\d+)\]\.(\w+)", param)
        if not match:
            msg = f"Unsupported parameter path: {param!r}"
            raise ValueError(msg)
        idx = int(match.group(1))
        attr = match.group(2)
        if idx >= len(new_stack.layers):
            msg = f"Layer index {idx} out of range"
            raise IndexError(msg)
        setattr(new_stack.layers[idx], attr, value)

    return new_stack
