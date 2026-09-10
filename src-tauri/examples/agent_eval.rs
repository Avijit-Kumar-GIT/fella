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
//!   memory           cold-run accuracy after priming, per-folder memory on vs off
//!   bench --dir <d>  a JSONL battery of real folder-QA tasks, same grader +
//!                    metrics as `accuracy`, with $/100. --harness:
//!                      fella (default) — the full loop
//!                      bare            — one chat call, files in the prompt,
//!                                        no tools; the baseline that isolates
//!                                        the harness's lift (Δacc = fella-bare)
//!                      openai-ci       — OpenAI Responses code_interpreter
//!   all              accuracy + robustness + session-memory + memory
//!
//! Opts: --models "a,b,c"  --judge <model>  --iters N (default 1)
//!       --only <id-substr>  --json <path>  --compare <old.json>  --dir <path>
//!
//! `bench` dir layout: `<d>/cases.jsonl` (one JSON object per line) + the data
//! files it names (paths relative to `<d>`). Each line:
//!   {"id": "...", "question": "...", "files": ["payments.csv"],
//!    "gold": {"figures": [123.45]} | {"approx": [0.18, 0.005]}
//!          | {"contains": ["Rent"]} | "refusal" | "notool",
//!    "tier": "easy", "reference": "..."}
//! Env:  EVAL_SHOW_ANSWERS=1  print every answer + its evidence to stderr
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
use fella_lib::engine::{memory, verify, AskEvent, EngineState};

// --- what a correct answer looks like -------------------------------------

#[derive(Clone)]
enum Gold {
    /// every figure must appear in the answer, within `close()` tolerance
    Figures(Vec<f64>),
    /// one figure within an explicit absolute tolerance (ratios, rounded
    /// numbers, an exact 0). `Approx(0.0, ..)` also passes on "none / nothing".
    Approx(f64, f64),
    /// each substring must be present (case-insensitive; a bare integer matches
    /// the number, so "1250" == "1,250")
    Contains(Vec<&'static str>),
    /// the answer must decline (no computed figure, says it can't)
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
        // a digit run glued to a letter or underscore is an identifier
        // fragment (txns_00.csv, q1, gpt-5), not a figure the model stated
        let in_identifier = start > 0
            && (b[start - 1].is_ascii_alphabetic() || b[start - 1] == b'_');
        while i < b.len() && (b[i].is_ascii_digit() || b[i] == b',' || b[i] == b'.') {
            i += 1;
        }
        if in_identifier {
            continue;
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
    // normalise typographic punctuation: a curly apostrophe (U+2019) in
    // "can't", and — the one that bit a real run — a non-breaking / en / em
    // dash where a model wrote a date like "2024‑11" instead of "2024-11".
    let low = r
        .text
        .to_lowercase()
        .replace(['\u{2019}', '\u{02BC}'], "'")
        .replace(['\u{201C}', '\u{201D}'], "\"")
        .replace(
            ['\u{2010}', '\u{2011}', '\u{2012}', '\u{2013}', '\u{2014}', '\u{2212}'],
            "-",
        );
    match gold {
        Gold::Figures(want) => {
            let got = numbers_in(&r.text);
            want.iter().all(|w| got.iter().any(|g| close(*g, *w)))
        }
        Gold::Approx(want, tol) => {
            let non_year: Vec<f64> = numbers_in(&r.text)
                .into_iter()
                .filter(|n| !(1900.0..=2100.0).contains(n))
                .collect();
            if *want == 0.0 && non_year.is_empty() && says_zero(&low) {
                return true; // "no healthcare transactions" == 0
            }
            non_year.iter().any(|g| (g - want).abs() <= *tol)
        }
        Gold::Contains(subs) => {
            let got = numbers_in(&r.text);
            subs.iter().all(|s| {
                // an all-digit sub matches the *number* (so "1250" == "1,250"
                // == "£1,250"); anything else is a literal substring, and a
                // `|` in it means "any of these forms" (e.g. a month written
                // "November 2021" or "2021-11")
                match s.parse::<f64>() {
                    Ok(want) => got.iter().any(|g| close(*g, want)),
                    Err(_) => s.to_lowercase().split('|').any(|alt| low.contains(alt.trim())),
                }
            })
        }
        Gold::Refusal => {
            // no computed figure (years excused), and it plainly declines
            let no_figures = numbers_in(&r.text).iter().all(|n| (1900.0..=2100.0).contains(n));
            const DECLINES: [&str; 12] = [
                "can't", "cannot", "can not", "unable", "no data", "not available",
                "no way to", "don't have", "isn't in", "doesn't", "no records", "not in the",
            ];
            no_figures && DECLINES.iter().any(|p| low.contains(p))
        }
        Gold::NoTool => r.evidence.is_empty() && !r.text.trim().is_empty(),
    }
}

/// A spelled-out zero ("no healthcare transactions", "nothing", "n/a") standing
/// in for the figure 0. Shared by `grade` and `closeness_det` a model that
/// correctly answers "you spent nothing" shouldn't score as if it missed 0.
fn says_zero(low: &str) -> bool {
    ["none", "nothing", "zero", "no ", "n/a", "not have any"]
        .iter()
        .any(|p| low.contains(p))
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
        Gold::Approx(w, _) => vec![*w],
        _ => Vec::new(),
    };
    let got = numbers_in(&r.text);
    let low = r.text.to_lowercase();
    let figure_recall = if want.is_empty() {
        1.0
    } else {
        want.iter()
            .filter(|w| {
                got.iter().any(|g| close(*g, **w)) || ((**w).abs() < 1e-9 && says_zero(&low))
            })
            .count() as f32
            / want.len() as f32
    };
    // an aggregate over no rows (one all-NULL row, or none) grounds the answer 0
    let empty_agg = r.evidence.iter().any(|e| {
        e.tool == "run_sql"
            && e.error.is_none()
            && e.rows.as_ref().is_some_and(|rs| {
                rs.is_empty() || (rs.len() == 1 && rs[0].iter().all(|c| c.is_null()))
            })
    });
    // a figure is grounded if it shows up in some evidence cell / summary
    let grounded = |n: f64| {
        (n.abs() < 1e-9 && empty_agg)
            || r.evidence.iter().any(|e| {
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
    // Grounding needs an evidence trail. A harness that cites figures but
    // doesn't expose one (the `openai-ci` comparison) would be capped at 0.7
    // for a perfect answer, so drop that term and renormalise recall + wording.
    // Fella runs always have evidence; a no-figure answer is unaffected either
    // way, so this only moves the CI rows.
    if r.evidence.is_empty() && !got.is_empty() {
        return (0.71 * figure_recall + 0.29 * token_f1(&r.text, &case.reference)).clamp(0.0, 1.0);
    }
    (0.5 * figure_recall + 0.3 * (1.0 - ungrounded_rate) + 0.2 * token_f1(&r.text, &case.reference))
        .clamp(0.0, 1.0)
}

/// A count, in **tool calls**, of the ones that did no useful work. Four kinds,
/// each call counted once.
#[derive(Default, Clone, Copy)]
struct Waste {
    /// an exact `(tool, args)` repeat, or the engine's "skipped (duplicate call)"
    duplicate: usize,
    /// an `inspect_table` / `list_files` peek that wasn't the
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

/// Count the tool calls that did no useful work.
///
/// Confidence per kind:
/// - `duplicate` / `errored` — always accurate.
/// - `redundant_schema` — every `inspect_table` / `list_files`
///   call. Accurate on the default and other small workspaces, where the system
///   prompt's schema block already lists the tables, their columns and sample
///   rows so any such call is the model re-discovering what it was told.
///   Over-counts on a large `folder-scale` workspace (schema block is names
///   only there, so a peek can be legitimate); read that column with the table
///   count in mind.
/// - `speculative` — a `run_sql` whose numbers the answer never uses, and only
///   once the run has made 3+ successful queries. Below that we can't tell an
///   intermediate step from a dead end, so we don't guess.
fn classify_waste(r: &RunResult) -> Waste {
    let ans = numbers_in(&r.text);
    let n_ok_sql = r
        .evidence
        .iter()
        .filter(|e| e.tool == "run_sql" && e.error.is_none())
        .count();
    let mut w = Waste::default();
    let mut seen: Vec<(String, String)> = Vec::new();

    for e in &r.evidence {
        let key = (e.tool.clone(), e.args.to_string());
        let repeat = seen.contains(&key);
        seen.push(key);

        if e.result_summary == "skipped (duplicate call)" || repeat {
            w.duplicate += 1;
        } else if e.error.is_some() {
            w.errored += 1;
        } else if matches!(e.tool.as_str(), "inspect_table" | "list_files") {
            w.redundant_schema += 1;
        } else if e.tool == "run_sql" && n_ok_sql >= 3 {
            let produced = numbers_in(&e.result_summary);
            let feeds_answer = produced.iter().any(|p| ans.iter().any(|a| close(*a, *p)));
            if !produced.is_empty() && !feeds_answer {
                w.speculative += 1;
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
//
// FROZEN. These cases and their `gold` values are the specification of what a
// good answer is. Do NOT edit a case or its expected value to make a number
// move that is teaching to the test. A new case or a changed expectation is
// its own commit, argued on its own merits. `grade()` may be fixed only with a
// trace showing the model's actual behaviour and an argument that the change
// makes the grader match ground truth (see the note above `grade`).

fn battery(g: &Goldens, rent_total: f64) -> Vec<EvalCase> {
    let t0 = "txns_00";
    let tg: &TableGold = &g.tables[t0];
    let top3 = tg.top3_categories();
    let top_cat_amt = top3.first().map(|(_, v)| *v).unwrap_or_default();
    let (top_merch, top_merch_amt) = tg.top_merchant.clone();
    let transport = *tg.by_category.get("transport").unwrap_or(&0.0);
    let rent_cat = *tg.by_category.get("rent").unwrap_or(&0.0);
    let rent_share = rent_cat / tg.total * 100.0;
    let (top_month, _top_month_amt) = tg.top_month(); // "YYYY-MM"
    let top_month_name = month_name(&top_month);
    let top_month_year = &top_month[..4];
    let avg_rent = rent_total / 5.0;
    let (wk_top_act, wk_top_min) = g.workout_top_activity.clone();

    let c = |id, category, question: String, gold, reference: String| EvalCase {
        id,
        category,
        question: question.trim().to_string(),
        gold,
        min_tools: 1,
        reference,
    };

    vec![
        // --- no tool needed ---
        c("chitchat", "NoTool",
          "what kinds of questions can you help me with?".into(),
          Gold::NoTool,
          "I answer questions about the files in your folder by running SQL/Python, and I show the working.".into()),
        c("define_term", "NoTool",
          "what does \"trailing twelve months\" mean?".into(),
          Gold::NoTool,
          "It's the sum over the most recent 12 months a rolling one-year window ending today.".into()),

        // --- single aggregate ---
        c("agg_rent", "Aggregate",
          "what's the total amount I paid in rent.csv?".into(),
          Gold::Figures(vec![rent_total]),
          format!("You paid {rent_total:.2} in total.")),
        c("agg_total", "Aggregate",
          format!("what was my total spending in {t0}.csv?"),
          Gold::Figures(vec![tg.total]),
          format!("Total spending in {t0} was {:.2}.", tg.total)),
        c("filter_agg", "Filter",
          format!("how much did I spend on transport in {t0}.csv?"),
          Gold::Figures(vec![transport]),
          format!("Transport spending was {transport:.2}.")),

        // --- the parse_num trap: AVG over a text amount column ---
        c("avg_rent_text", "Trap",
          "what's my average rent payment in rent.csv?".into(),
          Gold::Approx(avg_rent, 1.0),
          format!("Your average rent payment was {avg_rent:.2}.")),

        // --- min / max ---
        c("max_txn", "MinMax",
          format!("what was my single largest transaction in {t0}.csv?"),
          Gold::Approx(tg.max_amount, 0.5),
          format!("Your largest single transaction was {:.2}.", tg.max_amount)),

        // --- group / top-N ---
        c("group_top3", "GroupTopN",
          format!("in {t0}.csv, what did I spend per category? give the top 3."),
          Gold::Figures(vec![top_cat_amt]),
          format!("Top categories: {} {:.0}, {} {:.0}, {} {:.0}.",
                  top3[0].0, top3[0].1, top3[1].0, top3[1].1, top3[2].0, top3[2].1)),

        // --- time series ---
        // Accept either "November 2021" or the ISO "2021-11" both name the
        // right month; the grader shouldn't punish the ISO rendering.
        c("time_series", "TimeSeries",
          format!("which month had the highest total spending in {t0}.csv?"),
          Gold::Contains(vec![leak(&format!("{top_month_name} {top_month_year}|{top_month}"))]),
          format!("{top_month_name} {top_month_year} was your highest-spending month.")),

        // --- ratio / share (rounded) ---
        c("share", "Ratio",
          format!("what share of my {t0}.csv spending was rent?"),
          Gold::Approx(rent_share, 1.5),
          format!("Rent was about {rent_share:.0}% of your spending.")),

        // --- multi-step (two figures, dependent) ---
        c("multi_step", "MultiStep",
          format!("in {t0}.csv, which merchant did I spend the most at, and roughly how much?"),
          Gold::Figures(vec![top_merch_amt]),
          format!("Your biggest merchant was {top_merch}, about {top_merch_amt:.0}.")),

        // --- cross-file: transactions + folder ---
        c("grand_total", "Aggregate",
          "across every transactions table in the folder, how many rows and what total?".into(),
          Gold::Figures(vec![g.grand_rows() as f64, g.grand_total()]),
          format!("{} rows totalling {:.2} across the folder.", g.grand_rows(), g.grand_total())),

        // --- non-financial table (domain diversity) ---
        c("workout_total", "Aggregate",
          "how many minutes did I exercise in total, per workouts.csv?".into(),
          Gold::Figures(vec![g.workout_total_minutes as f64]),
          format!("You exercised {} minutes in total.", g.workout_total_minutes)),
        c("workout_top", "GroupTopN",
          "which activity did I spend the most minutes on, in workouts.csv?".into(),
          Gold::Contains(vec![leak(&wk_top_act)]),
          format!("You spent the most minutes on {wk_top_act} ({wk_top_min}).")),

        // --- documents ---
        c("doc_lookup", "DocLookup",
          "according to my notes, what is the monthly rent target and when did it change?".into(),
          Gold::Contains(vec!["1250", "march 2024"]),
          "The monthly rent target is 1250, raised in March 2024.".into()),
        c("doc_summary", "DocSummary",
          "what does my 2024 notes file mention happened that year?".into(),
          Gold::Contains(vec!["gym", "insurance", "flights"]),
          "The 2024 notes mention switching gym, annual insurance in April, and flights in July.".into()),

        // --- honest empty result ---
        c("empty_cat", "EmptyResult",
          format!("how much did I spend on healthcare in {t0}.csv?"),
          Gold::Approx(0.0, 0.5),
          "There are no healthcare transactions, so 0.".into()),

        // --- must decline: about the future ---
        c("refusal", "Refusal",
          "how much will I spend next month?".into(),
          Gold::Refusal,
          "Your files only hold past records, so I can't tell you next month's spend.".into()),
    ]
}

/// Leak a `String` to `&'static str` the battery is built once at startup and
/// lives for the whole run, so a few tiny leaks are fine and let `Gold::Contains`
/// keep its `&'static` shape.
fn leak(s: &str) -> &'static str {
    Box::leak(s.to_string().into_boxed_str())
}

fn month_name(yyyy_mm: &str) -> String {
    const M: [&str; 12] = [
        "January", "February", "March", "April", "May", "June", "July", "August", "September",
        "October", "November", "December",
    ];
    yyyy_mm
        .get(5..7)
        .and_then(|mm| mm.parse::<usize>().ok())
        .and_then(|m| M.get(m.wrapping_sub(1)))
        .map(|s| s.to_string())
        .unwrap_or_default()
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
        // OpenRouter model ladder (folder-qa bench), Sept 2026 list prices
        ("deepseek-v4-flash-0731", 0.065, 0.18),
        ("deepseek-v4-pro-0813", 1.0494, 3.1482),
        ("glm-5.3-flash", 0.075, 0.25),
        ("nemotron-3.5-lightning", 0.08, 0.20),
        ("muse-spark-1.3", 1.25, 4.25),
        ("muse-spark-1.3-contributor", 0.10, 0.20),
        ("muse-glimmer-30b", 0.30, 1.10),
        ("inkling-small", 0.45, 1.20),
        ("gemini-3.8-flash", 0.75, 3.75),
        ("gemma4:31b", 0.0, 0.0),
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

// --- comparison harness: OpenAI code_interpreter -------------------------
//
// `bench --harness openai-ci` runs each case through the OpenAI Responses API
// with the `code_interpreter` tool instead of Fella's engine — a locked-harness
// comparison on the same task set + grader. Small files are embedded in the
// prompt (no multipart upload, no extra reqwest feature); a case whose files
// exceed `CI_MAX_EMBED` bytes is skipped with an error.

const CI_MAX_EMBED: usize = 100 * 1024;

struct CiHarness {
    http: reqwest::Client,
    base: String,
    key: String,
    model: String,
}

/// Runner for one case iteration.
enum Runner<'a> {
    /// Fella's full loop: tools, schema block, verification.
    Fella,
    /// No harness — the case's files pasted into one chat completion, no
    /// tools, no execution. The baseline that isolates the harness's lift.
    Bare { dir: &'a Path, files: &'a [String] },
    /// OpenAI Responses API + code_interpreter (a comparison harness).
    OpenAiCi { h: &'a CiHarness, dir: &'a Path, files: &'a [String] },
}

/// Read a case's files into a fenced blob for a prompt-only runner. `Err` if a
/// file is missing or exceeds `CI_MAX_EMBED`.
fn embed_files(dir: &Path, files: &[String]) -> Result<String, String> {
    let mut blob = String::new();
    for f in files {
        match std::fs::read(dir.join(f)) {
            Ok(bytes) => match String::from_utf8(bytes) {
                Ok(c) if blob.len() + c.len() <= CI_MAX_EMBED => {
                    blob.push_str(&format!("### {f}\n```\n{c}\n```\n\n"));
                }
                Ok(_) => return Err(format!("{f} too large to embed")),
                // A binary format (xlsx, pdf). `bare` has no parser — say so and
                // let the model answer "can't" rather than hard-erroring the case.
                Err(e) => blob.push_str(&format!(
                    "### {f}\n(binary file, {} bytes — not readable without tools)\n\n",
                    e.as_bytes().len()
                )),
            },
            Err(e) => return Err(format!("read {f}: {e}")),
        }
    }
    Ok(blob)
}

/// No-harness baseline: one chat completion, files in the prompt, no tools.
/// The model is whatever `set_model` last configured on the engine.
async fn run_bare(engine: &EngineState, dir: &Path, question: &str, files: &[String]) -> RunResult {
    let t0 = Instant::now();
    let blob = match embed_files(dir, files) {
        Ok(b) => b,
        Err(e) => {
            return RunResult {
                text: String::new(), evidence: Vec::new(), hard_fail: false,
                prompt_tok: 0, completion_tok: 0, total: t0.elapsed(),
                first_token: None, steps: 0, err: Some(format!("bare: {e}")),
            }
        }
    };
    let sys = "You are a careful analyst. Any files the user has are included in the message. \
Answer the question directly from them — reason it out yourself, no tools. Reply with ONLY the final \
number or short phrase, no explanation. If the files can't answer it, say so plainly.";
    let user = if blob.is_empty() {
        question.to_string()
    } else {
        format!("{blob}Question: {question}")
    };
    let res = engine.ask_once_usage(None, sys, &user).await;
    let total = t0.elapsed();
    match res {
        Ok((text, usage)) => {
            let (p, c) = usage
                .map(|u| (u.prompt_tokens, u.completion_tokens))
                // chars/4 estimate when the provider gave nothing
                .unwrap_or(((sys.len() + user.len()) as u32 / 4, text.len() as u32 / 4));
            RunResult {
                text, evidence: Vec::new(), hard_fail: false,
                prompt_tok: p, completion_tok: c, total,
                first_token: None, steps: 0, err: None,
            }
        }
        Err(e) => RunResult {
            text: String::new(), evidence: Vec::new(), hard_fail: false,
            prompt_tok: 0, completion_tok: 0, total, first_token: None,
            steps: 0, err: Some(format!("bare: {e}")),
        },
    }
}

fn ci_text(v: &serde_json::Value) -> String {
    if let Some(t) = v.get("output_text").and_then(|t| t.as_str()) {
        if !t.trim().is_empty() {
            return t.trim().to_string();
        }
    }
    // walk output[] for the assistant message's text parts
    let mut out = String::new();
    if let Some(items) = v.get("output").and_then(|o| o.as_array()) {
        for it in items {
            if it.get("type").and_then(|t| t.as_str()) != Some("message") {
                continue;
            }
            for part in it.get("content").and_then(|c| c.as_array()).into_iter().flatten() {
                if let Some(t) = part.get("text").and_then(|t| t.as_str()) {
                    out.push_str(t);
                }
            }
        }
    }
    out.trim().to_string()
}

fn ci_steps(v: &serde_json::Value) -> usize {
    v.get("output")
        .and_then(|o| o.as_array())
        .map(|a| {
            a.iter()
                .filter(|it| {
                    it.get("type")
                        .and_then(|t| t.as_str())
                        .is_some_and(|t| t.contains("code_interpreter"))
                })
                .count()
        })
        .unwrap_or(0)
}

async fn run_openai_ci(h: &CiHarness, dir: &Path, question: &str, files: &[String]) -> RunResult {
    let empty = |err: Option<String>, total: Duration| RunResult {
        text: String::new(),
        evidence: Vec::new(),
        hard_fail: false,
        prompt_tok: 0,
        completion_tok: 0,
        total,
        first_token: None,
        steps: 0,
        err,
    };
    let t0 = Instant::now();

    let blob = match embed_files(dir, files) {
        Ok(b) => b,
        Err(e) => return empty(Some(format!("ci: {e}")), t0.elapsed()),
    };

    let body = serde_json::json!({
        "model": h.model,
        "instructions": "You are a careful data analyst. The user's files are included in the message. \
Use the code_interpreter tool to compute the answer from them. Reply with only the final number or \
short phrase — no explanation, no restating the question. If the files cannot answer it, say so plainly.",
        "input": format!("{blob}Question: {question}"),
        "tools": [{ "type": "code_interpreter", "container": { "type": "auto" } }],
    });

    let resp = h
        .http
        .post(format!("{}/responses", h.base))
        .bearer_auth(&h.key)
        .json(&body)
        .send()
        .await;
    let total = t0.elapsed();
    let resp = match resp {
        Ok(r) => r,
        Err(e) => return empty(Some(format!("ci send: {e}")), total),
    };
    if !resp.status().is_success() {
        let s = resp.status();
        let b = resp.text().await.unwrap_or_default();
        return empty(Some(format!("ci {s}: {}", b.chars().take(200).collect::<String>())), total);
    }
    let v: serde_json::Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => return empty(Some(format!("ci parse: {e}")), total),
    };
    RunResult {
        text: ci_text(&v),
        evidence: Vec::new(),
        hard_fail: false,
        prompt_tok: v["usage"]["input_tokens"].as_u64().unwrap_or(0) as u32,
        completion_tok: v["usage"]["output_tokens"].as_u64().unwrap_or(0) as u32,
        total,
        first_token: None,
        steps: ci_steps(&v),
        err: None,
    }
}

/// Run one case `iters` times and fold: `correct` = majority, everything
/// numeric = mean.
#[allow(clippy::too_many_arguments)]
async fn score_case(
    engine: &EngineState,
    case: &EvalCase,
    model: &str,
    profile: &str,
    judge: Option<&str>,
    conv: &str,
    iters: usize,
    runner: &Runner<'_>,
) -> CaseScore {
    let iters = iters.max(1);
    let mut oks = 0usize;
    let (mut cd, mut cj_sum, mut cj_n) = (0f32, 0f32, 0usize);
    let (mut ptok, mut ctok, mut secs, mut steps) = (0u64, 0u64, 0f64, 0usize);
    let mut first_toks: Vec<f64> = Vec::new();
    let mut wastes: Vec<Waste> = Vec::new();
    let mut any_hard = false;
    let mut last_err = None;

    let show = std::env::var_os("EVAL_SHOW_ANSWERS").is_some();
    for it in 0..iters {
        let r = match runner {
            Runner::Fella => {
                run_case(engine, &format!("{conv}-{it}"), &case.question, None).await
            }
            Runner::Bare { dir, files } => run_bare(engine, dir, &case.question, files).await,
            Runner::OpenAiCi { h, dir, files } => {
                run_openai_ci(h, dir, &case.question, files).await
            }
        };
        let ok = grade(&r, &case.gold);
        if ok {
            oks += 1;
        }
        if show {
            eprintln!(
                "\n[{} {model} it{it}] {}\n  Q: {}\n  A: {}",
                case.id,
                if ok { "PASS" } else { "FAIL" },
                case.question,
                r.text.replace('\n', "\n     ")
            );
            for e in &r.evidence {
                eprintln!(
                    "     · {} {}ms{}{}",
                    e.tool,
                    e.ms,
                    e.sql.as_deref().map(|s| format!("  {s}")).unwrap_or_default(),
                    e.error.as_deref().map(|s| format!("  ERR {s}")).unwrap_or_default(),
                );
            }
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
    let n = scores.len();
    let ok = scores.iter().filter(|s| s.correct).count();
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
        out.push(score_case(engine, c, model, profile, judge, &conv, iters, &Runner::Fella).await);
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
    let n_cases = cases.len();
    // "clears the bar" thresholds: >=80% accuracy, <= ~0.5 wasted calls/case
    let waste_bar = (n_cases as f64 * 0.5).ceil() as usize;
    println!("| model | acc | close(det) | waste/case | tok/correct-ans | $/100 | mean wall s |");
    println!("|---|:-:|--:|--:|--:|--:|--:|");
    let mut all = Vec::new();
    let mut best_priced: Option<(String, f64)> = None; // (model, $/100)
    let mut cleared_unpriced: Vec<String> = Vec::new();
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
        let price = price_per_100(m, pin, pout);
        let cost = price.map(|c| format!("${c:.2}")).unwrap_or_else(|| "n/a".into());
        let waste_per_case = total_waste(&scores) as f64 / scores.len().max(1) as f64;
        println!(
            "| {m} | {ok}/{n} | {:.2} | {:.2} | {:.0} | {cost} | {mean_s:.1} |",
            mean_closeness(&scores),
            waste_per_case,
            tokens_per_correct(&scores),
        );
        if a >= 0.8 && total_waste(&scores) <= waste_bar {
            match price {
                Some(c) if best_priced.as_ref().map(|(_, bc)| c < *bc).unwrap_or(true) => {
                    best_priced = Some((m.clone(), c));
                }
                Some(_) => {}
                None => cleared_unpriced.push(m.clone()),
            }
        }
        all.extend(scores);
    }
    let per_case_bar = waste_bar as f64 / n_cases.max(1) as f64;
    match &best_priced {
        Some((m, c)) => println!(
            "\n**Cheapest priced model at acc ≥ 0.8 and ≤ {per_case_bar:.1} wasted calls/case: `{m}` (${c:.2}/100).**"
        ),
        None if cleared_unpriced.is_empty() => {
            println!("\n_No model cleared the bar (acc ≥ 0.8, ≤ {per_case_bar:.1} wasted calls/case)._")
        }
        None => {}
    }
    if !cleared_unpriced.is_empty() {
        println!(
            "_Also cleared the bar (no list price in the table): {}._",
            cleared_unpriced.join(", ")
        );
    }
    all
}

// --- external benchmark (`bench --dir`) -----------------------------------
//
// Runs a JSONL battery of real tasks through the same engine + metrics as
// `accuracy`, so Fella's purpose-built folder-QA loop can be compared, on a
// neutral task set, against a general data-analysis agent. Each case gets a
// fresh workspace holding only its own files.

#[derive(serde::Deserialize)]
struct BenchSpec {
    id: String,
    question: String,
    #[serde(default)]
    files: Vec<String>,
    gold: BenchGold,
    #[serde(default)]
    category: Option<String>,
    #[serde(default)]
    tier: Option<String>,
    #[serde(default)]
    reference: Option<String>,
}

#[derive(serde::Deserialize)]
#[serde(untagged)]
enum BenchGold {
    Figures { figures: Vec<f64> },
    /// `[value, absolute tolerance]`
    Approx { approx: [f64; 2] },
    Contains { contains: Vec<String> },
    /// `"refusal"` | `"notool"`
    Tag(String),
}

impl BenchGold {
    fn into_gold(self) -> Result<Gold, String> {
        Ok(match self {
            BenchGold::Figures { figures } => Gold::Figures(figures),
            BenchGold::Approx { approx: [v, tol] } => Gold::Approx(v, tol),
            BenchGold::Contains { contains } => {
                Gold::Contains(contains.iter().map(|s| leak(s)).collect())
            }
            BenchGold::Tag(t) => match t.to_ascii_lowercase().as_str() {
                "refusal" => Gold::Refusal,
                "notool" | "no_tool" => Gold::NoTool,
                other => return Err(format!("unknown gold {other:?}")),
            },
        })
    }
}

/// Parse `<dir>/cases.jsonl`; blank lines and `#` comments are skipped.
/// Returns `(files, case)` pairs so the runner can stage each workspace.
fn load_bench_dir(dir: &Path) -> Vec<(Vec<String>, EvalCase)> {
    let path = dir.join("cases.jsonl");
    let txt = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("bench: can't read {}: {e}", path.display()));
    let mut out = Vec::new();
    for (n, line) in txt.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let spec: BenchSpec = serde_json::from_str(line)
            .unwrap_or_else(|e| panic!("bench: {}:{}: {e}", path.display(), n + 1));
        let cat = leak(
            spec.tier
                .as_deref()
                .or(spec.category.as_deref())
                .unwrap_or("bench"),
        );
        let gold = spec
            .gold
            .into_gold()
            .unwrap_or_else(|e| panic!("bench: {}:{}: {e}", path.display(), n + 1));
        out.push((
            spec.files,
            EvalCase {
                id: leak(&spec.id),
                category: cat,
                question: spec.question,
                gold,
                min_tools: 1,
                reference: spec.reference.unwrap_or_default(),
            },
        ));
    }
    out
}

fn safe_dirname(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect()
}

#[allow(clippy::too_many_arguments)]
async fn cmd_bench(
    engine: &EngineState,
    dir: &Path,
    data_dir: &Path,
    models: &[String],
    judge: Option<&str>,
    iters: usize,
    only: Option<&str>,
    harness: &str,
    json_out: Option<&str>,
) -> Vec<CaseScore> {
    let mut cases = load_bench_dir(dir);
    if let Some(sub) = only {
        cases.retain(|(_, c)| c.id.contains(sub));
    }

    // Comparison harness setup (once).
    let ci = if harness == "openai-ci" {
        let auth = std::fs::read_to_string(data_dir.join("auth.json")).unwrap_or_default();
        let key = serde_json::from_str::<serde_json::Value>(&auth)
            .ok()
            .and_then(|v| v.get("apikey:openai").and_then(|k| k.as_str()).map(str::to_string))
            .or_else(|| std::env::var("OPENAI_API_KEY").ok())
            .unwrap_or_else(|| {
                eprintln!("bench --harness openai-ci: no `apikey:openai` in {}/auth.json and no OPENAI_API_KEY", data_dir.display());
                std::process::exit(2);
            });
        Some(CiHarness {
            http: reqwest::Client::new(),
            base: env("OPENAI_BASE_URL", "https://api.openai.com/v1"),
            key,
            model: String::new(), // set per model below
        })
    } else {
        None
    };

    println!(
        "\n# External benchmark  ·  `{}`  ·  harness `{harness}`  ({iters} iter(s)/case, {} cases)\n",
        dir.display(),
        cases.len()
    );
    legend();
    println!("| model | case | correct | rate | close(det) | waste | in tok | out tok | $/100 | wall s |");
    println!("|---|---|:-:|--:|--:|--:|--:|--:|--:|--:|");

    let staging = std::env::temp_dir().join("fella-bench-ext");
    // A hosted endpoint (ollama-cloud especially) degrades under 15+ cases
    // back-to-back — the model starts emitting broken SQL. A short breather
    // between cases keeps a single-iter run honest; `--iters 3` + majority is
    // the real defence. `BENCH_PAUSE_MS=0` disables.
    let pause_ms: u64 = env("BENCH_PAUSE_MS", "400").parse().unwrap_or(400);
    let mut all = Vec::new();

    for m in models {
        // Fella drives the engine; the CI harness carries its own model string.
        let ci_for_model = ci.as_ref().map(|c| CiHarness {
            http: c.http.clone(),
            base: c.base.clone(),
            key: c.key.clone(),
            model: m.rsplit('/').next().unwrap_or(m).to_string(),
        });
        if ci_for_model.is_none() && !set_model(engine, m) {
            println!("| {m} | | | | | | | | | save failed |");
            continue;
        }
        let mut scores: Vec<CaseScore> = Vec::new();
        for (files, case) in &cases {
            let conv = format!("bench-{m}-{}", safe_dirname(case.id));
            let runner = if let Some(h) = &ci_for_model {
                Runner::OpenAiCi { h, dir, files }
            } else if harness == "bare" {
                Runner::Bare { dir, files }
            } else {
                // Fella: stage a fresh workspace with just this case's files.
                let ws = staging.join(safe_dirname(case.id));
                let _ = std::fs::remove_dir_all(&ws);
                if std::fs::create_dir_all(&ws).is_err() {
                    println!("| {m} | {} | ERR | | | | | | | mkdir failed |", case.id);
                    continue;
                }
                let mut staged = true;
                for f in files {
                    let src = dir.join(f);
                    let name = Path::new(f).file_name().unwrap_or(std::ffi::OsStr::new(f));
                    if std::fs::copy(&src, ws.join(name)).is_err() {
                        println!("| {m} | {} | ERR | | | | | | | missing {} |", case.id, f);
                        staged = false;
                        break;
                    }
                }
                if !staged {
                    continue;
                }
                if engine.open_workspace(&ws).is_err() {
                    println!("| {m} | {} | ERR | | | | | | | open_workspace failed |", case.id);
                    continue;
                }
                Runner::Fella
            };
            let s = score_case(engine, case, m, "bench", judge, &conv, iters, &runner).await;
            let price = price_per_100(m, s.prompt_tok as f64, s.completion_tok as f64);
            println!(
                "| {} | {} | {} | {:.0}% | {:.2} | {} `{}` | {} | {} | {} | {:.1} |",
                s.model,
                s.id,
                if s.err.is_some() { "ERR".into() } else { yn(s.correct) },
                s.correct_rate * 100.0,
                s.closeness_det,
                s.waste.total(),
                s.waste.breakdown(),
                s.prompt_tok,
                s.completion_tok,
                price.map(|c| format!("${c:.2}")).unwrap_or_else(|| "n/a".into()),
                s.total_s,
            );
            std::io::stdout().flush().ok();
            scores.push(s);
            if pause_ms > 0 {
                tokio::time::sleep(Duration::from_millis(pause_ms)).await;
            }
        }

        let (ok, n) = acc(&scores);
        let pin: f64 = scores.iter().map(|s| s.prompt_tok as f64).sum();
        let pout: f64 = scores.iter().map(|s| s.completion_tok as f64).sum();
        let avg_price = price_per_100(m, pin / n.max(1) as f64, pout / n.max(1) as f64);
        println!(
            "| **{m}** | **summary** | **{ok}/{n} correct** | | **{:.2}** | **{} waste** | | | **{}** | **{:.0} tok/correct** |",
            mean_closeness(&scores),
            total_waste(&scores),
            avg_price.map(|c| format!("${c:.2}/100 avg")).unwrap_or_else(|| "n/a".into()),
            tokens_per_correct(&scores),
        );
        all.extend(scores);
        // Flush after every model so a later hang / crash doesn't lose the
        // models already done (the expensive ladder runs are long).
        if let Some(p) = json_out {
            write_json(p, &all);
        }
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

fn first_line(s: &str) -> String {
    s.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("").to_string()
}

/// A deliberately messy one-file folder: cryptic column names, amounts as text,
/// and a `cat` column whose rent shows up as `Rent` / `rent` / `HOUSING` /
/// `mortgage` / `housing`. Returns (rent incl. housing+mortgage, literal-rent-only).
fn write_messy_spend(dir: &Path) -> (f64, f64) {
    std::fs::create_dir_all(dir).unwrap();
    let rows: &[(&str, &str, &str)] = &[
        ("2024-01-03", "\"$1,200.00\"", "Rent"),
        ("2024-01-14", "45.20", "groceries"),
        ("2024-02-03", "\"1,200\"", "HOUSING"),
        ("2024-02-19", "88.10", "Groceries"),
        ("2024-03-03", "1200", "mortgage"),
        ("2024-03-11", "12.00", "transport"),
        ("2024-04-03", "\"$1,250.00\"", "rent"),
        ("2024-04-22", "52.00", "groceries"),
        ("2024-05-03", "\"$1,250.00\"", "Rent"),
        ("2024-05-18", "9.50", "transport"),
        ("2024-06-03", "1250", "housing"),
    ];
    let mut s = String::from("txn_dt,amt,cat,memo\n");
    for (d, a, c) in rows {
        s.push_str(&format!("{d},{a},{c},\n"));
    }
    std::fs::write(dir.join("spend.csv"), s).unwrap();
    let rent_all = 1200.0 + 1200.0 + 1200.0 + 1250.0 + 1250.0 + 1250.0;
    let rent_literal = 1200.0 + 1250.0 + 1250.0;
    (rent_all, rent_literal)
}

/// Per-folder memory, cross-session: in session 1 the user asks about rent and
/// then *corrects* Fella ("housing and mortgage count as rent too"). Session 2
/// is a **cold conversation** (memory is the only carry) that asks for total
/// rent. Memory on should apply the correction and include housing+mortgage;
/// memory off can only match the literal `rent` rows.
async fn cmd_memory(
    engine: &EngineState,
    model: &str,
    data_dir: &Path,
    iters: usize,
) -> Vec<CaseScore> {
    set_model(engine, model);
    let iters = iters.max(1);

    let ws = std::env::temp_dir().join("fella-mem-bench");
    let _ = std::fs::remove_dir_all(&ws);
    let (rent_all, rent_literal) = write_messy_spend(&ws);
    std::env::set_var("FELLA_MEMORY", "1");
    let mem = memory::path_for(data_dir, &ws);
    let _ = std::fs::remove_file(&mem);
    let _ = std::fs::remove_file(mem.with_extension("episodes.jsonl"));
    if engine.open_workspace(&ws).is_err() {
        println!("(memory) could not open workspace");
        return Vec::new();
    }

    // Session 1: look at the categories, then correct Fella's model of "rent".
    let prime = "xs-prime";
    engine.forget_conversation(prime);
    let p1 = run_case(engine, prime, "what spending categories are in spend.csv?", None).await;
    let p2 = run_case(
        engine,
        prime,
        "actually, for rent totals count HOUSING and mortgage as rent too",
        None,
    )
    .await;
    println!("\n# Per-folder memory (cross-session)  \u{b7}  `{model}`   ({iters} iter(s))\n");
    println!("_session 1, turn 1_: {}", first_line(&p1.text));
    println!("_session 1, turn 2 (correction)_: {}", first_line(&p2.text));
    println!("\n_learned `memory.md` after session 1:_\n```");
    print!("{}", std::fs::read_to_string(&mem).unwrap_or_default());
    println!("```\n");

    let q = "what's my total rent spending in spend.csv?";
    let gold = Gold::Approx(rent_all, 1.0);
    let case = EvalCase {
        id: "mem_cold",
        category: "Aggregate",
        question: q.to_string(),
        gold: gold.clone(),
        min_tools: 1,
        reference: format!("You spent {rent_all:.2} on rent (rent + housing + mortgage)."),
    };
    println!(
        "session 2 gold: {rent_all:.0} (rent+housing+mortgage); literal `rent` only = {rent_literal:.0}\n"
    );
    legend();
    println!("| condition | cold correct | rate | close(det) | steps | in tok | out tok | sample answer |");
    println!("|---|:-:|--:|--:|--:|--:|--:|---|");

    let mut all = Vec::new();
    // `ro`: read the primed memory, record nothing so every cold iteration is
    // an identical first encounter (not one that benefits from the last).
    for (label, on) in [("memory on", true), ("memory off", false)] {
        std::env::set_var("FELLA_MEMORY", if on { "ro" } else { "0" });
        let (mut oks, mut cd, mut steps) = (0usize, 0f32, 0usize);
        let (mut ptok, mut ctok) = (0u64, 0u64);
        let mut sample = String::new();
        for it in 0..iters {
            let conv = format!("xs-cold-{}-{it}", if on { "on" } else { "off" });
            engine.forget_conversation(&conv);
            let r = run_case(engine, &conv, q, None).await;
            if grade(&r, &gold) {
                oks += 1;
            }
            if it == 0 {
                let sql: String = r
                    .evidence
                    .iter()
                    .filter(|e| e.tool == "run_sql")
                    .filter_map(|e| e.sql.as_deref())
                    .next_back()
                    .unwrap_or("(no sql)")
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ");
                sample = format!(
                    "{} — `{}`",
                    first_line(&r.text).chars().take(60).collect::<String>(),
                    sql.chars().take(120).collect::<String>()
                );
            }
            cd += closeness_det(&r, &case);
            steps += r.steps;
            ptok += r.prompt_tok as u64;
            ctok += r.completion_tok as u64;
        }
        std::env::set_var("FELLA_MEMORY", "1");
        let n = iters as f32;
        let correct = oks * 2 > iters;
        println!(
            "| {label} | {} | {:.0}% | {:.2} | {} | {} | {} | {sample} |",
            yn(correct),
            oks as f32 / n * 100.0,
            cd / n,
            steps / iters,
            ptok / iters as u64,
            ctok / iters as u64,
        );
        all.push(CaseScore {
            id: format!("mem_cold_{}", if on { "on" } else { "off" }),
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
    all
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
    let bench_dir = opt("--dir");
    let harness = opt("--harness").unwrap_or_else(|| "fella".into());
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
        "memory" => {
            cmd_memory(&engine, &models[0], &data_dir, iters).await
        }
        "bench" => {
            let Some(d) = &bench_dir else {
                eprintln!("bench: pass --dir <path-to-benchmark-dir> (holds cases.jsonl + data files)");
                std::process::exit(2);
            };
            cmd_bench(&engine, Path::new(d), &data_dir, &models, judge, iters, only.as_deref(), &harness, json_out.as_deref()).await
        }
        "all" => {
            let mut v = cmd_accuracy(&engine, &cases, &models, judge, iters).await;
            v.extend(cmd_robustness(&engine, &models[0], &ws, iters).await);
            v.extend(cmd_session_memory(&engine, &models[0], &g, iters).await);
            v.extend(cmd_memory(&engine, &models[0], &data_dir, iters).await);
            v
        }
        other => {
            eprintln!("unknown subcommand {other:?}. one of: accuracy prompt-ablation folder-scale model-ladder robustness session-memory memory bench all");
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
        // identifier fragments aren't figures
        assert_eq!(numbers_in("nothing in txns_00.csv or v2 tables"), Vec::<f64>::new());
        assert_eq!(numbers_in("file_9 and q12 aside, the total is 4200"), vec![4200.0]);
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
        // `|` in a Contains sub = accept any of the forms
        let month = Gold::Contains(vec!["november 2021|2021-11"]);
        assert!(grade(&rr("Highest was November 2021.", vec![]), &month));
        assert!(grade(&rr("Highest was 2021-11.", vec![]), &month));
        assert!(!grade(&rr("Highest was 2021-09.", vec![]), &month));
        assert!(grade(&rr("Your files can't tell the future.", vec![]), &Gold::Refusal));
        // a curly apostrophe (what many models emit) still counts as "can't"
        assert!(grade(
            &rr("I can\u{2019}t determine future spending from past records.", vec![]),
            &Gold::Refusal
        ));
        assert!(grade(&rr("No data on future spending is available.", vec![]), &Gold::Refusal));
        assert!(!grade(&rr("You'll spend 4200 next month.", vec![]), &Gold::Refusal));
        // declines but cites a computed figure -> not a clean refusal
        assert!(!grade(
            &rr("I can't project it, but your monthly average is 3900.", vec![]),
            &Gold::Refusal
        ));
        assert!(grade(&rr("I answer questions about your files.", vec![]), &Gold::NoTool));
        assert!(!grade(&rr("...", vec![ev("run_sql", "1 row: 5", None)]), &Gold::NoTool));

        // Approx: within the band; a year in the text doesn't count
        assert!(grade(&rr("Rent was about 18% of spending (2024).", vec![]), &Gold::Approx(17.6, 1.5)));
        assert!(!grade(&rr("Rent was about 25%.", vec![]), &Gold::Approx(17.6, 1.5)));
        // Approx(0): a literal 0 or a plain "no / none"
        assert!(grade(&rr("Healthcare spending was $0.00.", vec![]), &Gold::Approx(0.0, 0.5)));
        assert!(grade(&rr("There are no healthcare transactions.", vec![]), &Gold::Approx(0.0, 0.5)));
        assert!(!grade(&rr("You spent 120 on healthcare.", vec![]), &Gold::Approx(0.0, 0.5)));
    }

    #[test]
    fn battery_is_frozen_and_diverse() {
        // dummy goldens so battery() builds
        let mut g = Goldens { workout_total_minutes: 20_000, ..Default::default() };
        let mut tg = TableGold { rows: 6000, total: 738_022.30, max_amount: 242.98, ..Default::default() };
        for c in ["rent", "groceries", "transport", "dining", "utilities", "shopping"] {
            tg.by_category.insert(c.into(), 123_000.0);
        }
        tg.by_month.insert("2021-05".into(), 90_000.0);
        tg.top_merchant = ("Aldi".into(), 51_986.22);
        g.tables.insert("txns_00".into(), tg);
        g.workout_top_activity = ("run".into(), 5000);

        let cases = battery(&g, 6100.0);
        assert!(cases.len() >= 15, "battery should be broad");
        // spread across question kinds, not all "spending"
        let cats: std::collections::HashSet<_> = cases.iter().map(|c| c.category).collect();
        for want in ["NoTool", "Filter", "MinMax", "TimeSeries", "Ratio", "DocSummary", "EmptyResult", "Refusal"] {
            assert!(cats.contains(want), "missing a {want} case");
        }
        // ids unique
        let mut ids: Vec<_> = cases.iter().map(|c| c.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), cases.len(), "duplicate case id");
    }

    #[test]
    fn waste_classification() {
        // one run_sql that feeds the answer -> nothing wasted
        let clean = rr("The total is 450.", vec![ev("run_sql", "1 row: total 450", None)]);
        assert_eq!(classify_waste(&clean).total(), 0);

        // any schema/sample/list peek is redundant on a small workspace (the
        // schema block already had it); errored + exact-repeat count once each
        let messy = rr(
            "The total is 450.",
            vec![
                ev("inspect_table", "ledger: 3 cols", None),     // redundant
                ev("run_sql", "1 row: total 450", None),         // legit
                ev("run_sql", "err", Some("no such column: x")), // errored
                ev("run_sql", "1 row: total 450", None),         // repeat of #2
            ],
        );
        let w = classify_waste(&messy);
        assert_eq!(
            (w.redundant_schema, w.errored, w.duplicate, w.total()),
            (1, 1, 1, 3),
            "{}",
            w.breakdown()
        );

        // speculative only fires once a run has 3+ successful queries
        let spec = rr(
            "The answer is 450.",
            vec![
                ev("run_sql", "1 row: total 450", None),
                ev("run_sql", "12 rows: avg 37", None), // feeds nothing
                ev("run_sql", "3 rows: max 99", None),  // feeds nothing
            ],
        );
        assert_eq!(classify_waste(&spec).speculative, 2);

        // two queries, one intermediate -> NOT speculative (can't tell)
        let two = rr(
            "The answer is 450.",
            vec![
                ev("run_sql", "12 rows: subtotal 37", None),
                ev("run_sql", "1 row: total 450", None),
            ],
        );
        assert_eq!(classify_waste(&two).total(), 0);
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

    #[test]
    fn bench_gold_shapes_parse() {
        let g = |s: &str| serde_json::from_str::<BenchGold>(s).unwrap().into_gold().unwrap();
        assert!(matches!(g(r#"{"figures":[600,42]}"#), Gold::Figures(v) if v == vec![600.0, 42.0]));
        assert!(matches!(g(r#"{"approx":[0.18,0.005]}"#), Gold::Approx(v, t) if v == 0.18 && t == 0.005));
        assert!(matches!(g(r#"{"contains":["Rent","1250"]}"#), Gold::Contains(v) if v == vec!["Rent", "1250"]));
        assert!(matches!(g(r#""refusal""#), Gold::Refusal));
        assert!(matches!(g(r#""notool""#), Gold::NoTool));
        assert!(serde_json::from_str::<BenchGold>(r#""bogus""#).unwrap().into_gold().is_err());

        let spec: BenchSpec = serde_json::from_str(
            r#"{"id":"a","question":"q?","files":["x.csv"],"gold":{"figures":[1]},"tier":"easy"}"#,
        )
        .unwrap();
        assert_eq!(spec.id, "a");
        assert_eq!(spec.files, vec!["x.csv"]);
        assert_eq!(spec.tier.as_deref(), Some("easy"));
    }
}
