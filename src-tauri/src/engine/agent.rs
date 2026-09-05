//! The reasoning loop. Ask the model; if it calls tools, run them
//! (deterministically) and feed results back; otherwise verify and return.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::engine::error::{EngineError, EngineResult};
use crate::engine::evidence::{Answer, AskEvent, EvidenceItem};
use crate::engine::llm::{ChatMessage, LlmClient, ToolCall};
use crate::engine::state::EngineState;
use crate::engine::tools::Registry;
use crate::engine::{verify, Catalog};

/// Hard cap on tool-calling iterations per question, before the loop forces
/// a final answer. `FELLA_MAX_STEPS` overrides it a slower or less
/// tool-efficient model may need more room than the default before it's
/// confident enough to stop calling tools.
const MAX_STEPS: usize = 20;

fn max_steps() -> usize {
    super::env::positive("FELLA_MAX_STEPS", MAX_STEPS)
}

/// Resolves once `flag` is set used to race against `llm.chat`.
async fn cancelled(flag: &AtomicBool) {
    while !flag.load(Ordering::Relaxed) {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

pub async fn run(
    engine: &EngineState,
    llm: &LlmClient,
    registry: &Registry,
    conversation_id: &str,
    question: &str,
    cancel: &AtomicBool,
    emit: &(dyn Fn(AskEvent) + Send + Sync),
) -> EngineResult<Answer> {
    let catalog = engine.catalog();
    let user_context = engine.user_context();
    let schema = engine.schema_block();
    let recent = engine.session_block(conversation_id);
    let mut sys = system_prompt(&catalog, &user_context, &schema, recent.as_deref());
    if registry.has_mcp() {
        sys.push_str(
            "\nSome tools are named `connector__tool` these reach an outside service \
the user connected (e.g. their notes). Use one when the question is about that \
service. Its result is a tool result like any other still never state a figure \
you did not get from a tool.\n",
        );
    }
    let mut messages = vec![
        ChatMessage::System(sys),
        ChatMessage::User(question.to_string()),
    ];
    // With no folder open there is nothing to compute don't hand the model
    // tools it can only fail to call. This keeps a plain "hello" (or "what can
    // you do?") to a single fast turn instead of a many-step loop of
    // NoWorkspace errors, which on a small local model can take minutes.
    let schemas = if catalog.workspace.is_some() || registry.has_mcp() {
        registry.schemas()
    } else {
        Vec::new()
    };
    let mut evidence: Vec<EvidenceItem> = Vec::new();

    // Forward a retry/backoff line from the model client to the transcript.
    let notify = |line: &str| emit(AskEvent::Notice { text: line.to_string() });
    // Forward each token as the model streams it.
    let on_delta = |text: &str| emit(AskEvent::AssistantDelta { text: text.to_string() });

    // Exact `(tool, args)` pairs already run this question, mapped to the result
    // text we fed back. A small model re-issuing the same call is a common way
    // to burn the budget; we answer it from here instead of re-running the tool,
    // and re-supply the result inline (it may have since been elided from the
    // history by `trim_history`).
    let mut seen_calls: HashMap<(String, String), String> = HashMap::new();

    let run_start = Instant::now();
    let mut model_calls = 0usize;
    let mut tool_calls_total = 0usize;
    let steps = max_steps();
    for step in 0..steps {
        log::info!("agent step {}/{steps}", step + 1);
        let step_start = Instant::now();

        // Race the model call against a stop request; dropping the future
        // closes the HTTP connection so the model stops generating.
        let resp = tokio::select! {
            r = llm.chat(&messages, &schemas, &notify, &on_delta) => match r {
                Ok(resp) => resp,
                // Failed with work already in hand: hand back the partial
                // evidence and a note rather than losing the whole question.
                Err(e) if !evidence.is_empty() => {
                    log::warn!("agent: model call failed mid-run: {e}");
                    return Ok(finish(
                        engine,
                        format!(
                            "I couldn't finish the model call failed ({e}). \
                             Here's what I gathered so far."
                        ),
                        evidence,
                        emit,
                    ));
                }
                Err(e) => return Err(e),
            },
            _ = cancelled(cancel) => return Ok(stopped(engine, evidence, emit)),
        };
        model_calls += 1;

        if resp.tool_calls.is_empty() {
            log::info!(
                "agent run: {:?}, {model_calls} model call(s), {tool_calls_total} tool call(s), {} evidence",
                run_start.elapsed(),
                evidence.len()
            );
            // A model that returns neither text nor a tool call would otherwise
            // leave a blank reply. Give the user something to act on.
            let text = if resp.content.trim().is_empty() && evidence.is_empty() {
                "The model returned an empty reply. Try rephrasing the question, or switch model \
                 with /model."
                    .to_string()
            } else {
                resp.content
            };
            return Ok(finish(engine, text, evidence, emit));
        }
        tool_calls_total += resp.tool_calls.len();

        // `resp.content` (any "let me check…" preamble before the tool calls)
        // was already streamed through `on_delta`; just keep it in the history.
        messages.push(ChatMessage::Assistant {
            content: resp.content.clone(),
            tool_calls: resp.tool_calls.clone(),
        });

        for call in &resp.tool_calls {
            emit(AskEvent::ToolStart {
                tool: call.name.clone(),
                args: call.arguments.clone(),
            });
        }

        // Resolve exact-repeat calls from the memo synchronously (it needs
        // `&mut seen_calls`), then run the rest of this turn's calls
        // concurrently and stitch the results back in call order.
        let mut outcomes: Vec<Option<(EvidenceItem, String)>> =
            (0..resp.tool_calls.len()).map(|_| None).collect();
        let mut pending: Vec<usize> = Vec::new();
        for (i, call) in resp.tool_calls.iter().enumerate() {
            let key = (call.name.clone(), call.arguments.to_string());
            let dup = (!call.name.contains("__"))
                .then(|| seen_calls.get(&key))
                .flatten()
                .cloned();
            match dup {
                Some(prev) => {
                    let msg = format!(
                        "NOTE: this exact `{}` call was already made for this question, so it was \
not run again. Its result is repeated below - use it, refine the call, or give your answer now.\n\n{prev}",
                        call.name
                    );
                    outcomes[i] = Some((
                        EvidenceItem {
                            tool: call.name.clone(),
                            args: call.arguments.clone(),
                            note: note_of(&call.arguments),
                            sql: None,
                            result_summary: "skipped (duplicate call)".to_string(),
                            columns: None,
                            rows: None,
                            row_count: None,
                            output: None,
                            ms: 0,
                            error: None,
                        },
                        msg,
                    ));
                }
                None => pending.push(i),
            }
        }

        let ran = futures_util::future::join_all(
            pending.iter().map(|&i| run_tool_call(engine, registry, &resp.tool_calls[i])),
        )
        .await;
        for (&i, res) in pending.iter().zip(ran) {
            outcomes[i] = Some(res);
        }

        for (call, outcome) in resp.tool_calls.iter().zip(outcomes) {
            // Every slot is filled above (dup branch or the `pending`/`ran` zip);
            // treat a gap as a broken invariant that ends the run cleanly.
            let (item, llm_text) = outcome
                .ok_or_else(|| EngineError::msg("internal error: a tool call produced no outcome"))?;
            emit(AskEvent::ToolEnd { item: item.clone() });
            // Remember a fresh, successful built-in result so a later exact
            // repeat is answered from the memo rather than re-run.
            if !call.name.contains("__")
                && item.error.is_none()
                && item.result_summary != "skipped (duplicate call)"
            {
                let key = (call.name.clone(), call.arguments.to_string());
                seen_calls.insert(key, llm_text.clone());
            }
            evidence.push(item);
            messages.push(ChatMessage::Tool {
                call_id: call.id.clone(),
                name: call.name.clone(),
                content: llm_text,
            });
        }

        if cancel.load(Ordering::Relaxed) {
            return Ok(stopped(engine, evidence, emit));
        }
        trim_history(&mut messages);
        log::info!(
            "agent step {}/{steps} done in {:?} ({} tool call(s))",
            step + 1,
            step_start.elapsed(),
            resp.tool_calls.len()
        );
    }

    // Out of steps: one last turn with no tools, telling the model plainly
    // why so it writes a real (possibly hedged) answer instead of confused
    // or empty output. The canned fallback below stays as the last-resort
    // case (model call fails, or it still returns nothing).
    messages.push(ChatMessage::User(
        "You're out of tool-calling steps for this question. Don't call any more \
tools give your best answer now, using only what you've already found. If \
you're not confident, say so plainly rather than guessing."
            .to_string(),
    ));
    let resp = tokio::select! {
        r = llm.chat(&messages, &[], &notify, &on_delta) => r.unwrap_or_default(),
        _ = cancelled(cancel) => return Ok(stopped(engine, evidence, emit)),
    };
    model_calls += 1;
    let text = if resp.content.trim().is_empty() {
        "I ran out of analysis steps before reaching a confident answer.".to_string()
    } else {
        resp.content
    };
    log::info!(
        "agent run: {:?}, {model_calls} model call(s), {tool_calls_total} tool call(s), {} evidence (hit step cap)",
        run_start.elapsed(),
        evidence.len()
    );
    Ok(finish(engine, text, evidence, emit))
}

fn stopped(
    engine: &EngineState,
    evidence: Vec<EvidenceItem>,
    emit: &(dyn Fn(AskEvent) + Send + Sync),
) -> Answer {
    finish(engine, "Stopped.".to_string(), evidence, emit)
}

fn finish(
    engine: &EngineState,
    text: String,
    evidence: Vec<EvidenceItem>,
    emit: &(dyn Fn(AskEvent) + Send + Sync),
) -> Answer {
    log::info!(
        "agent done: {} char answer, {} evidence item(s)",
        text.len(),
        evidence.len()
    );
    let verification = verify::run(engine, &text, &evidence);
    let answer = Answer {
        text,
        evidence,
        verification,
    };
    emit(AskEvent::AnswerDone {
        answer: answer.clone(),
    });
    answer
}

/// A SQL failure the model can fix if we remind it of the real schema.
fn is_schema_error(msg: &str) -> bool {
    let l = msg.to_lowercase();
    l.contains("no such column")
        || l.contains("no such table")
        || l.contains("no such function")
        || l.contains("ambiguous column name")
}

/// Placeholder swapped in for stale tool results once the history gets long,
/// so a small model isn't re-reading every earlier table on every turn.
const ELIDED: &str = "[earlier result elided re-query if you still need it]";

/// Keep the last few tool results verbatim; blank out the older ones.
fn trim_history(messages: &mut [ChatMessage]) {
    let tool_idx: Vec<usize> = messages
        .iter()
        .enumerate()
        .filter(|(_, m)| matches!(m, ChatMessage::Tool { .. }))
        .map(|(i, _)| i)
        .collect();
    if tool_idx.len() <= 6 {
        return;
    }
    for &i in &tool_idx[..tool_idx.len() - 6] {
        if let ChatMessage::Tool { content, .. } = &mut messages[i] {
            if content != ELIDED {
                *content = ELIDED.to_string();
            }
        }
    }
}

/// Pull the model's plain-language `note` off a tool call, if it wrote one.
fn note_of(args: &serde_json::Value) -> Option<String> {
    args.get("note")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
}

/// Run one tool call to completion, producing its evidence item and the text
/// fed back to the model. Pure (no `&mut` state) so a turn's calls can run
/// through `join_all` concurrently.
async fn run_tool_call(
    engine: &EngineState,
    registry: &Registry,
    call: &ToolCall,
) -> (EvidenceItem, String) {
    let started = Instant::now();
    match registry.run(engine, &call.name, &call.arguments).await {
        Some(Ok(out)) => {
            let item = EvidenceItem {
                tool: call.name.clone(),
                args: call.arguments.clone(),
                note: note_of(&call.arguments),
                sql: out.sql,
                result_summary: out.summary,
                columns: out.columns,
                rows: out.rows,
                row_count: out.row_count,
                output: out.output,
                ms: started.elapsed().as_millis() as u64,
                error: None,
            };
            (item, out.llm_text)
        }
        Some(Err(e)) => {
            let mut msg = e.to_string();
            if call.name == "run_sql" && is_schema_error(&msg) {
                let schema = engine.schema_oneline();
                if !schema.trim().is_empty() {
                    msg = format!("{msg}\n\nTables in this workspace:\n{schema}");
                }
            }
            tool_error(&call.name, &call.arguments, msg, started)
        }
        None => tool_error(
            &call.name,
            &call.arguments,
            format!("no such tool `{}`", call.name),
            started,
        ),
    }
}

fn tool_error(
    name: &str,
    args: &serde_json::Value,
    message: String,
    started: Instant,
) -> (EvidenceItem, String) {
    (
        EvidenceItem {
            tool: name.to_string(),
            args: args.clone(),
            note: note_of(args),
            sql: None,
            result_summary: format!("error: {message}"),
            columns: None,
            rows: None,
            row_count: None,
            output: None,
            ms: started.elapsed().as_millis() as u64,
            error: Some(message.clone()),
        },
        format!("ERROR: {message}"),
    )
}

fn system_prompt(
    catalog: &Catalog,
    user_context: &[String],
    schema: &str,
    recent: Option<&str>,
) -> String {
    let dialect = if cfg!(feature = "duckdb") { "DuckDB" } else { "SQLite" };
    let steps = max_steps();
    let mut p = format!(
        "You are Fella, a careful data analyst. You answer questions about the \
user's local files by calling tools that run real computations.\n\n\
Rules:\n\
- Never state a figure (number, total, count, date range, trend) you did not \
get from a tool result. A question that asks for a total, count, average, \
share, min/max, or \"how much / how many\" ALWAYS needs a run_sql call; the \
sample rows below are not enough to compute one.\n\
- Answer with only the figures a tool returned. Don't add row counts, rounded \
or approximate numbers, or restate the query; the evidence panel shows the \
working. Lead with the answer; keep it to a sentence or two, or a small table \
only when it genuinely helps.\n\
- Before your first tool call, write one short plain sentence of what you're \
about to do, then make the call(s) in the same reply.\n\
- Prefer run_sql. Each table below shows its columns, types and sample rows, \
usually enough to query directly. Use describe_schema or sample_rows only for \
something you can't see below.\n\
- Independent lookups go in one reply as several tool calls; they run together.\n\
- Stop as soon as you can answer. Most questions are one or two run_sql calls; \
you have at most {steps} tool-calling steps, so don't wander past the question.\n\
- {dialect} SQL, one SELECT / WITH per call. Dates are ISO-8601 text, so use \
strftime()/date() (e.g. strftime('%Y-%m', d)).\n\
- run_python for stats SQL can't do (median, correlation, regression); it has \
a sql() helper.\n\
- Documents (notes, PDFs) are already listed below with their names and first \
line, so don't call list_files for them. For a question about their content, \
call read_file directly (pass `names: [...]` to read several at once); they are \
short. Use grep_files only to locate one specific term across many documents.\n\
- If the files can't answer a data question, say so plainly; don't guess.\n\
- A definition or plain \"what does X mean\" needs no tool. You may add one \
confident sentence of general background on its own line starting with \
`Background:`, with no specific figures in it. If unsure, say so.\n\
- You may pass a short `note` (4-8 plain words) on a tool call for the activity \
display, e.g. \"Add up spending by month\".\n\n"
    );

    if !user_context.is_empty() {
        p.push_str(
            "Your context, written by the user (fella.md) and any skills they enabled. \
Use it for the user's vocabulary, how their files are organised, and caveats to \
apply. It is background, not data: never take a figure from it.\n",
        );
        for c in user_context {
            p.push_str("---\n");
            p.push_str(c.trim());
            p.push('\n');
        }
        p.push_str("---\n\n");
    }

    match &catalog.workspace {
        Some(ws) => p.push_str(&format!("Workspace: {ws}\n")),
        None => {
            p.push_str("No workspace is open yet; tell the user to run /open <folder>.\n");
            return p;
        }
    }

    p.push_str(schema);

    if let Some(recent) = recent {
        p.push('\n');
        p.push_str(recent);
    }

    p
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::llm::ToolCall;

    fn open_catalog() -> Catalog {
        Catalog {
            workspace: Some("/tmp/ws".into()),
            sources: Vec::new(),
            skipped: Vec::new(),
        }
    }

    #[test]
    fn prompt_carries_schema_and_recent_turns() {
        let schema = "Tables (columns and types shown; use sample_rows for values):\n  ledger  (12 rows)\n    \"Amount Paid\" REAL  [coerced]\n";
        let recent = "Earlier in this conversation (reuse what still applies):\n- Q: \"total?\"  A: \"$4,850\"\n  used: SELECT SUM(\"Amount Paid\") FROM ledger\n";
        let p = system_prompt(&open_catalog(), &[], schema, Some(recent));
        assert!(p.contains("\"Amount Paid\" REAL  [coerced]"));
        assert!(p.contains("Earlier in this conversation"));
        assert!(p.contains("SELECT SUM(\"Amount Paid\") FROM ledger"));

        // No recent block on the first turn.
        let p0 = system_prompt(&open_catalog(), &[], schema, None);
        assert!(!p0.contains("Earlier in this conversation"));
    }

    #[test]
    fn schema_error_is_recognised() {
        assert!(is_schema_error("no such column: amount"));
        assert!(is_schema_error("Query error: no such table: ledgr"));
        assert!(!is_schema_error("query stopped after 15 s"));
    }

    #[test]
    fn trim_history_blanks_old_tool_results_only() {
        let mut msgs = vec![ChatMessage::System("sys".into()), ChatMessage::User("q".into())];
        for i in 0..9 {
            msgs.push(ChatMessage::Assistant {
                content: String::new(),
                tool_calls: vec![ToolCall {
                    id: format!("{i}"),
                    name: "run_sql".into(),
                    arguments: serde_json::json!({}),
                }],
            });
            msgs.push(ChatMessage::Tool {
                call_id: format!("{i}"),
                name: "run_sql".into(),
                content: format!("result {i}"),
            });
        }
        trim_history(&mut msgs);
        let tool_contents: Vec<&str> = msgs
            .iter()
            .filter_map(|m| match m {
                ChatMessage::Tool { content, .. } => Some(content.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(tool_contents.iter().filter(|c| **c == ELIDED).count(), 3);
        assert_eq!(tool_contents.last(), Some(&"result 8"));
        // Non-tool messages untouched.
        assert!(matches!(&msgs[0], ChatMessage::System(s) if s == "sys"));
    }
}
