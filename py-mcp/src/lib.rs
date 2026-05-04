//! PyO3 wrapper: a single `_native` module exposing `run(data_dir)` that
//! delegates to hyrr-core's MCP stdio server. Intentionally minimal —
//! any logic added here is drift risk away from the Rust SSoT.

use pyo3::prelude::*;

/// Enter the MCP stdio loop pinned to the given nuclear data library.
///
/// Blocks until stdin closes.
#[pyfunction]
#[pyo3(signature = (data_dir, library=None))]
fn run(data_dir: String, library: Option<String>) -> PyResult<()> {
    let lib = library.unwrap_or_else(|| {
        hyrr_core::mcp::transport::DEFAULT_LIBRARY.to_string()
    });
    hyrr_core::mcp::transport::run_mcp_server_with_library(&data_dir, &lib);
    Ok(())
}

/// Resolve the nucl-parquet data directory using the shared chain
/// (CLI arg → HYRR_DATA → sibling → ~/.hyrr/nucl-parquet → fallback).
#[pyfunction]
fn resolve_data_dir() -> String {
    hyrr_core::data_dir::resolve()
}

/// Default nuclear data library identifier (e.g. "tendl-2025").
#[pyfunction]
fn default_library() -> &'static str {
    hyrr_core::mcp::transport::DEFAULT_LIBRARY
}

#[pymodule]
fn _native(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(run, m)?)?;
    m.add_function(wrap_pyfunction!(resolve_data_dir, m)?)?;
    m.add_function(wrap_pyfunction!(default_library, m)?)?;
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    Ok(())
}
