//! JSON-RPC over stdin/stdout transport for MCP.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::{self, BufRead, Write};

use super::tools;

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
const PROTOCOL_VERSION: &str = "2024-11-05";

/// Default nuclear data library when none is specified.
pub const DEFAULT_LIBRARY: &str = "tendl-2025";

/// Run the MCP stdio server loop with the default library (`tendl-2025`).
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
            eprintln!("hyrr-mcp: failed to load nuclear data from {data_dir} (library {library}): {e}");
            std::process::exit(1);
        }
    };

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

        let response = handle_request(&db, request);
        let _ = writeln!(stdout, "{}", serde_json::to_string(&response).unwrap());
        let _ = stdout.flush();
    }
}

fn handle_request(
    db: &crate::db::ParquetDataStore,
    request: JsonRpcRequest,
) -> JsonRpcResponse {
    let id = request.id.clone();

    match request.method.as_str() {
        "initialize" => {
            let result = serde_json::json!({
                "protocolVersion": PROTOCOL_VERSION,
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

        "notifications/initialized" => {
            // No response needed for notifications
            JsonRpcResponse::success(id, Value::Null)
        }

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

            match tools::call_tool(db, name, &arguments) {
                Ok(result) => {
                    let response = serde_json::json!({
                        "content": [{
                            "type": "text",
                            "text": result
                        }]
                    });
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
    }
}
