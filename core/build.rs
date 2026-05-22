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

    // --- Embed nuclear data as a tar (#274) ---
    #[cfg(feature = "embed-data")]
    pack_data_tar(&default_library);
}

/// Pack the required nucl-parquet files into an uncompressed tar in OUT_DIR.
/// The tar is included at compile time via `include_bytes!` in `db.rs`.
#[cfg(feature = "embed-data")]
fn pack_data_tar(library: &str) {
    use std::fs;
    use std::io::Write;

    let data_root = Path::new("../nucl-parquet/data");
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR not set");
    let tar_path = Path::new(&out_dir).join("hyrr-data.tar");

    // Rerun if any data file changes.
    println!("cargo:rerun-if-changed=../nucl-parquet/data/meta/abundances.parquet");
    println!("cargo:rerun-if-changed=../nucl-parquet/data/stopping");
    println!("cargo:rerun-if-changed=../nucl-parquet/data/{library}");

    let file = fs::File::create(&tar_path).expect("create tar");
    let mut tar = tar::Builder::new(file);

    // Helper: add a single file under a relative path in the tar.
    let add_file = |tar: &mut tar::Builder<fs::File>, disk_path: &Path, tar_path: &str| {
        let data = fs::read(disk_path)
            .unwrap_or_else(|e| panic!("read {}: {e}", disk_path.display()));
        let mut header = tar::Header::new_gnu();
        header.set_size(data.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        tar.append_data(&mut header, tar_path, data.as_slice())
            .unwrap_or_else(|e| panic!("tar append {tar_path}: {e}"));
    };

    // Meta: single-file Dbs
    for name in &["abundances.parquet", "decay.parquet", "dose_constants.parquet"] {
        let disk = data_root.join("meta").join(name);
        if disk.exists() {
            add_file(&mut tar, &disk, &format!("meta/{name}"));
        }
    }

    // Stopping: full directory tree
    let stopping_dir = data_root.join("stopping");
    if stopping_dir.exists() {
        add_dir_recursive(&mut tar, &stopping_dir, "stopping");
    }

    // XS library
    let xs_dir = data_root.join(library).join("xs");
    if xs_dir.exists() {
        add_dir_recursive(&mut tar, &xs_dir, &format!("{library}/xs"));
    }

    // Emissions
    let emissions_dir = data_root.join("meta/ensdf/emissions");
    if emissions_dir.exists() {
        add_dir_recursive(&mut tar, &emissions_dir, "meta/ensdf/emissions");
    }

    tar.finish().expect("finish tar");

    let size = fs::metadata(&tar_path).map(|m| m.len()).unwrap_or(0);
    println!(
        "cargo:warning=core/build.rs: packed hyrr-data.tar ({} MB) into {}",
        size / 1_048_576,
        tar_path.display()
    );
}

#[cfg(feature = "embed-data")]
fn add_dir_recursive(tar: &mut tar::Builder<std::fs::File>, dir: &Path, prefix: &str) {
    use std::fs;

    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        let tar_path = format!("{prefix}/{name_str}");

        if path.is_dir() {
            add_dir_recursive(tar, &path, &tar_path);
        } else if path.extension().and_then(|e| e.to_str()) == Some("parquet")
            || path.extension().and_then(|e| e.to_str()) == Some("json")
        {
            let data = fs::read(&path)
                .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            tar.append_data(&mut header, &tar_path, data.as_slice())
                .unwrap_or_else(|e| panic!("tar append {tar_path}: {e}"));
        }
    }
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
