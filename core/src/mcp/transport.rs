//! JSON-RPC over stdin/stdout transport for MCP.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::{self, BufRead, Write};

use super::tools;
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
const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The most recent MCP protocol revision this server advertises. Returned from
/// `initialize` when the client requests a version we don't support (or none).
///
/// Deliberately *not* the `2026-07-28` revision: that spec's stateless
/// request/response redesign, MCP Apps/Tasks, and OAuth2/OIDC target
/// serverless/HTTP transports and buy a stdio tools-only server nothing (#535).
const LATEST_SUPPORTED_PROTOCOL: &str = "2025-06-18";

/// Protocol revisions this hand-rolled transport is faithful to (tools surface:
/// request/response/error shapes, `content` blocks, `isError`, error codes).
/// `2024-11-05` is retained for back-compat with older clients. Ordered newest
/// first; `LATEST_SUPPORTED_PROTOCOL` must appear here.
const SUPPORTED_PROTOCOL_VERSIONS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];

/// Negotiate the protocol version to advertise in `initialize`.
///
/// Per the MCP spec the server echoes the client's requested version when it
/// supports it, otherwise it returns its own latest supported revision so the
/// client can decide whether to proceed or disconnect.
fn negotiate_protocol_version(requested: Option<&str>) -> &'static str {
    requested
        .and_then(|req| SUPPORTED_PROTOCOL_VERSIONS.iter().find(|&&v| v == req))
        .copied()
        .unwrap_or(LATEST_SUPPORTED_PROTOCOL)
}

/// A JSON-RPC 2.0 notification carries no `id` and MUST receive no response.
/// MCP notifications additionally use the `notifications/*` method namespace;
/// either signal marks a message we must handle silently.
fn is_notification(id: &Option<Value>, method: &str) -> bool {
    id.is_none() || method.starts_with("notifications/")
}

/// Default nuclear data library — sourced from `hyrr.json` at build time (#269).
/// tendl-2023-iso has full ground/metastable isomeric splitting;
/// tendl-2025 dropped the g/m split entirely (#265).
pub const DEFAULT_LIBRARY: &str = env!("HYRR_DEFAULT_LIBRARY");

/// Run the MCP stdio server loop with the default library.
///
/// Convenience wrapper around [`run_mcp_server_with_library`].
pub fn run_mcp_server(data_dir: &str) {
    run_mcp_server_with_library(data_dir, DEFAULT_LIBRARY);
}

/// Run the MCP stdio server loop pinned to `library`.
///
/// `library` is the data-library identifier (e.g. `"tendl-2025"`,
/// `"endfb-8.1"`); it must correspond to a `<data_dir>/<library>/` tree.
/// The server's `library_used` echo footer reflects this value, and every
/// tool's data fetches happen against this library for the lifetime of
/// the process.
pub fn run_mcp_server_with_library(data_dir: &str, library: &str) {
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

    let db = match crate::db::ParquetDataStore::new(data_dir, library) {
        Ok(db) => db,
        Err(e) => {
            eprintln!(
                "hyrr-mcp: failed to load nuclear data from {data_dir} (library {library}): {e}"
            );
            std::process::exit(1);
        }
    };

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

        // Notifications (and any other id-less request) get no response line;
        // `handle_request` returns `None` for those.
        if let Some(response) = handle_request(&db, &mut materials, request) {
            let _ = writeln!(stdout, "{}", serde_json::to_string(&response).unwrap());
            let _ = stdout.flush();
        }
    }
}

/// Build the `initialize` response, negotiating the protocol version against
/// the client's `params.protocolVersion` request.
fn handle_initialize(id: Option<Value>, params: &Value) -> JsonRpcResponse {
    let requested = params.get("protocolVersion").and_then(Value::as_str);
    let result = serde_json::json!({
        "protocolVersion": negotiate_protocol_version(requested),
        "capabilities": {
            "tools": {}
        },
        "serverInfo": {
            "name": SERVER_NAME,
            "version": SERVER_VERSION
        }
    });
    JsonRpcResponse::success(id, result)
}

/// Dispatch the data-store-independent request methods: `initialize` and the
/// `ping` utility method. Returns `None` when `method` is neither, so the
/// caller can fall through to the data-backed tool methods.
fn handle_meta_request(id: Option<Value>, method: &str, params: &Value) -> Option<JsonRpcResponse> {
    match method {
        "initialize" => Some(handle_initialize(id, params)),
        // `ping` is a spec utility method: an empty result confirms liveness.
        "ping" => Some(JsonRpcResponse::success(id, serde_json::json!({}))),
        _ => None,
    }
}

/// Route a single request. Returns `None` when nothing should be written back —
/// i.e. the message is a JSON-RPC notification (see [`is_notification`]).
fn handle_request(
    db: &crate::db::ParquetDataStore,
    materials: &mut MaterialRegistry,
    request: JsonRpcRequest,
) -> Option<JsonRpcResponse> {
    // Notifications (`notifications/initialized`, `notifications/cancelled`, …)
    // and any id-less request MUST get no response. Do the work (none needed
    // for the notifications we receive) and write nothing.
    if is_notification(&request.id, &request.method) {
        return None;
    }

    let id = request.id.clone();

    if let Some(response) = handle_meta_request(id.clone(), &request.method, &request.params) {
        return Some(response);
    }

    let response = match request.method.as_str() {
        "tools/list" => {
            let tool_list = tools::list_tools();
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

            match tools::call_tool(db, materials, name, &arguments) {
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
    use super::*;
    use serde_json::json;

    #[test]
    fn negotiate_echoes_supported_client_version() {
        // A client on the original revision keeps it (back-compat).
        assert_eq!(negotiate_protocol_version(Some("2024-11-05")), "2024-11-05");
        // An intermediate supported revision is echoed too.
        assert_eq!(negotiate_protocol_version(Some("2025-03-26")), "2025-03-26");
        assert_eq!(
            negotiate_protocol_version(Some("2025-06-18")),
            LATEST_SUPPORTED_PROTOCOL
        );
    }

    #[test]
    fn negotiate_falls_back_to_latest_for_unknown_or_missing() {
        // A newer/unknown revision we don't support → our latest.
        assert_eq!(
            negotiate_protocol_version(Some("2026-07-28")),
            LATEST_SUPPORTED_PROTOCOL
        );
        assert_eq!(
            negotiate_protocol_version(Some("bogus")),
            LATEST_SUPPORTED_PROTOCOL
        );
        // No requested version (missing field) → our latest.
        assert_eq!(negotiate_protocol_version(None), LATEST_SUPPORTED_PROTOCOL);
    }

    #[test]
    fn latest_supported_is_in_the_supported_set() {
        assert!(SUPPORTED_PROTOCOL_VERSIONS.contains(&LATEST_SUPPORTED_PROTOCOL));
    }

    #[test]
    fn initialize_echoes_supported_version() {
        let resp = handle_initialize(Some(json!(1)), &json!({ "protocolVersion": "2024-11-05" }));
        let result = resp.result.expect("initialize returns a result");
        assert_eq!(result["protocolVersion"], "2024-11-05");
        assert_eq!(result["serverInfo"]["name"], SERVER_NAME);
        assert!(result["capabilities"]["tools"].is_object());
    }

    #[test]
    fn initialize_falls_back_for_unknown_version() {
        let resp = handle_initialize(Some(json!(1)), &json!({ "protocolVersion": "2026-07-28" }));
        let result = resp.result.expect("initialize returns a result");
        assert_eq!(result["protocolVersion"], LATEST_SUPPORTED_PROTOCOL);
    }

    #[test]
    fn notifications_are_silent() {
        // A JSON-RPC notification: no id, `notifications/*` method.
        assert!(is_notification(&None, "notifications/initialized"));
        // `notifications/*` with an (out-of-spec) id is still a notification.
        assert!(is_notification(&Some(json!(1)), "notifications/cancelled"));
        // Any id-less request is treated as a notification.
        assert!(is_notification(&None, "initialize"));
        // A normal request with an id is not.
        assert!(!is_notification(&Some(json!(1)), "initialize"));
        assert!(!is_notification(&Some(json!(1)), "tools/list"));
    }

    #[test]
    fn ping_returns_empty_result() {
        let resp =
            handle_meta_request(Some(json!(7)), "ping", &json!({})).expect("ping is a meta method");
        assert_eq!(resp.id, Some(json!(7)));
        assert!(resp.error.is_none(), "ping must not be an error");
        assert_eq!(
            resp.result,
            Some(json!({})),
            "ping result is an empty object"
        );
    }

    #[test]
    fn meta_request_declines_non_meta_methods() {
        // tools/* are data-backed and must fall through to the db dispatch.
        assert!(handle_meta_request(Some(json!(1)), "tools/list", &json!({})).is_none());
        assert!(handle_meta_request(Some(json!(1)), "tools/call", &json!({})).is_none());
    }
}
