// Prevents additional console window on Windows in release
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[allow(non_snake_case)]
mod commands;

use std::sync::Mutex;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.iter().any(|a| a == "--mcp") {
        // MCP mode: stdio JSON-RPC server, no GUI
        let data_dir = hyrr_core::data_dir::resolve();
        let library = resolve_mcp_library(&args);
        hyrr_core::mcp::transport::run_mcp_server_with_library(&data_dir, &library);
        return;
    }

    // GUI mode: standard Tauri app
    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_opener::init());

    if updater_enabled() {
        builder = builder.plugin(tauri_plugin_updater::Builder::new().build());
    }

    builder
        .manage(commands::DataStoreState(Mutex::new(None)))
        .invoke_handler(tauri::generate_handler![
            commands::init_data_store,
            commands::run_compute_stack,
            commands::compute_depth_preview,
            commands::data_release_url,
            commands::data_release_base_url,
            commands::data_version,
            commands::data_tarball_filename,
            commands::data_cache_root_pattern,
            commands::updater_enabled,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// Disable the updater plugin entirely on Linux package-manager installs
/// (.deb/.rpm).
fn updater_enabled() -> bool {
    if std::env::var("HYRR_DISABLE_UPDATER")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
    {
        return false;
    }
    #[cfg(target_os = "linux")]
    {
        return std::env::var("APPIMAGE").is_ok() || std::env::var("APPDIR").is_ok();
    }
    #[cfg(not(target_os = "linux"))]
    {
        true
    }
}

/// Resolve the nuclear data library: `--library <id>` arg → `HYRR_LIBRARY`
/// env → DEFAULT_LIBRARY (`tendl-2023-iso`).
fn resolve_mcp_library(args: &[String]) -> String {
    if args.len() >= 2 {
        for i in 0..args.len() - 1 {
            if args[i] == "--library" {
                return args[i + 1].clone();
            }
        }
    }
    if let Ok(id) = std::env::var("HYRR_LIBRARY") {
        if !id.is_empty() {
            return id;
        }
    }
    hyrr_core::mcp::transport::DEFAULT_LIBRARY.to_string()
}
