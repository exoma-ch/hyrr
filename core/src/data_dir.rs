//! nucl-parquet data directory resolution.
//!
//! Shared between the desktop binary and the standalone `hyrr-mcp`
//! binary so both pick the same data dir with the same priority.

/// Resolve the nucl-parquet data directory.
///
/// Priority:
/// 1. `--data-dir <path>` CLI argument
/// 2. `HYRR_DATA` environment variable
/// 3. Sibling `nucl-parquet/` relative to the executable (dev layout)
/// 4. `~/.hyrr/nucl-parquet`
/// 5. Fallback: `"nucl-parquet"` (relative)
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

    let exe_path = std::env::current_exe().unwrap_or_default();
    let exe_dir = exe_path.parent().unwrap_or(std::path::Path::new("."));

    // The v0.6.0+ nucl-parquet layout puts everything under `data/`
    // (`data/meta/`, `data/tendl-2025/`, `data/stopping/`, …). Earlier
    // versions had `meta/` at the root, which is what the previous
    // probe checked — that probe never matched on a current clone, so
    // the resolver fell through to the literal `"nucl-parquet"` and
    // every Tauri/MCP launch from outside the dev tree silently
    // failed. Probe `data/meta` instead.
    for candidate in &[
        exe_dir.join("../../../nucl-parquet"),
        exe_dir.join("../../nucl-parquet"),
        std::path::PathBuf::from("nucl-parquet"),
        std::path::PathBuf::from("../nucl-parquet"),
    ] {
        if candidate.join("data/meta").exists() {
            return candidate.to_string_lossy().to_string();
        }
    }

    if let Ok(home) = std::env::var("HOME") {
        let path = format!("{}/.hyrr/nucl-parquet", home);
        if std::path::Path::new(&path).join("data/meta").exists() {
            return path;
        }
    }

    "nucl-parquet".to_string()
}
