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
        hyrr_core::mcp::transport::run_mcp_server(&data_dir);
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

