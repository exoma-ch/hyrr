//! nucl-parquet data directory resolution.
//!
//! Shared between the desktop binary and the standalone `hyrr-mcp`
//! binary so both pick the same data dir with the same priority.
//!
//! ## Contract
//!
//! Returns a path that contains `meta/`, `stopping/`, and `<library>/xs/`
//! directly as immediate subdirectories. This is the path consumed by
//! `ParquetDataStore::new` (which joins `meta/`, `stopping/`, `<library>/`
//! to it without further indirection) and by `mcp::transport`'s pre-flight
//! probe.
//!
//! For the v0.6.0+ nucl-parquet layout — where files live under a `data/`
//! prefix inside a checkout — the returned path is the `data/` directory
//! itself, not the checkout root.

/// Resolve the nucl-parquet data directory. Returns a path that contains
/// `meta/`, `stopping/`, and `<library>/xs/` as direct children.
///
/// Priority:
/// 1. `--data-dir <path>` CLI argument (returned verbatim)
/// 2. `HYRR_DATA` environment variable (returned verbatim)
/// 3. Managed cache `~/.hyrr/nucl-parquet/v{DATA_VERSION}/.complete` sentinel
///    (returned as `<cache>/v{V}/data`) — populated by `data_fetch::ensure_*`.
/// 4. Sibling `nucl-parquet/data/` relative to the executable (dev layout)
/// 5. Legacy unmanaged `~/.hyrr/nucl-parquet/data/` (no sentinel)
/// 6. Fallback: `"nucl-parquet"` (relative; will fail loudly downstream)
pub fn resolve() -> String {
    let args: Vec<String> = std::env::args().collect();
    if args.len() >= 2 {
        for i in 0..args.len() - 1 {
            if args[i] == "--data-dir" {
                return args[i + 1].clone();
            }
        }
    }

    if let Ok(dir) = std::env::var("HYRR_DATA") {
        return dir;
    }

    // Managed cache: prefer this over dev-tree candidates so a Tauri user
    // who has both a sibling clone and a populated cache picks the cache
    // (which the installer/CLI explicitly populated).
    #[cfg(not(target_arch = "wasm32"))]
    if let Ok(cache) = crate::data_fetch::cache_dir() {
        if crate::data_fetch::is_cache_complete() {
            let data = cache.join("data");
            if data.is_dir() {
                return data.to_string_lossy().to_string();
            }
        }
    }

    let exe_path = std::env::current_exe().unwrap_or_default();
    let exe_dir = exe_path.parent().unwrap_or(std::path::Path::new("."));

    // Sibling-clone candidates. The v0.6.0+ layout puts data under `data/`
    // (`data/meta/`, `data/<library>/`, …). Probe `data/meta` to detect the
    // layout, *return `data/`* — consumers join `meta/` directly to this.
    for candidate in &[
        exe_dir.join("../../../nucl-parquet"),
        exe_dir.join("../../nucl-parquet"),
        std::path::PathBuf::from("nucl-parquet"),
        std::path::PathBuf::from("../nucl-parquet"),
    ] {
        if candidate.join("data/meta").exists() {
            return candidate.join("data").to_string_lossy().to_string();
        }
    }

    // Legacy unmanaged home dir (a user who manually `git clone`d into
    // `~/.hyrr/nucl-parquet`, no managed sentinel).
    if let Ok(home) = std::env::var("HOME") {
        let path = format!("{home}/.hyrr/nucl-parquet/data");
        if std::path::Path::new(&path).join("meta").exists() {
            return path;
        }
    }

    "nucl-parquet".to_string()
}
