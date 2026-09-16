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
    let ws = scratch("chart-data-ws");
    let data = scratch("chart-data");
    fs::write(
        ws.join("sales.csv"),
        "category,amount\nGroceries,412.5\nRent,1250\nTransport,88\n",
    )
    .unwrap();
    let engine = EngineState::new(&data).unwrap();
    engine.open_workspace(&ws).unwrap();
    let registry = Registry::standard();

    let sql = "SELECT category, amount FROM sales ORDER BY amount DESC";
    let args = serde_json::json!({
        "kind": "bar",
        "title": "Spending by category",
        "sql": sql,
        "unit": "$",
        "note": "Compare spending"
    });

    let out = registry.run(&engine, "make_chart", &args).await.unwrap().unwrap();
    let chart = out.chart.expect("chart field should be set");
    assert_eq!(out.sql.as_deref(), Some(sql));
    assert_eq!(chart.title.as_deref(), Some("Spending by category"));
    assert_eq!(chart.labels, vec!["Rent", "Groceries", "Transport"]);
    assert_eq!(chart.series[0].name, "amount");
    assert_eq!(chart.series[0].values, vec![1250.0, 412.5, 88.0]);
    assert_eq!(chart.unit.as_deref(), Some("$"));

    let _ = fs::remove_dir_all(&ws);
    let _ = fs::remove_dir_all(&data);
}

#[tokio::test]
async fn make_chart_rejects_model_supplied_values() {
    let data = scratch("chart-data2");
    let engine = EngineState::new(&data).unwrap();
    let registry = Registry::standard();

    let args = serde_json::json!({
        "kind": "bar",
        "labels": ["Rent"],
        "series": [{ "name": "amount", "values": [1250.0] }]
    });
    let out = registry.run(&engine, "make_chart", &args).await.unwrap();
    assert!(out.is_err());

    let _ = fs::remove_dir_all(&data);
}

#[tokio::test]
async fn make_chart_reports_a_tool_error_on_bad_args() {
    let data = scratch("chart-data2");
    let engine = EngineState::new(&data).unwrap();
    let registry = Registry::standard();

    // A missing query is a tool error, not a panic.
    let args = serde_json::json!({
        "kind": "bar"
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
        "sql": "SELECT category, amount FROM sales"
    });
    let out = registry.run(&engine, "make_chart", &args).await.unwrap();
    assert!(out.is_err());

    let _ = fs::remove_dir_all(&data);
}
