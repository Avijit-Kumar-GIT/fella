//! Drives the whole agent loop against a scripted fake OpenAI-compatible
//! endpoint: one tool-calling turn, then a final answer. No real model involved.

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use fella_lib::engine::evidence::VerificationStatus;
use fella_lib::engine::{AskEvent, EngineState};

fn scratch(tag: &str) -> PathBuf {
    // Point-at-a-mock tests: the warm-up ping would steal a scripted response.
    std::env::set_var("FELLA_SKIP_MODEL_WARMUP", "1");
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let p = std::env::temp_dir().join(format!("fella-{tag}-{n}"));
    fs::create_dir_all(&p).unwrap();
    p
}

/// A fake `/chat/completions` endpoint that returns `responses[i]` for the
/// i-th request.
fn fake_openai(responses: Vec<serde_json::Value>) -> (String, std::thread::JoinHandle<()>) {
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

fn openai_response(message: serde_json::Value) -> serde_json::Value {
    serde_json::json!({ "choices": [{ "message": message }] })
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

    let (url, server) = fake_openai(vec![
        openai_response(serde_json::json!({
                "role": "assistant",
                "content": "",
                "tool_calls": [
                    { "id": "call_1", "type": "function", "function": { "name": "run_sql",
                        "arguments": "{\"sql\":\"SELECT sum(amount) AS total FROM sales\"}" } }
                ]
        })),
        openai_response(
            serde_json::json!({ "role": "assistant", "content": "Total sales were 450." }),
        ),
    ]);

    let engine = EngineState::new(&data).unwrap();
    engine
        .save_settings(
            serde_json::json!({ "provider": "custom", "base_url": url, "model": "test" })
                .as_object()
                .unwrap(),
        )
        .unwrap();
    engine.set_api_key("custom", "sk-test").unwrap();
    engine.open_workspace(&ws).unwrap();

    let events: Arc<Mutex<Vec<AskEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = events.clone();
    let answer = engine
        .ask("c1", "how much did we sell?", None, move |ev| {
            sink.lock().unwrap().push(ev)
        })
        .await
        .unwrap();

    server.join().unwrap();

    assert!(answer.text.contains("450"), "answer was: {}", answer.text);
    assert_eq!(answer.evidence.len(), 1);
    assert_eq!(answer.status, VerificationStatus::Verified);
    let workspace = answer
        .workspace
        .as_ref()
        .expect("answer has a workspace snapshot");
    assert!(workspace.path.contains("fella-agent-ws-"));
    assert!(workspace.revision.starts_with('r'));
    let ev = &answer.evidence[0];
    assert_eq!(ev.id, "evidence-1");
    assert_eq!(ev.sources.len(), 1);
    assert_eq!(ev.sources[0].table, "sales");
    assert_eq!(ev.sources[0].source, "sales.csv");
    assert_eq!(ev.tool, "run_sql");
    assert_eq!(ev.row_count, Some(1));
    assert!(ev.sql.as_deref().unwrap().contains("sum(amount)"));
    assert!(ev.error.is_none());
    // verification: the cited query was re-run and the figure is backed
    assert!(
        answer.verification.iter().all(|c| c.ok),
        "{:?}",
        answer.verification
    );
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
async fn agent_calls_make_chart_then_answers_with_verified_visual_evidence() {
    let ws = scratch("chart-agent-ws");
    let data = scratch("chart-agent-data");
    fs::write(
        ws.join("sales.csv"),
        "month,amount\n2024-01,100\n2024-02,150\n2024-03,200\n",
    )
    .unwrap();

    let (url, server) = fake_openai(vec![
        openai_response(serde_json::json!({
            "role": "assistant",
            "content": "",
            "tool_calls": [{
                "id": "chart_call_1",
                "type": "function",
                "function": {
                    "name": "make_chart",
                    "arguments": "{\"kind\":\"auto\",\"title\":\"Sales over time\",\"sql\":\"SELECT month, amount FROM sales ORDER BY month\"}"
                }
            }]
        })),
        openai_response(serde_json::json!({
            "role": "assistant",
            "content": "Sales rose steadily from January through March."
        })),
    ]);

    let engine = EngineState::new(&data).unwrap();
    engine
        .save_settings(
            serde_json::json!({ "provider": "custom", "base_url": url, "model": "test" })
                .as_object()
                .unwrap(),
        )
        .unwrap();
    engine.set_api_key("custom", "sk-test").unwrap();
    engine.open_workspace(&ws).unwrap();

    let answer = engine
        .ask("chart-c1", "Show sales over time.", None, |_| {})
        .await
        .unwrap();

    server.join().unwrap();

    assert!(answer.text.contains("rose"), "answer: {}", answer.text);
    assert_eq!(answer.evidence.len(), 1);
    let evidence = &answer.evidence[0];
    assert_eq!(evidence.tool, "make_chart");
    assert_eq!(evidence.row_count, Some(3));
    assert!(evidence.error.is_none());
    let chart = evidence.chart.as_ref().expect("chart evidence");
    assert_eq!(
        chart.kind,
        fella_lib::engine::analytics::chart::ChartKind::Line
    );
    assert_eq!(chart.labels, vec!["2024-01", "2024-02", "2024-03"]);
    assert_eq!(chart.series[0].values, vec![100.0, 150.0, 200.0]);
    assert_eq!(answer.status, VerificationStatus::Verified);
    assert!(
        answer.verification.iter().all(|check| check.ok),
        "{:?}",
        answer.verification
    );

    let _ = fs::remove_dir_all(&ws);
    let _ = fs::remove_dir_all(&data);
}

#[tokio::test]
async fn cancel_stops_an_in_flight_run() {
    let ws = scratch("cancel-ws");
    let data = scratch("cancel-data");
    fs::write(ws.join("sales.csv"), "amount\n10\n20\n30\n").unwrap();

    // A `/chat/completions` endpoint that stalls ~2s before replying, so the run is still
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
            let payload = serde_json::to_vec(&openai_response(serde_json::json!({
                "role": "assistant", "content": "too late"
            })))
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
            serde_json::json!({ "provider": "custom", "base_url": url, "model": "test" })
                .as_object()
                .unwrap(),
        )
        .unwrap();
    engine.set_api_key("custom", "sk-test").unwrap();
    engine.open_workspace(&ws).unwrap();

    let running = engine.clone();
    let run = tokio::spawn(async move {
        running
            .ask("c1", "how much did we sell?", None, |_| {})
            .await
    });

    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    engine.cancel_run("c1");

    let answer = tokio::time::timeout(std::time::Duration::from_secs(3), run)
        .await
        .expect("cancel did not unblock the run")
        .unwrap()
        .unwrap();

    assert_eq!(answer.text, "Stopped.");
    assert!(
        answer.evidence.is_empty(),
        "evidence: {:?}",
        answer.evidence
    );

    let _ = server.join();
    let _ = fs::remove_dir_all(&ws);
    let _ = fs::remove_dir_all(&data);
}

#[tokio::test]
async fn openai_compatible_provider_runs_the_same_loop() {
    let ws = scratch("oai-ws");
    let data = scratch("oai-data");
    fs::write(ws.join("sales.csv"), "amount\n10\n20\n30\n").unwrap();

    let (url, server) = fake_openai(vec![
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

/// Two tool calls in one assistant turn run concurrently (two independent
/// sandbox runs: sequential startup would take about twice as long), and their
/// results come back in call order.
#[tokio::test]
#[cfg_attr(
    debug_assertions,
    ignore = "Wasmi's debug interpreter is too slow for this timing test; run it in release"
)]
async fn a_turns_tool_calls_run_concurrently() {
    let ws = scratch("par-ws");
    let data = scratch("par-data");
    fs::write(ws.join("s.csv"), "amount\n1\n").unwrap();

    let (url, server) = fake_openai(vec![
        openai_response(serde_json::json!({
                "role": "assistant",
                "content": "Checking two things at once.",
                "tool_calls": [
                    { "id": "call_1", "type": "function", "function": { "name": "run_python",
                        "arguments": "{\"code\":\"print('one')\"}" } },
                    { "id": "call_2", "type": "function", "function": { "name": "run_python",
                        "arguments": "{\"code\":\"print('two')\"}" } }
                ]
        })),
        openai_response(serde_json::json!({ "role": "assistant", "content": "Both done." })),
    ]);

    let engine = EngineState::new(&data).unwrap();
    engine
        .save_settings(
            serde_json::json!({ "provider": "custom", "base_url": url, "model": "test" })
                .as_object()
                .unwrap(),
        )
        .unwrap();
    engine.set_api_key("custom", "sk-test").unwrap();
    engine.open_workspace(&ws).unwrap();

    let events: Arc<Mutex<Vec<AskEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = events.clone();
    let t0 = std::time::Instant::now();
    let answer = engine
        .ask("c1", "check both", None, move |ev| {
            sink.lock().unwrap().push(ev)
        })
        .await
        .unwrap();
    let elapsed = t0.elapsed();
    server.join().unwrap();

    assert_eq!(answer.evidence.len(), 2);
    assert_eq!(
        answer
            .evidence
            .iter()
            .map(|e| e.id.as_str())
            .collect::<Vec<_>>(),
        vec!["evidence-1", "evidence-2"]
    );
    assert!(
        answer.evidence[0]
            .output
            .as_deref()
            .unwrap_or("")
            .contains("one"),
        "first tool evidence: {:?}, error: {:?}",
        answer.evidence[0].output,
        answer.evidence[0].error
    );
    assert!(
        answer.evidence[1]
            .output
            .as_deref()
            .unwrap_or("")
            .contains("two"),
        "second tool evidence: {:?}, error: {:?}",
        answer.evidence[1].output,
        answer.evidence[1].error
    );

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
    assert!(
        last_start < first_end,
        "both tools should start before either finishes: {kinds:?}"
    );

    if answer.evidence.iter().all(|e| e.error.is_none()) {
        // A fixed-ms bound here is flaky on a loaded/shared CI runner: sandbox
        // startup overhead alone can eat the slack a hardcoded
        // threshold assumed. Compare against the *actually measured* per-call
        // durations instead (`evidence[i].ms`, real wall-clock per tool call)
        // concurrent is ~= max(durations), sequential is ~= their sum, so a
        // bound partway between the two (75% of the sum) still clearly tells
        // them apart while scaling with however fast/slow this run's
        // startup overhead happens to be.
        let sum_ms: u64 = answer.evidence.iter().map(|e| e.ms).sum();
        assert!(
            (elapsed.as_millis() as u64) < sum_ms * 3 / 4,
            "two tool calls (summed {sum_ms}ms) took {elapsed:?} wall time; expected concurrent, not sequential"
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
