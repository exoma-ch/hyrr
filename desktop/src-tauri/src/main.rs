// Prevents additional console window on Windows in release
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[allow(non_snake_case)]
mod commands;
mod mcp;

use std::sync::Mutex;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.iter().any(|a| a == "--mcp") {
        // MCP mode: stdio JSON-RPC server, no GUI
        let data_dir = resolve_data_dir();
        mcp::transport::run_mcp_server(&data_dir);
        return;
    }

    // GUI mode: standard Tauri app
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .manage(commands::DataStoreState(Mutex::new(None)))
        .invoke_handler(tauri::generate_handler![
            commands::init_data_store,
            commands::run_compute_stack,
            commands::compute_depth_preview,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// Resolve nucl-parquet data directory.
///
/// Priority:
/// 1. `--data-dir <path>` CLI argument
/// 2. `HYRR_DATA` environment variable
/// 3. `../nucl-parquet` sibling directory (dev layout)
/// 4. Bundled resources (installed app)
/// 5. `~/.hyrr/nucl-parquet` fallback
fn resolve_data_dir() -> String {
    // Check CLI args
    let args: Vec<String> = std::env::args().collect();
    for i in 0..args.len() - 1 {
        if args[i] == "--data-dir" {
            return args[i + 1].clone();
        }
    }

    // Check env var
    if let Ok(dir) = std::env::var("HYRR_DATA") {
        return dir;
    }

    // Dev layout: sibling directory
    let exe_path = std::env::current_exe().unwrap_or_default();
    let exe_dir = exe_path.parent().unwrap_or(std::path::Path::new("."));

    // Try relative to project root (dev)
    for candidate in &[
        exe_dir.join("../../../nucl-parquet"),
        exe_dir.join("../../nucl-parquet"),
        std::path::PathBuf::from("nucl-parquet"),
        std::path::PathBuf::from("../nucl-parquet"),
    ] {
        if candidate.join("meta").exists() {
            return candidate.to_string_lossy().to_string();
        }
    }

    // Home directory fallback
    if let Ok(home) = std::env::var("HOME") {
        let path = format!("{}/.hyrr/nucl-parquet", home);
        if std::path::Path::new(&path).join("meta").exists() {
            return path;
        }
    }

    // Last resort
    "nucl-parquet".to_string()
}
