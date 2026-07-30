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
    let lib = library.unwrap_or_else(|| hyrr_core::mcp::transport::DEFAULT_LIBRARY.to_string());
    hyrr_core::mcp::transport::run_mcp_server_with_library(&data_dir, &lib);
    Ok(())
}

/// Resolve the nucl-parquet data directory using the shared chain
/// (CLI arg → HYRR_DATA → sibling → ~/.hyrr/nucl-parquet → fallback).
#[pyfunction]
fn resolve_data_dir() -> String {
    hyrr_core::data_dir::resolve()
}

/// Resolve the data directory with auto-fetch fallback. Priority:
///
/// 1. `NUCL_PARQUET_DATA` env (verbatim, no fetch) — backward compat with
///    the pre-#529 nucl-parquet client's own env var.
/// 2. `hyrr_core::data_dir::resolve()` — managed cache (sentinel-gated),
///    sibling dev tree, legacy home. Returns a path with `meta/`.
/// 3. Auto-fetch via `hyrr_core::data_fetch::ensure_meta_stopping` +
///    `ensure_library(library)` against the CalVer release matching the
///    submodule-pinned `DATA_VERSION` (SSoT: `core/build.rs` reads
///    `nucl-parquet/data/catalog.json::data_version`).
///
/// Any fetch failure raises `RuntimeError` with a diagnostic naming the
/// expected release tag, URL, and cache dir. **This is the #529 fix:**
/// the old path (`nucl_parquet::DataDir::ensure_lazy` + eager per-file
/// fetches with silently-ignored errors) let 404s slip through so the
/// caller landed in `run()` with a half-populated data dir — the physics
/// then failed downstream with confusing "no PSTAR table" style errors.
/// Now the fetch either completes or the CLI aborts before entering the
/// transport loop.
#[pyfunction]
#[pyo3(signature = (library=None))]
fn ensure_data(library: Option<String>) -> PyResult<String> {
    // Explicit override via NUCL_PARQUET_DATA (backward compat — the
    // pre-#529 fetch path in nucl_parquet::DataDir::resolve honoured it).
    // `--data-dir` / `HYRR_DATA` are handled by the Python entry point
    // in python/hyrr_mcp/__init__.py before we get here.
    if let Ok(v) = std::env::var("NUCL_PARQUET_DATA") {
        if !v.is_empty() {
            return Ok(v);
        }
    }

    // Local resolution: managed cache (sentinel-gated), sibling clone.
    let local = hyrr_core::data_dir::resolve();
    if std::path::Path::new(&local).join("meta").is_dir() {
        return Ok(local);
    }

    let lib = library.unwrap_or_else(|| hyrr_core::mcp::transport::DEFAULT_LIBRARY.to_string());
    let expected_url = hyrr_core::data_fetch::release_url();
    let expected_ver = hyrr_core::data_fetch::data_version();
    let cache_pattern = hyrr_core::data_fetch::cache_root_pattern();

    eprintln!(
        "hyrr-mcp: no local nucl-parquet data found; fetching release data-{expected_ver} …\n\
         (~54 MB metadata + stopping tables plus the {lib} library subtree — one-time on first run)"
    );

    hyrr_core::data_fetch::ensure_meta_stopping().map_err(|e| {
        make_fetch_error(
            "meta + stopping tables",
            &e,
            expected_ver,
            &expected_url,
            &cache_pattern,
            &lib,
        )
    })?;
    hyrr_core::data_fetch::ensure_library(&lib).map_err(|e| {
        make_fetch_error(
            &format!("library `{lib}`"),
            &e,
            expected_ver,
            &expected_url,
            &cache_pattern,
            &lib,
        )
    })?;

    let cache = hyrr_core::data_fetch::cache_dir().map_err(|e| {
        pyo3::exceptions::PyRuntimeError::new_err(format!(
            "hyrr-mcp: could not resolve cache dir after fetch: {e}"
        ))
    })?;
    Ok(cache.join("data").to_string_lossy().to_string())
}

/// Build the full multi-line diagnostic for a fetch failure. Kept as a
/// helper so the two `ensure_*` call sites can't drift on wording.
fn make_fetch_error(
    what: &str,
    err: &hyrr_core::data_fetch::FetchError,
    expected_ver: &str,
    expected_url: &str,
    cache_pattern: &str,
    library: &str,
) -> PyErr {
    pyo3::exceptions::PyRuntimeError::new_err(format!(
        "hyrr-mcp: could not fetch {what} for release data-{expected_ver}: {err}\n\
         \n\
         Expected release tag: data-{expected_ver}\n\
         Expected asset URL:   {expected_url}\n\
         Cache directory:      {cache_pattern}\n\
         \n\
         The pinned nucl-parquet data version (compiled in from the submodule's\n\
         catalog.json — the single source of truth for this build) doesn't match\n\
         any available release, or the fetch failed for network / disk reasons.\n\
         \n\
         Workarounds:\n  \
             * Download the release manually and pass --data-dir <PATH>:\n      \
                 gh release download data-{expected_ver} -R exoma-ch/nucl-parquet \\\n        \
                     -p 'nucl-parquet-data-{expected_ver}.tar.zst'\n      \
                 mkdir -p ~/.hyrr/nucl-parquet/{expected_ver} \\\n        \
                     && tar -I zstd -xf nucl-parquet-data-{expected_ver}.tar.zst \\\n           \
                     -C ~/.hyrr/nucl-parquet/{expected_ver}\n      \
                 uvx hyrr-mcp --data-dir ~/.hyrr/nucl-parquet/{expected_ver} --library {library}\n  \
             * Or point at an existing nucl-parquet checkout:\n      \
                 HYRR_DATA=/path/to/nucl-parquet/data uvx hyrr-mcp"
    ))
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
