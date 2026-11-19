//! Minimal MCP (Model Context Protocol) server over stdio for `rabbiz-*` apps.
//!
//! Newline-delimited JSON-RPC 2.0 (the MCP stdio transport). Apps register a
//! static [`Tool`] table and call [`serve`]; the helper owns `initialize`,
//! `ping`, `tools/list`, `tools/call` and notification swallowing.
//!
//! Intended use in `main()` before `run_app`:
//! ```ignore
//! if std::env::args().any(|a| a == "--mcp") {
//!     rabbiz_app_kit::mcp::serve(&TOOLS, "rabbiz-notes", "0.1.0");
//! }
//! ```

use std::io::{BufRead, Write};

/// One MCP tool: name, human description, JSON-Schema `inputSchema` (as a
/// JSON literal) and a handler receiving `params.arguments`.
pub struct Tool {
    pub name: &'static str,
    pub description: &'static str,
    /// `inputSchema` object serialized as JSON, e.g. `r#"{"type":"object"}"#`.
    pub input_schema: &'static str,
    pub handler: fn(&serde_json::Value) -> Result<String, String>,
}

fn result(id: &serde_json::Value, value: serde_json::Value) -> serde_json::Value {
    serde_json::json!({ "jsonrpc": "2.0", "id": id, "result": value })
}

fn error(id: &serde_json::Value, code: i64, message: &str) -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message },
    })
}

fn tool_text(text: String, is_error: bool) -> serde_json::Value {
    serde_json::json!({
        "content": [{ "type": "text", "text": text }],
        "isError": is_error,
    })
}

/// Serve MCP on stdin/stdout until EOF. Never returns on success path usage —
/// exits the process when stdin closes.
pub fn serve(tools: &'static [Tool], server_name: &str, server_version: &str) -> ! {
    let stdin = std::io::stdin();
    let mut out = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(msg) = serde_json::from_str::<serde_json::Value>(line) else {
            let _ = writeln!(
                out,
                "{}",
                error(&serde_json::Value::Null, -32700, "parse error")
            );
            let _ = out.flush();
            continue;
        };
        let id = msg.get("id").cloned().unwrap_or(serde_json::Value::Null);
        let method = msg.get("method").and_then(|m| m.as_str()).unwrap_or("");
        // Notifications (no "id") never get a response.
        let is_notification = msg.get("id").is_none();

        let response = match method {
            "initialize" => Some(result(
                &id,
                serde_json::json!({
                    "protocolVersion": msg
                        .pointer("/params/protocolVersion")
                        .cloned()
                        .unwrap_or_else(|| serde_json::json!("2025-03-26")),
                    "capabilities": { "tools": {} },
                    "serverInfo": { "name": server_name, "version": server_version },
                }),
            )),
            "ping" => Some(result(&id, serde_json::json!({}))),
            "tools/list" => Some(result(
                &id,
                serde_json::json!({
                    "tools": tools.iter().map(|t| serde_json::json!({
                        "name": t.name,
                        "description": t.description,
                        "inputSchema": serde_json::from_str::<serde_json::Value>(t.input_schema)
                            .unwrap_or_else(|_| serde_json::json!({ "type": "object" })),
                    })).collect::<Vec<_>>(),
                }),
            )),
            "tools/call" => {
                let name = msg
                    .pointer("/params/name")
                    .and_then(|n| n.as_str())
                    .unwrap_or("");
                let args = msg
                    .pointer("/params/arguments")
                    .cloned()
                    .unwrap_or_else(|| serde_json::json!({}));
                match tools.iter().find(|t| t.name == name) {
                    Some(t) => match (t.handler)(&args) {
                        Ok(text) => Some(result(&id, tool_text(text, false))),
                        Err(e) => Some(result(&id, tool_text(e, true))),
                    },
                    None => Some(error(&id, -32602, "unknown tool")),
                }
            }
            _ if is_notification => None,
            _ => Some(error(&id, -32601, "method not found")),
        };
        if let Some(resp) = response {
            let _ = writeln!(out, "{resp}");
            let _ = out.flush();
        }
    }
    std::process::exit(0);
}

#[cfg(test)]
mod tests {
    use super::*;

    static TOOLS: &[Tool] = &[Tool {
        name: "echo",
        description: "echo args",
        input_schema: r#"{"type":"object"}"#,
        handler: |args| Ok(args.to_string()),
    }];

    #[test]
    fn tool_text_shape() {
        let v = tool_text("hi".into(), false);
        assert_eq!(v["content"][0]["type"], "text");
        assert_eq!(v["content"][0]["text"], "hi");
        assert_eq!(v["isError"], false);
    }

    #[test]
    fn schema_parses() {
        for t in TOOLS {
            assert!(serde_json::from_str::<serde_json::Value>(t.input_schema).is_ok());
        }
    }
}
