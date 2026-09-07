//! Scored evaluation harness for Fella's agent loop.
//!
//! Where `agent_bench` times the loop, `agent_eval` scores it: is the number
//! right, how close is the whole answer, how many tool calls were *wasted*, how
//! many tokens per correct answer and it sweeps that across prompt
//! ablations, folder sizes, and models.
//!
//! Dev-only. Needs `--features eval` and a data dir with a real `fella.db` +
//! `auth.json` (copy them to a scratch dir first so it never touches live
//! state):
//!
//!   AGENT_EVAL_DATA_DIR=/path/to/copied/data-dir \
//!   cargo run --release --features eval --example agent_eval -- <subcommand> [opts]
//!
//! Subcommands:
//!   accuracy         graded battery, once per --models
//!   prompt-ablation  battery per system-prompt section dropped
//!   folder-scale     fixed battery across workspace sizes (adaptive vs fixed num_ctx)
//!   model-ladder     battery across a model list, with $/100 answers
//!   robustness       the trap battery (text amounts, totals row, mixed dates)
//!   session-memory   turn-2 accuracy with the recent-turns block on vs off
//!   all              accuracy + robustness + session-memory
//!
//! Opts: --models "a,b,c"  --judge <model>  --iters N  --only <id-substr>
//!       --json <path>  --compare <old.json>

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use fella_lib::engine::evidence::EvidenceItem;
use fella_lib::engine::testkit::{
    self, Goldens, Messiness, TableGold, WorkspaceSpec,
};
use fella_lib::engine::{verify, AskEvent, EngineState};

// --- what a correct answer looks like -------------------------------------

#[derive(Clone)]
enum Gold {
    /// every figure must appear in the answer, within tolerance
    Figures(Vec<f64>),
    /// each substring must be present (case-insensitive)
    Contains(Vec<&'static str>),
    /// the answer must decline (no figures, says it can't)
    Refusal,
    /// the answer must need no tool call at all
    NoTool,
}

#[derive(Clone)]
struct EvalCase {
    id: &'static str,
    category: &'static str,
    question: String,
    gold: Gold,
    /// how many tool calls a clean run needs (for the waste metric)
    min_tools: usize,
    /// the ideal answer, for closeness scoring
    reference: String,
}

// --- one run of one case -------------------------------------------------

struct RunResult {
    text: String,
    evidence: Vec<EvidenceItem>,
    hard_fail: bool,
    prompt_tok: u32,
    completion_tok: u32,
    total: Duration,
    first_token: Option<Duration>,
    steps: usize,
    err: Option<String>,
}

async fn run_case(engine: &EngineState, conv: &str, question: &str, model: Option<&str>) -> RunResult {
    #[derive(Clone)]
    struct Ev {
        at: Duration,
        kind: &'static str,
    }
    let evs: Arc<Mutex<Vec<Ev>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = evs.clone();
    let t0 = Instant::now();
    let res = engine
        .ask(conv, question, model, move |e: AskEvent| {
            let kind = match &e {
                AskEvent::AssistantDelta { .. } => "delta",
                AskEvent::ToolStart { .. } => "tool_start",
                AskEvent::ToolEnd { .. } => "tool_end",
                AskEvent::Notice { .. } => "notice",
                AskEvent::AnswerDone { .. } => "answer_done",
            };
            sink.lock().unwrap().push(Ev { at: t0.elapsed(), kind });
        })
        .await;
    let total = t0.elapsed();
    let evs = evs.lock().unwrap().clone();
    let first_token = evs.iter().find(|e| e.kind == "delta").map(|e| e.at);
    let mut steps = 0usize;
    let mut prev = "";
    for e in &evs {
        if e.kind == "tool_start" && prev != "tool_start" {
            steps += 1;
        }
        prev = e.kind;
    }

    match res {
        Ok(a) => {
            let (p, c) = a
                .usage
                .map(|u| (u.prompt_tokens, u.completion_tokens))
                .unwrap_or((0, 0));
            RunResult {
                hard_fail: verify::hard_fail(&a.verification).is_some(),
                text: a.text,
                evidence: a.evidence,
                prompt_tok: p,
                completion_tok: c,
                total,
                first_token,
                steps,
                err: None,
            }
        }
        Err(e) => RunResult {
            text: String::new(),
            evidence: Vec::new(),
            hard_fail: false,
            prompt_tok: 0,
            completion_tok: 0,
            total,
            first_token,
            steps,
            err: Some(e.to_string()),
        },
    }
}

// --- metrics -----------------------------------------------------------

/// Number-shaped runs in `text`, parsed. Mirrors `engine::verify::number_tokens`
/// (private there); currency signs, thousands separators and a trailing `%`
/// tolerated.
fn numbers_in(text: &str) -> Vec<f64> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if !b[i].is_ascii_digit() {
            i += 1;
            continue;
        }
        let start = i;
        while i < b.len() && (b[i].is_ascii_digit() || b[i] == b',' || b[i] == b'.') {
            i += 1;
        }
        let raw = &text[start..i];
        let cleaned: String = raw.chars().filter(|c| *c != ',').collect();
        let cleaned = cleaned.trim_end_matches('.');
        if let Ok(v) = cleaned.parse::<f64>() {
            out.push(v);
        }
    }
    out
}

fn close(a: f64, b: f64) -> bool {
    if a == b {
        return true;
    }
    let d = (a - b).abs();
    d < 0.5 || d / a.abs().max(b.abs()).max(1.0) < 0.01
}

/// Did the run land the right answer for its `Gold`?
fn grade(r: &RunResult, gold: &Gold) -> bool {
    if r.err.is_some() {
        return false;
    }
    let low = r.text.to_lowercase();
    match gold {
        Gold::Figures(want) => {
            let got = numbers_in(&r.text);
            want.iter().all(|w| got.iter().any(|g| close(*g, *w)))
        }
        Gold::Contains(subs) => subs.iter().all(|s| low.contains(&s.to_lowercase())),
        Gold::Refusal => {
            numbers_in(&r.text).iter().all(|n| (1900.0..=2100.0).contains(n))
                && (low.contains("can't")
                    || low.contains("cannot")
                    || low.contains("no ")
                    || low.contains("not in")
                    || low.contains("don't have"))
        }
        Gold::NoTool => r.evidence.is_empty() && !r.text.trim().is_empty(),
    }
}

fn token_f1(a: &str, b: &str) -> f32 {
    let toks = |s: &str| {
        s.to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|t| t.len() > 2)
            .map(str::to_string)
            .collect::<std::collections::HashSet<_>>()
    };
    let (ta, tb) = (toks(a), toks(b));
    if ta.is_empty() || tb.is_empty() {
        return 0.0;
    }
    let inter = ta.intersection(&tb).count() as f32;
    let (p, rec) = (inter / tb.len() as f32, inter / ta.len() as f32);
    if p + rec == 0.0 {
        0.0
    } else {
        2.0 * p * rec / (p + rec)
    }
}

/// 0..1: half figure recall, a third "no ungrounded figure", a fifth wording.
fn closeness_det(r: &RunResult, case: &EvalCase) -> f32 {
    let want: Vec<f64> = match &case.gold {
        Gold::Figures(w) => w.clone(),
        _ => Vec::new(),
    };
    let got = numbers_in(&r.text);
    let figure_recall = if want.is_empty() {
        1.0
    } else {
        want.iter().filter(|w| got.iter().any(|g| close(*g, **w))).count() as f32 / want.len() as f32
    };
    // a figure is grounded if it shows up in some evidence cell / summary
    let grounded = |n: f64| {
        r.evidence.iter().any(|e| {
            numbers_in(&e.result_summary).iter().any(|x| close(*x, n))
                || e.output.as_deref().map(|o| numbers_in(o).iter().any(|x| close(*x, n))).unwrap_or(false)
                || e.rows.as_ref().map(|rs| {
                    rs.iter().flatten().filter_map(|c| c.as_f64()).any(|x| close(x, n))
                }).unwrap_or(false)
        })
    };
    let ungrounded_rate = if got.is_empty() {
        0.0
    } else {
        got.iter().filter(|n| !(1900.0..=2100.0).contains(*n) && !grounded(**n)).count() as f32
            / got.len() as f32
    };
    (0.5 * figure_recall + 0.3 * (1.0 - ungrounded_rate) + 0.2 * token_f1(&r.text, &case.reference))
        .clamp(0.0, 1.0)
}

#[derive(Default, Clone, Copy)]
struct Waste {
    duplicate: usize,
    redundant_schema: usize,
    speculative: usize,
    error_retry: usize,
}
impl Waste {
    fn total(&self) -> usize {
        self.duplicate + self.redundant_schema + self.speculative + self.error_retry
    }
}

fn classify_waste(r: &RunResult, case: &EvalCase) -> Waste {
    let mut w = Waste::default();
    let mut seen: Vec<(String, String)> = Vec::new();
    for (i, e) in r.evidence.iter().enumerate() {
        let key = (e.tool.clone(), e.args.to_string());
        if e.result_summary == "skipped (duplicate call)" || seen.contains(&key) {
            w.duplicate += 1;
        }
        seen.push(key.clone());

        // a describe/sample/list call the shipped schema block already covers
        if matches!(e.tool.as_str(), "describe_schema" | "sample_rows" | "list_files") {
            w.redundant_schema += 1;
        }

        // an error later re-issued with the same args
        if e.error.is_some()
            && r.evidence[i + 1..]
                .iter()
                .any(|f| f.error.is_none() && (f.tool.clone(), f.args.to_string()) == key)
        {
            w.error_retry += 1;
        }

        // a run_sql whose numbers feed no figure in the answer
        if e.tool == "run_sql" && e.error.is_none() {
            let ans = numbers_in(&r.text);
            let produced = numbers_in(&e.result_summary);
            let fed = produced.iter().any(|p| ans.iter().any(|a| close(*a, *p)));
            if !fed && !produced.is_empty() {
                w.speculative += 1;
            }
        }
    }
    // don't count the calls a clean run legitimately needs
    let necessary = case.min_tools.min(r.evidence.len());
    let extra = r.evidence.len().saturating_sub(necessary);
    // waste can't exceed the extra calls (guards double-counting)
    let cap = extra;
    w.duplicate = w.duplicate.min(cap);
    w.redundant_schema = w.redundant_schema.min(cap);
    w.speculative = w.speculative.min(cap);
    w
}

// --- LLM judge (opt-in) ----------------------------------------------

async fn judge_closeness(
    engine: &EngineState,
    judge_model: &str,
    reference: &str,
    answer: &str,
) -> Option<f32> {
    let sys = "You grade how well a candidate answer matches a reference answer for a data question. \
Reply with ONLY a single digit 1-5: 5 = same facts and figures, no extra or wrong claims, confidence \
appropriate; 3 = roughly right but missing or muddled something; 1 = wrong or evasive.";
    let user = format!("REFERENCE:\n{reference}\n\nCANDIDATE:\n{answer}\n\nScore (1-5):");
    let reply = engine.ask_once(Some(judge_model), sys, &user).await.ok()?;
    let d = reply.chars().find(|c| ('1'..='5').contains(c))?;
    Some((d.to_digit(10)? as f32 - 1.0) / 4.0)
}

// --- battery ---------------------------------------------------------

fn battery(g: &Goldens, rent_total: f64) -> Vec<EvalCase> {
    let t0 = "txns_00";
    let tg: &TableGold = &g.tables[t0];
    let top3 = tg.top3_categories();
    let (top_cat, top_cat_amt) = top3.first().cloned().unwrap_or_default();
    let (top_merch, top_merch_amt) = tg.top_merchant.clone();
    vec![
        EvalCase {
            id: "chitchat",
            category: "NoTool",
            question: "what kinds of questions can you help me with?".into(),
            gold: Gold::NoTool,
            min_tools: 0,
            reference: "I answer questions about the files in your folder by running SQL/Python and showing the working.".into(),
        },
        EvalCase {
            id: "agg_rent",
            category: "Aggregate",
            question: "what's the total amount I paid in rent.csv?".into(),
            gold: Gold::Figures(vec![rent_total]),
            min_tools: 1,
            reference: format!("You paid {rent_total:.2} in total."),
        },
        EvalCase {
            id: "agg_total",
            category: "Aggregate",
            question: format!("what was my total spending in {t0}.csv?"),
            gold: Gold::Figures(vec![tg.total]),
            min_tools: 1,
            reference: format!("Total spending in {t0}.csv was {:.2}.", tg.total),
        },
        EvalCase {
            id: "group_top3",
            category: "GroupTopN",
            question: format!("in {t0}.csv, what did I spend per category? give the top 3."),
            gold: Gold::Figures(vec![top_cat_amt]),
            min_tools: 1,
            reference: format!(
                "Top categories: {} {:.0}, {} {:.0}, {} {:.0}.",
                top3[0].0, top3[0].1, top3[1].0, top3[1].1, top3[2].0, top3[2].1
            ),
        },
        EvalCase {
            id: "multi_step",
            category: "MultiStep",
            question: format!(
                "in {t0}.csv, which merchant did I spend the most at overall, and roughly how much?"
            ),
            gold: Gold::Figures(vec![top_merch_amt]),
            min_tools: 1,
            reference: format!("Your biggest merchant was {top_merch} at about {top_merch_amt:.0}."),
        },
        EvalCase {
            id: "grand_total",
            category: "Aggregate",
            question: "across every transactions table in the folder, how many rows and what total?".into(),
            gold: Gold::Figures(vec![g.grand_rows() as f64, g.grand_total()]),
            min_tools: 1,
            reference: format!(
                "{} rows totalling {:.2} across the folder.",
                g.grand_rows(),
                g.grand_total()
            ),
        },
        EvalCase {
            id: "doc_lookup",
            category: "DocLookup",
            question: "according to my notes, what is the monthly rent target and when did it change?".into(),
            gold: Gold::Contains(vec!["1250", "march 2024"]),
            min_tools: 1,
            reference: "The monthly rent target is 1250, raised in March 2024.".into(),
        },
        EvalCase {
            id: "refusal",
            category: "Refusal",
            question: "how much will I spend next month?".into(),
            gold: Gold::Refusal,
            min_tools: 0,
            reference: "Your files don't say anything about the future, so I can't tell.".into(),
        },
    ]
    .into_iter()
    .map(|mut c| {
        let _ = top_cat; // silence unused in some cfgs
        c.question = c.question.trim().to_string();
        c
    })
    .collect()
}

// --- shared setup -------------------------------------------------------

fn env(k: &str, d: &str) -> String {
    std::env::var(k).unwrap_or_else(|_| d.to_string())
}

fn set_model(engine: &EngineState, m: &str) -> bool {
    let mut patch = serde_json::Map::new();
    patch.insert("model".into(), serde_json::Value::String(m.to_string()));
    engine.save_settings(&patch).is_ok()
}

/// $ per 100 answers for a model, from a small static price table
/// ($ / 1M input, $ / 1M output). `None` for local / unknown.
fn price_per_100(model: &str, prompt_tok: f64, completion_tok: f64) -> Option<f64> {
    // Sept 2026 list prices; keep in sync with docs/DEV_SETUP.md.
    const P: &[(&str, f64, f64)] = &[
        ("gpt-5.6-luna", 0.20, 1.20),
        ("gpt-5.6-terra", 2.00, 12.00),
        ("gpt-5-nano", 0.05, 0.40),
        ("gpt-4o-mini", 0.15, 0.60),
        ("gpt-4.1-mini", 0.40, 1.60),
        ("grok-4.3", 1.25, 2.50),
        ("grok-4.1-fast", 0.20, 0.50),
    ];
    let stem = model.rsplit('/').next().unwrap_or(model);
    let (pin, pout) = P.iter().find(|(m, ..)| *m == stem).map(|(_, a, b)| (*a, *b))?;
    Some((prompt_tok * pin + completion_tok * pout) / 1e6 * 100.0)
}

struct CaseScore {
    id: String,
    model: String,
    profile: String,
    correct: bool,
    closeness_det: f32,
    closeness_judge: Option<f32>,
    waste: Waste,
    prompt_tok: u32,
    completion_tok: u32,
    total_s: f64,
    first_tok_s: Option<f64>,
    steps: usize,
    hard_fail: bool,
    err: Option<String>,
}

async fn score_case(
    engine: &EngineState,
    case: &EvalCase,
    model: &str,
    profile: &str,
    judge: Option<&str>,
    conv: &str,
) -> CaseScore {
    let r = run_case(engine, conv, &case.question, None).await;
    let correct = grade(&r, &case.gold);
    let closeness_judge = match judge {
        Some(jm) if r.err.is_none() => judge_closeness(engine, jm, &case.reference, &r.text).await,
        _ => None,
    };
    CaseScore {
        id: case.id.to_string(),
        model: model.to_string(),
        profile: profile.to_string(),
        correct,
        closeness_det: closeness_det(&r, case),
        closeness_judge,
        waste: classify_waste(&r, case),
        prompt_tok: r.prompt_tok,
        completion_tok: r.completion_tok,
        total_s: r.total.as_secs_f64(),
        first_tok_s: r.first_token.map(|d| d.as_secs_f64()),
        steps: r.steps,
        hard_fail: r.hard_fail,
        err: r.err,
    }
}

fn acc(scores: &[CaseScore]) -> (usize, usize) {
    let n = scores.iter().filter(|s| s.id != "chitchat").count();
    let ok = scores.iter().filter(|s| s.id != "chitchat" && s.correct).count();
    (ok, n)
}
fn mean_closeness(scores: &[CaseScore]) -> f32 {
    let v: Vec<f32> = scores.iter().map(|s| s.closeness_det).collect();
    if v.is_empty() { 0.0 } else { v.iter().sum::<f32>() / v.len() as f32 }
}
fn total_waste(scores: &[CaseScore]) -> usize {
    scores.iter().map(|s| s.waste.total()).sum()
}
fn tokens_per_correct(scores: &[CaseScore]) -> f64 {
    let tot: f64 = scores.iter().map(|s| (s.prompt_tok + s.completion_tok) as f64).sum();
    let ok = scores.iter().filter(|s| s.correct).count().max(1);
    tot / ok as f64
}

// --- subcommands -----------------------------------------------------

async fn run_battery(
    engine: &EngineState,
    cases: &[EvalCase],
    model: &str,
    profile: &str,
    judge: Option<&str>,
    tag: &str,
) -> Vec<CaseScore> {
    let mut out = Vec::new();
    for c in cases {
        let conv = format!("{tag}-{model}-{profile}-{}", c.id);
        out.push(score_case(engine, c, model, profile, judge, &conv).await);
        std::io::stdout().flush().ok();
    }
    out
}

async fn cmd_accuracy(engine: &EngineState, cases: &[EvalCase], models: &[String], judge: Option<&str>) -> Vec<CaseScore> {
    println!("\n# Accuracy\n");
    println!("| model | case | correct | close(det) | close(judge) | waste | in tok | out tok | s |");
    println!("|---|---|:-:|--:|--:|--:|--:|--:|--:|");
    let mut all = Vec::new();
    for m in models {
        if !set_model(engine, m) {
            println!("| {m} | | | | | | | | save failed |");
            continue;
        }
        let scores = run_battery(engine, cases, m, "full", judge, "acc").await;
        for s in &scores {
            println!(
                "| {} | {} | {} | {:.2} | {} | {} | {} | {} | {:.1} |",
                s.model,
                s.id,
                if s.err.is_some() { "ERR".into() } else { yn(s.correct) },
                s.closeness_det,
                s.closeness_judge.map(|c| format!("{c:.2}")).unwrap_or_else(|| "-".into()),
                s.waste.total(),
                s.prompt_tok,
                s.completion_tok,
                s.total_s,
            );
        }
        let (ok, n) = acc(&scores);
        println!(
            "| **{m}** | **summary** | **{ok}/{n}** | **{:.2}** | | **{}** | | | **{:.0} tok/correct** |",
            mean_closeness(&scores),
            total_waste(&scores),
            tokens_per_correct(&scores),
        );
        all.extend(scores);
    }
    all
}

async fn cmd_prompt_ablation(engine: &EngineState, cases: &[EvalCase], model: &str, judge: Option<&str>) -> Vec<CaseScore> {
    // cumulative-drop ladder: each row drops one more section than the last.
    let ladder: &[(&str, &[&str])] = &[
        ("full", &[]),
        ("-note_rule", &["note_rule"]),
        ("-background_rule", &["note_rule", "background_rule"]),
        ("-docs_rule", &["note_rule", "background_rule", "docs_rule"]),
        ("-parallel_rule", &["note_rule", "background_rule", "docs_rule", "parallel_rule"]),
        ("-plan_rule", &["note_rule", "background_rule", "docs_rule", "parallel_rule", "plan_rule"]),
        ("core+schema only", &[
            "note_rule", "background_rule", "docs_rule", "parallel_rule", "plan_rule",
            "python_rule", "dialect_rule", "stop_early_rule", "refuse_rule", "user_context", "session_block",
        ]),
        ("schema names-only", &["schema"]),
    ];
    set_model(engine, model);
    println!("\n# Prompt ablation  ·  model `{model}`\n");
    println!("| profile | acc | close(det) | waste | in tok (mean) |");
    println!("|---|:-:|--:|--:|--:|");
    let mut all = Vec::new();
    for (name, drop) in ladder {
        std::env::set_var("FELLA_PROMPT_DROP", drop.join(","));
        let scores = run_battery(engine, cases, model, name, judge, "ablate").await;
        std::env::remove_var("FELLA_PROMPT_DROP");
        let (ok, n) = acc(&scores);
        let mean_in = scores.iter().map(|s| s.prompt_tok as f64).sum::<f64>() / scores.len().max(1) as f64;
        println!(
            "| {name} | {ok}/{n} | {:.2} | {} | {mean_in:.0} |",
            mean_closeness(&scores),
            total_waste(&scores),
        );
        all.extend(scores);
    }
    println!("\n_Smallest profile that holds acc + closeness is the one to ship._");
    all
}

async fn cmd_folder_scale(engine: &EngineState, model: &str, ws: &Path) -> Vec<CaseScore> {
    set_model(engine, model);
    println!("\n# Folder scale  ·  model `{model}`\n");
    println!("| tables | rows/table | num_ctx | acc | first tok s | hit cap | waste |");
    println!("|--:|--:|---|:-:|--:|:-:|--:|");
    let mut all = Vec::new();
    let sizes: &[(usize, usize)] = &[(1, 2_000), (5, 2_000), (13, 1_000), (40, 500), (120, 200)];
    for &(n_tables, rows) in sizes {
        let dir = ws.join(format!("scale_{n_tables}x{rows}"));
        let spec = WorkspaceSpec { n_tables, rows_per_table: rows, messiness: Messiness::Clean, seed: 7 };
        let g = testkit::synth_workspace(&dir, &spec);
        let _ = testkit::write_rent_fixture(&dir);
        if engine.open_workspace(&dir).is_err() {
            println!("| {n_tables} | {rows} | | open failed | | | |");
            continue;
        }
        let cases = battery(&g, 6100.0);
        for (label, fixed) in [("adaptive", false), ("fixed 8192", true)] {
            if fixed {
                std::env::set_var("FELLA_OLLAMA_NUM_CTX_FIXED", "1");
            } else {
                std::env::remove_var("FELLA_OLLAMA_NUM_CTX_FIXED");
            }
            let scores = run_battery(engine, &cases, model, label, None, "scale").await;
            std::env::remove_var("FELLA_OLLAMA_NUM_CTX_FIXED");
            let (ok, n) = acc(&scores);
            let ft: Vec<f64> = scores.iter().filter_map(|s| s.first_tok_s).collect();
            let ft_mean = if ft.is_empty() { 0.0 } else { ft.iter().sum::<f64>() / ft.len() as f64 };
            let cap = scores.iter().filter(|s| s.err.as_deref().is_some_and(|e| e.contains("step"))).count();
            println!(
                "| {n_tables} | {rows} | {label} | {ok}/{n} | {ft_mean:.1} | {cap} | {} |",
                total_waste(&scores),
            );
            all.extend(scores);
        }
    }
    all
}

async fn cmd_model_ladder(engine: &EngineState, cases: &[EvalCase], models: &[String], judge: Option<&str>) -> Vec<CaseScore> {
    println!("\n# Model ladder\n");
    println!("| model | acc | close(det) | waste | tok/correct | $/100 | mean s |");
    println!("|---|:-:|--:|--:|--:|--:|--:|");
    let mut all = Vec::new();
    let mut best: Option<(String, f64)> = None;
    for m in models {
        if !set_model(engine, m) {
            println!("| {m} | save failed | | | | | |");
            continue;
        }
        let scores = run_battery(engine, cases, m, "full", judge, "ladder").await;
        let (ok, n) = acc(&scores);
        let a = ok as f64 / n.max(1) as f64;
        let mean_s = scores.iter().map(|s| s.total_s).sum::<f64>() / scores.len().max(1) as f64;
        let pin: f64 = scores.iter().map(|s| s.prompt_tok as f64).sum::<f64>() / scores.len().max(1) as f64;
        let pout: f64 = scores.iter().map(|s| s.completion_tok as f64).sum::<f64>() / scores.len().max(1) as f64;
        let cost = price_per_100(m, pin * scores.len() as f64, pout * scores.len() as f64)
            .map(|c| format!("${c:.2}"))
            .unwrap_or_else(|| "local".into());
        println!(
            "| {m} | {ok}/{n} | {:.2} | {} | {:.0} | {cost} | {mean_s:.1} |",
            mean_closeness(&scores),
            total_waste(&scores),
            tokens_per_correct(&scores),
        );
        // "cheapest that clears the bar": acc >= 0.8 and waste <= 2
        if a >= 0.8 && total_waste(&scores) <= 2 {
            let c = price_per_100(m, pin * scores.len() as f64, pout * scores.len() as f64).unwrap_or(0.0);
            if best.as_ref().map(|(_, bc)| c < *bc).unwrap_or(true) {
                best = Some((m.clone(), c));
            }
        }
        all.extend(scores);
    }
    if let Some((m, c)) = best {
        println!("\n**Cheapest model with acc ≥ 0.8 and waste ≤ 2: `{m}` (${c:.2}/100 answers).**");
    } else {
        println!("\n_No model cleared acc ≥ 0.8 / waste ≤ 2._");
    }
    all
}

async fn cmd_robustness(engine: &EngineState, model: &str, ws: &Path) -> Vec<CaseScore> {
    set_model(engine, model);
    println!("\n# Robustness (data traps)  ·  model `{model}`\n");
    println!("| trap | acc | close(det) | verify caught misses |");
    println!("|---|:-:|--:|--:|");
    let mut all = Vec::new();
    for (label, mess) in [
        ("text amounts", Messiness::TextAmounts),
        ("+ totals row", Messiness::TotalsRow),
        ("+ mixed dates", Messiness::MixedDates),
    ] {
        let dir = ws.join(format!("trap_{}", label.replace([' ', '+'], "_")));
        let spec = WorkspaceSpec { n_tables: 1, rows_per_table: 2_000, messiness: mess, seed: 11 };
        let g = testkit::synth_workspace(&dir, &spec);
        let _ = testkit::write_rent_fixture(&dir);
        engine.open_workspace(&dir).ok();
        let cases: Vec<EvalCase> = battery(&g, 6100.0)
            .into_iter()
            .filter(|c| matches!(c.category, "Aggregate" | "GroupTopN"))
            .collect();
        let scores = run_battery(engine, &cases, model, label, None, "trap").await;
        let (ok, n) = acc(&scores);
        let caught = scores.iter().filter(|s| !s.correct && s.hard_fail).count();
        let missed = scores.iter().filter(|s| !s.correct).count();
        println!(
            "| {label} | {ok}/{n} | {:.2} | {caught}/{missed} |",
            mean_closeness(&scores),
        );
        all.extend(scores);
    }
    all
}

async fn cmd_session_memory(engine: &EngineState, model: &str, g: &Goldens) -> Vec<CaseScore> {
    set_model(engine, model);
    let tg = &g.tables["txns_00"];
    let by_cat = tg.by_category.clone();
    let (c1, a1) = by_cat.iter().next().map(|(k, v)| (k.clone(), *v)).unwrap_or_default();
    let (c2, a2) = by_cat.iter().nth(1).map(|(k, v)| (k.clone(), *v)).unwrap_or_default();
    let q1 = format!("in txns_00.csv, what did I spend on {c1} in total?");
    let q2 = format!("and what about {c2}?");
    println!("\n# Session memory  ·  model `{model}`\n");
    println!("| condition | turn-2 correct | turn-2 close(det) | turn-2 steps |");
    println!("|---|:-:|--:|--:|");
    let mut all = Vec::new();
    for (label, keep) in [("memory on", true), ("memory off", false)] {
        let conv = format!("sm-{label}");
        run_case(engine, &conv, &q1, None).await; // turn 1
        if !keep {
            engine.forget_conversation(&conv);
        }
        let r2 = run_case(engine, &conv, &q2, None).await;
        let case2 = EvalCase {
            id: "sm_turn2",
            category: "Aggregate",
            question: q2.clone(),
            gold: Gold::Figures(vec![a2]),
            min_tools: 1,
            reference: format!("You spent {a2:.2} on {c2}."),
        };
        let correct = grade(&r2, &case2.gold);
        let cd = closeness_det(&r2, &case2);
        println!("| {label} | {} | {cd:.2} | {} |", yn(correct), r2.steps);
        all.push(CaseScore {
            id: format!("sm_turn2_{label}"),
            model: model.into(),
            profile: label.into(),
            correct,
            closeness_det: cd,
            closeness_judge: None,
            waste: classify_waste(&r2, &case2),
            prompt_tok: r2.prompt_tok,
            completion_tok: r2.completion_tok,
            total_s: r2.total.as_secs_f64(),
            first_tok_s: r2.first_token.map(|d| d.as_secs_f64()),
            steps: r2.steps,
            hard_fail: r2.hard_fail,
            err: r2.err,
        });
        let _ = (a1, &q1);
    }
    all
}

fn yn(b: bool) -> String {
    if b { "✓".into() } else { "✗".into() }
}

// --- json out / compare -------------------------------------------

fn write_json(path: &str, scores: &[CaseScore]) {
    let arr: Vec<serde_json::Value> = scores
        .iter()
        .map(|s| {
            serde_json::json!({
                "id": s.id, "model": s.model, "profile": s.profile,
                "correct": s.correct, "closeness_det": s.closeness_det,
                "closeness_judge": s.closeness_judge,
                "waste": s.waste.total(), "prompt_tok": s.prompt_tok,
                "completion_tok": s.completion_tok, "total_s": s.total_s,
                "steps": s.steps, "hard_fail": s.hard_fail, "err": s.err,
            })
        })
        .collect();
    if std::fs::write(path, serde_json::to_string_pretty(&arr).unwrap()).is_ok() {
        eprintln!("wrote {} records to {path}", scores.len());
    }
}

fn compare(old_path: &str, new_scores: &[CaseScore]) {
    let Ok(txt) = std::fs::read_to_string(old_path) else {
        eprintln!("compare: can't read {old_path}");
        return;
    };
    let Ok(old): Result<Vec<serde_json::Value>, _> = serde_json::from_str(&txt) else {
        eprintln!("compare: {old_path} is not an agent_eval json dump");
        return;
    };
    let key = |id: &str, model: &str, profile: &str| format!("{model}|{profile}|{id}");
    let mut old_by: BTreeMap<String, (bool, f64)> = BTreeMap::new();
    for r in &old {
        old_by.insert(
            key(
                r["id"].as_str().unwrap_or(""),
                r["model"].as_str().unwrap_or(""),
                r["profile"].as_str().unwrap_or(""),
            ),
            (
                r["correct"].as_bool().unwrap_or(false),
                r["prompt_tok"].as_f64().unwrap_or(0.0) + r["completion_tok"].as_f64().unwrap_or(0.0),
            ),
        );
    }
    println!("\n# Compare vs {old_path}\n");
    println!("| model|profile|case | correct Δ | tokens Δ |");
    println!("|---|:-:|--:|");
    for s in new_scores {
        let k = key(&s.id, &s.model, &s.profile);
        if let Some((oc, ot)) = old_by.get(&k) {
            let nc = s.correct;
            let nt = (s.prompt_tok + s.completion_tok) as f64;
            let cd = match (oc, nc) {
                (false, true) => "fixed ✓",
                (true, false) => "REGRESSED ✗",
                _ => "=",
            };
            println!("| {k} | {cd} | {:+.0} |", nt - ot);
        }
    }
}

// --- main --------------------------------------------------------

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = args.first().cloned().unwrap_or_default();
    let opt = |name: &str| {
        args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned()
    };
    let judge = opt("--judge");
    let json_out = opt("--json");
    let compare_to = opt("--compare");
    let only = opt("--only");
    let models: Vec<String> = opt("--models")
        .map(|s| s.split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect())
        .unwrap_or_default();

    let data_dir = match std::env::var("AGENT_EVAL_DATA_DIR").or_else(|_| std::env::var("BENCH_DATA_DIR")) {
        Ok(d) => PathBuf::from(d),
        Err(_) => {
            eprintln!("set AGENT_EVAL_DATA_DIR to a dir holding a copy of your fella.db + auth.json");
            std::process::exit(2);
        }
    };
    let ws = PathBuf::from(env(
        "AGENT_EVAL_WS",
        std::env::temp_dir().join("fella-eval-ws").to_str().unwrap_or("/tmp/fella-eval-ws"),
    ));

    let engine = EngineState::new(&data_dir).expect("engine init");
    let s = engine.settings();
    let models = if models.is_empty() { vec![s.model.clone()] } else { models };
    eprintln!(
        "eval: provider={} model(s)={:?} ws={}",
        s.provider,
        models,
        ws.display()
    );
    let health = engine.provider_health().await;
    if !health.reachable {
        eprintln!("eval: provider not reachable, aborting");
        std::process::exit(1);
    }

    // Default workspace: one clean 6k-row table + rent fixture + notes.
    let g = testkit::synth_workspace(&ws, &WorkspaceSpec::small_clean());
    let rent_total = testkit::write_rent_fixture(&ws);
    engine.open_workspace(&ws).expect("open workspace");
    let mut cases = battery(&g, rent_total);
    if let Some(sub) = &only {
        cases.retain(|c| c.id.contains(sub.as_str()));
    }
    let judge = judge.as_deref();

    let scores = match cmd.as_str() {
        "accuracy" => cmd_accuracy(&engine, &cases, &models, judge).await,
        "prompt-ablation" => cmd_prompt_ablation(&engine, &cases, &models[0], judge).await,
        "folder-scale" => cmd_folder_scale(&engine, &models[0], &ws).await,
        "model-ladder" => cmd_model_ladder(&engine, &cases, &models, judge).await,
        "robustness" => cmd_robustness(&engine, &models[0], &ws).await,
        "session-memory" => cmd_session_memory(&engine, &models[0], &g).await,
        "all" => {
            let mut v = cmd_accuracy(&engine, &cases, &models, judge).await;
            v.extend(cmd_robustness(&engine, &models[0], &ws).await);
            v.extend(cmd_session_memory(&engine, &models[0], &g).await);
            v
        }
        other => {
            eprintln!("unknown subcommand {other:?}. one of: accuracy prompt-ablation folder-scale model-ladder robustness session-memory all");
            std::process::exit(2);
        }
    };

    if let Some(p) = &json_out {
        write_json(p, &scores);
    }
    if let Some(p) = &compare_to {
        compare(p, &scores);
    }
    eprintln!("eval: done ({} scored)", scores.len());
}
