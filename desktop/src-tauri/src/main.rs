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
/// cache is not already complete. Idempotent: a returning user pays no
/// IO. Failures here are logged and swallowed because the user can still
/// fall through to the lazy-fetch path if the bundle copy fails (e.g. the
/// installer didn't ship resources for some reason).
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
    let cache = match hyrr_core::data_fetch::cache_dir() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("seed_cache: no cache_dir: {e}");
            return;
        }
    };
    let cache_data = cache.join("data");
    if let Err(e) = std::fs::create_dir_all(&cache_data) {
        eprintln!("seed_cache: mkdir {}: {e}", cache_data.display());
        return;
    }
    for child in &["meta", "stopping", "catalog.json", "suppliers.json"] {
        let from = bundled_data.join(child);
        let to = cache_data.join(child);
        if to.exists() {
            continue;
        }
        if from.is_dir() {
            if let Err(e) = copy_dir_recursive(&from, &to) {
                eprintln!("seed_cache: copy {} → {}: {e}", from.display(), to.display());
                return;
            }
        } else if from.is_file() {
            if let Err(e) = std::fs::copy(&from, &to) {
                eprintln!("seed_cache: copy {} → {}: {e}", from.display(), to.display());
                return;
            }
        }
    }
    // Sentinel: meta+stopping alone don't make a "complete" cache by the
    // contract — a library is still needed. We do NOT write the .complete
    // sentinel here; the lazy-fetch of a library is what eventually writes
    // it. Bundling just shaves the 27+27=54 MB blocking download off the
    // first sim.
}

fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_recursive(&from, &to)?;
        } else {
            std::fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

/// Resolve the nuclear data library: `--library <id>` arg → `HYRR_LIBRARY`
/// env → `tendl-2024` (DEFAULT_LIBRARY).
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
