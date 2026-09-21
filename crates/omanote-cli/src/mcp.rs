//! Minimal MCP server (JSON-RPC 2.0, one message per line on stdio).
//!
//! Register it with an agent, e.g. Claude Code:
//! `claude mcp add omanote -- omanote-cli mcp`

use std::io::{BufRead, Write};

use serde_json::{json, Value};

use crate::ops::Ctx;

const PROTOCOL: &str = "2025-06-18";

const INSTRUCTIONS: &str = "Omanote is a quick-notes app synced with Joplin Server. \
Notes are plain Markdown: the first line is the title. Checklists use '- [ ] item' / '- [x] item'. \
A first line of 'list', 'sum', 'avg', 'count', 'math' or 'code' (optionally 'list: Title') turns on \
the matching note mode. Notes live in folders under the user's working notebook. \
End a line with '=' to have Omanote show its result ('2+2=', 'total ='); variables like 'vat = 22%' are \
remembered. Note ids can be shortened to a unique prefix. Changes are saved locally and synced by the app; \
call `sync` to push them to Joplin Server immediately.";

fn tools() -> Value {
    let note = json!({ "type": "string", "description": "Note id or unique id prefix" });
    let folder = json!({ "type": "string", "description": "Folder id or folder name" });
    let text = json!({ "type": "string", "description": "Markdown text; the first line is the title" });
    json!([
        { "name": "list_folders", "description": "List the folders of the working notebook, with note counts.",
          "inputSchema": { "type": "object", "properties": {} },
          "annotations": { "readOnlyHint": true } },
        { "name": "list_notes", "description": "List notes, most recently edited first.",
          "inputSchema": { "type": "object", "properties": {
              "folder": folder, "limit": { "type": "integer", "description": "Max notes (default 50)" } } },
          "annotations": { "readOnlyHint": true } },
        { "name": "search_notes", "description": "Find notes whose text contains a string (case-insensitive).",
          "inputSchema": { "type": "object", "properties": { "query": { "type": "string" } }, "required": ["query"] },
          "annotations": { "readOnlyHint": true } },
        { "name": "read_note", "description": "Read the full text of a note.",
          "inputSchema": { "type": "object", "properties": { "id": note }, "required": ["id"] },
          "annotations": { "readOnlyHint": true } },
        { "name": "create_note", "description": "Create a note. Returns the new note.",
          "inputSchema": { "type": "object", "properties": { "text": text, "folder": folder }, "required": ["text"] } },
        { "name": "update_note", "description": "Replace the whole text of a note.",
          "inputSchema": { "type": "object", "properties": { "id": note, "text": text }, "required": ["id", "text"] },
          "annotations": { "destructiveHint": true } },
        { "name": "append_to_note", "description": "Append lines at the end of a note.",
          "inputSchema": { "type": "object", "properties": { "id": note, "text": { "type": "string" } }, "required": ["id", "text"] } },
        { "name": "move_note", "description": "Move a note to another folder.",
          "inputSchema": { "type": "object", "properties": { "id": note, "folder": folder }, "required": ["id", "folder"] } },
        { "name": "trash_note", "description": "Move a note to the Joplin trash (recoverable from Joplin).",
          "inputSchema": { "type": "object", "properties": { "id": note }, "required": ["id"] },
          "annotations": { "destructiveHint": true } },
        { "name": "sync", "description": "Sync with Joplin Server now.",
          "inputSchema": { "type": "object", "properties": {} } }
    ])
}

fn arg<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    args.get(key).and_then(Value::as_str).ok_or_else(|| format!("missing '{key}'"))
}

fn to<T: serde::Serialize>(v: Result<T, String>) -> Result<Value, String> {
    v.map(|x| serde_json::to_value(x).expect("serializable"))
}

fn call(ctx: &Ctx, name: &str, args: &Value) -> Result<Value, String> {
    match name {
        "list_folders" => to(ctx.folders()),
        "list_notes" => {
            let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(50) as usize;
            to(ctx.list(args.get("folder").and_then(Value::as_str), limit))
        }
        "search_notes" => to(ctx.search(arg(args, "query")?)),
        "read_note" => to(ctx.read(arg(args, "id")?)),
        "create_note" => to(ctx.create(args.get("folder").and_then(Value::as_str), arg(args, "text")?)),
        "update_note" => to(ctx.update(arg(args, "id")?, arg(args, "text")?)),
        "append_to_note" => to(ctx.append(arg(args, "id")?, arg(args, "text")?)),
        "move_note" => ctx.move_note(arg(args, "id")?, arg(args, "folder")?).map(|_| json!({ "ok": true })),
        "trash_note" => ctx.trash(arg(args, "id")?).map(|_| json!({ "ok": true })),
        "sync" => to(ctx.sync()),
        other => Err(format!("unknown tool '{other}'")),
    }
}

fn handle(ctx: &Result<Ctx, String>, msg: &Value) -> Option<Value> {
    let id = msg.get("id").cloned()?; // notifications get no reply
    let method = msg.get("method").and_then(Value::as_str).unwrap_or("");
    let params = msg.get("params").cloned().unwrap_or(json!({}));
    let result = match method {
        "initialize" => Ok(json!({
            "protocolVersion": params.get("protocolVersion").and_then(Value::as_str).unwrap_or(PROTOCOL),
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "omanote", "version": env!("CARGO_PKG_VERSION") },
            "instructions": INSTRUCTIONS,
        })),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tools() })),
        "tools/call" => {
            let name = params.get("name").and_then(Value::as_str).unwrap_or("");
            let args = params.get("arguments").cloned().unwrap_or(json!({}));
            let out = ctx.as_ref().map_err(Clone::clone).and_then(|c| call(c, name, &args));
            Ok(match out {
                Ok(v) => json!({ "content": [{ "type": "text", "text": serde_json::to_string_pretty(&v).unwrap() }] }),
                Err(e) => json!({ "content": [{ "type": "text", "text": e }], "isError": true }),
            })
        }
        _ => Err(json!({ "code": -32601, "message": format!("method not found: {method}") })),
    };
    Some(match result {
        Ok(r) => json!({ "jsonrpc": "2.0", "id": id, "result": r }),
        Err(err) => json!({ "jsonrpc": "2.0", "id": id, "error": err }),
    })
}

pub fn serve() -> Result<(), String> {
    // Opening can fail (app not set up yet): report it per tool call instead of dying.
    let ctx = Ctx::open();
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let line = line.map_err(|e| e.to_string())?;
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Value>(&line) {
            Ok(msg) => handle(&ctx, &msg),
            Err(e) => Some(json!({ "jsonrpc": "2.0", "id": null, "error": { "code": -32700, "message": e.to_string() } })),
        };
        if let Some(r) = reply {
            writeln!(stdout, "{r}").map_err(|e| e.to_string())?;
            stdout.flush().map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handshake_and_tool_list() {
        let ctx: Result<Ctx, String> = Err("not set up".into());
        let init = handle(&ctx, &json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "protocolVersion": "2025-06-18" } })).unwrap();
        assert_eq!(init["result"]["serverInfo"]["name"], "omanote");
        assert!(handle(&ctx, &json!({ "jsonrpc": "2.0", "method": "notifications/initialized" })).is_none());
        let list = handle(&ctx, &json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" })).unwrap();
        assert_eq!(list["result"]["tools"].as_array().unwrap().len(), 10);
        let call = handle(&ctx, &json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": { "name": "list_notes" } })).unwrap();
        assert_eq!(call["result"]["isError"], true);
    }
}
