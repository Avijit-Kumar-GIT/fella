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
//! Opts: --models "a,b,c"  --judge <model>  --iters N (default 1)
//!       --only <id-substr>  --json <path>  --compare <old.json>
//!
//! A --models entry is a bare model on the configured provider (`gemma4:31b`)
//! or `provider/model` to switch provider too (`xai/grok-4.3`,
//! `openai/gpt-5.6-luna`, `ollama-cloud/gemma4:31b`) the data dir's
//! auth.json must hold each provider's key. Only the first `/` is the
//! separator, so `gemma4:31b` keeps its colon.

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
    /// How many tool calls a clean run needs. Documented per case; the waste
    /// classifier is currently structural (it doesn't subtract this), kept for
    /// a future per-case "extra calls" metric.
    #[allow(dead_code)]
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

/// A count, in **tool calls**, of the ones that did no useful work. Four kinds,
/// each call counted once.
#[derive(Default, Clone, Copy)]
struct Waste {
    /// an exact `(tool, args)` repeat, or the engine's "skipped (duplicate call)"
    duplicate: usize,
    /// a `describe_schema` / `sample_rows` / `list_files` peek that wasn't the
    /// one free orientation call it came 2nd, or after a query already worked
    redundant_schema: usize,
    /// one of 3+ `run_sql` calls whose result the answer never uses
    speculative: usize,
    /// a call that returned an error (a well-oriented run rarely hits one)
    errored: usize,
}
impl Waste {
    fn total(&self) -> usize {
        self.duplicate + self.redundant_schema + self.speculative + self.errored
    }
    /// compact per-kind, e.g. `d0 r1 s0 e0`
    fn breakdown(&self) -> String {
        format!(
            "d{} r{} s{} e{}",
            self.duplicate, self.redundant_schema, self.speculative, self.errored
        )
    }
}

/// Count the tool calls that did no useful work. A single successful `run_sql`
/// that feeds the answer, or one orientation `describe_schema` before any
/// query, is *not* waste. Deliberately conservative: `speculative` only fires
/// once a run has made 3+ successful queries, so a normal 2-query multi-step
/// isn't penalised for its intermediate result.
fn classify_waste(r: &RunResult) -> Waste {
    let ans = numbers_in(&r.text);
    let n_ok_sql = r
        .evidence
        .iter()
        .filter(|e| e.tool == "run_sql" && e.error.is_none())
        .count();
    let mut w = Waste::default();
    let mut seen: Vec<(String, String)> = Vec::new();
    let mut ran_sql_ok = false;
    let mut inspects = 0usize;

    for e in &r.evidence {
        let key = (e.tool.clone(), e.args.to_string());
        let repeat = seen.contains(&key);
        seen.push(key);

        if e.result_summary == "skipped (duplicate call)" || repeat {
            w.duplicate += 1;
            continue;
        }
        if e.error.is_some() {
            w.errored += 1;
            continue;
        }
        if matches!(e.tool.as_str(), "describe_schema" | "sample_rows" | "list_files") {
            inspects += 1;
            if inspects > 1 || ran_sql_ok {
                w.redundant_schema += 1;
            }
            continue;
        }
        if e.tool == "run_sql" {
            ran_sql_ok = true;
            if n_ok_sql >= 3 {
                let produced = numbers_in(&e.result_summary);
                let feeds_answer = produced.iter().any(|p| ans.iter().any(|a| close(*a, *p)));
                if !produced.is_empty() && !feeds_answer {
                    w.speculative += 1;
                }
            }
        }
    }
    w
}

/// Element-wise mean of several runs' waste, rounded.
fn fold_waste(ws: &[Waste]) -> Waste {
    if ws.is_empty() {
        return Waste::default();
    }
    let n = ws.len();
    let m = |f: &dyn Fn(&Waste) -> usize| (ws.iter().map(f).sum::<usize>() + n / 2) / n;
    Waste {
        duplicate: m(&|w| w.duplicate),
        redundant_schema: m(&|w| w.redundant_schema),
        speculative: m(&|w| w.speculative),
        errored: m(&|w| w.errored),
    }
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

/// Point the engine at a model. `"grok-4.3"` keeps the current provider;
/// `"xai/grok-4.3"` (a known provider id, then `/`, then the model) also
/// switches provider + base_url so one run can sweep across providers as long
/// as the data dir's `auth.json` holds each provider's key. `gemma4:31b` keeps
/// its colon; only the first `/` is the provider separator.
fn set_model(engine: &EngineState, spec: &str) -> bool {
    let mut patch = serde_json::Map::new();
    if let Some((prov, model)) = spec.split_once('/') {
        if let Some(p) = fella_lib::engine::provider::get(prov) {
            patch.insert("provider".into(), prov.into());
            if !p.base_url.is_empty() {
                patch.insert("base_url".into(), p.base_url.into());
            }
            patch.insert("model".into(), model.into());
            return engine.save_settings(&patch).is_ok();
        }
    }
    patch.insert("model".into(), spec.into());
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
    /// `iters == 1`: this run. `> 1`: the majority verdict.
    correct: bool,
    /// fraction of `iters` that were correct (1.0 when `iters == 1` and correct)
    correct_rate: f32,
    iters: usize,
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

/// Run one case `iters` times and fold: `correct` = majority, everything
/// numeric = mean.
async fn score_case(
    engine: &EngineState,
    case: &EvalCase,
    model: &str,
    profile: &str,
    judge: Option<&str>,
    conv: &str,
    iters: usize,
) -> CaseScore {
    let iters = iters.max(1);
    let mut oks = 0usize;
    let (mut cd, mut cj_sum, mut cj_n) = (0f32, 0f32, 0usize);
    let (mut ptok, mut ctok, mut secs, mut steps) = (0u64, 0u64, 0f64, 0usize);
    let mut first_toks: Vec<f64> = Vec::new();
    let mut wastes: Vec<Waste> = Vec::new();
    let mut any_hard = false;
    let mut last_err = None;

    for it in 0..iters {
        let r = run_case(engine, &format!("{conv}-{it}"), &case.question, None).await;
        if grade(&r, &case.gold) {
            oks += 1;
        }
        cd += closeness_det(&r, case);
        if let (Some(jm), true) = (judge, r.err.is_none()) {
            if let Some(j) = judge_closeness(engine, jm, &case.reference, &r.text).await {
                cj_sum += j;
                cj_n += 1;
            }
        }
        ptok += r.prompt_tok as u64;
        ctok += r.completion_tok as u64;
        secs += r.total.as_secs_f64();
        steps += r.steps;
        if let Some(ft) = r.first_token {
            first_toks.push(ft.as_secs_f64());
        }
        wastes.push(classify_waste(&r));
        any_hard |= r.hard_fail;
        last_err = r.err;
    }

    let n = iters as f32;
    CaseScore {
        id: case.id.to_string(),
        model: model.to_string(),
        profile: profile.to_string(),
        correct: oks * 2 > iters,
        correct_rate: oks as f32 / n,
        iters,
        closeness_det: cd / n,
        closeness_judge: (cj_n > 0).then(|| cj_sum / cj_n as f32),
        waste: fold_waste(&wastes),
        prompt_tok: (ptok / iters as u64) as u32,
        completion_tok: (ctok / iters as u64) as u32,
        total_s: secs / n as f64,
        first_tok_s: (!first_toks.is_empty())
            .then(|| first_toks.iter().sum::<f64>() / first_toks.len() as f64),
        steps: steps / iters,
        hard_fail: any_hard,
        err: last_err,
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

#[allow(clippy::too_many_arguments)]
async fn run_battery(
    engine: &EngineState,
    cases: &[EvalCase],
    model: &str,
    profile: &str,
    judge: Option<&str>,
    tag: &str,
    iters: usize,
) -> Vec<CaseScore> {
    let mut out = Vec::new();
    for c in cases {
        let conv = format!("{tag}-{model}-{profile}-{}", c.id);
        out.push(score_case(engine, c, model, profile, judge, &conv, iters).await);
        std::io::stdout().flush().ok();
    }
    out
}

async fn cmd_accuracy(engine: &EngineState, cases: &[EvalCase], models: &[String], judge: Option<&str>, iters: usize) -> Vec<CaseScore> {
    println!("\n# Accuracy   ({iters} iter(s)/case)\n");
    legend();
    println!("| model | case | correct | rate | close(det) | close(judge) | waste (calls) | in tok | out tok | wall s |");
    println!("|---|---|:-:|--:|--:|--:|--:|--:|--:|--:|");
    let mut all = Vec::new();
    for m in models {
        if !set_model(engine, m) {
            println!("| {m} | | | | | | | | | save failed |");
            continue;
        }
        let scores = run_battery(engine, cases, m, "full", judge, "acc", iters).await;
        for s in &scores {
            println!(
                "| {} | {} | {} | {:.0}% | {:.2} | {} | {} `{}` | {} | {} | {:.1} |",
                s.model,
                s.id,
                if s.err.is_some() { "ERR".into() } else { yn(s.correct) },
                s.correct_rate * 100.0,
                s.closeness_det,
                s.closeness_judge.map(|c| format!("{c:.2}")).unwrap_or_else(|| "-".into()),
                s.waste.total(),
                s.waste.breakdown(),
                s.prompt_tok,
                s.completion_tok,
                s.total_s,
            );
        }
        let (ok, n) = acc(&scores);
        println!(
            "| **{m}** | **summary** | **{ok}/{n} correct** | | **{:.2}** | | **{} calls** | | | **{:.0} tok/correct-ans** |",
            mean_closeness(&scores),
            total_waste(&scores),
            tokens_per_correct(&scores),
        );
        all.extend(scores);
    }
    all
}

/// One-line unit key, printed above each table.
fn legend() {
    println!(
        "_units — **correct**: majority over iters · **rate**: % of iters correct · \
**close(det/judge)**: 0.00–1.00 · **waste**: # tool calls that did no useful work \
(`d`up `r`edundant-peek `s`peculative `e`rrored) · **tok**: tokens (prompt+completion) · \
**$/100**: USD per 100 answers, list price · **wall s / first-tok s**: seconds · **steps**: tool-call rounds_\n"
    );
}

async fn cmd_prompt_ablation(engine: &EngineState, cases: &[EvalCase], model: &str, judge: Option<&str>, iters: usize) -> Vec<CaseScore> {
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
        let scores = run_battery(engine, cases, model, name, judge, "ablate", iters).await;
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

async fn cmd_folder_scale(engine: &EngineState, model: &str, ws: &Path, iters: usize) -> Vec<CaseScore> {
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
            let scores = run_battery(engine, &cases, model, label, None, "scale", iters).await;
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

async fn cmd_model_ladder(engine: &EngineState, cases: &[EvalCase], models: &[String], judge: Option<&str>, iters: usize) -> Vec<CaseScore> {
    println!("\n# Model ladder   ({iters} iter(s)/case)\n");
    legend();
    let n_cases = cases.iter().filter(|c| c.id != "chitchat").count();
    // "clears the bar" thresholds: >=80% accuracy, <= ~0.5 wasted calls/case
    let waste_bar = (n_cases as f64 * 0.5).ceil() as usize;
    println!("| model | acc | close(det) | waste/case | tok/correct-ans | $/100 | mean wall s |");
    println!("|---|:-:|--:|--:|--:|--:|--:|");
    let mut all = Vec::new();
    let mut best: Option<(String, f64)> = None;
    for m in models {
        if !set_model(engine, m) {
            println!("| {m} | save failed | | | | | |");
            continue;
        }
        let scores = run_battery(engine, cases, m, "full", judge, "ladder", iters).await;
        let (ok, n) = acc(&scores);
        let a = ok as f64 / n.max(1) as f64;
        let mean_s = scores.iter().map(|s| s.total_s).sum::<f64>() / scores.len().max(1) as f64;
        let pin: f64 = scores.iter().map(|s| s.prompt_tok as f64).sum::<f64>();
        let pout: f64 = scores.iter().map(|s| s.completion_tok as f64).sum::<f64>();
        let cost = price_per_100(m, pin, pout)
            .map(|c| format!("${c:.2}"))
            .unwrap_or_else(|| "n/a".into());
        let waste_per_case = total_waste(&scores) as f64 / scores.len().max(1) as f64;
        println!(
            "| {m} | {ok}/{n} | {:.2} | {:.2} | {:.0} | {cost} | {mean_s:.1} |",
            mean_closeness(&scores),
            waste_per_case,
            tokens_per_correct(&scores),
        );
        if a >= 0.8 && total_waste(&scores) <= waste_bar {
            let c = price_per_100(m, pin, pout).unwrap_or(0.0);
            if best.as_ref().map(|(_, bc)| c < *bc).unwrap_or(true) {
                best = Some((m.clone(), c));
            }
        }
        all.extend(scores);
    }
    match best {
        Some((m, c)) => println!(
            "\n**Cheapest model with acc ≥ 0.8 and ≤ {:.1} wasted calls/case: `{m}` (${c:.2}/100 answers).**",
            waste_bar as f64 / n_cases.max(1) as f64
        ),
        None => println!("\n_No model cleared the bar (acc ≥ 0.8, ≤ ~0.5 wasted calls/case)._"),
    }
    all
}

async fn cmd_robustness(engine: &EngineState, model: &str, ws: &Path, iters: usize) -> Vec<CaseScore> {
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
        let scores = run_battery(engine, &cases, model, label, None, "trap", iters).await;
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

async fn cmd_session_memory(engine: &EngineState, model: &str, g: &Goldens, iters: usize) -> Vec<CaseScore> {
    set_model(engine, model);
    let tg = &g.tables["txns_00"];
    let by_cat = tg.by_category.clone();
    let (c1, a1) = by_cat.iter().next().map(|(k, v)| (k.clone(), *v)).unwrap_or_default();
    let (c2, a2) = by_cat.iter().nth(1).map(|(k, v)| (k.clone(), *v)).unwrap_or_default();
    let q1 = format!("in txns_00.csv, what did I spend on {c1} in total?");
    let q2 = format!("and what about {c2}?");
    let iters = iters.max(1);
    println!("\n# Session memory  ·  model `{model}`   ({iters} iter(s))\n");
    legend();
    println!("| condition | turn-2 correct | rate | turn-2 close(det) | turn-2 steps |");
    println!("|---|:-:|--:|--:|--:|");
    let case2 = |a: f64, c: &str| EvalCase {
        id: "sm_turn2",
        category: "Aggregate",
        question: q2.clone(),
        gold: Gold::Figures(vec![a]),
        min_tools: 1,
        reference: format!("You spent {a:.2} on {c}."),
    };
    let mut all = Vec::new();
    for (label, keep) in [("memory on", true), ("memory off", false)] {
        let c2case = case2(a2, &c2);
        let mut oks = 0usize;
        let (mut cd, mut steps, mut ptok, mut ctok) = (0f32, 0usize, 0u64, 0u64);
        for it in 0..iters {
            let conv = format!("sm-{label}-{it}");
            run_case(engine, &conv, &q1, None).await; // turn 1 primes memory
            if !keep {
                engine.forget_conversation(&conv);
            }
            let r2 = run_case(engine, &conv, &q2, None).await;
            if grade(&r2, &c2case.gold) {
                oks += 1;
            }
            cd += closeness_det(&r2, &c2case);
            steps += r2.steps;
            ptok += r2.prompt_tok as u64;
            ctok += r2.completion_tok as u64;
        }
        let n = iters as f32;
        let correct = oks * 2 > iters;
        println!(
            "| {label} | {} | {:.0}% | {:.2} | {} |",
            yn(correct),
            oks as f32 / n * 100.0,
            cd / n,
            steps / iters
        );
        all.push(CaseScore {
            id: format!("sm_turn2_{label}"),
            model: model.into(),
            profile: label.into(),
            correct,
            correct_rate: oks as f32 / n,
            iters,
            closeness_det: cd / n,
            closeness_judge: None,
            waste: Waste::default(),
            prompt_tok: (ptok / iters as u64) as u32,
            completion_tok: (ctok / iters as u64) as u32,
            total_s: 0.0,
            first_tok_s: None,
            steps: steps / iters,
            hard_fail: false,
            err: None,
        });
    }
    let _ = (a1, &q1, tg);
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
                "correct": s.correct, "correct_rate": s.correct_rate, "iters": s.iters,
                "closeness_det": s.closeness_det,
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
    let iters: usize = opt("--iters").and_then(|s| s.parse().ok()).unwrap_or(1).max(1);
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
        "accuracy" => cmd_accuracy(&engine, &cases, &models, judge, iters).await,
        "prompt-ablation" => cmd_prompt_ablation(&engine, &cases, &models[0], judge, iters).await,
        "folder-scale" => cmd_folder_scale(&engine, &models[0], &ws, iters).await,
        "model-ladder" => cmd_model_ladder(&engine, &cases, &models, judge, iters).await,
        "robustness" => cmd_robustness(&engine, &models[0], &ws, iters).await,
        "session-memory" => cmd_session_memory(&engine, &models[0], &g, iters).await,
        "all" => {
            let mut v = cmd_accuracy(&engine, &cases, &models, judge, iters).await;
            v.extend(cmd_robustness(&engine, &models[0], &ws, iters).await);
            v.extend(cmd_session_memory(&engine, &models[0], &g, iters).await);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn rr(text: &str, ev: Vec<EvidenceItem>) -> RunResult {
        RunResult {
            text: text.into(),
            evidence: ev,
            hard_fail: false,
            prompt_tok: 0,
            completion_tok: 0,
            total: Duration::ZERO,
            first_token: None,
            steps: 0,
            err: None,
        }
    }
    fn ev(tool: &str, summary: &str, err: Option<&str>) -> EvidenceItem {
        EvidenceItem {
            tool: tool.into(),
            // distinct args so two calls aren't seen as an exact repeat
            args: serde_json::json!({ "q": summary }),
            note: None,
            sql: Some(format!("SELECT /*{summary}*/ 1")),
            result_summary: summary.into(),
            columns: None,
            rows: None,
            row_count: None,
            output: None,
            ms: 1,
            error: err.map(str::to_string),
        }
    }

    #[test]
    fn numbers_and_close() {
        assert_eq!(numbers_in("total $6,100.00 up 12% since 2024"), vec![6100.0, 12.0, 2024.0]);
        assert!(close(6100.0, 6100.4));
        assert!(close(1000.0, 1009.0)); // within 1%
        assert!(!close(6100.0, 6300.0));
    }

    #[test]
    fn grade_by_gold_kind() {
        assert!(grade(&rr("Your total was 6,100.", vec![]), &Gold::Figures(vec![6100.0])));
        assert!(!grade(&rr("Your total was 5,900.", vec![]), &Gold::Figures(vec![6100.0])));
        assert!(grade(&rr("Target 1250, raised in March 2024.", vec![]),
            &Gold::Contains(vec!["1250", "march 2024"])));
        assert!(grade(&rr("Your files can't tell the future.", vec![]), &Gold::Refusal));
        assert!(!grade(&rr("You'll spend 4200 next month.", vec![]), &Gold::Refusal));
        assert!(grade(&rr("I answer questions about your files.", vec![]), &Gold::NoTool));
        assert!(!grade(&rr("...", vec![ev("run_sql", "1 row: 5", None)]), &Gold::NoTool));
    }

    #[test]
    fn waste_is_conservative() {
        // one run_sql that feeds the answer -> nothing wasted
        let clean = rr("The total is 450.", vec![ev("run_sql", "1 row: total 450", None)]);
        assert_eq!(classify_waste(&clean).total(), 0);

        // one orientation describe_schema before any query is FREE
        let oriented = rr(
            "The total is 450.",
            vec![
                ev("describe_schema", "ledger: 3 cols", None),
                ev("run_sql", "1 row: total 450", None),
            ],
        );
        assert_eq!(classify_waste(&oriented).total(), 0);

        // a describe_schema AFTER a query already worked is redundant;
        // an errored call and an exact repeat each count once
        let messy = rr(
            "The total is 450.",
            vec![
                ev("run_sql", "1 row: total 450", None),
                ev("describe_schema", "ledger: 3 cols", None), // after a query -> redundant
                ev("run_sql", "err", Some("no such column: x")), // errored
                ev("run_sql", "1 row: total 450", None),        // exact repeat of call #1
            ],
        );
        let w = classify_waste(&messy);
        assert_eq!(w.redundant_schema, 1, "{:?}", w.breakdown());
        assert_eq!(w.errored, 1, "{:?}", w.breakdown());
        assert_eq!(w.duplicate, 1, "{:?}", w.breakdown());
        assert_eq!(w.total(), 3);

        // speculative only fires once a run has 3+ successful queries
        let spec = rr(
            "The answer is 450.",
            vec![
                ev("run_sql", "1 row: total 450", None),
                ev("run_sql", "12 rows: avg 37", None), // feeds nothing
                ev("run_sql", "3 rows: max 99", None),  // feeds nothing
            ],
        );
        let w = classify_waste(&spec);
        assert_eq!(w.speculative, 2, "{:?}", w.breakdown());
    }

    #[test]
    fn closeness_rewards_grounded_figures() {
        let case = EvalCase {
            id: "x", category: "Aggregate", question: "q".into(),
            gold: Gold::Figures(vec![450.0]), min_tools: 1,
            reference: "Your total spending was 450.".into(),
        };
        let good = rr("Your total spending was 450.", vec![ev("run_sql", "1 row: total 450", None)]);
        let bad = rr("Your total was 999.", vec![ev("run_sql", "1 row: total 450", None)]);
        assert!(closeness_det(&good, &case) > 0.8);
        assert!(closeness_det(&bad, &case) < 0.5);
    }
}
