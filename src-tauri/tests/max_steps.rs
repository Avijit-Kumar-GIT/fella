//! The agent loop's step cap: configurable via `FELLA_MAX_STEPS`, and the
//! forced final turn (when steps run out) tells the model why. Its own test
//! binary, and everything below runs as ONE sequential test rather than
//! several `#[tokio::test]` fns, so mutating the process-global
//! `FELLA_MAX_STEPS` can't race with a sibling test (same reasoning as
//! `tests/sql_timeout.rs`'s `FELLA_QUERY_TIMEOUT_SECS`, which gets a whole
//! file to itself for the same reason).

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use fella_lib::engine::EngineState;

fn scratch(tag: &str) -> PathBuf {
    // Point-at-a-mock tests: the warm-up ping would steal a scripted response.
    std::env::set_var("FELLA_SKIP_MODEL_WARMUP", "1");
    let n = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let p = std::env::temp_dir().join(format!("fella-{tag}-{n}"));
    fs::create_dir_all(&p).unwrap();
    p
}

/// A fake `/api/chat` that returns `responses[i]` for the i-th request, and
/// records every request's parsed JSON body so a test can inspect exactly
/// what was sent on a given turn.
fn fake_ollama(
    responses: Vec<serde_json::Value>,
) -> (String, Arc<Mutex<Vec<serde_json::Value>>>, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let url = format!("http://{addr}");
    let calls = Arc::new(AtomicUsize::new(0));
    let seen = Arc::new(Mutex::new(Vec::new()));
    let seen_for_thread = seen.clone();

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
            if let Ok(json) = serde_json::from_slice::<serde_json::Value>(&body) {
                seen_for_thread.lock().unwrap().push(json);
            }

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

    (url, seen, handle)
}

fn tool_call_response() -> serde_json::Value {
    serde_json::json!({
        "message": {
            "role": "assistant",
            "content": "",
            "tool_calls": [
                { "function": { "name": "list_files", "arguments": {} } }
            ]
        }
    })
}

fn engine_on(ws: &std::path::Path, data: &std::path::Path, url: &str) -> EngineState {
    let engine = EngineState::new(data).unwrap();
    engine
        .save_settings(
            serde_json::json!({ "provider": "ollama", "base_url": url, "model": "test" })
                .as_object()
                .unwrap(),
        )
        .unwrap();
    engine.open_workspace(ws).unwrap();
    engine
}

#[tokio::test]
async fn step_cap_and_forced_final_turn() {
    // --- FELLA_MAX_STEPS is honored, and the forced turn's real answer wins
    // over the canned fallback. ---
    std::env::set_var("FELLA_MAX_STEPS", "2");
    let ws = scratch("steps-ws");
    let data = scratch("steps-data");
    fs::write(ws.join("sales.csv"), "amount\n10\n20\n").unwrap();

    let (url, _seen, server) = fake_ollama(vec![
        tool_call_response(),
        tool_call_response(),
        serde_json::json!({
            "message": { "role": "assistant", "content": "Here's my best guess from what I found." }
        }),
    ]);
    let engine = engine_on(&ws, &data, &url);
    let answer = engine.ask("c1", "how much did we sell?", None, |_| {}).await.unwrap();
    server.join().unwrap();

    assert_eq!(answer.evidence.len(), 2, "should stop at the 2-step cap");
    assert_eq!(answer.text, "Here's my best guess from what I found.");

    let _ = fs::remove_dir_all(&ws);
    let _ = fs::remove_dir_all(&data);

    // --- Forced turn falls back to the canned line only when the model
    // still says nothing. ---
    std::env::set_var("FELLA_MAX_STEPS", "1");
    let ws = scratch("steps-ws2");
    let data = scratch("steps-data2");
    fs::write(ws.join("sales.csv"), "amount\n10\n20\n").unwrap();

    let (url, _seen, server) = fake_ollama(vec![
        tool_call_response(),
        serde_json::json!({ "message": { "role": "assistant", "content": "" } }),
    ]);
    let engine = engine_on(&ws, &data, &url);
    let answer = engine.ask("c1", "how much did we sell?", None, |_| {}).await.unwrap();
    server.join().unwrap();

    assert_eq!(
        answer.text,
        "I ran out of analysis steps before reaching a confident answer."
    );

    let _ = fs::remove_dir_all(&ws);
    let _ = fs::remove_dir_all(&data);

    // --- The forced turn actually tells the model why tools disappeared. ---
    std::env::set_var("FELLA_MAX_STEPS", "1");
    let ws = scratch("steps-ws3");
    let data = scratch("steps-data3");
    fs::write(ws.join("sales.csv"), "amount\n10\n20\n").unwrap();

    let (url, seen, server) = fake_ollama(vec![
        tool_call_response(),
        serde_json::json!({ "message": { "role": "assistant", "content": "my best guess" } }),
    ]);
    let engine = engine_on(&ws, &data, &url);
    engine.ask("c1", "how much did we sell?", None, |_| {}).await.unwrap();
    server.join().unwrap();

    let requests = seen.lock().unwrap();
    let last = requests.last().expect("the forced final turn should have sent a request");
    let messages = last["messages"].as_array().expect("messages array");
    let has_nudge = messages.iter().any(|m| {
        m["content"]
            .as_str()
            .map(|c| c.contains("out of tool-calling steps"))
            .unwrap_or(false)
    });
    assert!(has_nudge, "forced turn should tell the model why: {messages:#?}");

    let _ = fs::remove_dir_all(&ws);
    let _ = fs::remove_dir_all(&data);
}
