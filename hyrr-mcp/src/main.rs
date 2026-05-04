//! hyrr-mcp — standalone stdio MCP server for HYRR.
//!
//! Same MCP surface as `hyrr --mcp` on the desktop binary; uses
//! the shared `hyrr_core::mcp` module, so tool definitions and
//! responses stay byte-identical across entry points.

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

    let data_dir = hyrr_core::data_dir::resolve();
    let library = resolve_library(&args);
    hyrr_core::mcp::transport::run_mcp_server_with_library(&data_dir, &library);
}

/// Resolve the nuclear data library: `--library <id>` arg → `HYRR_LIBRARY`
/// env → `tendl-2025` (DEFAULT_LIBRARY).
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
             --library <ID>     Nuclear data library, e.g. tendl-2025 (default), endfb-8.1\n    \
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
