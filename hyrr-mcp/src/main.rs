//! hyrr-mcp — standalone stdio MCP server for HYRR.
//!
//! Same MCP surface as `hyrr --mcp` on the desktop binary; uses
//! the shared `hyrr_core::mcp` module, so tool definitions and
//! responses stay byte-identical across entry points.
//!
//! On first run the server auto-fetches the required nuclear data
//! (~50 MB for meta+stopping+default library) from GitHub Releases
//! into `~/.hyrr/nucl-parquet/`. Progress is printed to stderr so
//! the JSON-RPC stdout stream stays clean. Subsequent runs are
//! instant (sentinel-gated, no network).

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("hyrr-mcp {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print_help();
        return;
    }

    let library = resolve_library(&args);

    // Auto-fetch nuclear data on first run (#264 US3). Progress goes to
    // stderr so the JSON-RPC stdout channel stays clean. The ensure_*
    // functions are sentinel-gated and no-op on a warm cache.
    if !has_explicit_data_dir(&args) {
        ensure_data_or_exit(&library);
    }

    let data_dir = hyrr_core::data_dir::resolve();
    hyrr_core::mcp::transport::run_mcp_server_with_library(&data_dir, &library);
}

/// Check whether the user supplied an explicit --data-dir or HYRR_DATA.
/// When they did, we skip auto-fetch — they own their data layout.
fn has_explicit_data_dir(args: &[String]) -> bool {
    if args.windows(2).any(|w| w[0] == "--data-dir") {
        return true;
    }
    std::env::var("HYRR_DATA").map(|v| !v.is_empty()).unwrap_or(false)
}

/// Fetch meta+stopping and the requested library into the managed cache.
/// Prints progress to stderr. On failure, prints a diagnostic and exits
/// with code 2 — better than a cryptic JSON-RPC error mid-conversation.
fn ensure_data_or_exit(library: &str) {
    use hyrr_core::data_fetch;

    let mut progress = data_fetch::throttle(|p| {
        let pct = match p.bytes_total {
            Some(total) if total > 0 => format!(" ({:.0}%)", p.bytes_done as f64 / total as f64 * 100.0),
            _ => String::new(),
        };
        eprintln!("hyrr-mcp: {:?}{}", p.stage, pct);
    });

    if let Err(e) = data_fetch::ensure_meta_stopping_with_progress(&mut progress) {
        eprintln!(
            "hyrr-mcp: failed to fetch nuclear data: {e}\n\n\
             Set HYRR_DATA to point at an existing nucl-parquet/data/ directory,\n\
             or check your network connection and try again."
        );
        std::process::exit(2);
    }
    if let Err(e) = data_fetch::ensure_library_with_progress(library, &mut progress) {
        eprintln!(
            "hyrr-mcp: failed to fetch library `{library}`: {e}\n\n\
             Set HYRR_DATA to point at an existing nucl-parquet/data/ directory,\n\
             or check your network connection and try again."
        );
        std::process::exit(2);
    }
}

/// Resolve the nuclear data library: `--library <id>` arg → `HYRR_LIBRARY`
/// env → `tendl-2023-iso` (DEFAULT_LIBRARY).
fn resolve_library(args: &[String]) -> String {
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

fn print_help() {
    println!(
        "hyrr-mcp {}\n\
         \n\
         Stdio MCP server exposing HYRR radio-isotope production tools.\n\
         \n\
         USAGE:\n    \
             hyrr-mcp [--data-dir <PATH>] [--library <ID>]\n\
         \n\
         OPTIONS:\n    \
             --data-dir <PATH>  Override nucl-parquet data directory\n    \
             --library <ID>     Nuclear data library, e.g. tendl-2023-iso (default), tendl-2025, endfb-8.1\n    \
             --version, -V      Print version and exit\n    \
             --help, -h         Print this help and exit\n\
         \n\
         ENVIRONMENT:\n    \
             HYRR_DATA          Nucl-parquet data directory (if --data-dir not set)\n    \
             HYRR_LIBRARY       Nuclear data library (if --library not set)\n\
         \n\
         Data resolution priority:\n    \
             1. --data-dir argument\n    \
             2. HYRR_DATA environment variable\n    \
             3. Sibling nucl-parquet/ directory (dev layout)\n    \
             4. ~/.hyrr/nucl-parquet\n\
         \n\
         Register with Claude Code:\n    \
             claude mcp add hyrr -- hyrr-mcp\n",
        env!("CARGO_PKG_VERSION")
    );
}
