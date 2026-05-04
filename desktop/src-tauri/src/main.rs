// Prevents additional console window on Windows in release
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[allow(non_snake_case)]
mod commands;

use std::sync::Mutex;

use tauri::Manager;

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
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .manage(commands::DataStoreState(Mutex::new(None)))
        .setup(|app| {
            // Seed the managed cache from bundled `meta/`+`stopping/` on
            // first launch so default-library users get an immediate first
            // simulation without a network call. Library-specific xs/ data
            // is still fetched lazily on first use of that library — see
            // `commands::ensure_data`.
            seed_cache_from_resources(app);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::init_data_store,
            commands::run_compute_stack,
            commands::compute_depth_preview,
            commands::ensure_data,
            commands::data_ready,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// Copy bundled `meta/` and `stopping/` into the managed cache iff the
/// cache is not already complete. The actual copy + sentinel-write is
/// done by `data_fetch::seed_from_dir` under the cache lock — that's
/// the only safe way to write into the final cache directory while a
/// concurrent `--mcp` invocation might be calling `ensure_library`.
/// Failures are logged and swallowed: the user can still fall through
/// to the lazy-fetch path on first simulation.
fn seed_cache_from_resources(app: &tauri::App) {
    if hyrr_core::data_fetch::is_cache_complete() {
        return;
    }
    let resource_root = match app.path().resource_dir() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("seed_cache: no resource_dir: {e}");
            return;
        }
    };
    let bundled_data = resource_root.join("data");
    if !bundled_data.join("meta").is_dir() {
        // No bundled data — falls through to lazy fetch. Common in dev
        // builds where bundle.resources isn't materialized.
        return;
    }
    if let Err(e) = hyrr_core::data_fetch::seed_from_dir(&bundled_data) {
        eprintln!("seed_cache: seed_from_dir: {e}");
    }
}

/// Resolve the nuclear data library: `--library <id>` arg → `HYRR_LIBRARY`
/// env → `tendl-2025` (DEFAULT_LIBRARY).
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
