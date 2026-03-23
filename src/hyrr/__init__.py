"""HYRR — Hierarchical Yield and Radionuclide Rates."""

# Version is kept in sync with pyproject.toml manually for now.
# TODO: Consider hatch-vcs or setuptools-scm for automatic versioning from git tags.
__version__ = "0.1.0"

from hyrr.api import run_simulation, run_simulation_from_json
from hyrr.db import (
    DEFAULT_LIBRARY,
    CrossSectionData,
    DatabaseProtocol,
    DataStore,
    DecayData,
    DecayMode,
)
from hyrr.models import (
    Beam,
    BeamProfile,
    DepthPoint,
    Element,
    IsotopeResult,
    Layer,
    LayerResult,
    ProjectileType,
    StackResult,
    TargetStack,
)
from hyrr.neutrons import (
    CompositeFlux,
    EpithermalFlux,
    MonoenergeticFlux,
    NeutronActivationResult,
    NeutronFlux,
    NeutronSource,
    ThermalFlux,
    WeisskopfFlux,
    compute_neutron_activation,
    compute_secondary_neutron_activation,
    flux_averaged_xs,
    neutron_multiplicity,
)
from hyrr.output import (
    purity_at,
    result_summary,
    result_to_csv_bundle,
    result_to_excel,
    result_to_pandas,
    result_to_polars,
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
try:
    from hyrr._native_bridge import HAS_NATIVE
except ImportError:
    HAS_NATIVE = False
from hyrr.sweep import sweep

__all__ = [
    "HAS_NATIVE",
    "Beam",
    "BeamProfile",
    "CrossSectionData",
    "DEFAULT_LIBRARY",
    "DatabaseProtocol",
    "DataStore",
    "DecayData",
    "DecayMode",
    "DepthPoint",
    "Element",
    "IsotopeResult",
    "Layer",
    "LayerResult",
    "ProjectileType",
    "StackResult",
    "TargetStack",
    "CompositeFlux",
    "EpithermalFlux",
    "MonoenergeticFlux",
    "NeutronActivationResult",
    "NeutronFlux",
    "NeutronSource",
    "ThermalFlux",
    "WeisskopfFlux",
    "compute_neutron_activation",
    "compute_secondary_neutron_activation",
    "purity_at",
    "result_summary",
    "result_to_csv_bundle",
    "result_to_excel",
    "result_to_pandas",
    "result_to_polars",
    "flux_averaged_xs",
    "neutron_multiplicity",
    "config_from_json",
    "config_to_json",
    "load_result",
    "result_from_json_str",
    "result_to_json_str",
    "run_simulation",
    "run_simulation_from_json",
    "save_result",
    "stack_to_config",
    "sweep",
]
