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

/// Resolve the data directory with auto-fetch fallback.
/// Same 3-tier logic as the standalone Rust binary:
///   1. nucl-parquet DataDir::resolve() (existing local data)
///   2. nucl-parquet DataDir::ensure_lazy() (lazy HTTP fetch on first run)
/// Returns the data root path or raises RuntimeError.
#[pyfunction]
#[pyo3(signature = (library=None))]
fn ensure_data(library: Option<String>) -> PyResult<String> {
    // Try existing local data first
    if let Ok(dd) = nucl_parquet::DataDir::resolve() {
        return Ok(dd.root().to_string_lossy().to_string());
    }

    // No local data — lazy-fetch from GitHub
    let dd = nucl_parquet::DataDir::ensure_lazy().map_err(|e| {
        pyo3::exceptions::PyRuntimeError::new_err(format!(
            "Failed to resolve or fetch nuclear data: {e}\n\n\
             Set HYRR_DATA or NUCL_PARQUET_DATA to point at a nucl-parquet data directory,\n\
             or check your network connection."
        ))
    })?;

    // Pre-fetch eager files that NpDataStore::new() needs
    let eager = [
        "meta/abundances.parquet",
        "meta/decay.parquet",
        "meta/dose_constants.parquet",
        "stopping/PSTAR.parquet",
        "stopping/ASTAR.parquet",
        "stopping/dSTAR.parquet",
        "stopping/tSTAR.parquet",
        "stopping/catima/catima.parquet",
        "stopping/compounds/PSTAR_compounds.parquet",
        "stopping/compounds/ASTAR_compounds.parquet",
    ];
    for f in &eager {
        if let Err(e) = dd.fetch_file(f) {
            eprintln!("hyrr-mcp: warning: failed to fetch {f}: {e}");
        }
    }

    // Pre-fetch library manifest
    let lib = library.unwrap_or_else(|| {
        hyrr_core::mcp::transport::DEFAULT_LIBRARY.to_string()
    });
    let manifest = format!("{lib}/manifest.json");
    let _ = dd.fetch_file(&manifest);

    Ok(dd.root().to_string_lossy().to_string())
}

/// Default nuclear data library identifier (e.g. "tendl-2023-iso").
#[pyfunction]
fn default_library() -> &'static str {
    hyrr_core::mcp::transport::DEFAULT_LIBRARY
}

#[pymodule]
fn _native(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(run, m)?)?;
    m.add_function(wrap_pyfunction!(resolve_data_dir, m)?)?;
    m.add_function(wrap_pyfunction!(ensure_data, m)?)?;
    m.add_function(wrap_pyfunction!(default_library, m)?)?;
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    Ok(())
}
