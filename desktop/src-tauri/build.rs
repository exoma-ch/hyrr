//! Tauri build hook + a guard against silently bundling empty
//! resources when the `nucl-parquet/` submodule isn't checked out.
//!
//! The failure mode this prevents: a fresh clone without
//! `--recurse-submodules`, then `cargo tauri build`, produces an
//! installer that ships with empty resource dirs (or, on some
//! platforms, fails to materialise them at all). The user launches,
//! `seed_cache_from_resources` returns silently because `data/meta`
//! isn't a dir, and they pay the *full* ~400 MB lazy fetch instead of
//! the ~50 MB the docs promise — a silent ~7× regression.

use std::path::Path;

fn main() {
    // Only enforce when actually bundling — `cargo check` and unit
    // tests should not fail just because the submodule is missing.
    if std::env::var("PROFILE").as_deref() == Ok("release") {
        let canary = Path::new("../../nucl-parquet/data/meta/elements.parquet");
        if !canary.exists() {
            panic!(
                "nucl-parquet submodule not checked out: {} missing.\n\
                 Run `git submodule update --init --recursive` before \
                 `cargo tauri build` — otherwise the installer ships \
                 with empty resource dirs and users pay the full ~400 MB \
                 lazy fetch on first launch.",
                canary.display()
            );
        }
        println!("cargo:rerun-if-changed=../../nucl-parquet/data/meta/elements.parquet");
    }

    tauri_build::build()
}
