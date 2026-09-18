//! The reasoning loop. Ask the model; if it calls tools, run them
//! (deterministically) and feed results back; otherwise verify and return.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::engine::analytics::{provenance, verify};
use crate::engine::error::{EngineError, EngineResult};
use crate::engine::evidence::{
    Answer, AskEvent, EvidenceItem, Usage, VerificationCheck, WorkspaceSnapshot,
};
use crate::engine::llm::{ChatMessage, LlmClient, ToolCall};
use crate::engine::state::EngineState;
use crate::engine::tools::Registry;
use crate::engine::{friction, Catalog};

/// Hard cap on tool-calling iterations per question, before the loop forces
/// a final answer. `FELLA_MAX_STEPS` overrides it a slower or less
/// tool-efficient model may need more room than the default before it's
/// confident enough to stop calling tools.
const MAX_STEPS: usize = 20;

fn max_steps() -> usize {
    super::env::positive("FELLA_MAX_STEPS", MAX_STEPS)
}

/// Round trips above which a run gets a live nudge to wrap up.
/// docs/HARNESS-COMPARISON.md targets <= 2 round trips/answer; this sits one
/// step above that so a normal two-round answer is never touched, only a run
/// that's already run past it. `FELLA_SOFT_STOP` overrides it for eval sweeps.
const SOFT_STOP_ROUND_TRIPS: usize = 3;

fn soft_stop_round_trips() -> usize {
    super::env::positive("FELLA_SOFT_STOP", SOFT_STOP_ROUND_TRIPS)
}

/// A live nudge for the round trip about to start, once a run has already
/// made `soft_threshold` or more tool-calling round trips escalates in tone
/// the further past it a run gets, rather than waiting for the hard
/// `max_steps` ceiling or hoping the static "stop early" prompt rule takes.
/// `None` below the threshold. Pure and testable without a real run, same
/// pattern as `trim_history`.
fn stop_pressure_nudge(
    rounds_done: usize,
    soft_threshold: usize,
    evidence_count: usize,
) -> Option<String> {
    if rounds_done < soft_threshold {
        return None;
    }
    let overage = rounds_done - soft_threshold + 1;
    Some(if overage == 1 {
        format!(
            "You've made {evidence_count} tool call(s) so far most questions need at most \
2. If you already have enough to answer, do so now rather than gathering more."
        )
    } else {
        format!(
            "You've made {evidence_count} tool call(s) well past what this kind of question \
usually needs. Unless something you truly need is still missing, stop and answer now with \
what you have don't keep exploring."
        )
    })
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
    cancel: Arc<AtomicBool>,
    emit: &(dyn Fn(AskEvent) + Send + Sync),
) -> EngineResult<Answer> {
    let catalog = engine.catalog();
    let workspace = match (&catalog.workspace, &catalog.revision) {
        (Some(path), Some(revision)) => Some(WorkspaceSnapshot {
            path: path.clone(),
            revision: revision.clone(),
        }),
        _ => None,
    };
    let user_context = engine.user_context();
    let schema = engine.schema_block();
    let recent = engine.session_block(conversation_id);
    let learned = engine.folder_memory_block();
    let mut sys = system_prompt(
        &PromptProfile::from_env(),
        &catalog,
        &user_context,
        &schema,
        recent.as_deref(),
        learned.as_deref(),
    );
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
    // NoWorkspace errors, which can take minutes on a slow provider.
    let schemas = if catalog.workspace.is_some() || registry.has_mcp() {
        registry.schemas()
    } else {
        Vec::new()
    };
    let mut evidence: Vec<EvidenceItem> = Vec::new();

    // Forward a retry/backoff line from the model client to the transcript.
    let notify = |line: &str| {
        emit(AskEvent::Notice {
            text: line.to_string(),
        })
    };
    // Forward each token as the model streams it.
    let on_delta = |text: &str| {
        emit(AskEvent::AssistantDelta {
            text: text.to_string(),
        })
    };

    // Exact `(tool, args)` pairs already run this question, mapped to the result
    // text we fed back. A small model re-issuing the same call is a common way
    // to burn the budget; we answer it from here instead of re-running the tool,
    // and re-supply the result inline (it may have since been elided from the
    // history by `trim_history`).
    let mut seen_calls: HashMap<(String, String), String> = HashMap::new();

    let run_start = Instant::now();
    let mut model_calls = 0usize;
    let mut tool_calls_total = 0usize;
    let mut usage: Option<Usage> = None;
    let steps = max_steps();
    let soft_stop = soft_stop_round_trips();
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
                        workspace.as_ref(),
                        question,
                        format!(
                            "I couldn't finish the model call failed ({e}). \
                             Here's what I gathered so far."
                        ),
                        evidence,
                        usage,
                        emit,
                    ));
                }
                Err(e) => return Err(e),
            },
            _ = cancelled(cancel.as_ref()) => {
                return Ok(stopped(engine, workspace.as_ref(), question, evidence, usage, emit))
            }
        };
        model_calls += 1;
        usage = Usage::merge(usage, resp.usage);

        if resp.tool_calls.is_empty() {
            log::info!(
                "agent run: {:?}, {model_calls} model call(s), {tool_calls_total} tool call(s), {} evidence",
                run_start.elapsed(),
                evidence.len()
            );
            // A model that returns neither text nor a tool call would otherwise
            // leave a blank reply. Give the user something to act on.
            let mut text = if resp.content.trim().is_empty() && evidence.is_empty() {
                "The model returned an empty reply. Try rephrasing the question, or switch model \
                 with /model."
                    .to_string()
            } else {
                resp.content
            };
            let mut checks = verify::run(engine, question, &text, &evidence);
            // One tool-free corrective turn when a cited query re-runs to a
            // different result (or no longer runs). The value the model
            // reconciles against comes from that re-run, so the answer stays
            // checkable. Only the re-run checks trigger this a fuzzier
            // "figure appears in no result" is left as a fold warning, since a
            // tool-free reconcile there tends to degrade a correct answer.
            // `FELLA_VERIFY_REASK=0` opts out.
            if reask_enabled() && !evidence.is_empty() && !cancel.load(Ordering::Relaxed) {
                if let Some(detail) = verify::rerun_regression(&checks) {
                    messages.push(ChatMessage::User(format!(
                        "Self-check: {detail}. Re-running the query behind your answer gives a \
different result now. Using only what you've already gathered no new tools give the \
corrected answer to match the re-run."
                    )));
                    let r = tokio::select! {
                        r = llm.chat(&messages, &[], &notify, &on_delta) => r.unwrap_or_default(),
                        _ = cancelled(cancel.as_ref()) => Default::default(),
                    };
                    usage = Usage::merge(usage, r.usage);
                    if !r.content.trim().is_empty() {
                        text = r.content;
                        checks = verify::run(engine, question, &text, &evidence);
                    }
                }
            }
            // Cost-gated self-consistency re-check (#80): the fallback tier for
            // whatever's left once the cheap checks above have already run and
            // fixed what they can. Only pays for a second, independent, no-tool
            // opinion when the checks above already left a warning standing -
            // never on a clean answer, so this can't touch the "<=2 round trips"
            // cost target on the common path. `FELLA_SELF_CHECK=0` opts out.
            if self_check_enabled()
                && !evidence.is_empty()
                && !cancel.load(Ordering::Relaxed)
                && checks.iter().any(|c| !c.ok)
            {
                messages.push(ChatMessage::User(
                    "Second opinion: answer this question again from scratch, using only the \
tool results already gathered. Be strict and literal use only run_sql figures, honour every \
filter word in the question exactly, and state just the number(s) don't round or estimate."
                        .to_string(),
                ));
                let r = tokio::select! {
                    r = llm.chat(&messages, &[], &notify, &on_delta) => r.unwrap_or_default(),
                    _ = cancelled(cancel.as_ref()) => Default::default(),
                };
                usage = Usage::merge(usage, r.usage);
                if let Some(check) = verify::self_consistency_check(&text, &r.content) {
                    checks.push(check);
                }
            }
            return Ok(finish_with(
                engine,
                workspace.as_ref(),
                text,
                evidence,
                usage,
                checks,
                emit,
            ));
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
                            id: String::new(),
                            tool: call.name.clone(),
                            sources: Vec::new(),
                            args: call.arguments.clone(),
                            note: note_of(&call.arguments),
                            sql: None,
                            result_summary: "skipped (duplicate call)".to_string(),
                            columns: None,
                            rows: None,
                            row_count: None,
                            output: None,
                            chart: None,
                            ms: 0,
                            error: None,
                        },
                        msg,
                    ));
                }
                None => pending.push(i),
            }
        }

        let ran = futures_util::future::join_all(pending.iter().map(|&i| {
            run_tool_call(
                engine,
                &catalog,
                registry,
                &resp.tool_calls[i],
                cancel.clone(),
            )
        }))
        .await;
        for (&i, res) in pending.iter().zip(ran) {
            outcomes[i] = Some(res);
        }

        for (call, outcome) in resp.tool_calls.iter().zip(outcomes) {
            // Every slot is filled above (dup branch or the `pending`/`ran` zip);
            // treat a gap as a broken invariant that ends the run cleanly.
            let (mut item, llm_text) = outcome.ok_or_else(|| {
                EngineError::msg("internal error: a tool call produced no outcome")
            })?;
            item.id = evidence_id(evidence.len());
            emit(AskEvent::ToolEnd {
                item: Box::new(item.clone()),
            });
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
            return Ok(stopped(
                engine,
                workspace.as_ref(),
                question,
                evidence,
                usage,
                emit,
            ));
        }
        trim_history(&mut messages);
        if let Some(nudge) = stop_pressure_nudge(step + 1, soft_stop, evidence.len()) {
            messages.push(ChatMessage::User(nudge));
        }
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
        _ = cancelled(cancel.as_ref()) => {
            return Ok(stopped(engine, workspace.as_ref(), question, evidence, usage, emit))
        }
    };
    model_calls += 1;
    usage = Usage::merge(usage, resp.usage);
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
    Ok(finish(
        engine,
        workspace.as_ref(),
        question,
        text,
        evidence,
        usage,
        emit,
    ))
}

fn stopped(
    engine: &EngineState,
    workspace: Option<&WorkspaceSnapshot>,
    question: &str,
    evidence: Vec<EvidenceItem>,
    usage: Option<Usage>,
    emit: &(dyn Fn(AskEvent) + Send + Sync),
) -> Answer {
    finish(
        engine,
        workspace,
        question,
        "Stopped.".to_string(),
        evidence,
        usage,
        emit,
    )
}

fn catalog_matches(engine: &EngineState, expected: &Catalog) -> bool {
    let current = engine.catalog();
    current.workspace == expected.workspace && current.revision == expected.revision
}

fn workspace_matches(engine: &EngineState, expected: Option<&WorkspaceSnapshot>) -> bool {
    let current = engine.catalog();
    match expected {
        Some(expected) => {
            current.workspace.as_deref() == Some(expected.path.as_str())
                && current.revision.as_deref() == Some(expected.revision.as_str())
        }
        None => current.workspace.is_none() && current.revision.is_none(),
    }
}

/// The corrective re-ask fires unless `FELLA_VERIFY_REASK=0`.
fn reask_enabled() -> bool {
    !matches!(std::env::var("FELLA_VERIFY_REASK").as_deref(), Ok("0"))
}

/// The self-consistency second opinion (#80) fires unless `FELLA_SELF_CHECK=0`.
fn self_check_enabled() -> bool {
    !matches!(std::env::var("FELLA_SELF_CHECK").as_deref(), Ok("0"))
}

fn finish(
    engine: &EngineState,
    workspace: Option<&WorkspaceSnapshot>,
    question: &str,
    text: String,
    evidence: Vec<EvidenceItem>,
    usage: Option<Usage>,
    emit: &(dyn Fn(AskEvent) + Send + Sync),
) -> Answer {
    let checks = verify::run(engine, question, &text, &evidence);
    finish_with(engine, workspace, text, evidence, usage, checks, emit)
}

fn finish_with(
    engine: &EngineState,
    workspace: Option<&WorkspaceSnapshot>,
    text: String,
    evidence: Vec<EvidenceItem>,
    usage: Option<Usage>,
    mut verification: Vec<VerificationCheck>,
    emit: &(dyn Fn(AskEvent) + Send + Sync),
) -> Answer {
    if !workspace_matches(engine, workspace) {
        verification.push(VerificationCheck {
            label: "workspace changed while this answer was running".into(),
            ok: false,
            detail: Some(
                "the data changed during this question, so its evidence may not describe the current workspace; ask again"
                    .into(),
            ),
        });
    }
    log::info!(
        "agent done: {} char answer, {} evidence item(s)",
        text.len(),
        evidence.len()
    );
    if let Some(reason) = friction::trigger(&verification, &evidence) {
        engine.record_friction_signal(reason, &evidence);
    }
    let status = verify::status(&verification, &evidence);
    let answer = Answer {
        text,
        evidence,
        verification,
        status,
        workspace: workspace.cloned(),
        usage,
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
    catalog: &Catalog,
    registry: &Registry,
    call: &ToolCall,
    cancel: Arc<AtomicBool>,
) -> (EvidenceItem, String) {
    let started = Instant::now();
    if !catalog_matches(engine, catalog) {
        return tool_error(
            &call.name,
            &call.arguments,
            "the workspace changed while this question was running; the tool call was not used ask again".into(),
            started,
        );
    }
    let result = registry
        .run_with_cancel(engine, &call.name, &call.arguments, cancel)
        .await;
    if !catalog_matches(engine, catalog) {
        return tool_error(
            &call.name,
            &call.arguments,
            "the workspace changed while this question was running; the tool result was discarded ask again".into(),
            started,
        );
    }
    match result {
        Some(Ok(out)) => {
            let sources = out
                .sql
                .as_deref()
                .map(|sql| provenance::for_sql(catalog, sql))
                .unwrap_or_default();
            let item = EvidenceItem {
                id: String::new(),
                tool: call.name.clone(),
                sources,
                args: call.arguments.clone(),
                note: note_of(&call.arguments),
                sql: out.sql,
                result_summary: out.summary,
                columns: out.columns,
                rows: out.rows,
                row_count: out.row_count,
                output: out.output,
                chart: out.chart,
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
            id: String::new(),
            tool: name.to_string(),
            sources: Vec::new(),
            args: args.clone(),
            note: note_of(args),
            sql: None,
            result_summary: format!("error: {message}"),
            columns: None,
            rows: None,
            row_count: None,
            output: None,
            chart: None,
            ms: started.elapsed().as_millis() as u64,
            error: Some(message.clone()),
        },
        format!("ERROR: {message}"),
    )
}

fn evidence_id(position: usize) -> String {
    format!("evidence-{}", position + 1)
}

/// Which sections of the system prompt to emit. `PromptProfile::full()` is the
/// prompt Fella ships; the eval harness flips flags to measure what each
/// section is worth. `core_rules` gates the three load-bearing rules
/// (never-untooled-figure, figures-only + shape, prefer-`run_sql`).
#[derive(Debug, Clone, Copy)]
pub struct PromptProfile {
    pub persona: bool,
    pub core_rules: bool,
    pub plan_rule: bool,
    pub parallel_rule: bool,
    pub stop_early_rule: bool,
    pub dialect_rule: bool,
    pub python_rule: bool,
    pub depth_rule: bool,
    pub aside_rule: bool,
    pub chart_rule: bool,
    pub docs_rule: bool,
    pub refuse_rule: bool,
    pub background_rule: bool,
    pub structure_rule: bool,
    pub note_rule: bool,
    pub opinion_rule: bool,
    pub user_context: bool,
    pub schema: bool,
    pub session_block: bool,
    /// The per-folder learned-notes block (`engine::memory`).
    pub folder_memory: bool,
}

impl PromptProfile {
    /// `full()`, minus any section named (comma-separated) in `FELLA_PROMPT_DROP`
    /// the eval harness's prompt-minimalism ablation sets this per run. Unset
    /// (the normal case) is exactly `full()`. Unknown names are ignored.
    pub fn from_env() -> Self {
        let mut p = Self::full();
        let Ok(drop) = std::env::var("FELLA_PROMPT_DROP") else {
            return p;
        };
        for name in drop.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            match name {
                "persona" => p.persona = false,
                "core_rules" => p.core_rules = false,
                "plan_rule" => p.plan_rule = false,
                "parallel_rule" => p.parallel_rule = false,
                "stop_early_rule" => p.stop_early_rule = false,
                "dialect_rule" => p.dialect_rule = false,
                "python_rule" => p.python_rule = false,
                "depth_rule" => p.depth_rule = false,
                "aside_rule" => p.aside_rule = false,
                "chart_rule" => p.chart_rule = false,
                "docs_rule" => p.docs_rule = false,
                "refuse_rule" => p.refuse_rule = false,
                "background_rule" => p.background_rule = false,
                "structure_rule" => p.structure_rule = false,
                "note_rule" => p.note_rule = false,
                "opinion_rule" => p.opinion_rule = false,
                "user_context" => p.user_context = false,
                "schema" => p.schema = false,
                "session_block" => p.session_block = false,
                "folder_memory" => p.folder_memory = false,
                _ => log::warn!("FELLA_PROMPT_DROP: unknown section {name:?}"),
            }
        }
        p
    }

    /// Exactly the prompt Fella ships today.
    pub fn full() -> Self {
        Self {
            persona: true,
            core_rules: true,
            plan_rule: true,
            parallel_rule: true,
            stop_early_rule: true,
            dialect_rule: true,
            python_rule: true,
            depth_rule: true,
            aside_rule: true,
            chart_rule: true,
            docs_rule: true,
            refuse_rule: true,
            background_rule: true,
            structure_rule: true,
            note_rule: true,
            opinion_rule: true,
            user_context: true,
            schema: true,
            session_block: true,
            folder_memory: true,
        }
    }
}

fn system_prompt(
    profile: &PromptProfile,
    catalog: &Catalog,
    user_context: &[String],
    schema: &str,
    recent: Option<&str>,
    learned: Option<&str>,
) -> String {
    let dialect = if cfg!(feature = "duckdb") {
        "DuckDB"
    } else {
        "SQLite"
    };
    let steps = max_steps();
    let mut p = String::new();

    if profile.persona {
        p.push_str(
            "You are Fella, a careful data analyst. You answer questions about the \
user's local files by calling tools that run real computations.\n\n",
        );
    }

    // Rules, in the shipped order; `core_rules` gates the non-contiguous set.
    let mut rules: Vec<String> = Vec::new();
    if profile.core_rules {
        rules.push(
            "Never state a figure (number, total, count, date range, trend) you did not \
get from a tool result. A question that asks for a total, count, average, \
share, min/max, or \"how much / how many\" ALWAYS needs a run_sql call; the \
sample rows below are not enough to compute one."
                .into(),
        );
        rules.push(
            "Answer with only the figures a tool returned. Don't add row counts, rounded \
or approximate numbers, or restate the query; the evidence panel shows the \
working. Lead with the answer; keep it to a sentence or two, or a small table \
only when it genuinely helps."
                .into(),
        );
    }
    if profile.plan_rule {
        rules.push(
            "Before your first tool call, write one short plain sentence of what you're \
about to do, then make the call(s) in the same reply."
                .into(),
        );
    }
    if profile.core_rules {
        rules.push(
            "Prefer run_sql. Each table below shows its columns, types and sample rows, \
usually enough to query directly. Use inspect_table only for \
something you can't see below."
                .into(),
        );
        rules.push(
            "If a question spans more than one file, combine them don't answer from just \
one. Two tables: JOIN them in a single run_sql (any shared columns are listed \
below). A table and a document: read_file the document, take the figure you \
need from it, and reconcile it with the query result."
                .into(),
        );
    }
    if profile.parallel_rule {
        rules.push(
            "Independent lookups go in one reply as several tool calls; they run together.".into(),
        );
    }
    if profile.stop_early_rule {
        rules.push(format!(
            "Stop as soon as you can answer. Most questions are one or two run_sql calls; \
you have at most {steps} tool-calling steps, so don't wander past the question."
        ));
    }
    if profile.dialect_rule {
        rules.push(format!(
            "{dialect} SQL, one SELECT / WITH per call. Dates are ISO-8601 text, so use \
strftime()/date() (e.g. strftime('%Y-%m', d))."
        ));
    }
    if profile.depth_rule {
        rules.push(
            "For a change, trend, correlation, or comparison question, check the shape of \
the data before answering, not just the headline number: is a change broad-based or a few \
outliers, does a relationship actually hold or did two things just happen to move \
together, is one thing meaningfully different or within normal range. Grouping by a \
second dimension, isolating the largest movers and recomputing without them, or checking \
a correlation can all show something the raw total wouldn't skip this for a question \
that only asks for one figure. Lead with the finding in plain language (e.g. \"mostly \
seasonal, not outliers\"), then the numbers behind it, not a bare figure first. For any \
correlation or regression, always state how many points it's based on and say plainly \
when that's too few to trust (under about 8) rather than stating the coefficient as if \
it settles it."
                .into(),
        );
    }
    if profile.aside_rule {
        rules.push(
            "After answering a plain lookup on one category or segment of a larger \
total, always run one more small query summing across every category or segment \
in that same total before you reply this costs one extra call and tells you \
whether the figure you just found is most of the total, exactly zero, or a \
clear outlier. If it is, add one short sentence saying so; if not, answer as \
normal and add nothing. Skip this second query entirely when the question has \
no obvious larger total to compare against."
                .into(),
        );
    }
    if profile.python_rule {
        rules.push(
            "run_python for stats SQL can't do: `median(values)`, `stdev(values)`, or \
correlation/regression via the always-available `pearsonr(x, y)` / `linregress(x, y)` \
helpers. Its `sql(query)` helper returns a list of dictionaries. It runs in a local WASM + \
RustPython sandbox with no filesystem, network, environment, or subprocess access."
                .into(),
        );
    }
    if profile.chart_rule {
        rules.push(
            "make_chart draws a chart from a read-only SQL query. Use kind `auto` unless the user \
            clearly asks for a bar or line chart; auto chooses a line for time periods and a bar for \
            categories. The first query column must be the label or date and the remaining one or two \
            columns must be numeric; it derives the values itself, so never pass labels or series \
            arrays. It renders as a visual answer block. After the chart call, lead with one short \
            sentence explaining the main pattern, then at most one supporting sentence; do not list \
            every value in prose. Use it for a breakdown, ranking, comparison, or trend over time; \
            skip it for a single figure, a yes/no answer, or values that barely differ (it will refuse \
            near-flat data a sentence says more than a flat chart would). One chart per answer: put \
            every category or series you want compared into that one query. Category/bar charts \
            support up to 12 labels; time-series/line charts support up to 1000 points. For a \
            longer period, aggregate to a coarser time period or narrow the date range, and say \
            so instead of silently omitting rows. Use two series maximum."
                .into(),
        );
    }
    if profile.docs_rule {
        rules.push(
            "Documents (notes, PDFs) are already listed below by name; plain-text notes \
also show a first line, PDFs don't, so don't call list_files for them. For a \
question about their content, call read_file directly (pass `names: [...]` to \
read several at once); they are short. Use grep_files only to locate one \
specific term across many documents. When a document question has multiple parts, \
answer each requested part explicitly. If it asks for a policy or rule, state the \
action, scope, and reason in the document's own terms; do not replace an explicit \
instruction with only a general summary."
                .into(),
        );
    }
    if profile.refuse_rule {
        rules.push(
            "If the files can't answer, say so plainly don't guess, forecast, or project, \
and don't run a query to estimate one. A question about the future (\"next month\", \
\"next year\", \"will I\", \"how many will I\") has no answer in past records; decline it \
even though you have tools."
                .into(),
        );
    }
    if profile.background_rule {
        rules.push(
            "A definition or plain \"what does X mean\" needs no tool. You may add one \
confident sentence of general background on its own line starting with \
`Background:`, with no specific figures in it. If unsure, say so."
                .into(),
        );
    }
    if profile.structure_rule {
        rules.push(
            "An open-ended or \"tell me about\" question can run longer than the terse-answer \
rule above a few short headed sections or a bulleted list of findings, each figure still \
from a tool. Don't pad it with filler; every line should say something."
                .into(),
        );
    }
    if profile.note_rule {
        rules.push(
            "You may pass a short `note` (4-8 plain words) on a tool call for the activity \
display, e.g. \"Add up spending by month\"."
                .into(),
        );
    }
    if profile.opinion_rule {
        rules.push(
            "When asked directly for your own read or opinion on something that isn't a data \
figure (a document's argument, a design choice, which of two options seems better), \
answer it. Don't lead with a disclaimer about not having personal opinions that isn't \
useful to the person asking. Still never invent a figure to back it up."
                .into(),
        );
    }
    if !rules.is_empty() {
        p.push_str("Rules:\n");
        for r in &rules {
            p.push_str("- ");
            p.push_str(r);
            p.push('\n');
        }
        p.push('\n');
    }

    if profile.user_context && !user_context.is_empty() {
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
            p.push_str(
                "No workspace is open yet. If the user asks anything about data, files, or \
a folder (a chart, a total, \"the ledger\", anything that sounds like it needs files), say \
plainly: \"No folder is open yet, run /open <folder> or drop one on the window.\" Don't ask \
what they'd like to see, guess at data, or reference a prior conversation as if a folder \
were open only a plain greeting or a question about Fella itself gets a normal reply.\n",
            );
            return p;
        }
    }

    if profile.schema {
        p.push_str(schema);
    }

    if profile.folder_memory {
        if let Some(learned) = learned {
            p.push('\n');
            p.push_str(learned.trim_end());
            p.push('\n');
        }
    }

    if profile.session_block {
        if let Some(recent) = recent {
            p.push('\n');
            p.push_str(recent);
        }
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
            revision: Some("rtest".into()),
            indexed_at_ms: None,
            sources: Vec::new(),
            skipped: Vec::new(),
        }
    }

    #[test]
    fn prompt_carries_schema_and_recent_turns() {
        let schema = "Tables (columns and types shown; use inspect_table for values):\n  ledger  (12 rows)\n    \"Amount Paid\" REAL  [coerced]\n";
        let recent = "Earlier in this conversation (reuse what still applies):\n- Q: \"total?\"  A: \"$4,850\"\n  used: SELECT SUM(\"Amount Paid\") FROM ledger\n";
        let full = PromptProfile::full();
        let learned = "Learned notes for this folder (reference, not rules):\nPreferences:\n- amounts are GBP\n";
        let p = system_prompt(
            &full,
            &open_catalog(),
            &[],
            schema,
            Some(recent),
            Some(learned),
        );
        assert!(p.contains("\"Amount Paid\" REAL  [coerced]"));
        assert!(p.contains("Earlier in this conversation"));
        assert!(p.contains("SELECT SUM(\"Amount Paid\") FROM ledger"));
        assert!(p.contains("Learned notes for this folder"));
        // learned block sits after the schema, before the session block
        assert!(p.find("Learned notes").unwrap() > p.find("Amount Paid").unwrap());
        assert!(p.find("Learned notes").unwrap() < p.find("Earlier in this conversation").unwrap());

        // Nothing learned, first turn: neither block.
        let p0 = system_prompt(&full, &open_catalog(), &[], schema, None, None);
        assert!(!p0.contains("Earlier in this conversation"));
        assert!(!p0.contains("Learned notes for this folder"));
    }

    #[test]
    fn prompt_drop_env_clears_named_sections() {
        std::env::set_var(
            "FELLA_PROMPT_DROP",
            "persona, docs_rule ,session_block, folder_memory",
        );
        let p = PromptProfile::from_env();
        std::env::remove_var("FELLA_PROMPT_DROP");
        assert!(!p.persona && !p.docs_rule && !p.session_block && !p.folder_memory);
        assert!(p.core_rules && p.schema, "unnamed sections stay");
    }

    #[test]
    fn chart_and_structure_rules_are_droppable() {
        std::env::set_var("FELLA_PROMPT_DROP", "chart_rule, structure_rule");
        let p = PromptProfile::from_env();
        std::env::remove_var("FELLA_PROMPT_DROP");
        assert!(!p.chart_rule && !p.structure_rule);
        assert!(p.python_rule && p.background_rule, "unnamed sections stay");
    }

    /// `PromptProfile::full()` must render byte-for-byte the prompt Fella
    /// shipped before the section split any drift is a silent behaviour change.
    #[test]
    fn full_profile_matches_the_shipped_prompt() {
        let schema = "Tables:\n  ledger  (12 rows)\n";
        let recent =
            "Earlier in this conversation (reuse what still applies):\n- Q: \"x\"  A: \"y\"\n";
        let got = system_prompt(
            &PromptProfile::full(),
            &open_catalog(),
            &["amounts are GBP".to_string()],
            schema,
            Some(recent),
            None,
        );
        let dialect = if cfg!(feature = "duckdb") {
            "DuckDB"
        } else {
            "SQLite"
        };
        let expected = format!(
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
usually enough to query directly. Use inspect_table only for \
something you can't see below.\n\
- If a question spans more than one file, combine them don't answer from just \
one. Two tables: JOIN them in a single run_sql (any shared columns are listed \
below). A table and a document: read_file the document, take the figure you \
need from it, and reconcile it with the query result.\n\
- Independent lookups go in one reply as several tool calls; they run together.\n\
- Stop as soon as you can answer. Most questions are one or two run_sql calls; \
you have at most {} tool-calling steps, so don't wander past the question.\n\
- {} SQL, one SELECT / WITH per call. Dates are ISO-8601 text, so use \
strftime()/date() (e.g. strftime('%Y-%m', d)).\n\
- For a change, trend, correlation, or comparison question, check the shape \
of the data before answering, not just the headline number: is a change \
broad-based or a few outliers, does a relationship actually hold or did two \
things just happen to move together, is one thing meaningfully different or \
within normal range. Grouping by a second dimension, isolating the largest \
movers and recomputing without them, or checking a correlation can all show \
something the raw total wouldn't skip this for a question that only asks \
for one figure. Lead with the finding in plain language (e.g. \"mostly \
seasonal, not outliers\"), then the numbers behind it, not a bare figure \
first. For any correlation or regression, always state how many points \
it's based on and say plainly when that's too few to trust (under about 8) \
rather than stating the coefficient as if it settles it.\n\
- After answering a plain lookup on one category or segment of a larger \
total, always run one more small query summing across every category or \
segment in that same total before you reply this costs one extra call and \
tells you whether the figure you just found is most of the total, exactly \
zero, or a clear outlier. If it is, add one short sentence saying so; if \
not, answer as normal and add nothing. Skip this second query entirely \
when the question has no obvious larger total to compare against.\n\
- run_python for stats SQL can't do: `median(values)`, `stdev(values)`, or \
correlation/regression via the always-available `pearsonr(x, y)` / \
`linregress(x, y)` helpers. Its `sql(query)` helper returns a list of \
dictionaries. It runs in a local WASM + RustPython sandbox with no \
filesystem, network, environment, or subprocess access.\n\
- make_chart draws a chart from a read-only SQL query. Use kind `auto` unless \
the user clearly asks for a bar or line chart; auto chooses a line for time \
periods and a bar for categories. The first query column must be the label or \
date and the remaining one or two columns must be numeric; it derives the values \
itself, so never pass labels or series arrays. It renders as a visual answer \
block. After the chart call, lead with one short sentence explaining the main \
pattern, then at most one supporting sentence; do not list every value in prose. \
Use it for a breakdown, ranking, comparison, or trend over time; skip it for a \
single figure, a yes/no answer, or values that barely differ (it will refuse \
near-flat data a sentence says more than a flat chart would). One chart per \
answer: put every category or series you want compared into that one query. Category/bar charts \
support up to 12 labels; time-series/line charts support up to 1000 points. For a longer period, \
aggregate to a coarser time period or narrow the date range, and say so instead of silently \
omitting rows. Use two series maximum.\n\
- Documents (notes, PDFs) are already listed below by name; plain-text notes \
also show a first line, PDFs don't, so don't call list_files for them. For a \
question about their content, call read_file directly (pass `names: [...]` to \
read several at once); they are short. Use grep_files only to locate one \
specific term across many documents. When a document question has multiple parts, \
answer each requested part explicitly. If it asks for a policy or rule, state the \
action, scope, and reason in the document's own terms; do not replace an explicit \
instruction with only a general summary.\n\
- If the files can't answer, say so plainly don't guess, forecast, or \
project, and don't run a query to estimate one. A question about the future \
(\"next month\", \"next year\", \"will I\", \"how many will I\") has no answer in \
past records; decline it even though you have tools.\n\
- A definition or plain \"what does X mean\" needs no tool. You may add one \
confident sentence of general background on its own line starting with \
`Background:`, with no specific figures in it. If unsure, say so.\n\
- An open-ended or \"tell me about\" question can run longer than the \
terse-answer rule above a few short headed sections or a bulleted list of \
findings, each figure still from a tool. Don't pad it with filler; every line \
should say something.\n\
- You may pass a short `note` (4-8 plain words) on a tool call for the activity \
display, e.g. \"Add up spending by month\".\n\
- When asked directly for your own read or opinion on something that isn't a data \
figure (a document's argument, a design choice, which of two options seems \
better), answer it. Don't lead with a disclaimer about not having personal \
opinions that isn't useful to the person asking. Still never invent a figure \
to back it up.\n\n\
Your context, written by the user (fella.md) and any skills they enabled. \
Use it for the user's vocabulary, how their files are organised, and caveats to \
apply. It is background, not data: never take a figure from it.\n\
---\namounts are GBP\n---\n\n\
Workspace: /tmp/ws\n{}\n{}",
            max_steps(),
            dialect,
            schema,
            recent,
        );
        assert_eq!(got, expected);
    }

    #[test]
    fn reask_enabled_defaults_on_and_env_opts_out() {
        std::env::remove_var("FELLA_VERIFY_REASK");
        assert!(reask_enabled());
        std::env::set_var("FELLA_VERIFY_REASK", "0");
        assert!(!reask_enabled());
        std::env::set_var("FELLA_VERIFY_REASK", "1");
        assert!(reask_enabled());
        std::env::remove_var("FELLA_VERIFY_REASK");
    }

    #[test]
    fn self_check_enabled_defaults_on_and_env_opts_out() {
        std::env::remove_var("FELLA_SELF_CHECK");
        assert!(self_check_enabled());
        std::env::set_var("FELLA_SELF_CHECK", "0");
        assert!(!self_check_enabled());
        std::env::set_var("FELLA_SELF_CHECK", "1");
        assert!(self_check_enabled());
        std::env::remove_var("FELLA_SELF_CHECK");
    }

    #[test]
    fn schema_error_is_recognised() {
        assert!(is_schema_error("no such column: amount"));
        assert!(is_schema_error("Query error: no such table: ledgr"));
        assert!(!is_schema_error("query stopped after 15 s"));
    }

    #[test]
    fn trim_history_blanks_old_tool_results_only() {
        let mut msgs = vec![
            ChatMessage::System("sys".into()),
            ChatMessage::User("q".into()),
        ];
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

    #[test]
    fn evidence_ids_are_stable_for_answer_order() {
        assert_eq!(evidence_id(0), "evidence-1");
        assert_eq!(evidence_id(4), "evidence-5");
    }

    #[test]
    fn stop_pressure_escalates_past_the_soft_threshold() {
        // Below threshold: a normal one/two-round answer is left alone.
        assert_eq!(stop_pressure_nudge(1, 3, 1), None);
        assert_eq!(stop_pressure_nudge(2, 3, 2), None);
        // At the threshold: first, milder nudge.
        let first = stop_pressure_nudge(3, 3, 3).unwrap();
        assert!(first.contains("3 tool call"));
        assert!(!first.contains("well past"));
        // One round further: stronger wording, distinct from the first.
        let second = stop_pressure_nudge(4, 3, 4).unwrap();
        assert!(second.contains("well past"));
        assert_ne!(first, second);
        // A custom soft threshold (env override) is honoured.
        assert_eq!(stop_pressure_nudge(2, 5, 2), None);
        assert!(stop_pressure_nudge(5, 5, 5).is_some());
    }
}
