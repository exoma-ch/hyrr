//! hyrr-mcp — standalone stdio MCP server for HYRR.
//!
//! Same MCP surface as `hyrr --mcp` on the desktop binary; uses
//! the shared `hyrr_core::mcp` module, so tool definitions and
//! responses stay byte-identical across entry points.
//!
//! On first run the server lazy-fetches the required nuclear data
//! files (~30 MB for a typical session) from GitHub via
//! nucl-parquet's `DataDir::ensure_lazy()`. Individual parquet files
//! are fetched on demand and cached in `~/.nucl-parquet/v{V}/`.
//! Progress is printed to stderr. Subsequent runs are instant.

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

    // Structured tracing (#159): stderr is the MCP log channel (stdout is the
    // JSON-RPC protocol stream), so the human layer never corrupts the protocol;
    // set HYRR_TRACE_FILE to also append structured JSONL. Held for the process.
    let _trace_guard = hyrr_core::trace_schema::init_native();

    let library = resolve_library(&args);
    let data_dir = resolve_data_dir(&args, &library);

    hyrr_core::mcp::transport::run_mcp_server_with_library(&data_dir, &library);
}

/// Resolve the data directory. Priority:
/// 1. --data-dir argument / HYRR_DATA env (explicit, no fetch)
/// 2. nucl-parquet DataDir::resolve() (existing local data)
/// 3. nucl-parquet DataDir::ensure_lazy() (lazy HTTP fetch)
fn resolve_data_dir(args: &[String], library: &str) -> String {
    // Explicit data dir — user owns it, no fetch
    if let Some(dir) = explicit_data_dir(args) {
        return dir;
    }

    // Try nucl-parquet's own resolution (env var, existing cache, sibling)
    if let Ok(dd) = nucl_parquet::DataDir::resolve() {
        return dd.root().to_string_lossy().to_string();
    }

    // No local data — use lazy fetch. Downloads catalog.json (~2 KB),
    // then individual parquet files on demand as NpDataStore opens them.
    match nucl_parquet::DataDir::ensure_lazy() {
        Ok(dd) => {
            // Pre-fetch the files NpDataStore::new() needs eagerly.
            // Without this, the Db::open() calls would fail on missing files.
            let eager_files = [
                "meta/abundances.parquet",
                "meta/decay.parquet",
                "meta/dose_constants.parquet",
                "stopping/PSTAR.parquet",
                "stopping/ASTAR.parquet",
                "stopping/dSTAR.parquet",
                "stopping/tSTAR.parquet",
                // CatIMA stopping was federated upstream from one monolith into
                // per-beam-isotope shards (nucl-parquet #254); pre-fetch the
                // beams HYRR bundles (matches core/src/stopping.rs
                // BUNDLED_CATIMA_PROJECTILES and scripts/copy-frontend-data.sh).
                // He3 is the ³He ("h") beam, now per-isotope CatIMA not ASTAR×4/3 (#194).
                "stopping/catima_He3.parquet",
                "stopping/catima_C12.parquet",
                "stopping/catima_O16.parquet",
                "stopping/catima_Ne20.parquet",
                "stopping/catima_Si28.parquet",
                "stopping/catima_Ar40.parquet",
                "stopping/catima_Fe56.parquet",
                "stopping/compounds/PSTAR_compounds.parquet",
                "stopping/compounds/ASTAR_compounds.parquet",
            ];
            for f in &eager_files {
                if let Err(e) = dd.fetch_file(f) {
                    eprintln!("hyrr-mcp: warning: failed to fetch {f}: {e}");
                }
            }

            // Pre-fetch the manifest for the requested library so
            // NpDataStore can find the xs/ directory.
            let manifest = format!("{library}/manifest.json");
            let _ = dd.fetch_file(&manifest);

            dd.root().to_string_lossy().to_string()
        }
        Err(e) => {
            eprintln!(
                "hyrr-mcp: failed to resolve nuclear data: {e}\n\n\
                 Set HYRR_DATA or NUCL_PARQUET_DATA to point at a nucl-parquet data directory,\n\
                 or check your network connection.\n"
            );
            std::process::exit(2);
        }
    }
}

/// Check whether the user supplied an explicit --data-dir or HYRR_DATA.
fn explicit_data_dir(args: &[String]) -> Option<String> {
    for w in args.windows(2) {
        if w[0] == "--data-dir" {
            return Some(w[1].clone());
        }
    }
    if let Ok(v) = std::env::var("HYRR_DATA") {
        if !v.is_empty() {
            return Some(v);
        }
    }
    None
}

/// Resolve the nuclear data library: `--library <id>` arg → `HYRR_LIBRARY`
/// env → DEFAULT_LIBRARY (`tendl-2023-iso`).
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
             NUCL_PARQUET_DATA  Nucl-parquet data directory (alternative)\n    \
             HYRR_LIBRARY       Nuclear data library (if --library not set)\n\
         \n\
         On first run, ~5 MB of metadata is fetched from GitHub.\n\
         Cross-section data is fetched per-element on demand as tools\n\
         query it. Cached in ~/.nucl-parquet/.\n\
         \n\
         Register with Claude Code:\n    \
             claude mcp add hyrr -- hyrr-mcp\n",
        env!("CARGO_PKG_VERSION")
    );
}
