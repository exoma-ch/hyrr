//! JSON-RPC over stdin/stdout transport for MCP.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::{self, BufRead, Write};
use std::sync::{Arc, RwLock};

use super::tools;
use crate::db::DatabaseProtocol;
use crate::materials::MaterialRegistry;

/// JSON-RPC 2.0 request.
#[derive(Debug, Deserialize)]
pub struct JsonRpcRequest {
    #[allow(dead_code)]
    pub jsonrpc: String,
    pub id: Option<Value>,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

/// JSON-RPC 2.0 response.
#[derive(Debug, Serialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

/// JSON-RPC 2.0 error.
#[derive(Debug, Serialize)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl JsonRpcResponse {
    fn success(id: Option<Value>, result: Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    fn error(id: Option<Value>, code: i64, message: String) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(JsonRpcError {
                code,
                message,
                data: None,
            }),
        }
    }
}

/// MCP server info.
const SERVER_NAME: &str = "hyrr";
const SERVER_VERSION: &str = crate::VERSION;

/// MCP protocol revisions this server can speak on the wire.
///
/// Per the MCP spec (revisions list at
/// <https://modelcontextprotocol.io/specification>), a compliant server
/// echoes the client's requested `protocolVersion` in `initialize` when
/// the version is supported, and otherwise returns its own latest
/// supported revision. See [`negotiate_protocol_version`].
///
/// We keep `2024-11-05` (the first revision) in this set for back-compat
/// with older clients. The upper bound is deliberately capped at
/// `2025-06-18`: the newer `2026-07-28` revision reshapes the transport
/// (stateless request/response + MCP Apps / MCP Tasks / OAuth) around
/// HTTP, and a stdio tools-only server gains nothing from claiming it.
/// See #535 for the audit that produced this list.
pub const SUPPORTED_PROTOCOL_VERSIONS: &[&str] = &["2024-11-05", "2025-03-26", "2025-06-18"];

/// The revision returned to a client that requests something we don't
/// recognise (typically: a future revision we haven't audited yet).
/// Kept as the newest entry in [`SUPPORTED_PROTOCOL_VERSIONS`].
pub const LATEST_SUPPORTED_PROTOCOL_VERSION: &str = "2025-06-18";

/// Choose the protocol revision to advertise on `initialize`.
///
/// Rules (per MCP spec):
///  * If the client asked for a version we support, echo it back.
///  * Otherwise (unknown revision, or the client sent nothing), return
///    our latest supported revision so the client can decide whether it
///    can downgrade.
pub fn negotiate_protocol_version(requested: Option<&str>) -> &'static str {
    if let Some(requested) = requested {
        for supported in SUPPORTED_PROTOCOL_VERSIONS {
            if *supported == requested {
                return supported;
            }
        }
    }
    LATEST_SUPPORTED_PROTOCOL_VERSION
}

/// Default nuclear data library — sourced from `hyrr.json` at build time (#269).
/// tendl-2023-iso has full ground/metastable isomeric splitting;
/// tendl-2025 dropped the g/m split entirely (#265).
pub const DEFAULT_LIBRARY: &str = env!("HYRR_DEFAULT_LIBRARY");

/// Run the MCP stdio server loop with the default library.
///
/// Convenience wrapper around [`run_mcp_server_with_library`]. Assumes a
/// user-supplied data dir (no healer); the MCP binary entry points pass
/// `spawn_heal: true` explicitly when they know the resolved path is the
/// managed cache.
pub fn run_mcp_server(data_dir: &str) {
    run_mcp_server_with_library(data_dir, DEFAULT_LIBRARY, false);
}

/// Run the MCP stdio server loop pinned to `library`.
///
/// `library` is the data-library identifier (e.g. `"tendl-2025"`,
/// `"endfb-8.1"`); it must correspond to a `<data_dir>/<library>/` tree.
/// The server's `library_used` echo footer reflects this value, and every
/// tool's data fetches happen against this library for the lifetime of
/// the process.
///
/// If `spawn_heal` is `true` and `data_dir` is the managed cache
/// (`~/.hyrr/nucl-parquet/v{DATA_VERSION}/data`), a background
/// [`crate::data_fetch::spawn_managed_cache_heal_thread`] fetches any
/// missing routed neutron / heavy-ion libraries (#709) WITHOUT blocking
/// this call. When the heal succeeds, this function transparently swaps
/// the in-memory data store for a fresh one (new fingerprint, empty
/// xs_cache — see PR #717 for why memoised misses would otherwise
/// outlive the fetch) and clears the simulate-result cache. Callers
/// pass `spawn_heal: false` for a user-supplied `--data-dir` /
/// `HYRR_DATA` / sibling checkout, which is out of scope for the healer.
pub fn run_mcp_server_with_library(data_dir: &str, library: &str, spawn_heal: bool) {
    // Kick off the opt-out, cached, non-blocking update check (#571)
    // BEFORE the pre-flight data-dir probe so the background thread has
    // the maximum head start against a fast-arriving `initialize` frame.
    // Fail-silent by construction: no stderr, no retries, no visible
    // effect on an air-gapped host beyond "no update notice".
    crate::update_check::spawn_background_check_if_stale(crate::update_check::SERVER_VERSION);

    // Pre-flight: verify the data directory actually contains a nucl-parquet
    // tree. ParquetDataStore::new only loads the eager metadata files; many
    // tools fault later when they reach for cross-sections / abundances /
    // decay data, which manifests as a mid-conversation panic from inside
    // an MCP call. Catch the missing-data case here with one actionable
    // line so the user can fix `HYRR_DATA` before Claude Code loses the
    // server connection.
    let meta_dir = std::path::Path::new(data_dir).join("meta");
    if !meta_dir.is_dir() {
        eprintln!(
            "hyrr-mcp: no nucl-parquet data found at {data_dir}\n\
             \n\
             Expected `{}` to exist. Set HYRR_DATA or pass --data-dir to point at a\n\
             nucl-parquet checkout, or clone\n\
             https://github.com/exoma-ch/nucl-parquet into ~/.hyrr/nucl-parquet.\n",
            meta_dir.display(),
        );
        std::process::exit(2);
    }

    let lib_dir = std::path::Path::new(data_dir).join(library);
    if !lib_dir.is_dir() {
        eprintln!(
            "hyrr-mcp: nuclear data library `{library}` not found in {data_dir}\n\
             \n\
             Expected `{}` to exist. Pick a different library with HYRR_LIBRARY\n\
             or --library, or run `nucl-parquet download {library}` to fetch it.\n",
            lib_dir.display(),
        );
        std::process::exit(2);
    }

    let initial_store = match crate::db::ParquetDataStore::new(data_dir, library) {
        Ok(db) => db,
        Err(e) => {
            eprintln!(
                "hyrr-mcp: failed to load nuclear data from {data_dir} (library {library}): {e}"
            );
            std::process::exit(1);
        }
    };

    // #709: hold the store behind `Arc<RwLock<Arc<…>>>`. Tool calls
    // read-lock and clone the inner Arc (cheap), so a background heal
    // thread can write-lock briefly to swap in a fresh
    // `ParquetDataStore` after new routed libraries land on disk.
    // Without the swap, `NpDataStore::ensure_xs` — which memoises
    // missing cross-sections as empty vectors on the first lookup —
    // would keep returning empty for `n` / heavy-ion queries even
    // after their libraries were extracted (PR #717 rev-1 review).
    let store: Arc<RwLock<Arc<crate::db::ParquetDataStore>>> =
        Arc::new(RwLock::new(Arc::new(initial_store)));

    // Spawn the routed-library heal thread AFTER the store is set up
    // and BEFORE the request loop starts — the handle is registered
    // globally so `tools::call_tool` can consult it on the first
    // request, and the thread does its I/O in the background so the
    // client's initialize timeout doesn't fire during a slow download.
    if spawn_heal {
        let store_for_swap = Arc::clone(&store);
        let data_dir_owned = data_dir.to_string();
        let library_owned = library.to_string();
        let _ = crate::data_fetch::spawn_managed_cache_heal_thread(move || {
            // Runs on the heal thread once the tarball is extracted and
            // the routed subtree(s) have been atomically renamed into
            // place. Rebuild the store — this re-derives the on-disk
            // fingerprint (which will include the new library dir once
            // #708 lands and is what makes the `mcp::cache` disk key
            // rotate naturally) and starts fresh xs caches — and clear
            // the in-memory `mcp::cache` LRU so a query cached against
            // the pre-heal store isn't served back.
            //
            // Returning `Err` here tells `spawn_managed_cache_heal_thread`
            // to keep the gate CLOSED and flip to `HealPhase::Failed`
            // instead of `Complete { added }` — a store rebuild failure
            // must not be misreported as "library available" (reviewer's
            // SHOULD-FIX #3).
            let new_store = crate::db::ParquetDataStore::new(&data_dir_owned, &library_owned)
                .map_err(|e| format!("{e}"))?;
            let mut writer = match store_for_swap.write() {
                Ok(w) => w,
                Err(poisoned) => poisoned.into_inner(),
            };
            *writer = Arc::new(new_store);
            drop(writer);
            crate::mcp::cache::clear_memory_cache();
            Ok(())
        });
    }

    let mut materials: MaterialRegistry = std::collections::HashMap::new();

    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut stdout = stdout.lock();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };

        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let request: JsonRpcRequest = match serde_json::from_str(line) {
            Ok(req) => req,
            Err(e) => {
                let resp = JsonRpcResponse::error(None, -32700, format!("Parse error: {}", e));
                let _ = writeln!(stdout, "{}", serde_json::to_string(&resp).unwrap());
                let _ = stdout.flush();
                continue;
            }
        };

        // Snapshot the store BEFORE the request runs. Read-locking
        // and cloning the inner `Arc` releases the lock immediately;
        // the request then holds the snapshot for its whole lifetime
        // so an in-flight call is never observed against a half-
        // swapped store. A heal-thread swap that lands mid-request
        // only affects the NEXT snapshot.
        let db_snap: Arc<crate::db::ParquetDataStore> = {
            let reader = match store.read() {
                Ok(r) => r,
                Err(poisoned) => poisoned.into_inner(),
            };
            Arc::clone(&reader)
        };

        // Notifications produce no response frame — see [`handle_request`].
        if let Some(response) = handle_request(&db_snap, &mut materials, request) {
            let _ = writeln!(stdout, "{}", serde_json::to_string(&response).unwrap());
            let _ = stdout.flush();
        }
    }
}

/// Build the version + update-awareness footer appended to `instructions`
/// at `initialize` (#571).
///
/// Three sources, all fail-silent:
///  * running version + compiled-in nucl-parquet `DATA_VERSION` — always
///    present.
///  * air-gapped staleness notice — CalVer-based, no network. Fires when
///    the pinned data is ≥ `DEFAULT_STALENESS_MONTHS` old. Safe on
///    air-gapped installs.
///  * optional network update notice — read from the on-disk cache
///    populated by the background thread in `run_mcp_server_with_library`.
///    Silently absent when the cache is empty / stale / disabled by env.
///
/// The rendering is intentionally one line per fact so an LLM client
/// relaying it back to the user in a chat doesn't have to reformat.
/// Prefixed with `\n\n---\n\n` so the split from the primary
/// instructions is visually obvious in Markdown-rendering clients.
pub fn build_version_footer() -> String {
    use crate::update_check;
    let mut out = String::new();
    out.push_str("\n\n---\n\n");
    out.push_str(&format!(
        "Running: `hyrr {}` (nuclear data `{}`).",
        SERVER_VERSION,
        crate::data_fetch::data_version(),
    ));
    if let Some(notice) =
        update_check::data_staleness_notice(update_check::DEFAULT_STALENESS_MONTHS)
    {
        out.push_str(&format!(
            " Warning: the compiled-in nuclear data (`{}`) is {} months old \
             (threshold: {} months) — consider upgrading with `uvx hyrr-mcp@latest`.",
            notice.data_version, notice.months_stale, notice.threshold_months,
        ));
    }
    if let Some(check) = update_check::read_cached_check() {
        if check.newer_available {
            out.push_str(&format!(
                " A newer release (`{}`) is available; upgrade with \
                 `uvx hyrr-mcp@latest` (recommended MCP client config: keep \
                 the package unpinned and add `--refresh` occasionally so \
                 uvx pulls new releases; pinning `hyrr-mcp=={}` freezes \
                 forever).",
                check.latest, check.current,
            ));
        }
    }
    out
}

/// Return `true` when the incoming frame is a JSON-RPC 2.0 *notification*
/// and therefore MUST NOT receive a response.
///
/// A notification is characterised by the absence of `id` (JSON-RPC 2.0
/// §4.1). MCP additionally namespaces its own notifications under the
/// `notifications/*` method prefix; a well-behaved client will always
/// send those without an `id`, but we defensively treat any frame in
/// that namespace as a notification so a malformed client can't trick
/// us into replying.
fn is_notification(request: &JsonRpcRequest) -> bool {
    request.id.is_none() || request.method.starts_with("notifications/")
}

/// Extract a human-readable message from a `catch_unwind` payload. Panics
/// most commonly carry a `&'static str` or a `String`; anything else lands
/// as a placeholder so the client at least learns that a panic happened.
fn panic_message(payload: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&'static str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "panic with non-string payload".to_string()
    }
}

/// Route a parsed JSON-RPC frame to its handler.
///
/// Returns `None` for JSON-RPC notifications (see [`is_notification`]);
/// the caller must write nothing to stdout in that case. All other
/// requests return `Some(response)` — either a successful result or a
/// structured JSON-RPC error.
fn handle_request(
    db: &crate::db::ParquetDataStore,
    materials: &mut MaterialRegistry,
    request: JsonRpcRequest,
) -> Option<JsonRpcResponse> {
    if is_notification(&request) {
        // Notifications are processed for side-effects only. Today we
        // hold no per-connection state that any `notifications/*`
        // frame would need to mutate (in particular `initialized` is a
        // no-op ack), so we simply drop the frame without responding.
        return None;
    }

    let id = request.id.clone();

    let response = match request.method.as_str() {
        "initialize" => {
            let requested = request
                .params
                .get("protocolVersion")
                .and_then(|v| v.as_str());
            let version = negotiate_protocol_version(requested);
            // MCP `instructions` (#528) — self-descriptive scope so a fresh
            // client can tell what HYRR does and, more importantly, what it
            // does NOT model (primary-only under a beam-stop = 0 for the
            // downstream layers, silently). The library id is injected so
            // the string matches the actually-loaded dataset.
            //
            // Append the running version + update-awareness footer (#571) so
            // an agent relays "you're on X, Y is available" to the user
            // without any per-tool-call nag. This is a strictly *reading*
            // path — the network check runs on a background thread spawned
            // in `run_mcp_server_with_library`; here we only read whatever
            // (if anything) has landed in the cache.
            let mut instructions = tools::server_instructions(db.library());
            instructions.push_str(&build_version_footer());
            let result = serde_json::json!({
                "protocolVersion": version,
                "capabilities": {
                    "tools": {}
                },
                "serverInfo": {
                    "name": SERVER_NAME,
                    "version": SERVER_VERSION
                },
                "instructions": instructions,
            });
            JsonRpcResponse::success(id, result)
        }

        // MCP utility method — an empty result is the spec-mandated ack.
        "ping" => JsonRpcResponse::success(id, serde_json::json!({})),

        "tools/list" => {
            // The live library id, so every outward referral in a description
            // names the data this server actually computed against (#681) —
            // same injection as `server_instructions` above.
            let tool_list = tools::list_tools(db.library());
            let result = serde_json::json!({
                "tools": tool_list
            });
            JsonRpcResponse::success(id, result)
        }

        "tools/call" => {
            let name = request
                .params
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let arguments = request
                .params
                .get("arguments")
                .cloned()
                .unwrap_or(Value::Object(serde_json::Map::new()));

            // Catch panics at the tool-dispatch boundary (#712 review, #355).
            // hyrr-core panicking mid-tool used to kill the stdio server: the
            // process exited, the client saw its pipe close, and nothing in
            // between was recoverable. Wrapping the call in `catch_unwind`
            // turns a panic into an `isError` result the client can render
            // and recover from — the same shape as a returned `Err(String)`,
            // just with a payload that names the panic.
            //
            // AssertUnwindSafe is sound in practice because every `tool_*`
            // panics BEFORE it mutates the shared `MaterialRegistry` (only
            // `tool_define_material` mutates it, and its parser errors out
            // long before the insert). The only truly-shared state visible
            // through `db` is the xs_cache Mutex, whose poison error the
            // db.rs recovery path handles (`unwrap_or_else(|e| e.into_inner())`)
            // rather than propagating.
            //
            // This is the belt-and-braces safety net. The primary defence
            // is `parse_layers` (typed rejection before compute) plus
            // `StoppingError::LayerUnresolvedThickness` (typed error from
            // compute); the catch_unwind exists so an as-yet-unaudited
            // panic path still becomes a Result-shaped response instead of
            // a dead server.
            //
            // For `hyrr-mcp` release builds, `hyrr-mcp/Cargo.toml`
            // explicitly pins `panic = "unwind"` — without that override,
            // the shipped binary would abort and the "server did not exit"
            // message would be a lie. WASM keeps `abort` (its own profile).
            let call =
                std::panic::AssertUnwindSafe(|| tools::call_tool(db, materials, name, &arguments));
            let result = std::panic::catch_unwind(call).unwrap_or_else(|panic_payload| {
                let msg = panic_message(&panic_payload);
                Err(format!(
                    "Internal error: `{name}` panicked ({msg}). This is a bug; \
                     please report it with the arguments that triggered it. \
                     The server did not exit."
                ))
            });
            match result {
                Ok(result) => {
                    // Text block first, then one embedded `resource` block per
                    // attached Parquet table (#427).
                    let mut content = vec![serde_json::json!({
                        "type": "text",
                        "text": result.text
                    })];
                    for res in &result.resources {
                        content.push(serde_json::json!({
                            "type": "resource",
                            "resource": {
                                "uri": res.uri,
                                "mimeType": res.mime_type,
                                "blob": res.blob_base64
                            }
                        }));
                    }
                    let response = serde_json::json!({ "content": content });
                    JsonRpcResponse::success(id, response)
                }
                Err(e) => {
                    let response = serde_json::json!({
                        "content": [{
                            "type": "text",
                            "text": format!("Error: {}", e)
                        }],
                        "isError": true
                    });
                    JsonRpcResponse::success(id, response)
                }
            }
        }

        _ => JsonRpcResponse::error(id, -32601, format!("Method not found: {}", request.method)),
    };

    Some(response)
}

#[cfg(test)]
mod tests {
    //! Protocol-level regression tests for #535. These exercise
    //! negotiation + notification-silence without touching the physics
    //! DB, so they run hermetically (no `nucl-parquet` data required).

    use super::*;

    fn make_request(id: Option<Value>, method: &str, params: Value) -> JsonRpcRequest {
        JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id,
            method: method.to_string(),
            params,
        }
    }

    // -- negotiation ---------------------------------------------------

    #[test]
    fn negotiate_echoes_supported_version() {
        for supported in SUPPORTED_PROTOCOL_VERSIONS {
            assert_eq!(
                negotiate_protocol_version(Some(supported)),
                *supported,
                "should echo supported version {supported} verbatim"
            );
        }
    }

    #[test]
    fn negotiate_returns_latest_for_unknown_version() {
        // A revision we've never heard of (e.g. a future spec bump).
        assert_eq!(
            negotiate_protocol_version(Some("2099-12-31")),
            LATEST_SUPPORTED_PROTOCOL_VERSION,
        );
    }

    #[test]
    fn negotiate_returns_latest_when_client_omits_version() {
        assert_eq!(
            negotiate_protocol_version(None),
            LATEST_SUPPORTED_PROTOCOL_VERSION,
        );
    }

    #[test]
    fn latest_is_included_in_supported_set() {
        assert!(
            SUPPORTED_PROTOCOL_VERSIONS.contains(&LATEST_SUPPORTED_PROTOCOL_VERSION),
            "LATEST_SUPPORTED must be one of SUPPORTED_PROTOCOL_VERSIONS",
        );
    }

    #[test]
    fn original_revision_still_supported_for_back_compat() {
        // Older clients pinned to 2024-11-05 must keep working.
        assert!(SUPPORTED_PROTOCOL_VERSIONS.contains(&"2024-11-05"));
    }

    // -- notification detection ---------------------------------------

    #[test]
    fn notification_has_no_id() {
        let req = make_request(None, "notifications/initialized", Value::Null);
        assert!(is_notification(&req));
    }

    #[test]
    fn notifications_namespace_is_always_a_notification() {
        // Even if a misbehaving client attaches an id, MCP `notifications/*`
        // must never be answered.
        let req = make_request(
            Some(Value::from(1u64)),
            "notifications/cancelled",
            Value::Null,
        );
        assert!(is_notification(&req));
    }

    #[test]
    fn normal_request_with_id_is_not_a_notification() {
        let req = make_request(Some(Value::from(1u64)), "tools/list", Value::Null);
        assert!(!is_notification(&req));
    }

    // -- initialize routing (no DB needed) ----------------------------
    //
    // handle_request needs a ParquetDataStore for tools/*, but the
    // `initialize` / `ping` / notification arms don't touch it — so we
    // just do the routing logic here by hand to match what
    // handle_request does for those arms.

    #[test]
    fn initialize_echoes_client_requested_supported_version() {
        let version = negotiate_protocol_version(Some("2024-11-05"));
        assert_eq!(version, "2024-11-05");
    }

    #[test]
    fn initialize_falls_back_to_latest_for_unknown_client_version() {
        let version = negotiate_protocol_version(Some("2030-01-01"));
        assert_eq!(version, LATEST_SUPPORTED_PROTOCOL_VERSION);
    }

    // -- version footer (#571) ----------------------------------------

    #[test]
    fn version_footer_always_names_running_version_and_data_version() {
        // The footer must ALWAYS carry the two identifiers, regardless
        // of network / cache / env state — this is the load-bearing
        // "who am I" fact an MCP agent relays to the user.
        let footer = build_version_footer();
        assert!(
            footer.contains(SERVER_VERSION),
            "footer must name SERVER_VERSION = {SERVER_VERSION}, got: {footer}"
        );
        assert!(
            footer.contains(crate::data_fetch::data_version()),
            "footer must name compiled-in DATA_VERSION = {}, got: {footer}",
            crate::data_fetch::data_version(),
        );
        // Separator so a Markdown-rendering client renders the split
        // between primary instructions and the version footer.
        assert!(
            footer.contains("---"),
            "footer must open with a section separator"
        );
    }

    #[test]
    fn version_footer_does_not_block_on_network() {
        // Load-bearing pitfall from #571: `initialize` MUST NOT block on
        // network I/O. The footer is the initialize-time render, so it
        // must complete promptly regardless of network state. We give
        // it a generous 500 ms ceiling — the fast path is µs-scale
        // (env read + optional cache file read).
        let t0 = std::time::Instant::now();
        let _ = build_version_footer();
        let elapsed = t0.elapsed();
        assert!(
            elapsed < std::time::Duration::from_millis(500),
            "build_version_footer took {elapsed:?}; must never block on network"
        );
    }
}
