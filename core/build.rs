//! Sync `DATA_VERSION` (used by `data_fetch.rs` to build the GitHub
//! Releases URL) from the pinned `nucl-parquet/` submodule's
//! pyproject.toml. The submodule is the single source of truth.
//!
//! Why this exists: the previous setup had `DATA_VERSION` as a
//! hand-maintained `pub const` next to a submodule pin. The same class
//! of drift just bit us in #117 (default library `tendl-2024` vs docs
//! `tendl-2025`); this prevents the analogous bug for data version.
//!
//! On a fresh clone without `--recurse-submodules` the file is missing
//! — we emit a warning and fall back to a sentinel version so wasm/CI
//! builds that don't actually call the network path still compile.
//! Native paths that *do* hit the network would then 404 visibly,
//! which is a clearer failure mode than a silent stale download.

use std::path::Path;

fn main() {
    let pyproject = Path::new("../nucl-parquet/pyproject.toml");
    println!("cargo:rerun-if-changed=../nucl-parquet/pyproject.toml");

    let version = match std::fs::read_to_string(pyproject) {
        Ok(s) => extract_project_version(&s).unwrap_or_else(|| {
            println!(
                "cargo:warning=core/build.rs: could not parse [project].version from {}; \
                 falling back to 0.0.0-unknown. Bump the submodule cleanly.",
                pyproject.display()
            );
            "0.0.0-unknown".to_string()
        }),
        Err(_) => {
            println!(
                "cargo:warning=core/build.rs: {} not found — submodule not checked out? \
                 Falling back to 0.0.0-unknown. Run `git submodule update --init --recursive`.",
                pyproject.display()
            );
            "0.0.0-unknown".to_string()
        }
    };

    println!("cargo:rustc-env=HYRR_DATA_VERSION={version}");
}

/// Tiny TOML walker — avoids pulling a `toml` build-dep for one
/// version string. Handles the standard pyproject layout: top-level
/// `[project]` section with `version = "..."`. Tolerates leading
/// whitespace and inline comments after the value.
fn extract_project_version(content: &str) -> Option<String> {
    let mut in_project = false;
    for raw in content.lines() {
        let line = raw.trim();
        if line.starts_with('[') && line.ends_with(']') {
            in_project = line == "[project]";
            continue;
        }
        if !in_project {
            continue;
        }
        let Some(rest) = line.strip_prefix("version") else {
            continue;
        };
        let rest = rest.trim_start();
        let Some(after_eq) = rest.strip_prefix('=') else {
            continue;
        };
        // Strip trailing comment, then surrounding quotes/whitespace.
        let value = after_eq.split('#').next()?.trim();
        let trimmed = value.trim_matches('"').trim_matches('\'').trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    None
}
