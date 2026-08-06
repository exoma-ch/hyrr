//! hyrr-mcp — standalone stdio MCP server for HYRR.
//!
//! Same MCP surface as `hyrr --mcp` on the desktop binary; uses
//! the shared `hyrr_core::mcp` module, so tool definitions and
//! responses stay byte-identical across entry points.
//!
//! On first run — when no `--data-dir` / `HYRR_DATA` / `NUCL_PARQUET_DATA`
//! points at an existing nucl-parquet checkout, and the managed cache is
//! empty — the server fetches a ~54 MB metadata + stopping bundle plus
//! the requested library subtree from the GitHub release matching the
//! **submodule-pinned** data version (see [`hyrr_core::data_fetch`] for
//! the SSoT). Progress is printed to stderr. Subsequent runs short-circuit
//! against the sentinel and start instantly.
//!
//! **Failure mode contract** (#529): a fetch that can't complete — 404
//! on the release asset, network blackhole, disk full — is a hard exit
//! with a clear multi-line diagnostic naming the expected release tag,
//! the URL that was tried, and the cache path. The MCP process never
//! continues into the transport loop with a partial/empty data dir; the
//! symptom that surfaced as #529 (silent stopping-table lookup failures
//! downstream of an aborted lazy fetch) is impossible by construction.

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
///
/// 1. `--data-dir <PATH>` argument (verbatim, no fetch)
/// 2. `HYRR_DATA` env (verbatim, no fetch)
/// 3. `NUCL_PARQUET_DATA` env (verbatim, no fetch — backward compat)
/// 4. Managed cache `~/.hyrr/nucl-parquet/v{DATA_VERSION}/data` if the
///    `.complete` sentinel is present
/// 5. Sibling nucl-parquet checkout (dev tree) if it has `data/meta/`
/// 6. **Auto-fetch** the release matching the submodule-pinned
///    `DATA_VERSION` — hard-exit with a diagnostic if that fails.
///
/// Steps 4-5 delegate to [`hyrr_core::data_dir::resolve`]; step 6 to
/// [`hyrr_core::data_fetch::ensure_meta_stopping`] + `ensure_library`,
/// which use the correct CalVer release URL (`data-{V}/…`) derived at
/// build time from the pinned submodule catalog — the same SSoT the
/// desktop binary and PyO3 CLI use, so no drift between entry points.
fn resolve_data_dir(args: &[String], library: &str) -> String {
    if let Some(dir) = explicit_data_dir(args) {
        return dir;
    }

    // Local resolution: managed cache (sentinel-gated), sibling clone,
    // legacy home. Returns the fallback string "nucl-parquet" (relative)
    // if nothing matches, which the `meta/` probe below rejects so we
    // fall through to the fetch path.
    let local = hyrr_core::data_dir::resolve();
    if std::path::Path::new(&local).join("meta").is_dir() {
        return local;
    }

    // Cold cache: auto-fetch the CalVer release pinned by the submodule.
    fetch_or_die(library)
}

/// Fetch the required nuclear data through hyrr-core's canonical
/// fetch path, or hard-exit with a diagnostic that names the expected
/// release tag, URL, and cache dir.
///
/// **This is where #529's silent-failure surface got closed off.** The
/// old path (`nucl_parquet::DataDir::ensure_lazy` + eager per-file
/// fetches with `if let Err(e) = ... eprintln!("warning: ...")`) let
/// individual 404s slip through; the process then entered
/// `run_mcp_server_with_library` with a half-populated data dir and
/// the first tool call surfaced a confusing stopping-table error
/// downstream. Now every `FetchError` short-circuits to a full
/// diagnostic + `exit(2)` before the transport loop starts.
fn fetch_or_die(library: &str) -> String {
    let expected_url = hyrr_core::data_fetch::release_url();
    let expected_ver = hyrr_core::data_fetch::data_version();
    let cache_pattern = hyrr_core::data_fetch::cache_root_pattern();

    eprintln!(
        "hyrr-mcp: no local nucl-parquet data found; fetching release data-{expected_ver} …\n\
         (~54 MB metadata + stopping tables plus the {library} library subtree — one-time on first run)"
    );

    if let Err(e) = hyrr_core::data_fetch::ensure_meta_stopping() {
        die_with_fetch_diagnostic(
            "meta + stopping tables",
            &e,
            expected_ver,
            &expected_url,
            &cache_pattern,
            library,
        );
    }
    if let Err(e) = hyrr_core::data_fetch::ensure_library(library) {
        die_with_fetch_diagnostic(
            &format!("library `{library}`"),
            &e,
            expected_ver,
            &expected_url,
            &cache_pattern,
            library,
        );
    }

    let cache = match hyrr_core::data_fetch::cache_dir() {
        Ok(c) => c.join("data"),
        Err(e) => {
            eprintln!("hyrr-mcp: FATAL: could not resolve cache dir after fetch: {e}");
            std::process::exit(2);
        }
    };
    cache.to_string_lossy().to_string()
}

/// Emit the full multi-line diagnostic for a fetch failure and exit(2).
/// Kept as a helper so the two `ensure_*` call sites can't drift on
/// message shape.
fn die_with_fetch_diagnostic(
    what: &str,
    err: &hyrr_core::data_fetch::FetchError,
    expected_ver: &str,
    expected_url: &str,
    cache_pattern: &str,
    library: &str,
) -> ! {
    eprintln!(
        "\n\
         hyrr-mcp: FATAL: could not fetch {what} for release data-{expected_ver}: {err}\n\
         \n\
         Expected release tag: data-{expected_ver}\n\
         Expected asset URL:   {expected_url}\n\
         Cache directory:      {cache_pattern}\n\
         \n\
         The pinned nucl-parquet data version (compiled in from the submodule's\n\
         catalog.json — the single source of truth for this build) doesn't match\n\
         any available release, or the fetch failed for network / disk reasons.\n\
         \n\
         Workarounds:\n  \
             * Download the release manually and pass --data-dir <PATH>:\n      \
                 gh release download data-{expected_ver} -R exoma-ch/nucl-parquet \\\n        \
                     -p 'nucl-parquet-data-{expected_ver}.tar.zst'\n      \
                 mkdir -p ~/.hyrr/nucl-parquet/{expected_ver} \\\n        \
                     && tar -I zstd -xf nucl-parquet-data-{expected_ver}.tar.zst \\\n           \
                     -C ~/.hyrr/nucl-parquet/{expected_ver}\n      \
                 hyrr-mcp --data-dir ~/.hyrr/nucl-parquet/{expected_ver} --library {library}\n  \
             * Or point at an existing nucl-parquet checkout:\n      \
                 HYRR_DATA=/path/to/nucl-parquet/data hyrr-mcp\n"
    );
    std::process::exit(2);
}

/// Check whether the user supplied an explicit `--data-dir`, `HYRR_DATA`
/// env, or `NUCL_PARQUET_DATA` env. Any of these bypass the auto-fetch —
/// the user is telling us where the data lives.
fn explicit_data_dir(args: &[String]) -> Option<String> {
    for w in args.windows(2) {
        if w[0] == "--data-dir" {
            return Some(w[1].clone());
        }
    }
    for var in ["HYRR_DATA", "NUCL_PARQUET_DATA"] {
        if let Ok(v) = std::env::var(var) {
            if !v.is_empty() {
                return Some(v);
            }
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
             HYRR_DATA                   Nucl-parquet data directory (if --data-dir not set)\n    \
             NUCL_PARQUET_DATA           Nucl-parquet data directory (backward-compat alternative)\n    \
             HYRR_LIBRARY                Nuclear data library (if --library not set)\n    \
             HYRR_DISABLE_UPDATE_CHECK   Opt-out (1/true/yes) of the once-per-24h network\n                                 update check (#571). The compiled-in CalVer\n                                 staleness warning still fires without any network.\n\
         \n\
         On first run (no local data, no --data-dir / HYRR_DATA), fetches the\n\
         nucl-parquet release matching the submodule-pinned data version\n\
         (~54 MB metadata + stopping tables plus the requested library subtree).\n\
         Cached in ~/.hyrr/nucl-parquet/v{{V}}/. A failed fetch is a hard exit\n\
         with a clear diagnostic — never a silent-empty data dir.\n\
         \n\
         Update awareness (#571):\n    \
             The server surfaces its own version, the compiled-in nuclear-data\n    \
             CalVer, and — when known — the latest release on GitHub via the\n    \
             MCP `initialize` instructions and the `get_version_info` tool. The\n    \
             network check runs in the background (5 s timeout, cached under\n    \
             $XDG_CACHE_HOME/hyrr/ for 24 h, fail-silent). Air-gapped installs\n    \
             see no network activity but still get the CalVer staleness warning.\n    \
             Set HYRR_DISABLE_UPDATE_CHECK=1 to skip the network check entirely.\n    \
             Recommended MCP-client config: leave `hyrr-mcp` unpinned in your MCP\n    \
             registry and periodically run `uvx --refresh hyrr-mcp` — a pinned\n    \
             `hyrr-mcp==X.Y.Z` freezes this server forever, including any silent\n    \
             physics-altering fixes shipped upstream (see #533, #529, #488).\n\
         \n\
         Register with Claude Code:\n    \
             claude mcp add hyrr -- hyrr-mcp\n",
        env!("CARGO_PKG_VERSION")
    );
}

#[cfg(test)]
mod tests {
    /// Regression guard for #529: the compiled-in `DATA_VERSION` must
    /// match the CalVer pattern (`YYYY.MM.MICRO`) that nucl-parquet's
    /// data-releases actually publish. A SemVer-shaped value (e.g. the
    /// pre-#529 `0.15.0`) would silently 404 at fetch time because no
    /// `data-0.15.0` release exists — that's the exact drift that #529
    /// tracked. The `data_version_is_resolved_from_submodule` sibling
    /// test in hyrr-core asserts N.N.N; this one adds the CalVer year
    /// prefix so a future crate-version leak (e.g. `1.2.3` from a hand-
    /// edited catalog) can't slip through with the shape intact.
    ///
    /// Skips cleanly (via panic → `#[should_panic]` inversion, no) if
    /// `DATA_VERSION` is the build.rs fallback sentinel; that's a
    /// separate assertion in hyrr-core's `data_version_is_resolved_from_submodule`.
    #[test]
    fn data_version_is_calver() {
        let v = hyrr_core::data_fetch::data_version();

        // Sentinel from core/build.rs when the submodule is missing at
        // build time. That's a separate failure surface asserted in
        // hyrr-core's own test suite; here we just skip so this test
        // stays green on a fresh clone without --recurse-submodules.
        if v == "0.0.0-unknown" {
            return;
        }

        let parts: Vec<&str> = v.split('.').collect();
        assert_eq!(parts.len(), 3, "DATA_VERSION = {v:?} is not N.N.N");
        let year: u32 = parts[0]
            .parse()
            .unwrap_or_else(|_| panic!("DATA_VERSION year segment not numeric: {v:?}"));
        assert!(
            (2024..=2100).contains(&year),
            "DATA_VERSION = {v:?} first segment {year} isn't a plausible CalVer year — \
             probably drifted back to SemVer (this is #529 regressing). \
             Update core/build.rs to derive from nucl-parquet/data/catalog.json::data_version."
        );
    }

    /// The compiled release URL must:
    ///   1. use the `data-{V}` tag shape (no leading `v` — nucl-parquet#151 Path A),
    ///   2. match `nucl-parquet-data-{V}.tar.zst` filename shape,
    ///   3. carry the CalVer `data_version()` as both segments.
    ///
    /// Pins the shape hyrr-mcp constructs — if this fails the release
    /// URL doesn't match any real GitHub asset.
    #[test]
    fn release_url_uses_calver_data_tag() {
        let v = hyrr_core::data_fetch::data_version();
        if v == "0.0.0-unknown" {
            return;
        }
        let url = hyrr_core::data_fetch::release_url();

        assert!(
            url.contains(&format!("/data-{v}/")),
            "release_url() = {url:?} missing /data-{v}/ tag segment"
        );
        assert!(
            url.contains(&format!("nucl-parquet-data-{v}.tar.zst")),
            "release_url() = {url:?} missing nucl-parquet-data-{v}.tar.zst filename"
        );
        assert!(
            !url.contains(&format!("/data-v{v}/")),
            "release_url() = {url:?} regressed to legacy `v`-prefixed tag (pre nucl-parquet#151)"
        );
        assert!(
            !url.contains(&format!("nucl-parquet-data-v{v}.tar.zst")),
            "release_url() = {url:?} regressed to legacy `v`-prefixed filename"
        );
    }

    /// Regression guard for #529: `FetchError::HttpStatus` must
    /// serialise the canonical release URL into its payload's `url`
    /// field. hyrr-mcp's `die_with_fetch_diagnostic` reprints the URL,
    /// but the structured payload is the mechanised recovery hook the
    /// frontend / MCP consumers can parse — losing the URL here is a
    /// #529-shaped bug against those consumers.
    #[test]
    fn http_status_fetch_error_payload_carries_release_url() {
        let v = hyrr_core::data_fetch::data_version();
        if v == "0.0.0-unknown" {
            return;
        }
        let payload = hyrr_core::data_fetch::FetchErrorPayload::from(
            &hyrr_core::data_fetch::FetchError::HttpStatus(404),
        );
        let s = payload.to_json_string();
        assert!(
            s.contains(&format!("data-{v}")),
            "HttpStatus payload missing data-{v} tag: {s}"
        );
        assert!(
            s.contains(&format!("nucl-parquet-data-{v}.tar.zst")),
            "HttpStatus payload missing tarball filename: {s}"
        );
    }
}
