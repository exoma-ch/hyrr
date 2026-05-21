//! Sync `DATA_VERSION` (used by `data_fetch.rs` to build the GitHub
//! Releases URL) from the pinned `nucl-parquet/` submodule's
//! `data/catalog.json::data_version`. The submodule is the single
//! source of truth.
//!
//! Why this exists: the previous setup had `DATA_VERSION` as a
//! hand-maintained `pub const` next to a submodule pin. The same class
//! of drift just bit us in #117 (default library `tendl-2024` vs docs
//! `tendl-2025`); this prevents the analogous bug for data version.
//!
//! Upstream wire-format note (nucl-parquet PR #151 — "Path A"): the
//! data version split off from the Python package version. It now
//! lives in `data/catalog.json` (CalVer YYYY.MM.MICRO, e.g.
//! `2026.5.0`) rather than `pyproject.toml::[project].version`
//! (SemVer). The release tag (`data-2026.5.0`) and tarball filename
//! (`nucl-parquet-data-2026.5.0.tar.zst`) follow the same CalVer.
//!
//! On a fresh clone without `--recurse-submodules` the file is missing
//! — we emit a warning and fall back to a sentinel version so wasm/CI
//! builds that don't actually call the network path still compile.
//! Native paths that *do* hit the network would then 404 visibly,
//! which is a clearer failure mode than a silent stale download.

use std::path::Path;

fn main() {
    // --- DATA_VERSION from nucl-parquet submodule ---
    let catalog = Path::new("../nucl-parquet/data/catalog.json");
    println!("cargo:rerun-if-changed=../nucl-parquet/data/catalog.json");

    let version = match std::fs::read_to_string(catalog) {
        Ok(s) => extract_json_string(&s, "data_version").unwrap_or_else(|| {
            println!(
                "cargo:warning=core/build.rs: could not parse top-level \"data_version\" from {}; \
                 falling back to 0.0.0-unknown. Bump the submodule cleanly.",
                catalog.display()
            );
            "0.0.0-unknown".to_string()
        }),
        Err(_) => {
            println!(
                "cargo:warning=core/build.rs: {} not found — submodule not checked out? \
                 Falling back to 0.0.0-unknown. Run `git submodule update --init --recursive`.",
                catalog.display()
            );
            "0.0.0-unknown".to_string()
        }
    };
    println!("cargo:rustc-env=HYRR_DATA_VERSION={version}");

    // --- DEFAULT_LIBRARY from hyrr.json (SSoT #269) ---
    let hyrr_json = Path::new("../hyrr.json");
    println!("cargo:rerun-if-changed=../hyrr.json");

    let default_library = match std::fs::read_to_string(hyrr_json) {
        Ok(s) => extract_json_string(&s, "default_library").unwrap_or_else(|| {
            println!(
                "cargo:warning=core/build.rs: could not parse \"default_library\" from hyrr.json; \
                 falling back to tendl-2023-iso."
            );
            "tendl-2023-iso".to_string()
        }),
        Err(_) => {
            println!(
                "cargo:warning=core/build.rs: hyrr.json not found; falling back to tendl-2023-iso."
            );
            "tendl-2023-iso".to_string()
        }
    };
    println!("cargo:rustc-env=HYRR_DEFAULT_LIBRARY={default_library}");
}

/// Extract a top-level `"key": "value"` string from JSON without
/// pulling a `serde_json` build-dep. Tolerates whitespace around `:`.
fn extract_json_string(content: &str, field: &str) -> Option<String> {
    let key = format!("\"{}\"", field);
    let idx = content.find(&key)?;
    let rest = &content[idx + key.len()..];
    // Skip whitespace and the colon.
    let after_colon = rest.trim_start().strip_prefix(':')?.trim_start();
    // Read the quoted value.
    let after_open_quote = after_colon.strip_prefix('"')?;
    let close = after_open_quote.find('"')?;
    let value = &after_open_quote[..close];
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}
