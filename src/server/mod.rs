pub mod tools;

use anyhow::Result;
use serde_json::{json, Value};
use std::io::{BufRead, Write};

use crate::store::graph::GraphStore;

/// Maximum request line size (1 MB) to prevent memory exhaustion
const MAX_REQUEST_SIZE: usize = 1_024 * 1_024;

pub fn run_mcp_server(store: GraphStore) -> Result<()> {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut out = stdout.lock();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(e) => {
                eprintln!("[cartograph] stdin read error: {e}");
                break;
            }
        };

        let line = line.trim().to_string();
        if line.is_empty() {
            continue;
        }

        if line.len() > MAX_REQUEST_SIZE {
            eprintln!("[cartograph] request too large ({} bytes), skipping", line.len());
            let error_response = json!({
                "jsonrpc": "2.0",
                "id": null,
                "error": {
                    "code": -32600,
                    "message": "Request too large"
                }
            });
            writeln!(out, "{}", serde_json::to_string(&error_response)?)?;
            out.flush()?;
            continue;
        }

        let request: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("[cartograph] JSON parse error: {e}");
                let error_response = json!({
                    "jsonrpc": "2.0",
                    "id": null,
                    "error": {
                        "code": -32700,
                        "message": "Invalid JSON in request"
                    }
                });
                writeln!(out, "{}", serde_json::to_string(&error_response)?)?;
                out.flush()?;
                continue;
            }
        };

        let id = request.get("id").cloned().unwrap_or(Value::Null);
        let method = request
            .get("method")
            .and_then(|m| m.as_str())
            .unwrap_or("");

        eprintln!("[cartograph] method={method} id={id}");

        // `initialized` is a notification — no response needed
        if method == "initialized" {
            continue;
        }

        let response = dispatch(&store, method, &id, &request);

        writeln!(out, "{}", serde_json::to_string(&response)?)?;
        out.flush()?;
    }

    Ok(())
}

fn dispatch(store: &GraphStore, method: &str, id: &Value, request: &Value) -> Value {
    match method {
        "initialize" => json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "protocolVersion": "2024-11-05",
                "capabilities": {
                    "tools": {}
                },
                "serverInfo": {
                    "name": "cartograph",
                    "version": "0.1.0"
                }
            }
        }),

        "tools/list" => {
            let tool_list: Vec<Value> = tools::tool_definitions()
                .into_iter()
                .map(|t| {
                    json!({
                        "name": t.name,
                        "description": t.description,
                        "inputSchema": t.input_schema
                    })
                })
                .collect();

            json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "tools": tool_list
                }
            })
        }

        "tools/call" => {
            let params = request.get("params").cloned().unwrap_or(json!({}));
            let tool_name = params
                .get("name")
                .and_then(|n| n.as_str())
                .unwrap_or("");
            let arguments = params
                .get("arguments")
                .cloned()
                .unwrap_or(json!({}));

            eprintln!("[cartograph] tools/call tool={tool_name}");

            match tools::execute_tool(store, tool_name, &arguments) {
                Ok(text) => json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "content": [
                            {
                                "type": "text",
                                "text": text
                            }
                        ]
                    }
                }),
                Err(e) => {
                    eprintln!("[cartograph] tool execution error: {e}");
                    json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "error": {
                            "code": -32603,
                            "message": "Tool execution error"
                        }
                    })
                }
            }
        }

        other => {
            eprintln!("[cartograph] unknown method: {other}");
            json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": {
                    "code": -32601,
                    "message": "Method not found",
                    "data": format!("Unknown method: {other}")
                }
            })
        }
    }
}
