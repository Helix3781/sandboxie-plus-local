//! sbie-mcp - local MCP (Model Context Protocol) server for Sandboxie-Plus.
//!
//! Transport: newline-delimited JSON-RPC 2.0 over stdio. No sockets, no network.
use sbiebox::{boot, box_status, create_box, delete_box, delete_snapshot, import_program, list_boxes, list_snapshots, restore, run_program, take_snapshot, terminate, Env};
use serde_json::{json, Value};
use std::io::{BufRead, Write};

const PROTOCOL_VERSION: &str = "2024-11-05";

fn main() {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        if line.trim().is_empty() {
            continue;
        }
        let request: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(err) => {
                let resp = error_response(Value::Null, -32700, &format!("parse error: {}", err));
                write_message(&mut stdout, &resp);
                continue;
            }
        };
        let id = request.get("id").cloned().unwrap_or(Value::Null);
        let method = request.get("method").and_then(|m| m.as_str()).unwrap_or("");
        let params = request.get("params").cloned().unwrap_or(Value::Null);
        let is_notification = request.get("id").is_none();

        let result = match method {
            "initialize" => Ok(json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": { "tools": { "listChanged": false } },
                "serverInfo": { "name": "sbie-mcp", "version": env!("CARGO_PKG_VERSION") },
                "instructions": "Local Sandboxie-Plus box manager. All operations run on this machine; nothing is sent anywhere."
            })),
            "notifications/initialized" | "notifications/cancelled" => continue,
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({ "tools": tool_definitions() })),
            "tools/call" => call_tool(&params),
            other => Err((-32601, format!("unknown method '{}'", other))),
        };

        if is_notification {
            continue;
        }
        let resp = match result {
            Ok(value) => json!({"jsonrpc": "2.0", "id": id, "result": value}),
            Err((code, message)) => error_response(id, code, &message),
        };
        write_message(&mut stdout, &resp);
    }
}

fn write_message(out: &mut std::io::Stdout, value: &Value) {
    let _ = writeln!(out, "{}", serde_json::to_string(value).unwrap_or_default());
    let _ = out.flush();
}

fn error_response(id: Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

fn tool_definitions() -> Vec<Value> {
    let box_arg = json!({
        "type": "object",
        "properties": { "box": { "type": "string", "description": "sandbox name" } },
        "required": ["box"]
    });
    vec![
        json!({
            "name": "sbie_list_boxes",
            "description": "List local sandboxes with their root folder and current snapshot.",
            "inputSchema": { "type": "object", "properties": {} }
        }),
        json!({
            "name": "sbie_box_status",
            "description": "Report whether a sandbox is running, its pids, current snapshot and box type.",
            "inputSchema": box_arg
        }),
        json!({
            "name": "sbie_list_snapshots",
            "description": "List the snapshots (layers) of a sandbox.",
            "inputSchema": box_arg
        }),
        json!({
            "name": "sbie_restore_snapshot",
            "description": "Materialize a snapshot onto the sandbox folder (stops the box first). Fixes the case where a restored box cannot start because parent layers were never copied.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "box": { "type": "string" },
                    "snapshot": { "type": "string", "description": "snapshot id" },
                    "waitMs": { "type": "integer", "description": "how long to wait for the box to go idle", "default": 30000 }
                },
                "required": ["box", "snapshot"]
            }
        }),
        json!({
            "name": "sbie_take_snapshot",
            "description": "Create a new snapshot of the current sandbox state (stops the box first).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "box": { "type": "string" },
                    "name": { "type": "string" },
                    "waitMs": { "type": "integer", "default": 30000 }
                },
                "required": ["box"]
            }
        }),
        json!({
            "name": "sbie_delete_snapshot",
            "description": "Delete one snapshot of a sandbox.",
            "inputSchema": {
                "type": "object",
                "properties": { "box": { "type": "string" }, "snapshot": { "type": "string" } },
                "required": ["box", "snapshot"]
            }
        }),
        json!({
            "name": "sbie_boot",
            "description": "Restore a snapshot and then start a program inside the sandbox in one step.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "box": { "type": "string" },
                    "snapshot": { "type": "string", "description": "defaults to the current snapshot" },
                    "program": { "type": "string", "description": "program path, may be box-internal such as C:\\\\Program Files\\\\App\\\\App.exe" },
                    "args": { "type": "array", "items": { "type": "string" } },
                    "waitMs": { "type": "integer", "default": 30000 },
                    "waitForWindow": { "type": "string", "description": "optional window title substring to wait for" }
                },
                "required": ["box", "program"]
            }
        }),
        json!({
            "name": "sbie_run",
            "description": "Start a program inside an already running sandbox.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "box": { "type": "string" },
                    "program": { "type": "string" },
                    "args": { "type": "array", "items": { "type": "string" } }
                },
                "required": ["box", "program"]
            }
        }),
        json!({
            "name": "sbie_terminate",
            "description": "Terminate every process running in a sandbox.",
            "inputSchema": box_arg
        }),
        json!({
            "name": "sbie_create_box",
            "description": "Create a sandbox. type=standard for a standard sandbox, type=app for an application-compartment box with data protection.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "box": { "type": "string" },
                    "type": { "type": "string", "enum": ["standard", "app"], "default": "standard" }
                },
                "required": ["box"]
            }
        }),
        json!({
            "name": "sbie_delete_box",
            "description": "Delete a sandbox definition, optionally purging its files.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "box": { "type": "string" },
                    "purge": { "type": "boolean", "default": false }
                },
                "required": ["box"]
            }
        }),
        json!({
            "name": "sbie_autorun",
            "description": "Run this sandbox's Windows autorun entries once, inside the box (registry Run/RunOnce plus Start Menu Startup folders, as seen from inside the sandbox). Also returns the Startup-folder items found in the box.",
            "inputSchema": {
                "type": "object",
                "properties": { "box": { "type": "string" } },
                "required": ["box"]
            }
        }),
        json!({
            "name": "sbie_autostart",
            "description": "Query or set whether this sandbox auto-runs its autorun entries at system boot ([GlobalSettings] StartSystemBox). Omit `enable` to query.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "box": { "type": "string" },
                    "enable": { "type": "boolean", "description": "true = on, false = off, omit = query" }
                },
                "required": ["box"]
            }
        }),
        json!({
            "name": "sbie_import_program",
            "description": "Copy a portable (\"green\") program folder into a sandbox so it permanently lives inside the box, and register it in the box's Run Menu. The host copy is left untouched; the program keeps its original path inside the box (C:\\... maps to <box>\\drive\\C\\...).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "box": { "type": "string" },
                    "folder": { "type": "string", "description": "host folder holding the portable program" },
                    "exe": { "type": "string", "description": "executable name relative to the folder; auto-detected when the folder has exactly one .exe" },
                    "label": { "type": "string", "description": "Run Menu entry name; defaults to the exe file stem" },
                    "addToRunMenu": { "type": "boolean", "default": true }
                },
                "required": ["box", "folder"]
            }
        }),
    ]
}

fn s(params: &Value, key: &str) -> Option<String> {
    params.get(key).and_then(|v| v.as_str()).map(|v| v.to_string())
}

fn args_array(params: &Value) -> Vec<String> {
    params
        .get("args")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
        .unwrap_or_default()
}

fn wait_ms(params: &Value) -> u64 {
    params.get("waitMs").and_then(|v| v.as_u64()).unwrap_or(30_000)
}

fn call_tool(params: &Value) -> Result<Value, (i64, String)> {
    let name = s(params, "name").ok_or((-32602, "missing tool name".to_string()))?;
    let arguments = params.get("arguments").cloned().unwrap_or(json!({}));
    let mut env = Env::load().map_err(|e| (-32000, e))?;

    let outcome: Result<Value, String> = match name.as_str() {
        "sbie_list_boxes" => Ok(Value::Array(list_boxes(&env))),
        "sbie_box_status" => require(&arguments, "box").and_then(|b| box_status(&env, &b)),
        "sbie_list_snapshots" => require(&arguments, "box").and_then(|b| list_snapshots(&env, &b)),
        "sbie_restore_snapshot" => match (require(&arguments, "box"), require(&arguments, "snapshot")) {
            (Ok(b), Ok(id)) => restore(&env, &b, &id, wait_ms(&arguments)),
            (Err(e), _) | (_, Err(e)) => Err(e),
        },
        "sbie_take_snapshot" => require(&arguments, "box")
            .and_then(|b| take_snapshot(&env, &b, &s(&arguments, "name").unwrap_or_default(), wait_ms(&arguments))),
        "sbie_delete_snapshot" => match (require(&arguments, "box"), require(&arguments, "snapshot")) {
            (Ok(b), Ok(id)) => delete_snapshot(&env, &b, &id),
            (Err(e), _) | (_, Err(e)) => Err(e),
        },
        "sbie_boot" => match (require(&arguments, "box"), require(&arguments, "program")) {
            (Ok(b), Ok(p)) => boot(
                &env,
                &b,
                s(&arguments, "snapshot").as_deref(),
                &p,
                &args_array(&arguments),
                wait_ms(&arguments),
                s(&arguments, "waitForWindow").as_deref(),
            ),
            (Err(e), _) | (_, Err(e)) => Err(e),
        },
        "sbie_run" => match (require(&arguments, "box"), require(&arguments, "program")) {
            (Ok(b), Ok(p)) => run_program(&env, &b, &p, &args_array(&arguments)),
            (Err(e), _) | (_, Err(e)) => Err(e),
        },
        "sbie_terminate" => require(&arguments, "box").and_then(|b| terminate(&env, &b, wait_ms(&arguments))),
        "sbie_create_box" => require(&arguments, "box")
            .and_then(|b| create_box(&mut env, &b, &s(&arguments, "type").unwrap_or_else(|| "standard".to_string()))),
        "sbie_delete_box" => require(&arguments, "box").and_then(|b| {
            let purge = arguments.get("purge").and_then(|v| v.as_bool()).unwrap_or(false);
            delete_box(&mut env, &b, purge, wait_ms(&arguments))
        }),
        "sbie_import_program" => match (require(&arguments, "box"), require(&arguments, "folder")) {
            (Ok(b), Ok(folder)) => {
                let menu = arguments.get("addToRunMenu").and_then(|v| v.as_bool()).unwrap_or(true);
                import_program(
                    &mut env,
                    &b,
                    &folder,
                    s(&arguments, "exe").as_deref(),
                    s(&arguments, "label").as_deref(),
                    menu,
                )
            }
            (Err(e), _) | (_, Err(e)) => Err(e),
        },
        other => Err(format!("unknown tool '{}'", other)),
    };

    match outcome {
        Ok(value) => Ok(json!({
            "content": [ { "type": "text", "text": serde_json::to_string_pretty(&value).unwrap_or_default() } ],
            "isError": false
        })),
        Err(err) => Ok(json!({
            "content": [ { "type": "text", "text": err } ],
            "isError": true
        })),
    }
}

fn require(params: &Value, key: &str) -> Result<String, String> {
    params
        .get(key)
        .and_then(|v| v.as_str())
        .filter(|v| !v.is_empty())
        .map(|v| v.to_string())
        .ok_or_else(|| format!("missing '{}' argument", key))
}
