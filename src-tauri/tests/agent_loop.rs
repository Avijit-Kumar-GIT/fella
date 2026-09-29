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
    let (url, _, handle) = fake_openai_with_requests(responses);
    (url, handle)
}

fn fake_openai_with_requests(
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
    let requests = Arc::new(Mutex::new(Vec::new()));
    let seen = requests.clone();

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
            seen.lock()
                .unwrap()
                .push(serde_json::from_slice(&body).unwrap());

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

    (url, requests, handle)
}

fn openai_response(message: serde_json::Value) -> serde_json::Value {
    serde_json::json!({ "choices": [{ "message": message }] })
}

fn contract_turn(contract: serde_json::Value) -> serde_json::Value {
    openai_response(serde_json::json!({
        "role": "assistant",
        "content": "",
        "tool_calls": [{
            "id": "contract",
            "type": "function",
            "function": {
                "name": "__analysis_contract",
                "arguments": contract.to_string()
            }
        }]
    }))
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
    assert_eq!(
        answer.status,
        VerificationStatus::Verified,
        "verification checks: {:?}",
        answer.verification
    );
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
            AskEvent::TurnState { .. } => "turn_state",
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
async fn semantic_verification_repairs_archive_scope_before_accepting_an_answer() {
    let ws = scratch("scope-repair-ws");
    let data = scratch("scope-repair-data");
    fs::create_dir_all(ws.join("archive")).unwrap();
    fs::write(ws.join("current.csv"), "amount\n100\n").unwrap();
    fs::write(ws.join("archive").join("old.csv"), "amount\n200\n").unwrap();

    let (url, server) = fake_openai(vec![
        openai_response(serde_json::json!({
            "role": "assistant",
            "content": "I checked both files.",
            "tool_calls": [{
                "id": "wrong-scope",
                "type": "function",
                "function": {
                    "name": "run_sql",
                    "arguments": "{\"sql\":\"SELECT SUM(amount) AS total FROM current UNION ALL SELECT SUM(amount) AS total FROM old\"}"
                }
            }]
        })),
        openai_response(serde_json::json!({
            "role": "assistant",
            "content": "The current export totals $300.",
        })),
        openai_response(serde_json::json!({
            "role": "assistant",
            "content": "I corrected the source scope.",
            "tool_calls": [{
                "id": "correct-scope",
                "type": "function",
                "function": {
                    "name": "run_sql",
                    "arguments": "{\"sql\":\"SELECT SUM(amount) AS total FROM current\"}"
                }
            }]
        })),
        openai_response(serde_json::json!({
            "role": "assistant",
            "content": "The current export totals $100."
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
        .ask(
            "scope-repair",
            "What is the total in the current export?",
            None,
            |_| {},
        )
        .await
        .unwrap();
    server.join().unwrap();

    assert!(answer.text.contains("100"), "answer: {}", answer.text);
    assert_eq!(answer.status, VerificationStatus::Verified);
    assert!(answer.evidence.iter().any(|item| {
        item.error
            .as_deref()
            .is_some_and(|error| error.contains("superseded"))
    }));
    assert!(answer.evidence.iter().any(|item| {
        item.error.is_none()
            && item
                .sql
                .as_deref()
                .is_some_and(|sql| sql.contains("FROM current"))
    }));

    let _ = fs::remove_dir_all(&ws);
    let _ = fs::remove_dir_all(&data);
}

#[tokio::test]
async fn semantic_verification_repairs_a_zero_row_filter_instead_of_accepting_zero() {
    let ws = scratch("empty-filter-repair-ws");
    let data = scratch("empty-filter-repair-data");
    fs::write(
        ws.join("transactions.csv"),
        "description,amount\nlandlord transfer,1200\n",
    )
    .unwrap();

    let (url, server) = fake_openai(vec![
        openai_response(serde_json::json!({
            "role": "assistant",
            "content": "I found no matching rows, so the total is $0.",
            "tool_calls": [{
                "id": "empty-filter",
                "type": "function",
                "function": {
                    "name": "run_sql",
                    "arguments": "{\"sql\":\"SELECT SUM(amount) AS total FROM transactions WHERE description LIKE '%current export%'\"}"
                }
            }]
        })),
        openai_response(serde_json::json!({
            "role": "assistant",
            "content": "The transaction total is $0.",
        })),
        openai_response(serde_json::json!({
            "role": "assistant",
            "content": "I corrected the filter.",
            "tool_calls": [{
                "id": "fixed-filter",
                "type": "function",
                "function": {
                    "name": "run_sql",
                    "arguments": "{\"sql\":\"SELECT SUM(amount) AS total FROM transactions\"}"
                }
            }]
        })),
        openai_response(serde_json::json!({
            "role": "assistant",
            "content": "The transaction total is $1,200."
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
        .ask(
            "empty-filter-repair",
            "What is the transaction total?",
            None,
            |_| {},
        )
        .await
        .unwrap();
    server.join().unwrap();

    assert!(answer.text.contains("1,200"), "answer: {}", answer.text);
    assert_eq!(answer.status, VerificationStatus::Verified);
    assert!(answer.evidence.iter().any(|item| {
        item.error
            .as_deref()
            .is_some_and(|error| error.contains("superseded"))
    }));

    let _ = fs::remove_dir_all(&ws);
    let _ = fs::remove_dir_all(&data);
}

#[tokio::test]
async fn semantic_verification_repairs_a_derived_value_missing_from_query_results() {
    let ws = scratch("derived-value-repair-ws");
    let data = scratch("derived-value-repair-data");
    fs::write(
        ws.join("rent.csv"),
        "period,rent\nfirst,100\nfirst,100\nsecond,125\nsecond,125\n",
    )
    .unwrap();

    let (url, requests, server) = fake_openai_with_requests(vec![
        openai_response(serde_json::json!({
            "role": "assistant",
            "content": "I will compare the two periods.",
            "tool_calls": [{
                "id": "period-totals",
                "type": "function",
                "function": {
                    "name": "run_sql",
                    "arguments": "{\"sql\":\"SELECT period, SUM(rent) AS total FROM rent GROUP BY period ORDER BY period\"}"
                }
            }]
        })),
        openai_response(serde_json::json!({
            "role": "assistant",
            "content": "The first period totaled $200 and the second period $250, so the second period was $50 higher."
        })),
        openai_response(serde_json::json!({
            "role": "assistant",
            "content": "I need the requested difference in the computed result.",
            "tool_calls": [{
                "id": "period-difference",
                "type": "function",
                "function": {
                    "name": "run_sql",
                    "arguments": "{\"sql\":\"SELECT SUM(CASE WHEN period = 'first' THEN rent ELSE 0 END) AS first_period, SUM(CASE WHEN period = 'second' THEN rent ELSE 0 END) AS second_period, SUM(CASE WHEN period = 'second' THEN rent ELSE 0 END) - SUM(CASE WHEN period = 'first' THEN rent ELSE 0 END) AS difference FROM rent\"}"
                }
            }]
        })),
        openai_response(serde_json::json!({
            "role": "assistant",
            "content": "The difference was $50 (second period $250 minus first period $200)."
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
        .ask(
            "derived-value-repair",
            "Did I pay more in the second period or first period, and by how much?",
            None,
            |_| {},
        )
        .await
        .unwrap();
    let captured = requests.lock().unwrap();
    assert_eq!(
        captured.len(),
        4,
        "expected a bounded repair round trip; answer={}; status={:?}; checks={:?}; evidence={:?}",
        answer.text,
        answer.status,
        answer.verification,
        answer.evidence
    );
    assert!(captured[2]
        .to_string()
        .contains("Semantic verification failed"));
    drop(captured);
    server.join().unwrap();

    assert!(answer.text.contains("$50"), "answer: {}", answer.text);
    assert_eq!(answer.status, VerificationStatus::Verified);
    assert!(answer.evidence.iter().any(|item| {
        item.error
            .as_deref()
            .is_some_and(|error| error.contains("superseded"))
    }));
    assert!(answer.evidence.iter().any(|item| {
        item.error.is_none()
            && item
                .sql
                .as_deref()
                .is_some_and(|sql| sql.contains("AS difference"))
    }));

    let _ = fs::remove_dir_all(&ws);
    let _ = fs::remove_dir_all(&data);
}

#[tokio::test]
async fn sequential_questions_keep_the_workspace_and_resolve_each_subject() {
    let ws = scratch("sequential-baselines-ws");
    let data = scratch("sequential-baselines-data");
    fs::write(
        ws.join("spend.csv"),
        "month,category,amount\n2024-01-01,rent,1200\n2024-02-01,rent,1300\n",
    )
    .unwrap();
    fs::write(
        ws.join("rent_ledger.csv"),
        "date,rent\nJan 1, 2024,1400\nFeb 1, 2024,1400\n",
    )
    .unwrap();
    fs::write(
        ws.join("books.csv"),
        "title,rating\nThe Dispossessed,5\nInvisible Cities,4\n",
    )
    .unwrap();

    let (url, seen, server) = fake_openai_with_requests(vec![
        contract_turn(serde_json::json!({
            "interpretation": "assumed",
            "subject": "rent",
            "measures": [{ "concept": "total spending", "field": "amount", "operation": "sum" }],
            "filters": [{ "concept": "rent category", "field": "category", "candidate_values": ["rent"] }],
            "time": { "field": "month", "range": "2024" },
            "group_by": [],
            "assumptions": [],
            "unresolved": []
        })),
        openai_response(serde_json::json!({
            "role": "assistant",
            "content": "Total spending on rent in 2024 was $2,500.00."
        })),
        contract_turn(serde_json::json!({
            "interpretation": "assumed",
            "subject": "reading list",
            "measures": [{ "concept": "average rating", "field": "rating", "operation": "average" }],
            "filters": [],
            "group_by": [],
            "assumptions": [],
            "unresolved": []
        })),
        openai_response(serde_json::json!({
            "role": "assistant",
            "content": "The average rating across the reading list is 4.5."
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

    let first = engine
        .ask(
            "same-conversation",
            "What was my total spending on rent in 2024?",
            None,
            |_| {},
        )
        .await
        .unwrap();
    let second = engine
        .ask(
            "same-conversation",
            "What's the average rating across all the books in my reading list?",
            None,
            |_| {},
        )
        .await
        .unwrap();
    server.join().unwrap();

    assert!(
        first.text.contains("2,500.00"),
        "first answer: {}",
        first.text
    );
    assert_eq!(first.status, VerificationStatus::Verified);
    assert_eq!(
        first.grounding.as_ref().unwrap().source.as_deref(),
        Some("spend")
    );
    assert!(
        second.text.contains("4.5"),
        "second answer: {}",
        second.text
    );
    assert_eq!(second.status, VerificationStatus::Verified);
    assert_eq!(
        second.grounding.as_ref().unwrap().source.as_deref(),
        Some("books")
    );

    let requests = seen.lock().unwrap();
    let second_system = requests[2]["messages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|message| message["role"] == "system")
        .and_then(|message| message["content"].as_str())
        .unwrap_or("");
    assert!(
        second_system.contains("Earlier in this conversation")
            && second_system.contains("total spending on rent"),
        "the second question must retain same-conversation context: {second_system}"
    );

    let _ = fs::remove_dir_all(&ws);
    let _ = fs::remove_dir_all(&data);
}

#[tokio::test]
async fn unresolved_contract_defers_direct_data_tools_until_revised() {
    let ws = scratch("contract-gate-ws");
    let data = scratch("contract-gate-data");
    fs::write(ws.join("sales.csv"), "amount\n10\n20\n").unwrap();

    // Deliberately put an unresolved contract and a runnable SQL call in the
    // same model response. The SQL is deferred until the model has seen the
    // grounded interpretation, so a semantic correction can influence it.
    let (url, server) = fake_openai(vec![
        openai_response(serde_json::json!({
            "role": "assistant",
            "content": "I need to resolve the period first.",
            "tool_calls": [
                { "id": "sql", "type": "function", "function": {
                    "name": "run_sql",
                    "arguments": "{\"sql\":\"SELECT sum(amount) AS total FROM sales\"}"
                } },
                { "id": "contract", "type": "function", "function": {
                    "name": "__analysis_contract",
                    "arguments": serde_json::json!({
                        "interpretation": "assumed",
                        "measures": [{ "concept": "amount", "field": "amount", "operation": "sum" }],
                        "unresolved": ["which period field should be used"]
                    }).to_string()
                } }
            ]
        })),
        openai_response(serde_json::json!({
            "role": "assistant",
            "content": "I will use the available sales table and compute the total.",
            "tool_calls": [{ "id": "sql-2", "type": "function", "function": {
                "name": "run_sql",
                "arguments": "{\"sql\":\"SELECT sum(amount) AS total FROM sales\"}"
            } }]
        })),
        openai_response(serde_json::json!({
            "role": "assistant",
            "content": "The sales total is 30."
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

    let events: Arc<Mutex<Vec<AskEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = events.clone();
    let answer = engine
        .ask(
            "contract-gate",
            "compare sales over time",
            None,
            move |event| sink.lock().unwrap().push(event),
        )
        .await
        .unwrap();
    server.join().unwrap();

    assert!(answer.text.contains("30"), "answer: {}", answer.text);
    assert_eq!(answer.evidence.len(), 1);
    assert!(answer.evidence[0]
        .sql
        .as_deref()
        .is_some_and(|sql| sql.contains("sum(amount)")));
    assert_eq!(answer.status, VerificationStatus::Verified);
    assert_eq!(
        answer.contract.as_ref().unwrap().interpretation,
        fella_lib::engine::runtime::InterpretationStatus::Ambiguous
    );
    let events = events.lock().unwrap();
    assert!(events.iter().any(|event| matches!(
        event,
        AskEvent::ToolStart { tool, .. } if tool == "run_sql"
    )));

    let _ = fs::remove_dir_all(&ws);
    let _ = fs::remove_dir_all(&data);
}

#[tokio::test]
async fn ambiguous_semantics_can_inspect_before_clarifying_and_never_query() {
    let ws = scratch("clarification-ws");
    let data = scratch("clarification-data");
    fs::write(
        ws.join("transactions.csv"),
        "description,area,amount\nsalary,income,5000\nrent,housing,1500\n",
    )
    .unwrap();

    let (url, seen, server) = fake_openai_with_requests(vec![
        openai_response(serde_json::json!({
            "role": "assistant",
            "content": "I should inspect the transaction labels before deciding whether to ask.",
            "tool_calls": [
                { "id": "contract", "type": "function", "function": {
                    "name": "__analysis_contract",
                    "arguments": serde_json::json!({
                        "interpretation": "assumed",
                        "subject": "transactions",
                        "measures": [{ "concept": "amount", "field": "amount", "operation": "sum" }],
                        "filters": [],
                        "group_by": [],
                        "assumptions": [],
                        "unresolved": [],
                        "clarification": {
                            "question": "Should this total include income, or only spending?",
                            "options": ["Include income", "Spending only"],
                            "reason": "The mounted data contains both income and spending rows."
                        }
                    }).to_string()
                } },
                { "id": "inspect", "type": "function", "function": {
                    "name": "inspect_table",
                    "arguments": "{\"name\":\"transactions\",\"rows\":5}"
                } }
            ]
        })),
        openai_response(serde_json::json!({
            "role": "assistant",
            "content": "The inspection shows income and housing rows. Should the total include income, or should it count spending only?"
        })),
        openai_response(serde_json::json!({
            "role": "assistant",
            "content": "I will calculate the spending-only population.",
            "tool_calls": [{ "id": "spending", "type": "function", "function": {
                "name": "run_sql",
                "arguments": "{\"sql\":\"SELECT SUM(amount) AS spending_total FROM transactions WHERE lower(area) <> 'income'\"}"
            } }]
        })),
        openai_response(serde_json::json!({
            "role": "assistant",
            "content": "Spending only totals $1,500."
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

    let events: Arc<Mutex<Vec<AskEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = events.clone();
    let answer = engine
        .ask(
            "clarification",
            "What is the amount in the current export?",
            None,
            move |event| sink.lock().unwrap().push(event),
        )
        .await
        .unwrap();

    assert_eq!(answer.status, VerificationStatus::InsufficientData);
    assert_eq!(answer.evidence.len(), 1);
    assert_eq!(answer.evidence[0].tool, "inspect_table");
    assert!(answer.evidence[0]
        .output
        .as_deref()
        .is_some_and(|output| output.contains("common values") && output.contains("income")));
    let clarification = answer.clarification.as_ref().unwrap();
    assert_eq!(clarification.options, ["Include income", "Spending only"]);
    assert!(answer.text.contains("include income"));
    assert!(!events.lock().unwrap().iter().any(|event| matches!(
        event,
        AskEvent::ToolStart { tool, .. } if tool == "run_sql"
    )));

    let continued = engine
        .ask("clarification", "Spending only", None, |_| {})
        .await
        .unwrap();
    assert!(
        continued.text.contains("1,500"),
        "answer: {}",
        continued.text
    );
    assert_eq!(continued.evidence.len(), 1);
    assert_eq!(continued.evidence[0].tool, "run_sql");
    server.join().unwrap();

    let requests = seen.lock().unwrap();
    assert_eq!(requests.len(), 4);
    let investigation_tools = requests[1]["tools"].as_array().unwrap();
    let investigation_names = investigation_tools
        .iter()
        .filter_map(|tool| tool["function"]["name"].as_str())
        .collect::<Vec<_>>();
    assert!(investigation_names.contains(&"inspect_table"));
    assert!(investigation_names.contains(&"read_file"));
    assert!(!investigation_names.contains(&"run_sql"));
    assert!(!investigation_names.contains(&"run_python"));
    assert!(!investigation_names.contains(&"make_chart"));
    let continuation_prompt = requests[2].to_string();
    assert!(continuation_prompt.contains("What is the amount in the current export?"));
    assert!(continuation_prompt.contains("include income"));
    assert!(continuation_prompt.contains("Spending only"));
    assert!(requests[2]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .any(|tool| tool["function"]["name"] == "run_sql"));

    let _ = fs::remove_dir_all(&ws);
    let _ = fs::remove_dir_all(&data);
}

#[tokio::test]
async fn direct_data_calls_do_not_require_a_contract() {
    let ws = scratch("missing-contract-ws");
    let data = scratch("missing-contract-data");
    fs::write(ws.join("sales.csv"), "amount\n10\n20\n").unwrap();

    let (url, server) = fake_openai(vec![
        openai_response(serde_json::json!({
            "role": "assistant",
            "content": "I will check that.",
            "tool_calls": [{ "id": "sql", "type": "function", "function": {
                "name": "run_sql",
                "arguments": "{\"sql\":\"SELECT sum(amount) AS total FROM sales\"}"
            } }]
        })),
        openai_response(serde_json::json!({
            "role": "assistant",
            "content": "I need to state the analytical interpretation before checking the data."
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
        .ask("missing-contract", "compare sales over time", None, |_| {})
        .await
        .unwrap();
    server.join().unwrap();

    assert_eq!(answer.evidence.len(), 1);
    assert!(answer.evidence[0]
        .sql
        .as_deref()
        .is_some_and(|sql| sql.contains("sum(amount)")));
    assert_eq!(answer.status, VerificationStatus::Verified);
    assert!(answer.contract.is_none());

    let _ = fs::remove_dir_all(&ws);
    let _ = fs::remove_dir_all(&data);
}

#[tokio::test]
async fn analyst_can_inspect_labels_before_selecting_a_computation() {
    let ws = scratch("analyst-inspection-ws");
    let data = scratch("analyst-inspection-data");
    fs::write(
        ws.join("bank.csv"),
        "date,flow,details,amount\n2024-01-02,out,Monthly lease,1200\n2024-02-02,out,Monthly lease,1200\n2024-03-02,out,Utilities,100\n2024-03-15,in,Payroll,5000\n",
    )
    .unwrap();

    let (url, seen, server) = fake_openai_with_requests(vec![
        openai_response(serde_json::json!({
            "role": "assistant",
            "content": "I will inspect the transaction labels first.",
            "tool_calls": [{ "id": "inspect", "type": "function", "function": {
                "name": "inspect_table",
                "arguments": "{\"name\":\"bank\",\"rows\":5}"
            } }]
        })),
        openai_response(serde_json::json!({
            "role": "assistant",
            "content": "The lease rows represent rent. I will total those in Q1.",
            "tool_calls": [{ "id": "sql", "type": "function", "function": {
                "name": "run_sql",
                "arguments": "{\"sql\":\"SELECT SUM(amount) AS rent_total FROM bank WHERE date >= '2024-01-01' AND date < '2024-04-01' AND lower(details) LIKE '%lease%'\"}"
            } }]
        })),
        openai_response(serde_json::json!({
            "role": "assistant",
            "content": "Rent paid in Q1 2024 was $2,400."
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
        .ask(
            "analyst-inspection",
            "How much did I pay for rent in the first quarter of 2024?",
            None,
            |_| {},
        )
        .await
        .unwrap();
    server.join().unwrap();

    assert!(answer.text.contains("2,400"), "answer: {}", answer.text);
    assert_eq!(
        answer
            .evidence
            .iter()
            .map(|item| item.tool.as_str())
            .collect::<Vec<_>>(),
        ["inspect_table", "run_sql"]
    );
    assert!(answer.evidence[0]
        .output
        .as_deref()
        .is_some_and(|output| output.contains("Monthly lease")));
    let requests = seen.lock().unwrap();
    let first_system = requests[0]["messages"][0]["content"].as_str().unwrap_or("");
    assert!(first_system.contains("Analyst loop"));
    assert!(first_system.contains("optional, not a prerequisite"));
    assert!(requests[1].to_string().contains("Monthly lease"));

    let _ = fs::remove_dir_all(&ws);
    let _ = fs::remove_dir_all(&data);
}

#[tokio::test]
async fn model_selected_data_call_waits_for_the_contract_before_execution() {
    let ws = scratch("compiled-plan-gate-ws");
    let data = scratch("compiled-plan-gate-data");
    fs::write(
        ws.join("sales.csv"),
        "month,amount\n2024-01,10\n2024-02,20\n2024-03,30\n",
    )
    .unwrap();

    let (url, server) = fake_openai(vec![
        openai_response(serde_json::json!({
            "role": "assistant",
            "content": "I will compare the monthly totals.",
            "tool_calls": [
                { "id": "model-sql", "type": "function", "function": {
                    "name": "run_sql",
                    "arguments": "{\"sql\":\"SELECT month, SUM(amount) AS total FROM sales GROUP BY month\"}"
                } },
                { "id": "contract", "type": "function", "function": {
                    "name": "__analysis_contract",
                    "arguments": serde_json::json!({
                        "interpretation": "assumed",
                        "measures": [{ "concept": "amount", "field": "amount", "operation": "sum" }],
                        "time": { "field": "month", "bucket": "month" }
                    }).to_string()
                } }
            ]
        })),
        openai_response(serde_json::json!({
            "role": "assistant",
            "content": "I will use the grounded monthly interpretation.",
            "tool_calls": [{ "id": "model-sql-2", "type": "function", "function": {
                "name": "run_sql",
                "arguments": "{\"sql\":\"SELECT month, SUM(amount) AS total FROM sales GROUP BY month\"}"
            } }]
        })),
        openai_response(serde_json::json!({
            "role": "assistant",
            "content": "Monthly sales were 10, 20, and 30."
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

    let context_refs = vec![fella_lib::engine::ContextReference {
        kind: "source".into(),
        key: ws.join("sales.csv").to_string_lossy().into_owned(),
        label: "sales.csv".into(),
        detail: None,
    }];
    let events: Arc<Mutex<Vec<AskEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = events.clone();
    let answer = engine
        .ask_with_mode_and_context(
            "compiled-plan-gate",
            "show sales by month",
            None,
            false,
            &context_refs,
            move |event| sink.lock().unwrap().push(event),
        )
        .await
        .unwrap();
    server.join().unwrap();

    assert_eq!(answer.evidence.len(), 1);
    assert_eq!(answer.evidence[0].tool, "run_sql");
    assert!(answer.evidence[0]
        .sql
        .as_deref()
        .unwrap()
        .contains("GROUP BY month"));
    assert_eq!(
        answer.grounding.as_ref().unwrap().probes[0].kind,
        "context_source"
    );
    let sql_starts = events
        .lock()
        .unwrap()
        .iter()
        .filter(|event| matches!(event, AskEvent::ToolStart { tool, .. } if tool == "run_sql"))
        .count();
    assert_eq!(sql_starts, 1, "one model-selected computation is executed");

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
        .ask("chart-c1", "Make a chart of sales.", None, |_| {})
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

    let events: Arc<Mutex<Vec<AskEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = events.clone();
    let answer = engine
        .ask("c1", "total?", None, move |event| {
            sink.lock().unwrap().push(event)
        })
        .await
        .unwrap();

    assert_eq!(answer.evidence.len(), 1, "evidence was kept");
    assert_eq!(answer.evidence[0].tool, "run_sql");
    assert!(
        answer.text.contains("gathered so far"),
        "text: {}",
        answer.text
    );
    assert!(events.lock().unwrap().iter().any(|event| matches!(
        event,
        AskEvent::TurnState {
            state: fella_lib::engine::runtime::TurnState::Retry,
            ..
        }
    )));

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
