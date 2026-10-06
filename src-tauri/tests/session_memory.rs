//! Cross-question memory: a follow-up question in the same conversation gets a
//! distilled recap of the earlier turn in its system prompt; a new
//! conversation id starts clean. Driven by a scripted fake OpenAI-compatible
//! endpoint.

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
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let p = std::env::temp_dir().join(format!("fella-{tag}-{n}"));
    fs::create_dir_all(&p).unwrap();
    p
}

/// Fake `/chat/completions` that replays `responses` in order and records every
/// parsed request body for inspection.
fn fake_openai(
    responses: Vec<serde_json::Value>,
) -> (
    String,
    Arc<Mutex<Vec<serde_json::Value>>>,
    std::thread::JoinHandle<()>,
) {
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

fn openai_response(message: serde_json::Value) -> serde_json::Value {
    serde_json::json!({ "choices": [{ "message": message }] })
}

fn engine_on(ws: &std::path::Path, data: &std::path::Path, url: &str) -> EngineState {
    let engine = EngineState::new(data).unwrap();
    engine
        .save_settings(
            serde_json::json!({ "provider": "custom", "base_url": url, "model": "test" })
                .as_object()
                .unwrap(),
        )
        .unwrap();
    engine.set_api_key("custom", "sk-test").unwrap();
    engine.open_workspace(ws).unwrap();
    engine
}

fn sql_turn(sql: &str) -> serde_json::Value {
    openai_response(serde_json::json!({
            "role": "assistant",
            "content": "",
            "tool_calls": [ { "id": "call_1", "type": "function", "function": {
                "name": "run_sql", "arguments": serde_json::json!({ "sql": sql }).to_string()
            } } ]
    }))
}

fn answer_turn(text: &str) -> serde_json::Value {
    openai_response(serde_json::json!({ "role": "assistant", "content": text }))
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
    fs::write(
        ws.join("ledger.csv"),
        "month,amount\n2024-01,1200\n2024-02,1300\n",
    )
    .unwrap();

    // Q1: one SQL call then an answer.  Q2: straight to an answer (no tools).
    let (url, seen, server) = fake_openai(vec![
        sql_turn("SELECT SUM(amount) AS total FROM ledger"),
        answer_turn("You paid 2500 in total."),
        answer_turn("For 2024 it was 2500."),
    ]);
    let engine = engine_on(&ws, &data, &url);

    engine
        .ask("conv-A", "what did I pay in total?", None, |_| {})
        .await
        .unwrap();
    engine
        .ask("conv-A", "and just for 2024?", None, |_| {})
        .await
        .unwrap();
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
async fn restored_conversation_reuses_same_revision_execution_evidence() {
    let ws = scratch("continuity-restart-ws");
    let data = scratch("continuity-restart-data");
    fs::write(
        ws.join("ledger.csv"),
        "month,amount\n2024-01,1200\n2024-02,1300\n",
    )
    .unwrap();

    let (url, _, first_server) = fake_openai(vec![
        sql_turn("SELECT SUM(amount) AS total FROM ledger"),
        answer_turn("The total was 2500."),
    ]);
    let engine = engine_on(&ws, &data, &url);
    let first = engine
        .ask(
            "continuity-conversation",
            "What did I pay in total?",
            None,
            |_| {},
        )
        .await
        .unwrap();
    first_server.join().unwrap();
    let first_revision = first.workspace.as_ref().unwrap().revision.clone();
    let transcript = serde_json::json!({
        "id": "continuity-conversation",
        "saved_at_ms": 1,
        "workspace": ws.to_string_lossy(),
        "messages": [
            {
                "id": "user-first",
                "role": "user",
                "text": "What did I pay in total?",
                "ts": 1
            },
            {
                "id": "assistant-first",
                "role": "assistant",
                "text": first.text,
                "ts": 2,
                "answer": serde_json::to_value(&first).unwrap()
            }
        ]
    });
    engine
        .archive_conversation("continuity-conversation", &transcript.to_string())
        .unwrap();
    drop(engine);

    let (follow_url, requests, follow_server) = fake_openai(vec![
        openai_response(serde_json::json!({
            "role": "assistant",
            "content": "I will reuse the earlier checked execution result.",
            "tool_calls": [{
                "id": "prior-analysis",
                "type": "function",
                "function": {
                    "name": "read_prior_analysis",
                    "arguments": serde_json::json!({ "turn_id": first.turn_id }).to_string()
                }
            }]
        })),
        answer_turn("For the same 2024 period, the total remains 2500."),
    ]);
    let engine = engine_on(&ws, &data, &follow_url);
    assert_eq!(
        engine.catalog().revision.as_deref(),
        Some(first_revision.as_str()),
        "an unchanged local workspace must retain the revision used by its prior turn"
    );
    let follow_up = engine
        .ask("continuity-conversation", "And in 2024?", None, |_| {})
        .await
        .unwrap();
    follow_server.join().unwrap();

    assert!(follow_up.text.contains("2500"));
    assert_eq!(follow_up.evidence.len(), 1);
    assert_eq!(follow_up.evidence[0].tool, "read_prior_analysis");
    assert!(follow_up.evidence[0]
        .sources
        .iter()
        .any(|source| source.source.ends_with("ledger.csv")));
    let stored = engine.analysis_turn_load(&follow_up.turn_id).unwrap();
    assert_eq!(stored.prior_turn_refs, [first.turn_id.clone()]);
    assert_eq!(follow_up.trace.model.as_deref(), Some("test"));
    assert_eq!(follow_up.trace.model_calls.len(), 2);
    assert_eq!(
        follow_up.trace.mode,
        Some(fella_lib::engine::runtime::InteractionMode::WorkspaceAsk)
    );

    let request = &requests.lock().unwrap()[0];
    let system = system_of(request);
    assert!(system.contains(&first.turn_id));
    assert!(system.contains("retrieve its execution evidence"));
    assert!(request["tools"]
        .as_array()
        .unwrap()
        .iter()
        .any(|tool| tool["function"]["name"] == "read_prior_analysis"));

    drop(engine);
    let _ = fs::remove_dir_all(&ws);
    let _ = fs::remove_dir_all(&data);
}

#[tokio::test]
async fn general_answer_and_inspect_share_traceable_conversation_routes() {
    let ws = scratch("trace-routes-ws");
    let data = scratch("trace-routes-data");
    fs::write(ws.join("ledger.csv"), "month,amount\n2024-01,1200\n").unwrap();
    let (url, requests, server) = fake_openai(vec![
        answer_turn("Rust is a systems programming language."),
        openai_response(serde_json::json!({
            "role": "assistant",
            "content": "I will inspect the ledger's fields.",
            "tool_calls": [{
                "id": "inspect-ledger",
                "type": "function",
                "function": {
                    "name": "inspect_table",
                    "arguments": serde_json::json!({ "name": "ledger", "rows": 5 }).to_string()
                }
            }]
        })),
        answer_turn("The ledger has month and amount columns."),
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

    let general = engine
        .ask("shared-conversation", "What is Rust?", None, |_| {})
        .await
        .unwrap();
    assert_eq!(
        general.trace.mode,
        Some(fella_lib::engine::runtime::InteractionMode::ModelOnly)
    );
    assert_eq!(general.trace.model.as_deref(), Some("test"));
    assert_eq!(general.trace.model_calls.len(), 1);
    assert!(general.trace.elapsed_ms.is_some());

    engine.open_workspace(&ws).unwrap();
    let inspected = engine
        .ask_with_mode(
            "shared-conversation",
            "Which columns are in the ledger?",
            None,
            true,
            |_| {},
        )
        .await
        .unwrap();
    assert_eq!(
        inspected.trace.mode,
        Some(fella_lib::engine::runtime::InteractionMode::WorkspaceInspect)
    );
    assert_eq!(inspected.trace.model_calls.len(), 2);
    assert_eq!(inspected.evidence[0].tool, "inspect_table");
    assert_eq!(
        engine
            .analysis_turn_load(&general.turn_id)
            .unwrap()
            .conversation_id,
        engine
            .analysis_turn_load(&inspected.turn_id)
            .unwrap()
            .conversation_id
    );

    server.join().unwrap();
    let requests = requests.lock().unwrap();
    assert!(requests[0]["tools"]
        .as_array()
        .is_none_or(|tools| tools.is_empty()));
    let inspect_tools = requests[1]["tools"].as_array().unwrap();
    assert!(inspect_tools
        .iter()
        .any(|tool| tool["function"]["name"] == "inspect_table"));
    assert!(!inspect_tools
        .iter()
        .any(|tool| tool["function"]["name"] == "run_python"));
    assert!(system_of(&requests[1]).contains("What is Rust?"));

    drop(engine);
    let _ = fs::remove_dir_all(&ws);
    let _ = fs::remove_dir_all(&data);
}

#[tokio::test]
async fn changed_workspace_revision_uses_fresh_analysis_not_prior_evidence() {
    let ws = scratch("stale-continuity-ws");
    let data = scratch("stale-continuity-data");
    fs::write(ws.join("sales.csv"), "amount\n100\n").unwrap();

    let (first_url, _, first_server) = fake_openai(vec![
        sql_turn("SELECT SUM(amount) AS total FROM sales"),
        answer_turn("The total is 100."),
    ]);
    let engine = engine_on(&ws, &data, &first_url);
    let first = engine
        .ask("stale-conversation", "What are total sales?", None, |_| {})
        .await
        .unwrap();
    first_server.join().unwrap();

    fs::write(ws.join("sales.csv"), "amount\n900\n").unwrap();
    engine.open_workspace(&ws).unwrap();
    let (current_url, requests, current_server) = fake_openai(vec![
        sql_turn("SELECT SUM(amount) AS total FROM sales"),
        answer_turn("The current total is 900."),
    ]);
    engine
        .save_settings(
            serde_json::json!({
                "provider": "custom",
                "base_url": current_url,
                "model": "test"
            })
            .as_object()
            .unwrap(),
        )
        .unwrap();
    let current = engine
        .ask(
            "stale-conversation",
            "What are total sales now?",
            None,
            |_| {},
        )
        .await
        .unwrap();
    current_server.join().unwrap();

    assert_ne!(
        first.workspace.as_ref().unwrap().revision,
        current.workspace.as_ref().unwrap().revision
    );
    assert_eq!(current.evidence[0].tool, "run_sql");
    assert!(format!("{:?}", current.evidence[0].rows).contains("900"));
    let request = &requests.lock().unwrap()[0];
    let tool_names = request["tools"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|tool| tool["function"]["name"].as_str())
        .collect::<Vec<_>>();
    assert!(!tool_names.contains(&"read_prior_analysis"));
    assert!(system_of(request).contains("stale for the current workspace"));

    drop(engine);
    let _ = fs::remove_dir_all(&ws);
    let _ = fs::remove_dir_all(&data);
}

#[tokio::test]
async fn archived_conversation_restores_backend_context_after_restart() {
    let ws = scratch("restore-ws");
    let data = scratch("restore-data");
    fs::write(
        ws.join("ledger.csv"),
        "month,amount\n2024-01,1200\n2024-02,1300\n",
    )
    .unwrap();

    let (url, seen, server) = fake_openai(vec![answer_turn("For 2024 it was 2500.")]);
    let engine = engine_on(&ws, &data, &url);
    engine
        .archive_conversation(
            "restored-conversation",
            &serde_json::json!({
                "id": "restored-conversation",
                "saved_at_ms": 1,
                "workspace": ws.to_string_lossy(),
                "messages": [
                    {
                        "id": "user-1",
                        "role": "user",
                        "text": "what did I pay in total?",
                        "ts": 1
                    },
                    {
                        "id": "assistant-1",
                        "role": "assistant",
                        "text": "You paid 2500 in total.",
                        "ts": 2,
                        "answer": {
                            "text": "You paid 2500 in total.",
                            "evidence": [{
                                "tool": "run_sql",
                                "sql": "SELECT SUM(amount) AS total FROM ledger"
                            }]
                        }
                    }
                ]
            })
            .to_string(),
        )
        .unwrap();

    engine
        .ask("restored-conversation", "and just for 2024?", None, |_| {})
        .await
        .unwrap();
    server.join().unwrap();

    let reqs = seen.lock().unwrap();
    let system = system_of(&reqs[0]);
    assert!(
        system.contains("Earlier in this conversation"),
        "archived context should be restored:\n{system}"
    );
    assert!(system.contains("what did I pay in total?"));
    assert!(system.contains("SELECT SUM(amount)"));

    let _ = fs::remove_dir_all(&ws);
    let _ = fs::remove_dir_all(&data);
}

#[tokio::test]
async fn canonical_turn_rerun_requires_the_same_workspace_and_records_lineage() {
    let ws = scratch("rerun-ws");
    let other_ws = scratch("rerun-other-ws");
    let data = scratch("rerun-data");
    fs::write(ws.join("ledger.csv"), "amount\n12\n").unwrap();
    fs::write(other_ws.join("other.csv"), "amount\n99\n").unwrap();

    let (url, seen, server) = fake_openai(vec![
        answer_turn("The total is 12."),
        answer_turn("The total is still 12."),
    ]);
    let engine = engine_on(&ws, &data, &url);
    let context_refs = vec![fella_lib::engine::runtime::ContextReference {
        kind: "source".into(),
        key: "ledger.csv".into(),
        label: "ledger.csv".into(),
        detail: Some("2 columns".into()),
    }];
    let first = engine
        .ask_with_mode_and_context(
            "rerun-conversation",
            "what is the total?",
            None,
            false,
            &context_refs,
            |_| {},
        )
        .await
        .unwrap();
    let stored = engine.analysis_turn_load(&first.turn_id).unwrap();
    assert_eq!(stored.workspace.as_deref(), ws.to_str());
    assert_eq!(stored.question, "what is the total?");
    assert_eq!(stored.context_refs, context_refs);
    assert_eq!(stored.rerun_of, None);

    engine.open_workspace(&other_ws).unwrap();
    let mismatch = engine
        .analysis_turn_rerun(&first.turn_id, None, false, |_| {})
        .await;
    assert!(mismatch.is_err(), "a rerun must not cross workspace mounts");

    engine.open_workspace(&ws).unwrap();
    let rerun = engine
        .analysis_turn_rerun(&first.turn_id, None, false, |_| {})
        .await
        .unwrap();
    let rerun_record = engine.analysis_turn_load(&rerun.turn_id).unwrap();
    assert_eq!(
        rerun_record.rerun_of.as_deref(),
        Some(first.turn_id.as_str())
    );
    assert_eq!(rerun_record.workspace.as_deref(), ws.to_str());
    assert_eq!(rerun_record.workspace_revision, stored.workspace_revision);
    assert_eq!(rerun_record.context_refs, context_refs);

    server.join().unwrap();
    let requests = seen.lock().unwrap();
    assert!(requests.iter().all(|request| {
        let system = system_of(request);
        system.contains("User-selected starting points") && system.contains("ledger.csv")
    }));
    let _ = fs::remove_dir_all(&ws);
    let _ = fs::remove_dir_all(&other_ws);
    let _ = fs::remove_dir_all(&data);
}

#[tokio::test]
async fn canonical_turn_reports_source_changes_before_replay() {
    let ws = scratch("replay-status-ws");
    let data = scratch("replay-status-data");
    fs::write(ws.join("ledger.csv"), "amount\n12\n").unwrap();

    let (url, _seen, server) = fake_openai(vec![answer_turn("The total is 12.")]);
    let engine = engine_on(&ws, &data, &url);
    let first = engine
        .ask(
            "replay-status-conversation",
            "what is the total?",
            None,
            |_| {},
        )
        .await
        .unwrap();

    let before = engine.analysis_turn_replay_status(&first.turn_id).unwrap();
    assert!(before.same_workspace);
    assert!(!before.revision_changed);
    assert!(before.snapshot_available);
    assert!(before.can_rerun);
    assert!(before.source_changes.is_empty());

    fs::write(ws.join("ledger.csv"), "amount\n12\n17\n").unwrap();
    engine.reindex().unwrap();

    let after = engine.analysis_turn_replay_status(&first.turn_id).unwrap();
    assert!(after.same_workspace);
    assert!(after.revision_changed);
    assert!(after.snapshot_available);
    assert_eq!(after.source_changes.len(), 1);
    assert_eq!(
        after.source_changes[0].kind,
        fella_lib::engine::runtime::WorkspaceChangeKind::Changed
    );
    assert!(after.source_changes[0]
        .details
        .iter()
        .any(|detail| detail.contains("row count")));

    server.join().unwrap();
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

    let (url, seen, server) =
        fake_openai(vec![answer_turn("a"), answer_turn("b"), answer_turn("c")]);
    let engine = engine_on(&ws, &data, &url);

    engine
        .ask("tab-1", "q", Some("model-a"), |_| {})
        .await
        .unwrap();
    engine
        .ask("tab-2", "q", Some("model-b"), |_| {})
        .await
        .unwrap();
    engine.ask("tab-3", "q", None, |_| {}).await.unwrap();
    server.join().unwrap();

    let reqs = seen.lock().unwrap();
    assert_eq!(reqs[0]["model"], "model-a");
    assert_eq!(reqs[1]["model"], "model-b");
    assert_eq!(reqs[2]["model"], "test", "None uses the saved default");

    let _ = fs::remove_dir_all(&ws);
    let _ = fs::remove_dir_all(&data);
}

#[tokio::test]
async fn openai_request_carries_budget() {
    let ws = scratch("tune-ws");
    let data = scratch("tune-data");
    fs::write(ws.join("t.csv"), "a\n1\n").unwrap();

    let (url, seen, server) = fake_openai(vec![answer_turn("ok")]);
    let engine = engine_on(&ws, &data, &url);
    engine.ask("c", "hi", None, |_| {}).await.unwrap();
    server.join().unwrap();

    let reqs = seen.lock().unwrap();
    let req = &reqs[0];
    assert_eq!(req["stream_options"]["include_usage"], true);
    assert_eq!(req["max_tokens"], 1024, "generation is bounded");
    assert_eq!(req["temperature"], 0.2);

    // The step budget is stated once in the system prompt, not spliced into
    // the running history mid-run.
    let sys = system_of(req);
    assert!(
        sys.contains("at most"),
        "budget stated in the prompt:\n{sys}"
    );
    for m in req["messages"].as_array().unwrap() {
        assert!(
            !m["content"]
                .as_str()
                .unwrap_or_default()
                .contains("tool calls left"),
            "no mid-run nudge should land in history"
        );
    }

    let _ = fs::remove_dir_all(&ws);
    let _ = fs::remove_dir_all(&data);
}

#[tokio::test]
async fn inspect_table_includes_sample_rows() {
    let ws = scratch("desc-ws");
    let data = scratch("desc-data");
    fs::write(
        ws.join("ledger.csv"),
        "month,amount\n2024-01,1200\n2024-02,1300\n",
    )
    .unwrap();

    let (url, seen, server) = fake_openai(vec![
        openai_response(serde_json::json!({
                "role": "assistant",
                "content": "",
                "tool_calls": [
                    { "id": "call_1", "type": "function", "function": {
                        "name": "inspect_table", "arguments": "{\"name\":\"ledger\"}"
                    } }
                ]
        })),
        answer_turn("The ledger has two columns."),
    ]);
    let engine = engine_on(&ws, &data, &url);
    engine
        .ask("c", "what's in the ledger?", None, |_| {})
        .await
        .unwrap();
    server.join().unwrap();

    // The post-tool request carries the inspect_table result as a tool message.
    let reqs = seen.lock().unwrap();
    let tool_msg = reqs[1]["messages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["role"] == "tool")
        .and_then(|m| m["content"].as_str())
        .unwrap_or("");
    assert!(
        tool_msg.contains("row(s):"),
        "inspect_table folds in the first rows so no follow-up call is needed:\n{tool_msg}"
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

    let (url, seen, server) = fake_openai(vec![
        answer_turn("Apples are 5."),  // [0] conv-A Q1
        answer_turn("Bananas are 7."), // [1] conv-B Q1
        answer_turn("Still apples."),  // [2] conv-A Q2
    ]);
    let engine = engine_on(&ws, &data, &url);

    engine
        .ask("conv-A", "how much are apples?", None, |_| {})
        .await
        .unwrap();
    engine
        .ask("conv-B", "how much are bananas?", None, |_| {})
        .await
        .unwrap();
    engine
        .ask("conv-A", "and are they fresh?", None, |_| {})
        .await
        .unwrap();
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

    let (url, seen, server) = fake_openai(vec![
        answer_turn("First."),
        answer_turn("After forgetting."),
    ]);
    let engine = engine_on(&ws, &data, &url);

    engine
        .ask("conv-X", "first question?", None, |_| {})
        .await
        .unwrap();
    engine.forget_conversation("conv-X");
    engine
        .ask("conv-X", "second question?", None, |_| {})
        .await
        .unwrap();
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

    let (url, seen, server) = fake_openai(vec![
        answer_turn("First answer."),
        answer_turn("Second answer."),
    ]);
    let engine = engine_on(&ws, &data, &url);

    engine
        .ask("conv-1", "first question?", None, |_| {})
        .await
        .unwrap();
    engine
        .ask("conv-2", "unrelated question?", None, |_| {})
        .await
        .unwrap();
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
