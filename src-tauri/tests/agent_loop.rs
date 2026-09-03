//! Drives the whole agent loop against a scripted fake Ollama server: one
//! tool-calling turn, then a final answer. No real model involved.

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
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

/// A fake `/api/chat` that returns `responses[i]` for the i-th request.
fn fake_ollama(responses: Vec<serde_json::Value>) -> (String, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let url = format!("http://{addr}");
    let calls = Arc::new(AtomicUsize::new(0));

    let handle = std::thread::spawn(move || {
        for stream in listener.incoming().take(responses.len()) {
            let stream = stream.unwrap();
            let mut reader = BufReader::new(&stream);

            // Consume request line + headers, note Content-Length.
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

    (url, handle)
}

#[tokio::test]
async fn agent_calls_a_tool_then_answers() {
    let ws = scratch("agent-ws");
    let data = scratch("agent-data");
    fs::write(
        ws.join("sales.csv"),
        "month,amount\n2024-01,100\n2024-02,150\n2024-03,200\n",
    )
    .unwrap();

    let (url, server) = fake_ollama(vec![
        serde_json::json!({
            "message": {
                "role": "assistant",
                "content": "",
                "tool_calls": [
                    { "function": { "name": "run_sql",
                        "arguments": { "sql": "SELECT sum(amount) AS total FROM sales" } } }
                ]
            }
        }),
        serde_json::json!({
            "message": { "role": "assistant", "content": "Total sales were 450." }
        }),
    ]);

    let engine = EngineState::new(&data).unwrap();
    engine
        .save_settings(
            serde_json::json!({ "provider": "ollama", "base_url": url, "model": "test" })
                .as_object()
                .unwrap(),
        )
        .unwrap();
    engine.open_workspace(&ws).unwrap();

    let events: Arc<Mutex<Vec<AskEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = events.clone();
    let answer = engine
        .ask("c1", "how much did we sell?", None, move |ev| sink.lock().unwrap().push(ev))
        .await
        .unwrap();

    server.join().unwrap();

    assert!(answer.text.contains("450"), "answer was: {}", answer.text);
    assert_eq!(answer.evidence.len(), 1);
    let ev = &answer.evidence[0];
    assert_eq!(ev.tool, "run_sql");
    assert_eq!(ev.row_count, Some(1));
    assert!(ev.sql.as_deref().unwrap().contains("sum(amount)"));
    assert!(ev.error.is_none());
    // verification: the cited query was re-run and the figure is backed
    assert!(answer.verification.iter().all(|c| c.ok), "{:?}", answer.verification);
    assert!(answer
        .verification
        .iter()
        .any(|c| c.label.contains("re-checked the queries")));
    assert!(answer
        .verification
        .iter()
        .any(|c| c.label.contains("every number in the answer")));

    let kinds: Vec<_> = events
        .lock()
        .unwrap()
        .iter()
        .map(|e| match e {
            AskEvent::AssistantDelta { .. } => "delta",
            AskEvent::ToolStart { .. } => "tool_start",
            AskEvent::ToolEnd { .. } => "tool_end",
            AskEvent::Notice { .. } => "notice",
            AskEvent::AnswerDone { .. } => "answer_done",
        })
        .collect();
    assert!(kinds.contains(&"tool_start"));
    assert!(kinds.contains(&"tool_end"));
    assert_eq!(kinds.last(), Some(&"answer_done"));

    let _ = fs::remove_dir_all(&ws);
    let _ = fs::remove_dir_all(&data);
}

#[tokio::test]
async fn cancel_stops_an_in_flight_run() {
    let ws = scratch("cancel-ws");
    let data = scratch("cancel-data");
    fs::write(ws.join("sales.csv"), "amount\n10\n20\n30\n").unwrap();

    // A `/api/chat` that stalls ~2s before replying, so the run is still
    // waiting on the model when we cancel. Write errors are ignored: the
    // client drops the connection the moment the run is cancelled.
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let url = format!("http://{addr}");
    let server = std::thread::spawn(move || {
        if let Some(Ok(stream)) = listener.incoming().next() {
            let mut reader = BufReader::new(&stream);
            let mut len = 0usize;
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                    break;
                }
                if let Some(v) = line.to_lowercase().strip_prefix("content-length:") {
                    len = v.trim().parse().unwrap_or(0);
                }
            }
            let mut body = vec![0u8; len];
            let _ = reader.read_exact(&mut body);
            std::thread::sleep(std::time::Duration::from_secs(2));
            let payload = serde_json::to_vec(&serde_json::json!({
                "message": { "role": "assistant", "content": "too late" }
            }))
            .unwrap();
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                payload.len()
            );
            let mut w: &std::net::TcpStream = &stream;
            let _ = w.write_all(head.as_bytes());
            let _ = w.write_all(&payload);
            let _ = w.flush();
        }
    });

    let engine = Arc::new(EngineState::new(&data).unwrap());
    engine
        .save_settings(
            serde_json::json!({ "provider": "ollama", "base_url": url, "model": "test" })
                .as_object()
                .unwrap(),
        )
        .unwrap();
    engine.open_workspace(&ws).unwrap();

    let running = engine.clone();
    let run = tokio::spawn(async move { running.ask("c1", "how much did we sell?", None, |_| {}).await });

    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    engine.cancel_run("c1");

    let answer = tokio::time::timeout(std::time::Duration::from_secs(3), run)
        .await
        .expect("cancel did not unblock the run")
        .unwrap()
        .unwrap();

    assert_eq!(answer.text, "Stopped.");
    assert!(answer.evidence.is_empty(), "evidence: {:?}", answer.evidence);

    let _ = server.join();
    let _ = fs::remove_dir_all(&ws);
    let _ = fs::remove_dir_all(&data);
}

#[tokio::test]
async fn openai_compatible_provider_runs_the_same_loop() {
    let ws = scratch("oai-ws");
    let data = scratch("oai-data");
    fs::write(ws.join("sales.csv"), "amount\n10\n20\n30\n").unwrap();

    let (url, server) = fake_ollama(vec![
        serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{
                        "id": "call_abc",
                        "type": "function",
                        "function": {
                            "name": "run_sql",
                            "arguments": "{\"sql\": \"SELECT sum(amount) AS total FROM sales\"}"
                        }
                    }]
                }
            }]
        }),
        serde_json::json!({
            "choices": [{ "message": { "role": "assistant", "content": "The total is 60." } }]
        }),
    ]);

    let engine = EngineState::new(&data).unwrap();
    engine
        .save_settings(
            serde_json::json!({ "provider": "custom", "base_url": url, "model": "gpt-x" })
                .as_object()
                .unwrap(),
        )
        .unwrap();
    engine.set_api_key("custom", "sk-test").unwrap();
    assert!(engine.settings().has_credential);
    engine.open_workspace(&ws).unwrap();

    let answer = engine.ask("c1", "total?", None, |_| {}).await.unwrap();
    server.join().unwrap();

    assert!(answer.text.contains("60"), "answer: {}", answer.text);
    assert_eq!(answer.evidence.len(), 1);
    assert_eq!(answer.evidence[0].tool, "run_sql");
    assert!(answer.verification.iter().all(|c| c.ok));

    let _ = fs::remove_dir_all(&ws);
    let _ = fs::remove_dir_all(&data);
}

/// The model calls a tool successfully, then every following turn 429s past the
/// retry budget. The run should still return `Ok` with the evidence gathered so
/// far and a note not a bare error that loses the work.
#[tokio::test]
async fn keeps_partial_evidence_when_the_model_fails_after_a_tool_call() {
    let ws = scratch("partial-ws");
    let data = scratch("partial-data");
    fs::write(ws.join("sales.csv"), "amount\n10\n20\n30\n").unwrap();

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());

    std::thread::spawn(move || {
        for (n, stream) in listener.incoming().take(10).enumerate() {
            let stream = stream.unwrap();
            let mut reader = BufReader::new(&stream);
            let mut len = 0usize;
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                    break;
                }
                if let Some(v) = line.to_lowercase().strip_prefix("content-length:") {
                    len = v.trim().parse().unwrap_or(0);
                }
            }
            let mut body = vec![0u8; len];
            let _ = reader.read_exact(&mut body);

            let mut w: &std::net::TcpStream = &stream;
            if n == 0 {
                let payload = serde_json::to_vec(&serde_json::json!({
                    "choices": [{ "message": {
                        "role": "assistant",
                        "content": null,
                        "tool_calls": [{
                            "id": "call_1",
                            "type": "function",
                            "function": {
                                "name": "run_sql",
                                "arguments": "{\"sql\": \"SELECT sum(amount) AS total FROM sales\"}"
                            }
                        }]
                    }}]
                }))
                .unwrap();
                let head = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    payload.len()
                );
                let _ = w.write_all(head.as_bytes());
                let _ = w.write_all(&payload);
            } else {
                let _ = w.write_all(
                    b"HTTP/1.1 429 Too Many Requests\r\nRetry-After: 0\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}",
                );
            }
            let _ = w.flush();
        }
    });

    let engine = EngineState::new(&data).unwrap();
    engine
        .save_settings(
            serde_json::json!({ "provider": "custom", "base_url": url, "model": "gpt-x" })
                .as_object()
                .unwrap(),
        )
        .unwrap();
    engine.set_api_key("custom", "sk-test").unwrap();
    engine.open_workspace(&ws).unwrap();

    let answer = engine.ask("c1", "total?", None, |_| {}).await.unwrap();

    assert_eq!(answer.evidence.len(), 1, "evidence was kept");
    assert_eq!(answer.evidence[0].tool, "run_sql");
    assert!(
        answer.text.contains("gathered so far"),
        "text: {}",
        answer.text
    );

    let _ = fs::remove_dir_all(&ws);
    let _ = fs::remove_dir_all(&data);
}

/// Two tool calls in one assistant turn run concurrently (two ~0.4 s python
/// sleeps: sequential would be ~0.8 s), and their results come back in call
/// order.
#[tokio::test]
async fn a_turns_tool_calls_run_concurrently() {
    let ws = scratch("par-ws");
    let data = scratch("par-data");
    fs::write(ws.join("s.csv"), "amount\n1\n").unwrap();

    let (url, server) = fake_ollama(vec![
        serde_json::json!({
            "message": {
                "role": "assistant",
                "content": "Checking two things at once.",
                "tool_calls": [
                    { "function": { "name": "run_python",
                        "arguments": { "code": "import time; time.sleep(0.4); print('one')" } } },
                    { "function": { "name": "run_python",
                        "arguments": { "code": "import time; time.sleep(0.4); print('two')" } } }
                ]
            }
        }),
        serde_json::json!({ "message": { "role": "assistant", "content": "Both done." } }),
    ]);

    let engine = EngineState::new(&data).unwrap();
    engine
        .save_settings(
            serde_json::json!({ "provider": "ollama", "base_url": url, "model": "test" })
                .as_object()
                .unwrap(),
        )
        .unwrap();
    engine.open_workspace(&ws).unwrap();

    let events: Arc<Mutex<Vec<AskEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = events.clone();
    let t0 = std::time::Instant::now();
    let answer = engine
        .ask("c1", "check both", None, move |ev| sink.lock().unwrap().push(ev))
        .await
        .unwrap();
    let elapsed = t0.elapsed();
    server.join().unwrap();

    assert_eq!(answer.evidence.len(), 2);
    assert!(answer.evidence[0].output.as_deref().unwrap_or("").contains("one"));
    assert!(answer.evidence[1].output.as_deref().unwrap_or("").contains("two"));

    // Every ToolStart is emitted before the first ToolEnd.
    let kinds: Vec<&str> = events
        .lock()
        .unwrap()
        .iter()
        .map(|e| match e {
            AskEvent::ToolStart { .. } => "start",
            AskEvent::ToolEnd { .. } => "end",
            _ => "other",
        })
        .collect();
    let first_end = kinds.iter().position(|k| *k == "end").unwrap();
    let last_start = kinds.iter().rposition(|k| *k == "start").unwrap();
    assert!(last_start < first_end, "both tools should start before either finishes: {kinds:?}");

    if answer.evidence.iter().all(|e| e.error.is_none()) {
        assert!(
            elapsed < std::time::Duration::from_millis(750),
            "two 0.4s sleeps took {elapsed:?}; expected concurrent (~0.4s), not sequential"
        );
    }

    let _ = fs::remove_dir_all(&ws);
    let _ = fs::remove_dir_all(&data);
}

/// The OpenAI-compatible wire streams text token by token (it used to arrive
/// in one delta).
#[tokio::test]
async fn openai_wire_streams_tokens() {
    let ws = scratch("sse-ws");
    let data = scratch("sse-data");
    fs::write(ws.join("s.csv"), "amount\n5\n").unwrap();

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let stream = listener.incoming().next().unwrap().unwrap();
        let mut reader = BufReader::new(&stream);
        let mut len = 0usize;
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                break;
            }
            if let Some(v) = line.to_lowercase().strip_prefix("content-length:") {
                len = v.trim().parse().unwrap_or(0);
            }
        }
        let mut body = vec![0u8; len];
        let _ = reader.read_exact(&mut body);

        let sse = "data: {\"choices\":[{\"delta\":{\"content\":\"The \"}}]}\n\
                   data: {\"choices\":[{\"delta\":{\"content\":\"total \"}}]}\n\
                   data: {\"choices\":[{\"delta\":{\"content\":\"is 5.\"}}]}\n\
                   data: [DONE]\n";
        let mut w: &std::net::TcpStream = &stream;
        let _ = write!(
            w,
            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            sse.len(),
            sse
        );
        let _ = w.flush();
    });

    let engine = EngineState::new(&data).unwrap();
    engine
        .save_settings(
            serde_json::json!({ "provider": "custom", "base_url": url, "model": "gpt-x" })
                .as_object()
                .unwrap(),
        )
        .unwrap();
    engine.set_api_key("custom", "sk-test").unwrap();
    engine.open_workspace(&ws).unwrap();

    let deltas: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let d = deltas.clone();
    let answer = engine
        .ask("c1", "total?", None, move |ev| {
            if let AskEvent::AssistantDelta { text } = ev {
                d.lock().unwrap().push(text);
            }
        })
        .await
        .unwrap();
    server.join().unwrap();

    assert!(answer.text.contains("is 5."), "text: {}", answer.text);
    assert!(
        deltas.lock().unwrap().len() >= 2,
        "text should arrive in multiple deltas: {:?}",
        *deltas.lock().unwrap()
    );

    let _ = fs::remove_dir_all(&ws);
    let _ = fs::remove_dir_all(&data);
}
