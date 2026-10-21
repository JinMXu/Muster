//! Minimal MCP (Model Context Protocol) stdio server: `muster mcp`.
//!
//! MCP clients (Claude Code, Cursor, ...) spawn this process and talk
//! newline-delimited JSON-RPC over stdin/stdout. Each tool maps 1:1 onto an
//! existing IPC verb, so the tool layer stays a thin adapter: discovery,
//! session creation, typing, capture and waiting all behave exactly like the
//! `muster` CLI. The GUI is started lazily on the first tool call that needs
//! the bridge.

use std::io::{BufRead, Write};

use serde_json::{json, Value};

/// MCP revisions we understand. The client's requested revision is echoed
/// back when supported; otherwise we answer with the newest one we know.
const PROTOCOL_VERSIONS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];
const DEFAULT_PROTOCOL: &str = "2025-06-18";
/// Generous ceiling for one request line; `send` of a large script is the
/// realistic maximum.
const MAX_MESSAGE_BYTES: usize = 1024 * 1024;

pub fn run() -> i32 {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    serve(
        stdin.lock(),
        stdout.lock(),
        crate::services::ipc::client::call,
    )
}

/// Read one JSON-RPC message per line until EOF. Exposed for tests with
/// in-memory reader/writer and an injected tool caller.
fn serve<R: BufRead, W: Write>(
    mut reader: R,
    mut out: W,
    call: impl Fn(&str, Value) -> Result<Value, String>,
) -> i32 {
    let mut line = String::new();
    loop {
        line.clear();
        let n = match reader.read_line(&mut line) {
            Ok(n) => n,
            Err(_) => return 1,
        };
        if n == 0 {
            return 0;
        }
        let message = line.trim();
        if message.is_empty() {
            continue;
        }
        if message.len() > MAX_MESSAGE_BYTES {
            if write_line(&mut out, &rpc_error(Value::Null, -32600, "message too large")).is_err() {
                return 1;
            }
            continue;
        }
        let msg: Value = match serde_json::from_str(message) {
            Ok(v) => v,
            Err(e) => {
                let resp = rpc_error(Value::Null, -32700, &format!("parse error: {e}"));
                if write_line(&mut out, &resp).is_err() {
                    return 1;
                }
                continue;
            }
        };
        if let Some(resp) = handle_message(&msg, &call) {
            if write_line(&mut out, &resp).is_err() {
                return 1;
            }
        }
    }
}

fn write_line<W: Write>(out: &mut W, value: &Value) -> std::io::Result<()> {
    writeln!(out, "{value}")?;
    out.flush()
}

fn handle_message(
    msg: &Value,
    call: &impl Fn(&str, Value) -> Result<Value, String>,
) -> Option<Value> {
    let Some(method) = msg.get("method").and_then(|v| v.as_str()) else {
        // A request (has an id) without a method is malformed; a message
        // without either is a response we never asked for — ignore it.
        return msg.get("id").map(|id| {
            rpc_error(id.clone(), -32600, "invalid request")
        });
    };
    let id = msg.get("id").cloned();
    // Notifications (no id) never get a response.
    if id.is_none() {
        return None;
    }
    let id = id.unwrap_or(Value::Null);

    match method {
        "initialize" => Some(json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": initialize_result(msg.get("params")),
        })),
        "ping" => Some(json!({ "jsonrpc": "2.0", "id": id, "result": {} })),
        "tools/list" => Some(json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": { "tools": tools() },
        })),
        "tools/call" => Some(tools_call(id, msg.get("params"), call)),
        other => Some(rpc_error(id, -32601, &format!("method not found: {other}"))),
    }
}

fn initialize_result(params: Option<&Value>) -> Value {
    let requested = params
        .and_then(|p| p.get("protocolVersion"))
        .and_then(|v| v.as_str());
    let version = requested
        .filter(|v| PROTOCOL_VERSIONS.contains(v))
        .unwrap_or(DEFAULT_PROTOCOL);
    json!({
        "protocolVersion": version,
        "capabilities": { "tools": { "listChanged": false } },
        "serverInfo": { "name": "muster", "version": env!("CARGO_PKG_VERSION") },
        "instructions": "Muster is a Windows terminal workspace. Use muster_ls to discover windows, projects, tabs and terminal session ids; muster_new or muster_split to create a session; muster_send and muster_send_keys to type into one; muster_capture to read its output; muster_procs for its process tree; muster_wait to block until a coding agent reaches a state. Muster starts automatically on the first tool call if it is not running."
    })
}

fn tools_call(
    id: Value,
    params: Option<&Value>,
    call: &impl Fn(&str, Value) -> Result<Value, String>,
) -> Value {
    let name = params
        .and_then(|p| p.get("name"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let args = params
        .and_then(|p| p.get("arguments"))
        .cloned()
        .unwrap_or_else(|| json!({}));
    let Some(verb) = tool_verb(name) else {
        return tool_error(id, format!("unknown tool '{name}'"));
    };
    match call(verb, args) {
        Ok(data) => {
            let text =
                serde_json::to_string_pretty(&data).unwrap_or_else(|_| data.to_string());
            json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "content": [{ "type": "text", "text": text }],
                    "isError": false,
                },
            })
        }
        Err(e) => tool_error(id, e),
    }
}

fn tool_error(id: Value, message: String) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "content": [{ "type": "text", "text": message }],
            "isError": true,
        },
    })
}

fn rpc_error(id: Value, code: i64, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message },
    })
}

fn tool_verb(name: &str) -> Option<&'static str> {
    Some(match name {
        "muster_doctor" => "doctor",
        "muster_ls" => "ls",
        "muster_agents" => "agents",
        "muster_new" => "new",
        "muster_split" => "split",
        "muster_send" => "send",
        "muster_send_keys" => "send-keys",
        "muster_capture" => "capture",
        "muster_procs" => "procs",
        "muster_run" => "run",
        "muster_wait" => "wait",
        "muster_usage" => "usage",
        _ => return None,
    })
}

fn tools() -> Vec<Value> {
    let empty = json!({ "type": "object", "properties": {}, "additionalProperties": false });
    vec![
        json!({
            "name": "muster_ls",
            "description": "List every open Muster window, project, tab and pane, including pane kinds, working directories and terminal session ids. Start here before driving a session.",
            "inputSchema": empty,
        }),
        json!({
            "name": "muster_agents",
            "description": "List coding agents detected in Muster terminal sessions, with state (working|waiting|done) and attention (approval when the agent is blocked on a permission prompt).",
            "inputSchema": empty,
        }),
        json!({
            "name": "muster_doctor",
            "description": "Report Muster's bridge health: version, open windows, live sessions and tracked agents.",
            "inputSchema": empty,
        }),
        json!({
            "name": "muster_new",
            "description": "Open a directory as a new Muster project (starting Muster if needed) and return the first terminal session id.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "directory": { "type": "string", "description": "Absolute directory to open. Omit to use Muster's default." }
                },
                "additionalProperties": false
            },
        }),
        json!({
            "name": "muster_split",
            "description": "Split the focused pane in the target window and return the new terminal session id.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "directory": { "type": "string", "description": "Working directory for the new pane. Omit to inherit." },
                    "vertical": { "type": "boolean", "description": "false splits right, true splits down. Default false." }
                },
                "additionalProperties": false
            },
        }),
        json!({
            "name": "muster_send",
            "description": "Type text into a terminal session. Set enter=true to submit it like pressing Enter. Use muster_send_keys for shortcuts.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "Terminal session id from muster_ls, muster_new or muster_split." },
                    "text": { "type": "string", "description": "Text to send verbatim." },
                    "enter": { "type": "boolean", "description": "Append Enter after the text. Default false." }
                },
                "required": ["id", "text"],
                "additionalProperties": false
            },
        }),
        json!({
            "name": "muster_send_keys",
            "description": "Send key combinations to a session, e.g. \"ctrl+c\", \"enter\", or an array such as [\"ctrl+c\", \"enter\"].",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "Terminal session id." },
                    "keys": {
                        "description": "One combo or a list of combos.",
                        "anyOf": [
                            { "type": "string" },
                            { "type": "array", "items": { "type": "string" } }
                        ]
                    }
                },
                "required": ["id", "keys"],
                "additionalProperties": false
            },
        }),
        json!({
            "name": "muster_capture",
            "description": "Read recent ANSI-stripped output lines from a terminal session. This is a snapshot; call again for newer output.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "Terminal session id." },
                    "lines": { "type": "integer", "minimum": 1, "maximum": 400, "description": "How many lines to return (default 120)." }
                },
                "required": ["id"],
                "additionalProperties": false
            },
        }),
        json!({
            "name": "muster_procs",
            "description": "Show a terminal session's process tree and its listening TCP ports.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "Terminal session id." }
                },
                "required": ["id"],
                "additionalProperties": false
            },
        }),
        json!({
            "name": "muster_run",
            "description": "Run a shell command in a new Muster tab and wait for it to finish. Returns captured output, exit code and timeout status. Default timeout 600s.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "command": { "type": "string", "description": "Command line to run." },
                    "directory": { "type": "string", "description": "Working directory. Omit to inherit." },
                    "timeout_secs": { "type": "integer", "minimum": 1, "maximum": 3600, "description": "Default 600." }
                },
                "required": ["command"],
                "additionalProperties": false
            },
        }),
        json!({
            "name": "muster_wait",
            "description": "Block until a session's coding agent reaches a state: working, waiting (includes approval prompts) or done (default). A closed session or disappeared agent counts as done. Default timeout 600s.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "Terminal session id." },
                    "until": { "type": "string", "enum": ["working", "waiting", "done"], "description": "Target state; default done." },
                    "timeout_secs": { "type": "integer", "minimum": 1, "maximum": 86400, "description": "Default 600." }
                },
                "required": ["id"],
                "additionalProperties": false
            },
        }),
        json!({
            "name": "muster_usage",
            "description": "Token usage comparison by agent and model: runs, tokens per run, fresh input, cache hit rate, tokens per turn and average duration. Use view=runs or coverage for history and adapter availability.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "view": { "type": "string", "enum": ["compare", "runs", "coverage", "overview"], "description": "Default compare." },
                    "range": { "type": "string", "enum": ["today", "7d", "30d", "all"], "description": "Default 7d. CLI 'today' means the last 24h." },
                    "project_id": { "type": "string", "description": "Optional Muster project id." },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 1000, "description": "Max runs (view=runs)." }
                },
                "additionalProperties": false
            },
        }),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::io::Cursor;

    fn run_serve(input: &str) -> (Vec<Value>, Vec<(String, Value)>) {
        let calls = RefCell::new(Vec::new());
        let call = |verb: &str, args: Value| {
            calls
                .borrow_mut()
                .push((verb.to_string(), args.clone()));
            Ok(json!({ "verb": verb, "args": args }))
        };
        let mut out = Vec::new();
        let code = serve(Cursor::new(input), &mut out, call);
        assert_eq!(code, 0);
        let responses = String::from_utf8(out)
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        (responses, calls.into_inner())
    }

    #[test]
    fn initialize_negotiates_and_lists_tools() {
        let input = concat!(
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1"}}}"#,
            "\n",
            r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
            "\n",
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
            "\n",
        );
        let (responses, calls) = run_serve(input);
        assert_eq!(responses.len(), 2, "notification must not get a response");
        assert_eq!(responses[0]["result"]["protocolVersion"], "2024-11-05");
        assert_eq!(responses[0]["result"]["serverInfo"]["name"], "muster");
        let tools = responses[1]["result"]["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 12);
        assert!(tools
            .iter()
            .any(|t| t["name"] == "muster_send" && t["inputSchema"]["required"][0] == "id"));
        assert!(calls.is_empty());
    }

    #[test]
    fn tool_calls_map_to_ipc_verbs() {
        let input = concat!(
            r#"{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"muster_capture","arguments":{"id":"abc","lines":50}}}"#,
            "\n",
            r#"{"jsonrpc":"2.0","id":8,"method":"tools/call","params":{"name":"muster_send_keys","arguments":{"id":"abc","keys":["ctrl+c","enter"]}}}"#,
            "\n",
            r#"{"jsonrpc":"2.0","id":9,"method":"tools/call","params":{"name":"muster_usage","arguments":{"view":"runs","range":"7d","limit":20}}}"#,
            "\n",
        );
        let (responses, calls) = run_serve(input);
        assert_eq!(calls[0].0, "capture");
        assert_eq!(calls[0].1["lines"], 50);
        assert_eq!(calls[1].0, "send-keys");
        assert_eq!(calls[1].1["keys"][0], "ctrl+c");
        assert_eq!(calls[2].0, "usage");
        assert_eq!(calls[2].1["view"], "runs");
        assert_eq!(calls[2].1["limit"], 20);
        assert_eq!(responses[0]["result"]["isError"], false);
        assert!(responses[0]["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("capture"));
    }

    #[test]
    fn unknown_methods_and_tools_are_reported() {
        let input = concat!(
            r#"{"jsonrpc":"2.0","id":1,"method":"nope"}"#,
            "\n",
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"nope","arguments":{}}}"#,
            "\n",
            "not json\n",
        );
        let (responses, calls) = run_serve(input);
        assert!(calls.is_empty());
        assert_eq!(responses[0]["error"]["code"], -32601);
        assert_eq!(responses[1]["result"]["isError"], true);
        assert_eq!(responses[2]["error"]["code"], -32700);
    }

    #[test]
    fn tool_failures_become_error_results() {
        let call = |_verb: &str, _args: Value| Err("bridge offline".to_string());
        let mut out = Vec::new();
        let input = concat!(
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"muster_ls","arguments":{}}}"#,
            "\n",
        );
        assert_eq!(serve(Cursor::new(input), &mut out, call), 0);
        let resp: Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(resp["result"]["isError"], true);
        assert_eq!(resp["result"]["content"][0]["text"], "bridge offline");
    }

    #[test]
    fn request_without_method_is_invalid() {
        let (responses, _) = run_serve("{\"jsonrpc\":\"2.0\",\"id\":9}\n");
        assert_eq!(responses[0]["error"]["code"], -32600);
    }
}
