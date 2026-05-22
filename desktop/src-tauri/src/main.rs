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
    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_opener::init())
        .manage(commands::BundledDataDir(Mutex::new(None)));

    // Auto-updater: skip on Linux package-manager installs (.deb / .rpm),
    // where the OS owns updates and a parallel auto-updater would conflict
    // with apt/dnf. AppImage / DMG / MSI / NSIS installs all opt in. The
    // env var is set by the bundler at build time per `linux.deb.depends`.
    // See #116 — the spike resolution settled on minisign single-key with
    // an offline backup; the runtime trust check is configured in
    // `tauri.conf.json` under `plugins.updater.pubkey`.
    if updater_enabled() {
        builder = builder.plugin(tauri_plugin_updater::Builder::new().build());
    }

    builder
        .manage(commands::DataStoreState(Mutex::new(None)))
        .setup(|app| {
            // Resolve the bundled resource dir and store it so
            // `init_data_store` can read directly from it — no copy to
            // a writable cache. The installer ships meta/ + stopping/ +
            // tendl-2023-iso/ in the bundle resources; ParquetDataStore
            // reads them in-place (read-only is fine, we never write).
            if let Ok(resource_root) = app.path().resource_dir() {
                let bundled_data = resource_root.join("data");
                if bundled_data.join("meta").is_dir() {
                    let state = app.state::<commands::BundledDataDir>();
                    *state.0.lock().unwrap() = Some(bundled_data.to_string_lossy().to_string());
                }
            }
            Ok(())
        })
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
/// (.deb/.rpm). Detection: the bundler sets `APPDIR` for AppImage runs and
/// leaves it unset for system-package installs; we also honour an explicit
/// `HYRR_DISABLE_UPDATER=1` env var so packagers can force-disable.
fn updater_enabled() -> bool {
    if std::env::var("HYRR_DISABLE_UPDATER")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
    {
        return false;
    }
    // Linux: only enable for AppImage runs. apt/dnf own the rest.
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
