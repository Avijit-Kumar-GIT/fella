//! End-to-end checks for semantic guidance attached to model-facing SQL.

use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use fella_lib::engine::tools::Registry;
use fella_lib::engine::EngineState;

fn scratch(tag: &str) -> PathBuf {
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let p = std::env::temp_dir().join(format!("fella-{tag}-{n}"));
    fs::create_dir_all(&p).unwrap();
    p
}

#[tokio::test]
async fn run_sql_warns_when_a_label_filter_can_miss_by_case() {
    let data = scratch("semantic-filter-data");
    let workspace = scratch("semantic-filter-workspace");
    fs::write(
        workspace.join("spending.csv"),
        "purpose,amount\nleisure,10\nwork,20\n",
    )
    .unwrap();

    let engine = EngineState::new(&data).unwrap();
    engine.open_workspace(&workspace).unwrap();
    let out = Registry::standard()
        .run(
            &engine,
            "run_sql",
            &serde_json::json!({
                "sql": "SELECT SUM(amount) AS total FROM spending WHERE purpose = 'Leisure'"
            }),
        )
        .await
        .unwrap()
        .unwrap();

    assert!(out
        .llm_text
        .contains("text filters on \"purpose\" match exact case"));
    assert!(out.llm_text.contains("lower(\"purpose\") = lower('value')"));
    assert_eq!(
        out.rows
            .as_ref()
            .and_then(|rows| rows.first())
            .and_then(|row| row.first()),
        Some(&serde_json::Value::Null)
    );

    let _ = fs::remove_dir_all(&data);
    let _ = fs::remove_dir_all(&workspace);
}

#[tokio::test]
async fn run_sql_does_not_add_label_guidance_to_an_iso_date_filter() {
    let data = scratch("semantic-date-data");
    let workspace = scratch("semantic-date-workspace");
    fs::write(
        workspace.join("spending.csv"),
        "date,amount\n2026-01-01,10\n2026-02-01,20\n",
    )
    .unwrap();

    let engine = EngineState::new(&data).unwrap();
    engine.open_workspace(&workspace).unwrap();
    let out = Registry::standard()
        .run(
            &engine,
            "run_sql",
            &serde_json::json!({
                "sql": "SELECT SUM(amount) AS total FROM spending WHERE date = '2026-01-01'"
            }),
        )
        .await
        .unwrap()
        .unwrap();

    assert!(!out.llm_text.contains("text filters on"));
    assert!(!out
        .llm_text
        .contains("values that differ only in capitalisation"));

    let _ = fs::remove_dir_all(&data);
    let _ = fs::remove_dir_all(&workspace);
}
