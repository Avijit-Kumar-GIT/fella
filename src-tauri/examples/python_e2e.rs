//! Live end-to-end check for the Python sandbox and agent loop.
//!
//! This is deliberately an example rather than a normal test: it needs a live
//! Ollama Cloud credential and the answer is model-generated. It stages the
//! correlation fixture, asks the question that should select `run_python`,
//! then asserts both the tool trace and the grounded answer.
//!
//! Run from `src-tauri` with a scratch copy of `fella.db` + `auth.json`:
//!
//!   FELLA_E2E_DATA_DIR=/tmp/fella-e2e-data \
//!   cargo run --release --example python_e2e
//!
//! `FELLA_E2E_CASE_DIR` can point at another directory containing
//! `sleep.jsonl` and `screen_time.tsv`. `FELLA_E2E_MODEL` defaults to
//! `gemma4:31b`.

use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use fella_lib::engine::{AskEvent, EngineState};

const QUESTION: &str =
    "Is there a relationship between how much screen time I have in a day and how well I sleep that night?";

fn required_path(name: &str) -> PathBuf {
    std::env::var_os(name)
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            eprintln!("set {name} to a directory containing the copied Fella data");
            std::process::exit(2);
        })
}

fn unique_workspace() -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before Unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("fella-python-e2e-{stamp}"))
}

fn stage_file(case_dir: &Path, workspace: &Path, name: &str) {
    let source = case_dir.join(name);
    let target = workspace.join(name);
    std::fs::copy(&source, &target)
        .unwrap_or_else(|e| panic!("copy {} to {}: {e}", source.display(), target.display()));
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let _ = rustls::crypto::ring::default_provider().install_default();

    let data_dir = required_path("FELLA_E2E_DATA_DIR");
    let case_dir = std::env::var_os("FELLA_E2E_CASE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("../bench/analysis-depth"));
    let model = std::env::var("FELLA_E2E_MODEL").unwrap_or_else(|_| "gemma4:31b".into());
    let workspace = unique_workspace();
    std::fs::create_dir_all(&workspace).expect("create temporary workspace");
    stage_file(&case_dir, &workspace, "sleep.jsonl");
    stage_file(&case_dir, &workspace, "screen_time.tsv");

    async {
        let engine = EngineState::new(&data_dir).expect("engine init");
        engine
            .save_settings(&serde_json::Map::from_iter([
                (
                    "provider".into(),
                    serde_json::Value::String("ollama-cloud".into()),
                ),
                (
                    "base_url".into(),
                    serde_json::Value::String("https://ollama.com".into()),
                ),
                ("model".into(), serde_json::Value::String(model.clone())),
            ]))
            .expect("configure Ollama Cloud in the scratch data dir");

        let health = engine.provider_health().await;
        assert!(
            health.reachable,
            "Ollama Cloud is not reachable (rejected={})",
            health.rejected
        );
        engine
            .open_workspace(&workspace)
            .expect("open staged correlation workspace");

        let started = Instant::now();
        let answer = engine
            .ask("python-e2e", QUESTION, Some(&model), |event| {
                if let AskEvent::ToolStart { tool, .. } = event {
                    eprintln!("tool start: {tool}");
                }
            })
            .await
            .expect("complete live agent answer");

        eprintln!("answer: {}", answer.text.replace('\n', " "));
        for evidence in &answer.evidence {
            eprintln!(
                "tool result: {} {}ms{}{}",
                evidence.tool,
                evidence.ms,
                evidence
                    .result_summary
                    .as_str()
                    .chars()
                    .take(240)
                    .collect::<String>(),
                evidence
                    .error
                    .as_deref()
                    .map(|error| format!(" error={error}"))
                    .unwrap_or_default()
            );
        }

        let python_calls: Vec<_> = answer
            .evidence
            .iter()
            .filter(|e| e.tool == "run_python")
            .collect();
        assert!(!python_calls.is_empty(), "model did not select run_python");
        let python = python_calls
            .iter()
            .find(|e| e.result_summary.starts_with("python finished in "))
            .unwrap_or_else(|| {
                panic!(
                    "run_python was selected but every call failed; tools used: {:?}; answer: {}",
                    answer
                        .evidence
                        .iter()
                        .map(|e| e.tool.as_str())
                        .collect::<Vec<_>>(),
                    answer.text
                )
            });
        if python_calls
            .iter()
            .any(|e| !e.result_summary.starts_with("python finished in "))
        {
            eprintln!("note: the model recovered from a failed Python attempt");
        }

        let answer_lower = answer.text.to_lowercase();
        assert!(
            answer_lower.contains("60"),
            "answer did not cite the 60-day sample: {}",
            answer.text
        );
        assert!(
            answer_lower.contains("negative") || answer_lower.contains("weak"),
            "answer did not describe the negative/weak relationship: {}",
            answer.text
        );

        println!("model: {model}");
        println!("question: {QUESTION}");
        println!("answer: {}", answer.text.replace('\n', " "));
        println!("python_ms: {}", python.ms);
        println!("total_ms: {}", started.elapsed().as_millis());
        println!(
            "tools: {:?}",
            answer.evidence.iter().map(|e| &e.tool).collect::<Vec<_>>()
        );
    }
    .await;

    let _ = std::fs::remove_dir_all(&workspace);
}
