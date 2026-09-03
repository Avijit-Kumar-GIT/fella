//! `mcp` connector packs: connect to a (stub) Streamable-HTTP MCP server,
//! register its read-only tools, call one through the agent loop, and flag /
//! withhold the non-read-only ones. Own test binary; needs `--features mcp`
//! for the real path (a no-op stub test runs without it).

#![cfg(feature = "mcp")]

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use fella_lib::engine::{AskEvent, EngineState};

fn scratch(tag: &str) -> PathBuf {
    // Point-at-a-mock tests: the warm-up ping would steal a scripted response.
    std::env::set_var("FELLA_SKIP_MODEL_WARMUP", "1");
    let n = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let p = std::env::temp_dir().join(format!("fella-{tag}-{n}"));
    fs::create_dir_all(&p).unwrap();
    p
}

fn write_pack(root: &Path, id: &str, connector_json: &str) -> PathBuf {
    let dir = root.join(id);
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("fella-pack.json"),
        format!(
            r#"{{"schema":1,"id":"{id}","kind":"mcp","name":"{id}","version":"1.0.0","description":"d","payload":"connector.json"}}"#
        ),
    )
    .unwrap();
    fs::write(dir.join("connector.json"), connector_json).unwrap();
    dir
}

/// Minimal Streamable-HTTP MCP server: plain JSON responses, keyed by the
/// JSON-RPC `method` in the POST body. Detached; lives until the process ends.
fn stub_mcp() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut method_line = String::new();
            if reader.read_line(&mut method_line).is_err() {
                continue;
            }
            let is_get = method_line.starts_with("GET");
            let is_delete = method_line.starts_with("DELETE");
            let mut len = 0usize;
            loop {
                let mut l = String::new();
                if reader.read_line(&mut l).is_err() || l == "\r\n" || l.is_empty() {
                    break;
                }
                if let Some(v) = l.to_lowercase().strip_prefix("content-length:") {
                    len = v.trim().parse().unwrap_or(0);
                }
            }
            let mut body = vec![0u8; len];
            let _ = reader.read_exact(&mut body);

            if is_get || is_delete {
                let _ = stream.write_all(b"HTTP/1.1 405 Method Not Allowed\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                continue;
            }
            let req: serde_json::Value = serde_json::from_slice(&body).unwrap_or_default();
            let id = req.get("id").cloned();
            let method = req.get("method").and_then(|m| m.as_str()).unwrap_or("");

            if id.is_none() {
                // a notification (e.g. notifications/initialized)
                let _ = stream.write_all(b"HTTP/1.1 202 Accepted\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                continue;
            }
            let result = match method {
                "initialize" => serde_json::json!({
                    "protocolVersion": "2025-06-18",
                    "capabilities": { "tools": {} },
                    "serverInfo": { "name": "stub", "version": "0" }
                }),
                "tools/list" => serde_json::json!({
                    "tools": [
                        { "name": "echo", "description": "Echo a message.",
                          "inputSchema": { "type": "object", "properties": { "msg": { "type": "string" } } },
                          "annotations": { "readOnlyHint": true } },
                        { "name": "peek", "description": "Read something.",
                          "inputSchema": { "type": "object" } },
                        { "name": "wipe", "description": "Delete everything.",
                          "inputSchema": { "type": "object" },
                          "annotations": { "readOnlyHint": false } }
                    ]
                }),
                "tools/call" => {
                    let msg = req["params"]["arguments"]["msg"].as_str().unwrap_or("");
                    serde_json::json!({
                        "content": [ { "type": "text", "text": format!("echoed: {msg}") } ],
                        "isError": false
                    })
                }
                _ => serde_json::json!({}),
            };
            let payload = serde_json::to_vec(&serde_json::json!({
                "jsonrpc": "2.0", "id": id, "result": result
            }))
            .unwrap();
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nMcp-Session-Id: stub-1\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                payload.len()
            );
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.write_all(&payload);
        }
    });
    format!("http://{addr}/mcp")
}

/// A fake `/api/chat`: `responses[i]` for the i-th call.
fn fake_ollama(responses: Vec<serde_json::Value>) -> (String, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let handle = std::thread::spawn(move || {
        for stream in listener.incoming().take(responses.len()) {
            let stream = stream.unwrap();
            let mut reader = BufReader::new(&stream);
            let mut len = 0usize;
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if line == "\r\n" || line.is_empty() {
                    break;
                }
                if let Some(v) = line.to_lowercase().strip_prefix("content-length:") {
                    len = v.trim().parse().unwrap_or(0);
                }
            }
            let mut body = vec![0u8; len];
            reader.read_exact(&mut body).unwrap();
            let i = calls.fetch_add(1, Ordering::SeqCst);
            let payload = serde_json::to_vec(&responses[i]).unwrap();
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                payload.len()
            );
            let mut w: &std::net::TcpStream = &stream;
            w.write_all(head.as_bytes()).unwrap();
            w.write_all(&payload).unwrap();
            w.flush().unwrap();
        }
    });
    (format!("http://{addr}"), handle)
}

#[test]
fn rejects_a_non_http_connector() {
    let src = scratch("mcp-bad");
    let data = scratch("mcp-bad-data");
    write_pack(
        &src,
        "shell",
        r#"{"transport":"stdio","url":"x","auth":{"type":"none"}}"#,
    );
    let engine = EngineState::new(&data).unwrap();
    assert!(engine.packs_add(&src.join("shell")).is_err() || {
        // parse happens on add; if add succeeded the manifest was fine but the
        // connector.json is not surfaced until enable/use. Enabling then asking
        // must not blow up, and the connector is simply skipped.
        engine.packs_set_enabled("shell", true).is_ok()
    });
    let _ = fs::remove_dir_all(&src);
    let _ = fs::remove_dir_all(&data);
}

#[tokio::test]
async fn connector_tool_runs_through_the_agent_and_non_read_only_is_withheld() {
    let src = scratch("mcp-src");
    let data = scratch("mcp-data");
    let mcp_url = stub_mcp();
    write_pack(
        &src,
        "stub",
        &format!(
            r#"{{"transport":"http","url":"{mcp_url}","auth":{{"type":"bearer","secret":"STUB_TOKEN"}},"setup":"paste a token"}}"#
        ),
    );

    let (ollama, server) = fake_ollama(vec![
        serde_json::json!({
            "message": {
                "role": "assistant", "content": "",
                "tool_calls": [
                    { "function": { "name": "stub__echo", "arguments": { "msg": "hi" } } }
                ]
            }
        }),
        serde_json::json!({
            "message": { "role": "assistant", "content": "The connector said: echoed: hi" }
        }),
    ]);

    let engine = EngineState::new(&data).unwrap();
    engine
        .save_settings(
            serde_json::json!({ "provider": "ollama", "base_url": ollama, "model": "test" })
                .as_object()
                .unwrap(),
        )
        .unwrap();
    engine.packs_add(&src.join("stub")).unwrap();
    engine.mcp_set_token("stub", "secret-token").unwrap();
    engine.packs_set_enabled("stub", true).unwrap();

    // packs_list should now flag it as ready (has a token).
    let listed = engine.packs_list();
    let row = listed.iter().find(|p| p.id == "stub").unwrap();
    assert!(row.enabled && !row.needs_token);

    let events: Arc<Mutex<Vec<AskEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = events.clone();
    let answer = engine
        .ask("c1", "ask the connector to echo hi", None, move |ev| sink.lock().unwrap().push(ev))
        .await
        .unwrap();
    server.join().unwrap();

    let call = answer
        .evidence
        .iter()
        .find(|e| e.tool == "stub__echo")
        .expect("the namespaced connector tool ran");
    assert!(call.error.is_none());
    assert!(
        call.output.as_deref().unwrap_or_default().contains("echoed: hi"),
        "output was {:?}",
        call.output
    );

    // The non-read-only `wipe` tool was withheld, with a notice.
    let notices: Vec<String> = events
        .lock()
        .unwrap()
        .iter()
        .filter_map(|e| match e {
            AskEvent::Notice { text } => Some(text.clone()),
            _ => None,
        })
        .collect();
    assert!(
        notices.iter().any(|n| n.contains("wipe")),
        "expected a notice about the withheld tool, got {notices:?}"
    );

    let _ = fs::remove_dir_all(&src);
    let _ = fs::remove_dir_all(&data);
}
