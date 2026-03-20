"""Bridge to native Rust extension (hyrr._native via PyO3/maturin).

When the native extension is available, physics compute is routed through
hyrr-core (Rust) for ~10-50× speedup. When unavailable (e.g., pure-pip
install without Rust toolchain), falls back to the pure-Python path
transparently.
"""

from __future__ import annotations

import logging

logger = logging.getLogger(__name__)

try:
    from hyrr._native import (  # type: ignore[import-not-found]
        PyDataStore as NativeDataStore,
        compute_stack_json as _native_compute_stack_json,
        py_parse_formula as native_parse_formula,
        resolve_material_json as _native_resolve_material_json,
    )

    HAS_NATIVE = True
    logger.debug("hyrr._native loaded — using Rust compute backend")
except ImportError:
    HAS_NATIVE = False
    NativeDataStore = None  # type: ignore[assignment,misc]
    _native_compute_stack_json = None  # type: ignore[assignment]
    native_parse_formula = None  # type: ignore[assignment]
    _native_resolve_material_json = None  # type: ignore[assignment]
    logger.debug("hyrr._native not available — using pure-Python fallback")

__all__ = [
    "HAS_NATIVE",
    "NativeDataStore",
    "native_parse_formula",
]
