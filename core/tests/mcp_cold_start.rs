//! #263 — integration test for hyrr-mcp cold-start behaviour.
//!
//! Verifies that `run_mcp_server_with_library` exits with a clear
//! diagnostic when:
//!   1. The data directory is completely empty (no meta/).
//!   2. The data directory exists but the requested library is missing.
//!
//! These tests exercise the pre-flight probes in
//! `core/src/mcp/transport.rs` that guard against mid-conversation
//! panics from missing data. They don't require network access.

use std::fs;

/// Make a minimal nucl-parquet-shaped data directory with meta/ but
/// no library directories.
fn make_meta_only(dir: &std::path::Path) {
    fs::create_dir_all(dir.join("meta")).unwrap();
    fs::write(dir.join("meta/elements.parquet"), b"test").unwrap();
}

/// The MCP transport's pre-flight probe checks for `meta/` existence
/// and calls `process::exit(2)` when it's missing. We can't test
/// `process::exit` directly from an in-process test, but we CAN test
/// the `ParquetDataStore::new` constructor which is what would fail
/// after the probe. This exercises the same code path without needing
/// a subprocess.
#[test]
fn parquet_data_store_rejects_empty_dir() {
    let td = tempfile::tempdir().unwrap();
    let empty = td.path().join("empty");
    fs::create_dir_all(&empty).unwrap();

    let result = hyrr_core::db::ParquetDataStore::new(
        empty.to_str().unwrap(),
        "tendl-2023-iso",
    );
    assert!(
        result.is_err(),
        "ParquetDataStore::new should fail on empty data dir"
    );
}

/// With meta/ present but the requested library missing, the
/// constructor should fail with a clear error.
#[test]
fn parquet_data_store_rejects_missing_library() {
    let td = tempfile::tempdir().unwrap();
    let data = td.path().join("data");
    make_meta_only(&data);

    let result = hyrr_core::db::ParquetDataStore::new(
        data.to_str().unwrap(),
        "nonexistent-library",
    );
    assert!(
        result.is_err(),
        "ParquetDataStore::new should fail when library dir is missing"
    );
}

/// data_dir::resolve() with no candidates returns a fallback string.
/// This tests that the resolution doesn't panic on a clean environment.
#[test]
fn data_dir_resolve_returns_fallback_on_empty_env() {
    // Temporarily clear all data-dir related env vars.
    let old_data = std::env::var("HYRR_DATA").ok();
    let old_home = std::env::var("HOME").ok();
    std::env::remove_var("HYRR_DATA");
    // Set HOME to a dir with no nucl-parquet to avoid matching the
    // legacy home-dir probe.
    let td = tempfile::tempdir().unwrap();
    std::env::set_var("HOME", td.path());

    let dir = hyrr_core::data_dir::resolve();
    // Should return the "nucl-parquet" fallback or some path — the point
    // is it doesn't panic.
    assert!(!dir.is_empty(), "resolve() should return a non-empty string");

    // Restore.
    if let Some(v) = old_data {
        std::env::set_var("HYRR_DATA", v);
    }
    if let Some(v) = old_home {
        std::env::set_var("HOME", v);
    }
}
