//! make_chart: end-to-end through the tool registry.

use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use fella_lib::engine::tools::Registry;
use fella_lib::engine::EngineState;

fn scratch(tag: &str) -> PathBuf {
    let n = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let p = std::env::temp_dir().join(format!("fella-{tag}-{n}"));
    fs::create_dir_all(&p).unwrap();
    p
}

#[tokio::test]
async fn make_chart_returns_structured_chart_data() {
    let data = scratch("chart-data");
    let engine = EngineState::new(&data).unwrap();
    let registry = Registry::standard();

    let args = serde_json::json!({
        "kind": "bar",
        "title": "Spending by category",
        "labels": ["Groceries", "Rent", "Transport"],
        "series": [{ "name": "amount", "values": [412.5, 1250.0, 88.0] }],
        "unit": "$"
    });

    let out = registry.run(&engine, "make_chart", &args).await.unwrap().unwrap();
    let chart = out.chart.expect("chart field should be set");
    assert_eq!(chart.title.as_deref(), Some("Spending by category"));
    assert_eq!(chart.labels, vec!["Groceries", "Rent", "Transport"]);
    assert_eq!(chart.series[0].name, "amount");
    assert_eq!(chart.series[0].values, vec![412.5, 1250.0, 88.0]);
    assert_eq!(chart.unit.as_deref(), Some("$"));

    let _ = fs::remove_dir_all(&data);
}

#[tokio::test]
async fn make_chart_reports_a_tool_error_on_bad_args() {
    let data = scratch("chart-data2");
    let engine = EngineState::new(&data).unwrap();
    let registry = Registry::standard();

    // labels/series length mismatch -> a tool error, not a panic.
    let args = serde_json::json!({
        "kind": "bar",
        "labels": ["a", "b"],
        "series": [{ "name": "s", "values": [1.0] }]
    });
    let out = registry.run(&engine, "make_chart", &args).await.unwrap();
    assert!(out.is_err());

    let _ = fs::remove_dir_all(&data);
}

#[tokio::test]
async fn make_chart_rejects_an_unknown_kind() {
    let data = scratch("chart-data3");
    let engine = EngineState::new(&data).unwrap();
    let registry = Registry::standard();

    let args = serde_json::json!({
        "kind": "pie",
        "labels": ["a"],
        "series": [{ "name": "s", "values": [1.0] }]
    });
    let out = registry.run(&engine, "make_chart", &args).await.unwrap();
    assert!(out.is_err());

    let _ = fs::remove_dir_all(&data);
}
