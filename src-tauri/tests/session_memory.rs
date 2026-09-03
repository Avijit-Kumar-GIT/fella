//! Cross-question memory: a follow-up question in the same conversation gets a
//! distilled recap of the earlier turn in its system prompt; a new
//! conversation id starts clean. Driven by a scripted fake Ollama.

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

/// Fake `/api/chat` that replays `responses` in order and records every parsed
/// request body for inspection.
fn fake_ollama(
    responses: Vec<serde_json::Value>,
) -> (String, Arc<Mutex<Vec<serde_json::Value>>>, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let url = format!("http://{addr}");
    let calls = Arc::new(AtomicUsize::new(0));
    let seen = Arc::new(Mutex::new(Vec::new()));
    let seen_t = seen.clone();

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
            if let Ok(j) = serde_json::from_slice::<serde_json::Value>(&body) {
                seen_t.lock().unwrap().push(j);
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

fn sql_turn(sql: &str) -> serde_json::Value {
    serde_json::json!({
        "message": {
            "role": "assistant",
            "content": "",
            "tool_calls": [ { "function": { "name": "run_sql", "arguments": { "sql": sql } } } ]
        }
    })
}

fn answer_turn(text: &str) -> serde_json::Value {
    serde_json::json!({ "message": { "role": "assistant", "content": text } })
}

fn system_of(req: &serde_json::Value) -> String {
    req["messages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["role"] == "system")
        .and_then(|m| m["content"].as_str())
        .unwrap_or("")
        .to_string()
}

#[tokio::test]
async fn follow_up_question_sees_the_earlier_turn() {
    let ws = scratch("mem-ws");
    let data = scratch("mem-data");
    fs::write(ws.join("ledger.csv"), "month,amount\n2024-01,1200\n2024-02,1300\n").unwrap();

    // Q1: one SQL call then an answer.  Q2: straight to an answer (no tools).
    let (url, seen, server) = fake_ollama(vec![
        sql_turn("SELECT SUM(amount) AS total FROM ledger"),
        answer_turn("You paid 2500 in total."),
        answer_turn("For 2024 it was 2500."),
    ]);
    let engine = engine_on(&ws, &data, &url);

    engine.ask("conv-A", "what did I pay in total?", None, |_| {}).await.unwrap();
    engine.ask("conv-A", "and just for 2024?", None, |_| {}).await.unwrap();
    server.join().unwrap();

    let reqs = seen.lock().unwrap();
    // Requests: [0] Q1 turn1, [1] Q1 turn2 (post-tool), [2] Q2 turn1.
    let q2_sys = system_of(&reqs[2]);
    assert!(
        q2_sys.contains("Earlier in this conversation"),
        "follow-up prompt should recap the last turn:\n{q2_sys}"
    );
    assert!(q2_sys.contains("what did I pay in total?"));
    assert!(q2_sys.contains("SELECT SUM(amount)"));

    let _ = fs::remove_dir_all(&ws);
    let _ = fs::remove_dir_all(&data);
}

#[tokio::test]
async fn each_tab_answers_with_its_own_model() {
    // Same provider / login, different model per conversation. `None` falls
    // back to the saved default ("test" from `engine_on`).
    let ws = scratch("model-ws");
    let data = scratch("model-data");
    fs::write(ws.join("t.csv"), "a\n1\n").unwrap();

    let (url, seen, server) = fake_ollama(vec![
        answer_turn("a"),
        answer_turn("b"),
        answer_turn("c"),
    ]);
    let engine = engine_on(&ws, &data, &url);

    engine.ask("tab-1", "q", Some("llama3.1"), |_| {}).await.unwrap();
    engine.ask("tab-2", "q", Some("gemma2"), |_| {}).await.unwrap();
    engine.ask("tab-3", "q", None, |_| {}).await.unwrap();
    server.join().unwrap();

    let reqs = seen.lock().unwrap();
    assert_eq!(reqs[0]["model"], "llama3.1");
    assert_eq!(reqs[1]["model"], "gemma2");
    assert_eq!(reqs[2]["model"], "test", "None uses the saved default");

    let _ = fs::remove_dir_all(&ws);
    let _ = fs::remove_dir_all(&data);
}

#[tokio::test]
async fn ollama_request_carries_tuning_and_budget() {
    let ws = scratch("tune-ws");
    let data = scratch("tune-data");
    fs::write(ws.join("t.csv"), "a\n1\n").unwrap();

    let (url, seen, server) = fake_ollama(vec![answer_turn("ok")]);
    let engine = engine_on(&ws, &data, &url);
    engine.ask("c", "hi", None, |_| {}).await.unwrap();
    server.join().unwrap();

    let reqs = seen.lock().unwrap();
    let req = &reqs[0];
    assert_eq!(req["keep_alive"], "30m", "model is kept warm between questions");
    assert_eq!(req["think"], false, "reasoning trace is off by default");
    assert_eq!(req["options"]["num_ctx"], 8192, "context window is sized up");
    assert_eq!(req["options"]["num_predict"], 1024, "generation is bounded");
    assert_eq!(req["options"]["temperature"], 0.2);

    // The step budget is stated once in the system prompt, not spliced into
    // the running history mid-run.
    let sys = system_of(req);
    assert!(sys.contains("at most"), "budget stated in the prompt:\n{sys}");
    for m in req["messages"].as_array().unwrap() {
        assert!(
            !m["content"].as_str().unwrap_or_default().contains("tool calls left"),
            "no mid-run nudge should land in history"
        );
    }

    let _ = fs::remove_dir_all(&ws);
    let _ = fs::remove_dir_all(&data);
}

#[tokio::test]
async fn describe_schema_now_includes_sample_rows() {
    let ws = scratch("desc-ws");
    let data = scratch("desc-data");
    fs::write(ws.join("ledger.csv"), "month,amount\n2024-01,1200\n2024-02,1300\n").unwrap();

    let (url, seen, server) = fake_ollama(vec![
        serde_json::json!({
            "message": {
                "role": "assistant",
                "content": "",
                "tool_calls": [
                    { "function": { "name": "describe_schema", "arguments": { "name": "ledger" } } }
                ]
            }
        }),
        answer_turn("The ledger has two columns."),
    ]);
    let engine = engine_on(&ws, &data, &url);
    engine.ask("c", "what's in the ledger?", None, |_| {}).await.unwrap();
    server.join().unwrap();

    // The post-tool request carries the describe_schema result as a tool message.
    let reqs = seen.lock().unwrap();
    let tool_msg = reqs[1]["messages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["role"] == "tool")
        .and_then(|m| m["content"].as_str())
        .unwrap_or("");
    assert!(
        tool_msg.contains("sample rows"),
        "describe_schema should fold in samples so no follow-up call is needed:\n{tool_msg}"
    );

    let _ = fs::remove_dir_all(&ws);
    let _ = fs::remove_dir_all(&data);
}

#[tokio::test]
async fn interleaved_conversations_keep_their_own_memory() {
    // A asks, then B asks, then A follows up. A's follow-up must still see A's
    // first turn and must NOT see B's. (Before per-conversation memory, B's turn
    // wiped A's.)
    let ws = scratch("mem-mix-ws");
    let data = scratch("mem-mix-data");
    fs::write(ws.join("ledger.csv"), "month,amount\n2024-01,1200\n").unwrap();

    let (url, seen, server) = fake_ollama(vec![
        answer_turn("Apples are 5."),   // [0] conv-A Q1
        answer_turn("Bananas are 7."),  // [1] conv-B Q1
        answer_turn("Still apples."),   // [2] conv-A Q2
    ]);
    let engine = engine_on(&ws, &data, &url);

    engine.ask("conv-A", "how much are apples?", None, |_| {}).await.unwrap();
    engine.ask("conv-B", "how much are bananas?", None, |_| {}).await.unwrap();
    engine.ask("conv-A", "and are they fresh?", None, |_| {}).await.unwrap();
    server.join().unwrap();

    let reqs = seen.lock().unwrap();
    let a2_sys = system_of(&reqs[2]);
    assert!(
        a2_sys.contains("Earlier in this conversation") && a2_sys.contains("how much are apples?"),
        "conv-A's follow-up should still recap conv-A's first turn:\n{a2_sys}"
    );
    assert!(
        !a2_sys.contains("bananas"),
        "conv-A must not see conv-B's turn:\n{a2_sys}"
    );

    let _ = fs::remove_dir_all(&ws);
    let _ = fs::remove_dir_all(&data);
}

#[tokio::test]
async fn forget_conversation_clears_the_memory() {
    let ws = scratch("mem-forget-ws");
    let data = scratch("mem-forget-data");
    fs::write(ws.join("ledger.csv"), "month,amount\n2024-01,1200\n").unwrap();

    let (url, seen, server) = fake_ollama(vec![
        answer_turn("First."),
        answer_turn("After forgetting."),
    ]);
    let engine = engine_on(&ws, &data, &url);

    engine.ask("conv-X", "first question?", None, |_| {}).await.unwrap();
    engine.forget_conversation("conv-X");
    engine.ask("conv-X", "second question?", None, |_| {}).await.unwrap();
    server.join().unwrap();

    let reqs = seen.lock().unwrap();
    assert!(
        !system_of(&reqs[1]).contains("Earlier in this conversation"),
        "a forgotten conversation starts clean again:\n{}",
        system_of(&reqs[1])
    );

    let _ = fs::remove_dir_all(&ws);
    let _ = fs::remove_dir_all(&data);
}

#[tokio::test]
async fn a_new_conversation_id_starts_clean() {
    let ws = scratch("mem2-ws");
    let data = scratch("mem2-data");
    fs::write(ws.join("ledger.csv"), "month,amount\n2024-01,1200\n").unwrap();

    let (url, seen, server) = fake_ollama(vec![
        answer_turn("First answer."),
        answer_turn("Second answer."),
    ]);
    let engine = engine_on(&ws, &data, &url);

    engine.ask("conv-1", "first question?", None, |_| {}).await.unwrap();
    engine.ask("conv-2", "unrelated question?", None, |_| {}).await.unwrap();
    server.join().unwrap();

    let reqs = seen.lock().unwrap();
    let second_sys = system_of(&reqs[1]);
    assert!(
        !second_sys.contains("Earlier in this conversation"),
        "a different conversation id must not carry memory:\n{second_sys}"
    );

    let _ = fs::remove_dir_all(&ws);
    let _ = fs::remove_dir_all(&data);
}
