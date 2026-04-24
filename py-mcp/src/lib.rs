//! PyO3 wrapper: a single `_native` module exposing `run(data_dir)` that
//! delegates to hyrr-core's MCP stdio server. Intentionally minimal —
//! any logic added here is drift risk away from the Rust SSoT.

use pyo3::prelude::*;

/// Enter the MCP stdio loop. Blocks until stdin closes.
#[pyfunction]
fn run(data_dir: String) -> PyResult<()> {
    hyrr_core::mcp::transport::run_mcp_server(&data_dir);
    Ok(())
}

/// Resolve the nucl-parquet data directory using the shared chain
/// (CLI arg → HYRR_DATA → sibling → ~/.hyrr/nucl-parquet → fallback).
#[pyfunction]
fn resolve_data_dir() -> String {
    hyrr_core::data_dir::resolve()
}

#[pymodule]
fn _native(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(run, m)?)?;
    m.add_function(wrap_pyfunction!(resolve_data_dir, m)?)?;
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    Ok(())
}
