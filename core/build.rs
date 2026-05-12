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
    let catalog = Path::new("../nucl-parquet/data/catalog.json");
    println!("cargo:rerun-if-changed=../nucl-parquet/data/catalog.json");

    let version = match std::fs::read_to_string(catalog) {
        Ok(s) => extract_data_version(&s).unwrap_or_else(|| {
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
}

/// Tiny JSON walker — avoids pulling a `serde_json` build-dep for one
/// string field. Matches the top-level `"data_version": "..."` pair.
/// Tolerates whitespace variation around `:` and between the key and
/// its value. JSON is unambiguous about string quoting (always `"`),
/// so this is simpler than the previous hand-rolled TOML walker.
fn extract_data_version(content: &str) -> Option<String> {
    // Quick-and-dirty: scan for the `"data_version"` key, then read the
    // following quoted string. Sufficient for a fixed top-level field
    // in a generated catalog.json — we don't need to parse nested
    // objects or escape sequences.
    let key = "\"data_version\"";
    let idx = content.find(key)?;
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
