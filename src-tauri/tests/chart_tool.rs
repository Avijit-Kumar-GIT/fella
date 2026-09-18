//! make_chart: end-to-end through the tool registry.

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

    let out = registry
        .run(&engine, "make_chart", &args)
        .await
        .unwrap()
        .unwrap();
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
async fn make_chart_auto_chooses_a_line_for_time_periods() {
    let ws = scratch("chart-auto-ws");
    let data = scratch("chart-auto-data");
    fs::write(
        ws.join("sales.csv"),
        "month,amount\n2024-01,10\n2024-02,20\n2024-03,15\n",
    )
    .unwrap();
    let engine = EngineState::new(&data).unwrap();
    engine.open_workspace(&ws).unwrap();
    let registry = Registry::standard();

    let out = registry
        .run(
            &engine,
            "make_chart",
            &serde_json::json!({
                "kind": "auto",
                "title": "Sales over time",
                "sql": "SELECT month, amount FROM sales ORDER BY month"
            }),
        )
        .await
        .unwrap()
        .unwrap();

    assert_eq!(
        out.chart.expect("chart field should be set").kind,
        fella_lib::engine::analytics::chart::ChartKind::Line
    );

    let _ = fs::remove_dir_all(&ws);
    let _ = fs::remove_dir_all(&data);
}

#[tokio::test]
async fn make_chart_auto_chooses_a_bar_for_categories() {
    let ws = scratch("chart-auto-category-ws");
    let data = scratch("chart-auto-category-data");
    fs::write(
        ws.join("sales.csv"),
        "category,amount\nRent,1200\nFood,300\nTravel,90\n",
    )
    .unwrap();
    let engine = EngineState::new(&data).unwrap();
    engine.open_workspace(&ws).unwrap();

    let out = Registry::standard()
        .run(
            &engine,
            "make_chart",
            &serde_json::json!({
                "kind": "auto",
                "sql": "SELECT category, amount FROM sales ORDER BY amount DESC"
            }),
        )
        .await
        .unwrap()
        .unwrap();

    assert_eq!(
        out.chart.expect("chart field should be set").kind,
        fella_lib::engine::analytics::chart::ChartKind::Bar
    );

    let _ = fs::remove_dir_all(&ws);
    let _ = fs::remove_dir_all(&data);
}

#[tokio::test]
async fn make_chart_preserves_two_series_and_negative_values() {
    let ws = scratch("chart-two-series-ws");
    let data = scratch("chart-two-series-data");
    fs::write(
        ws.join("forecast.csv"),
        "month,planned,actual\n2024-01,100,80\n2024-02,120,150\n2024-03,140,130\n",
    )
    .unwrap();
    fs::write(
        ws.join("net.csv"),
        "month,net_change\n2024-01,-50\n2024-02,120\n2024-03,-20\n",
    )
    .unwrap();
    let engine = EngineState::new(&data).unwrap();
    engine.open_workspace(&ws).unwrap();
    let registry = Registry::standard();

    let forecast = registry
        .run(
            &engine,
            "make_chart",
            &serde_json::json!({
                "kind": "line",
                "sql": "SELECT month, planned, actual FROM forecast ORDER BY month"
            }),
        )
        .await
        .unwrap()
        .unwrap()
        .chart
        .expect("forecast chart");
    assert_eq!(
        forecast.kind,
        fella_lib::engine::analytics::chart::ChartKind::Line
    );
    assert_eq!(forecast.series.len(), 2);
    assert_eq!(forecast.series[0].values, vec![100.0, 120.0, 140.0]);
    assert_eq!(forecast.series[1].values, vec![80.0, 150.0, 130.0]);

    let net = registry
        .run(
            &engine,
            "make_chart",
            &serde_json::json!({
                "kind": "line",
                "sql": "SELECT month, net_change FROM net ORDER BY month"
            }),
        )
        .await
        .unwrap()
        .unwrap()
        .chart
        .expect("net chart");
    assert_eq!(net.series[0].values, vec![-50.0, 120.0, -20.0]);

    let _ = fs::remove_dir_all(&ws);
    let _ = fs::remove_dir_all(&data);
}

#[tokio::test]
async fn make_chart_reads_json_tsv_and_currency_formatted_sources() {
    let ws = scratch("chart-source-formats-ws");
    let data = scratch("chart-source-formats-data");
    fs::write(
        ws.join("mood.json"),
        r#"[
          {"date":"2024-01-01","score":2},
          {"date":"2024-01-02","score":4},
          {"date":"2024-01-03","score":3}
        ]"#,
    )
    .unwrap();
    fs::write(
        ws.join("screen.tsv"),
        "date\tapp\tminutes\n2024-01-01\tbrowser\t30\n2024-01-01\tsocial\t10\n2024-01-02\tbrowser\t40\n2024-01-02\tsocial\t20\n2024-01-03\treading\t15\n2024-01-03\tbrowser\t25\n",
    )
    .unwrap();
    fs::write(
        ws.join("costs.csv"),
        "item,cost\nLaptop repair,\"$1,200.00\"\nTrain ticket,\"$35.50\"\nBooks,\"$80.00\"\n",
    )
    .unwrap();
    let engine = EngineState::new(&data).unwrap();
    engine.open_workspace(&ws).unwrap();
    let registry = Registry::standard();

    let mood = registry
        .run(
            &engine,
            "make_chart",
            &serde_json::json!({
                "kind": "line",
                "sql": "SELECT date, score FROM mood ORDER BY date"
            }),
        )
        .await
        .unwrap()
        .unwrap()
        .chart
        .expect("json chart");
    assert_eq!(mood.labels, vec!["2024-01-01", "2024-01-02", "2024-01-03"]);
    assert_eq!(mood.series[0].values, vec![2.0, 4.0, 3.0]);

    let screen = registry
        .run(
            &engine,
            "make_chart",
            &serde_json::json!({
                "kind": "bar",
                "sql": "SELECT app, SUM(minutes) AS minutes FROM screen GROUP BY app ORDER BY minutes DESC"
            }),
        )
        .await
        .unwrap()
        .unwrap()
        .chart
        .expect("tsv chart");
    assert_eq!(screen.labels, vec!["browser", "social", "reading"]);
    assert_eq!(screen.series[0].values, vec![95.0, 30.0, 15.0]);

    let costs = registry
        .run(
            &engine,
            "make_chart",
            &serde_json::json!({
                "kind": "bar",
                "sql": "SELECT item, cost FROM costs ORDER BY cost DESC"
            }),
        )
        .await
        .unwrap()
        .unwrap()
        .chart
        .expect("currency chart");
    assert_eq!(costs.labels, vec!["Laptop repair", "Books", "Train ticket"]);
    assert_eq!(costs.series[0].values, vec![1200.0, 80.0, 35.5]);

    let _ = fs::remove_dir_all(&ws);
    let _ = fs::remove_dir_all(&data);
}

#[tokio::test]
async fn make_chart_rejects_flat_data_instead_of_emitting_a_visual() {
    let ws = scratch("chart-flat-ws");
    let data = scratch("chart-flat-data");
    fs::write(
        ws.join("flat.csv"),
        "month,value\n2024-01,100\n2024-02,100\n2024-03,100\n2024-04,100\n",
    )
    .unwrap();
    let engine = EngineState::new(&data).unwrap();
    engine.open_workspace(&ws).unwrap();

    let out = Registry::standard()
        .run(
            &engine,
            "make_chart",
            &serde_json::json!({
                "kind": "auto",
                "sql": "SELECT month, value FROM flat ORDER BY month"
            }),
        )
        .await
        .unwrap();
    let error = match out {
        Ok(_) => panic!("flat data should be rejected"),
        Err(error) => error,
    };
    assert!(error.to_string().contains("flat"), "{error}");

    let _ = fs::remove_dir_all(&ws);
    let _ = fs::remove_dir_all(&data);
}

#[tokio::test]
async fn make_chart_rejects_missing_numeric_values() {
    let ws = scratch("chart-null-ws");
    let data = scratch("chart-null-data");
    fs::write(
        ws.join("readings.csv"),
        "date,reading\n2024-01-01,10\n2024-01-02,\n2024-01-03,14\n",
    )
    .unwrap();
    let engine = EngineState::new(&data).unwrap();
    engine.open_workspace(&ws).unwrap();

    let out = Registry::standard()
        .run(
            &engine,
            "make_chart",
            &serde_json::json!({
                "kind": "line",
                "sql": "SELECT date, reading FROM readings ORDER BY date"
            }),
        )
        .await
        .unwrap();
    let error = match out {
        Ok(_) => panic!("missing values should be rejected"),
        Err(error) => error,
    };
    assert!(error.to_string().contains("not a finite number"), "{error}");

    let _ = fs::remove_dir_all(&ws);
    let _ = fs::remove_dir_all(&data);
}

#[tokio::test]
async fn make_chart_rejects_more_than_the_readable_category_limit() {
    let ws = scratch("chart-limit-ws");
    let data = scratch("chart-limit-data");
    let mut csv = String::from("date,value\n");
    for day in 1..=13 {
        csv.push_str(&format!("2024-01-{day:02},{}\n", day * 10));
    }
    fs::write(ws.join("daily.csv"), csv).unwrap();
    let engine = EngineState::new(&data).unwrap();
    engine.open_workspace(&ws).unwrap();

    let out = Registry::standard()
        .run(
            &engine,
            "make_chart",
            &serde_json::json!({
                "kind": "line",
                "sql": "SELECT date, value FROM daily ORDER BY date"
            }),
        )
        .await
        .unwrap();
    let error = match out {
        Ok(_) => panic!("too many rows should be rejected"),
        Err(error) => error,
    };
    assert!(error.to_string().contains("max 12"), "{error}");

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
