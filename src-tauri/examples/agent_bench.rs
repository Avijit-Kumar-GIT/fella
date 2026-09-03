//! Agent-loop latency benchmark against a real provider.
//!
//! Not built by `cargo test` / CI. It reads settings + credentials from a data
//! dir you point it at (copy your real `fella.db` + `auth.json` into a scratch
//! dir first so it never touches live state), builds a workspace of fixtures at
//! several sizes, and times a battery of questions end to end.
//!
//!   BENCH_DATA_DIR=/path/to/copied/data-dir \
//!   BENCH_WS=/path/to/scratch/workspace \
//!   BENCH_ITERS=3 \
//!   BENCH_ONLY=agg_tiny \        # optional: run only ids containing this
//!   cargo run --release --example agent_bench
//!
//! Knobs (`FELLA_OLLAMA_NUM_CTX`, `FELLA_OLLAMA_KEEP_ALIVE`,
//! `FELLA_MODEL_MAX_OUTPUT`, `FELLA_MAX_STEPS`, `FELLA_MODEL_TIMEOUT_SECS`) are
//! read by the engine as usual, so a sweep is just re-running with them set.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use fella_lib::engine::{AskEvent, EngineState};

#[derive(Clone)]
#[allow(dead_code)] // `tool` is captured for ad-hoc debugging, not scored
struct Ev {
    at: Duration,
    kind: &'static str,
    tool: Option<String>,
}

#[allow(dead_code)] // `tool_ms_max` kept for eyeballing parallel tool batches
struct Sample {
    total: Duration,
    first_token: Option<Duration>,
    model_calls: usize,
    tool_calls: usize,
    tool_ms_sum: u64,
    tool_ms_max: u64,
    model_time: Duration,
    tool_time: Duration,
    steps: usize,
    hit_cap: bool,
    verif_pass: usize,
    verif_total: usize,
    answer_chars: usize,
    err: Option<String>,
}

struct Q {
    id: &'static str,
    text: &'static str,
}

fn env(k: &str, d: &str) -> String {
    std::env::var(k).unwrap_or_else(|_| d.to_string())
}

fn write_fixtures(ws: &Path) {
    std::fs::create_dir_all(ws).unwrap();

    // tiny: a hand-typed rent ledger, amounts as text with currency.
    std::fs::write(
        ws.join("rent.csv"),
        "month,amount paid,method\n\
         2024-01,\"$1,200.00\",ACH\n\
         2024-02,\"1,200\",ACH\n\
         2024-03,1200,check\n\
         2024-04,\"$1,250.00\",ACH\n\
         2024-05,\"$1,250.00\",check\n",
    )
    .unwrap();

    // medium: ~6k transactions across 2 years, 6 categories, 40 merchants.
    gen_txns(&ws.join("transactions.csv"), 6_000, 2023);

    // large: ~120k transactions across 4 years.
    gen_txns(&ws.join("transactions_big.csv"), 120_000, 2021);

    // a couple of notes for the document path.
    std::fs::write(
        ws.join("notes-budget.md"),
        "# Budget notes\n\nMonthly rent target is 1200. Groceries budget is 500/mo.\n\
         In March 2024 the landlord raised rent to 1250. Utilities average 140.\n",
    )
    .unwrap();
    std::fs::write(
        ws.join("notes-2024.md"),
        "# 2024\n\nSwitched gym in Feb. Annual insurance paid in April (890).\n\
         Biggest single expense: flights in July.\n",
    )
    .unwrap();
}

fn gen_txns(path: &Path, n: usize, start_year: i32) {
    let cats = ["groceries", "rent", "transport", "dining", "utilities", "shopping"];
    let merchants = [
        "Aldi", "Tesco", "Uber", "Shell", "Amazon", "Netflix", "Spotify", "EDF",
        "Thameslink", "Pret", "Nando's", "IKEA", "Boots", "Costa", "Greggs", "Deliveroo",
    ];
    let mut s = String::with_capacity(n * 40);
    s.push_str("date,amount,category,merchant\n");
    let mut seed: u64 = 0x9E3779B97F4A7C15;
    let mut rng = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    for i in 0..n {
        let day = (i as i64) % 1400;
        let year = start_year + (day / 365) as i32;
        let doy = (day % 365) + 1;
        let month = (doy / 31) + 1;
        let dom = (doy % 28) + 1;
        let cat = cats[(rng() % cats.len() as u64) as usize];
        let merch = merchants[(rng() % merchants.len() as u64) as usize];
        let amount = 3.0 + (rng() % 24000) as f64 / 100.0;
        s.push_str(&format!(
            "{year:04}-{month:02}-{dom:02},{amount:.2},{cat},{merch}\n"
        ));
    }
    std::fs::write(path, s).unwrap();
}

fn battery() -> Vec<Q> {
    vec![
        Q { id: "chitchat", text: "what kinds of questions can you help me with?" },
        Q { id: "agg_tiny", text: "what's the total amount I paid in rent.csv?" },
        Q { id: "agg_medium", text: "what was my total spending in transactions.csv?" },
        Q { id: "group_medium", text: "in transactions.csv, what did I spend per category? give the top 3." },
        Q { id: "multi_step", text: "in transactions.csv, which merchant did I spend the most at overall, and roughly how much?" },
        Q { id: "agg_large", text: "in transactions_big.csv, how many rows are there and what's the total amount?" },
        Q { id: "doc_lookup", text: "according to my notes, what is the monthly rent target and when did it change?" },
    ]
}

async fn run_one(engine: &EngineState, conv: &str, q: &Q) -> Sample {
    let evs: Arc<Mutex<Vec<Ev>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = evs.clone();
    let t0 = Instant::now();
    let res = engine
        .ask(conv, q.text, None, move |e: AskEvent| {
            let at = t0.elapsed();
            let (kind, tool) = match &e {
                AskEvent::AssistantDelta { .. } => ("delta", None),
                AskEvent::ToolStart { tool, .. } => ("tool_start", Some(tool.clone())),
                AskEvent::ToolEnd { item } => ("tool_end", Some(item.tool.clone())),
                AskEvent::Notice { .. } => ("notice", None),
                AskEvent::AnswerDone { .. } => ("answer_done", None),
            };
            sink.lock().unwrap().push(Ev { at, kind, tool });
        })
        .await;
    let total = t0.elapsed();
    let evs: Vec<Ev> = evs.lock().unwrap().clone();

    let first_token = evs.iter().find(|e| e.kind == "delta").map(|e| e.at);

    // A "step" is a batch of tool_start events; model_calls = steps + 1 (the
    // answer turn). model_time = wall not covered by a running tool, up to
    // answer_done; tool_time = the union of tool_start..tool_end spans (tools in
    // one step run concurrently, so take the max end - min start of the batch).
    let mut steps = 0usize;
    let mut tool_time = Duration::ZERO;
    let mut i = 0;
    let mut prev_kind = "";
    let mut batch_start: Option<Duration> = None;
    let mut open_tools = 0i32;
    let mut last_batch_start_at = Duration::ZERO;
    for e in &evs {
        match e.kind {
            "tool_start" => {
                if prev_kind != "tool_start" {
                    steps += 1;
                    batch_start = Some(e.at);
                    last_batch_start_at = e.at;
                }
                open_tools += 1;
            }
            "tool_end" => {
                open_tools -= 1;
                if open_tools <= 0 {
                    if let Some(bs) = batch_start.take() {
                        tool_time += e.at.saturating_sub(bs);
                    }
                }
            }
            _ => {}
        }
        prev_kind = e.kind;
        i += 1;
    }
    let _ = (i, last_batch_start_at);
    let done_at = evs.iter().rev().find(|e| e.kind == "answer_done").map(|e| e.at).unwrap_or(total);
    let model_time = done_at.saturating_sub(tool_time);

    if std::env::var_os("BENCH_SHOW_ANSWERS").is_some() {
        match &res {
            Ok(a) => {
                eprintln!("\n[{}] Q: {}\n  A: {}", q.id, q.text, a.text.replace('\n', "\n     "));
                for e in &a.evidence {
                    eprintln!(
                        "     · {} {}ms{}",
                        e.tool,
                        e.ms,
                        e.sql.as_deref().map(|s| format!("  {s}")).unwrap_or_default()
                    );
                }
                for c in &a.verification {
                    eprintln!("     {} {}", if c.ok { "ok  " } else { "WARN" }, c.label);
                }
            }
            Err(e) => eprintln!("\n[{}] ERR: {e}", q.id),
        }
    }

    match res {
        Ok(ans) => {
            let tool_calls = ans.evidence.len();
            let tool_ms_sum: u64 = ans.evidence.iter().map(|e| e.ms).sum();
            let tool_ms_max: u64 = ans.evidence.iter().map(|e| e.ms).max().unwrap_or(0);
            let hit_cap = ans.text.contains("ran out of analysis steps")
                || ans.text.contains("gathered so far")
                || ans.text.contains("out of tool-calling steps");
            let verif_total = ans.verification.len();
            let verif_pass = ans.verification.iter().filter(|c| c.ok).count();
            Sample {
                total,
                first_token,
                model_calls: steps + 1,
                tool_calls,
                tool_ms_sum,
                tool_ms_max,
                model_time,
                tool_time,
                steps,
                hit_cap,
                verif_pass,
                verif_total,
                answer_chars: ans.text.len(),
                err: None,
            }
        }
        Err(e) => Sample {
            total,
            first_token,
            model_calls: steps + 1,
            tool_calls: 0,
            tool_ms_sum: 0,
            tool_ms_max: 0,
            model_time,
            tool_time,
            steps,
            hit_cap: false,
            verif_pass: 0,
            verif_total: 0,
            answer_chars: 0,
            err: Some(e.to_string()),
        },
    }
}

fn secs(d: Duration) -> f64 {
    d.as_secs_f64()
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let _ = rustls::crypto::ring::default_provider().install_default();

    // BENCH_DATA_DIR must hold a copy of your real `fella.db` + `auth.json` so
    // the bench uses your provider/model/credentials without touching live state.
    let data_dir = match std::env::var("BENCH_DATA_DIR") {
        Ok(d) => PathBuf::from(d),
        Err(_) => {
            eprintln!(
                "set BENCH_DATA_DIR to a dir holding a copy of your fella.db + auth.json"
            );
            std::process::exit(2);
        }
    };
    let ws = PathBuf::from(env(
        "BENCH_WS",
        std::env::temp_dir().join("fella-bench-ws").to_str().unwrap_or("/tmp/fella-bench-ws"),
    ));
    let iters: usize = env("BENCH_ITERS", "3").parse().unwrap_or(3);
    let only = std::env::var("BENCH_ONLY").ok();

    eprintln!("bench: data_dir={} ws={}", data_dir.display(), ws.display());
    write_fixtures(&ws);

    let engine = EngineState::new(&data_dir).expect("engine init");
    if let Ok(m) = std::env::var("BENCH_MODEL") {
        let mut patch = serde_json::Map::new();
        patch.insert("model".into(), serde_json::Value::String(m));
        engine.save_settings(&patch).expect("set model");
    }
    let s = engine.settings();
    eprintln!(
        "bench: provider={} base_url={} model={} has_credential={}",
        s.provider, s.base_url, s.model, s.has_credential
    );
    for (k, def) in [
        ("FELLA_OLLAMA_NUM_CTX", "8192"),
        ("FELLA_OLLAMA_KEEP_ALIVE", "30m"),
        ("FELLA_MODEL_MAX_OUTPUT", "1024"),
        ("FELLA_MAX_STEPS", "20"),
    ] {
        eprintln!("bench: {k}={}", env(k, &format!("{def} (default)")));
    }

    let health = engine.provider_health().await;
    eprintln!(
        "bench: health reachable={} rejected={} models={}",
        health.reachable,
        health.rejected,
        health.models.len()
    );
    if std::env::var_os("BENCH_LIST_MODELS").is_some() {
        for m in &health.models {
            println!("MODEL\t{m}");
        }
        return;
    }
    if !health.reachable {
        eprintln!("bench: provider not reachable, aborting");
        std::process::exit(1);
    }
    if !health.models.is_empty() && !health.models.iter().any(|m| m == &s.model) {
        eprintln!(
            "bench: WARNING configured model {:?} is not in the provider's list; first few are: {:?}",
            s.model,
            &health.models[..health.models.len().min(8)]
        );
    }

    let cat = engine.open_workspace(&ws).expect("open workspace");
    eprintln!("bench: workspace opened, {} sources", cat.sources.len());

    // --- cross-model sweep: one compact summary row per model --------------
    if let Ok(models) = std::env::var("BENCH_MODELS") {
        let n = env("BENCH_ITERS", "2").parse().unwrap_or(2);
        let core: Vec<Q> = battery()
            .into_iter()
            .filter(|q| matches!(q.id, "agg_tiny" | "agg_medium" | "group_medium" | "multi_step" | "doc_lookup"))
            .collect();
        println!("\n# Cross-model sweep\n");
        println!("num_ctx {} · keep_alive {} · think {} · {n} warm iters/question after 1 warm-up\n",
            env("FELLA_OLLAMA_NUM_CTX", "8192"),
            env("FELLA_OLLAMA_KEEP_ALIVE", "30m"),
            env("FELLA_OLLAMA_THINK", "false"));
        println!("| model | total s (mean) | 1st tok s | model calls | steps>1 | verif ok | worst q |");
        println!("|---|--:|--:|--:|--:|:-:|---|");
        for m in models.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()) {
            let mut patch = serde_json::Map::new();
            patch.insert("model".into(), serde_json::Value::String(m.to_string()));
            if engine.save_settings(&patch).is_err() {
                println!("| {m} | — | — | — | — | — | save failed |");
                continue;
            }
            let mut totals = Vec::new();
            let mut ftoks = Vec::new();
            let mut calls = Vec::new();
            let mut multi = 0usize;
            let mut verif_ok = true;
            let mut worst = ("", 0.0f64);
            for q in &core {
                for it in 0..=n {
                    let sm = run_one(&engine, &format!("sweep-{m}-{}-{it}", q.id), q).await;
                    if it == 0 {
                        continue; // warm-up
                    }
                    totals.push(secs(sm.total));
                    if let Some(ft) = sm.first_token {
                        ftoks.push(secs(ft));
                    }
                    calls.push(sm.model_calls as f64);
                    if sm.steps > 1 {
                        multi += 1;
                    }
                    if sm.verif_total > 0 && sm.verif_pass < sm.verif_total {
                        verif_ok = false;
                    }
                    if secs(sm.total) > worst.1 {
                        worst = (q.id, secs(sm.total));
                    }
                    if sm.err.is_some() {
                        verif_ok = false;
                    }
                }
            }
            let mean = |v: &[f64]| if v.is_empty() { 0.0 } else { v.iter().sum::<f64>() / v.len() as f64 };
            println!(
                "| {m} | {:.1} | {:.1} | {:.1} | {} | {} | {} {:.1}s |",
                mean(&totals),
                mean(&ftoks),
                mean(&calls),
                multi,
                if verif_ok { "yes" } else { "NO" },
                worst.0,
                worst.1,
            );
            std::io::stdout().flush().ok();
        }
        eprintln!("bench: sweep done");
        return;
    }

    let qs = battery();
    let qs: Vec<&Q> = qs
        .iter()
        .filter(|q| only.as_deref().is_none_or(|o| q.id.contains(o)))
        .collect();

    println!("\n# Agent-loop benchmark\n");
    println!(
        "provider `{}` · model `{}` · num_ctx {} · keep_alive {} · {} iteration(s)/question\n",
        s.provider,
        s.model,
        env("FELLA_OLLAMA_NUM_CTX", "8192"),
        env("FELLA_OLLAMA_KEEP_ALIVE", "30m"),
        iters
    );
    println!("| question | run | total s | 1st tok s | model calls | tool calls | model s | tool s | steps | cap | verif | note |");
    println!("|---|--:|--:|--:|--:|--:|--:|--:|--:|:-:|:-:|---|");

    let mut agg: Vec<(String, Vec<Sample>)> = Vec::new();
    for q in &qs {
        let mut samples = Vec::new();
        for it in 0..iters {
            // Fresh conversation id per iteration so iteration 2+ can't answer
            // from session memory of iteration 1 - this isolates "warm model"
            // from "cached answer".
            let conv = format!("{}-{it}", q.id);
            let sm = run_one(&engine, &conv, q).await;
            let tag = if it == 0 { "cold" } else { "warm" };
            let first_token =
                sm.first_token.map(|d| format!("{:.1}", secs(d))).unwrap_or_else(|| "-".into());
            let verif = format!("{}/{}", sm.verif_pass, sm.verif_total);
            let last = sm.err.clone().unwrap_or_else(|| format!("{}c", sm.answer_chars));
            println!(
                "| {} | {} {} | {:.1} | {} | {} | {} | {:.1} | {:.1} | {} | {} | {} | {} |",
                q.id,
                it + 1,
                tag,
                secs(sm.total),
                first_token,
                sm.model_calls,
                sm.tool_calls,
                secs(sm.model_time),
                secs(sm.tool_time),
                sm.steps,
                if sm.hit_cap { "Y" } else { "" },
                verif,
                last,
            );
            std::io::stdout().flush().ok();
            samples.push(sm);
        }
        agg.push((q.id.to_string(), samples));
    }

    println!("\n## Summary (warm runs, mean)\n");
    println!("| question | n | total s | model s | tool s | model calls | tool calls | tool ms sum | verif |");
    println!("|---|--:|--:|--:|--:|--:|--:|--:|:-:|");
    for (id, samples) in &agg {
        let warm: Vec<&Sample> = if samples.len() > 1 { samples[1..].iter().collect() } else { samples.iter().collect() };
        let n = warm.len().max(1) as f64;
        let mean = |f: &dyn Fn(&Sample) -> f64| warm.iter().map(|s| f(s)).sum::<f64>() / n;
        println!(
            "| {} | {} | {:.1} | {:.1} | {:.1} | {:.1} | {:.1} | {:.0} | {}/{} |",
            id,
            warm.len(),
            mean(&|s| secs(s.total)),
            mean(&|s| secs(s.model_time)),
            mean(&|s| secs(s.tool_time)),
            mean(&|s| s.model_calls as f64),
            mean(&|s| s.tool_calls as f64),
            mean(&|s| s.tool_ms_sum as f64),
            warm.first().map(|s| s.verif_pass).unwrap_or(0),
            warm.first().map(|s| s.verif_total).unwrap_or(0),
        );
    }

    // Cold vs warm delta on the first question that had >1 iter.
    if iters > 1 {
        println!("\n## Cold vs warm (iteration 1 vs mean of rest)\n");
        println!("| question | cold total s | warm total s | delta s |");
        println!("|---|--:|--:|--:|");
        for (id, samples) in &agg {
            if samples.len() < 2 {
                continue;
            }
            let cold = secs(samples[0].total);
            let warm = samples[1..].iter().map(|s| secs(s.total)).sum::<f64>()
                / (samples.len() - 1) as f64;
            println!("| {} | {:.1} | {:.1} | {:+.1} |", id, cold, warm, cold - warm);
        }
    }

    // --- follow-up scenario: does session memory make turn 2 faster? ---------
    {
        println!("\n## Follow-up in one conversation (session memory)\n");
        println!("| turn | total s | model calls | tool calls | verif | note |");
        println!("|---|--:|--:|--:|:-:|---|");
        let conv = "followup-scenario";
        let q1 = Q { id: "fu1", text: "in transactions.csv, what did I spend on groceries in total?" };
        let q2 = Q { id: "fu2", text: "and what about dining?" };
        for (label, q) in [("1 (fresh)", &q1), ("2 (follow-up)", &q2)] {
            let sm = run_one(&engine, conv, q).await;
            println!(
                "| {} | {:.1} | {} | {} | {}/{} | {} |",
                label,
                secs(sm.total),
                sm.model_calls,
                sm.tool_calls,
                sm.verif_pass,
                sm.verif_total,
                sm.err.unwrap_or_else(|| format!("{}c", sm.answer_chars)),
            );
            std::io::stdout().flush().ok();
        }
    }

    // --- knob sweep: num_ctx and warm-up, on two representative questions ----
    if std::env::var_os("BENCH_SWEEP").is_some() {
        println!("\n## Knob sweep\n");
        println!("(each cell: fresh conversation, model already warm from the runs above)\n");
        println!("| question | num_ctx | total s | model calls | tool calls | steps | cap | verif |");
        println!("|---|--:|--:|--:|--:|--:|:-:|:-:|");
        let sweep_qs = [
            Q { id: "sweep_group", text: "in transactions.csv, what did I spend per category? give the top 3." },
            Q { id: "sweep_multi", text: "in transactions.csv, which merchant did I spend the most at, and how much, and in which month was my single biggest purchase?" },
        ];
        for ctx in ["2048", "8192", "16384"] {
            std::env::set_var("FELLA_OLLAMA_NUM_CTX", ctx);
            for (k, q) in sweep_qs.iter().enumerate() {
                let conv = format!("sweep-{ctx}-{k}");
                let sm = run_one(&engine, &conv, q).await;
                println!(
                    "| {} | {} | {:.1} | {} | {} | {} | {} | {}/{} |",
                    q.id,
                    ctx,
                    secs(sm.total),
                    sm.model_calls,
                    sm.tool_calls,
                    sm.steps,
                    if sm.hit_cap { "Y" } else { "" },
                    sm.verif_pass,
                    sm.verif_total,
                );
                std::io::stdout().flush().ok();
            }
        }
        std::env::set_var("FELLA_OLLAMA_NUM_CTX", "8192");
    }

    eprintln!("bench: done");
}
