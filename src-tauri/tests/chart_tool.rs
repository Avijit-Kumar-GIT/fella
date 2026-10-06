//! make_chart: end-to-end through the tool registry.

use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use std::sync::{atomic::AtomicBool, Arc};

use fella_lib::engine::evidence::EvidenceItem;
use fella_lib::engine::tools::{Registry, ToolContext};
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
    assert_eq!(
        chart.series[0].values,
        vec![Some(1250.0), Some(412.5), Some(88.0)]
    );
    assert_eq!(chart.unit.as_deref(), Some("$"));

    let _ = fs::remove_dir_all(&ws);
    let _ = fs::remove_dir_all(&data);
}

#[tokio::test]
async fn make_chart_reuses_a_python_result_without_rerunning_or_copying_source_rows() {
    let data = scratch("chart-python-result-data");
    let engine = EngineState::new(&data).unwrap();
    let registry = Registry::standard();
    let code = "fella_table(['month', 'revenue'], [['Jan', 10], ['Feb', 30], ['Mar', 20]])\nprint('computed monthly revenue')";
    let result = registry
        .run(&engine, "run_python", &serde_json::json!({ "code": code }))
        .await
        .unwrap()
        .unwrap();
    let source = EvidenceItem {
        id: "evidence-python-1".into(),
        tool: "run_python".into(),
        sources: Vec::new(),
        args: serde_json::json!({ "code": code }),
        note: None,
        sql: None,
        result_summary: result.summary,
        columns: result.columns,
        rows: result.rows,
        row_count: result.row_count,
        output: result.output,
        chart: None,
        result_table: result.result_table,
        python_input_trace: None,
        python_queries: result.python_queries,
        python_queries_complete: result.python_queries_complete,
        ms: 0,
        error: None,
    };
    let prior = [source];
    let context = ToolContext {
        prior_evidence: &prior,
        conversation_id: None,
    };
    let chart = registry
        .run_with_context_cancel(
            &engine,
            "make_chart",
            &serde_json::json!({
                "kind": "line",
                "source_evidence_id": "evidence-python-1",
                "x_field": "month",
                "series_fields": ["revenue"],
                "missing_treatment": "gap"
            }),
            &context,
            Arc::new(AtomicBool::new(false)),
        )
        .await
        .unwrap()
        .unwrap();

    assert!(chart.sql.is_none(), "reused result should not rerun SQL");
    assert!(
        chart.rows.is_none(),
        "chart evidence should not duplicate the result table"
    );
    let spec = chart.chart.unwrap();
    assert_eq!(spec.labels, vec!["Jan", "Feb", "Mar"]);
    assert_eq!(
        spec.series[0].values,
        vec![Some(10.0), Some(30.0), Some(20.0)]
    );
    assert_eq!(
        spec.metadata.unwrap().source_evidence_id.as_deref(),
        Some("evidence-python-1")
    );

    let mut preview = prior[0].clone();
    preview.result_table = None;
    preview.columns = Some(vec!["month".into(), "revenue".into()]);
    preview.rows = Some(vec![
        vec![serde_json::json!("Jan"), serde_json::json!(10)],
        vec![serde_json::json!("Feb"), serde_json::json!(30)],
    ]);
    preview.row_count = Some(3);
    let preview = [preview];
    let preview_context = ToolContext {
        prior_evidence: &preview,
        conversation_id: None,
    };
    let preview_result = registry
        .run_with_context_cancel(
            &engine,
            "make_chart",
            &serde_json::json!({
                "kind": "line",
                "source_evidence_id": "evidence-python-1",
                "x_field": "month",
                "series_fields": ["revenue"],
                "missing_treatment": "gap"
            }),
            &preview_context,
            Arc::new(AtomicBool::new(false)),
        )
        .await
        .unwrap();
    let preview_result = match preview_result {
        Ok(_) => panic!("bounded preview should not be accepted as a complete chart source"),
        Err(error) => error,
    };
    assert!(preview_result.to_string().contains("bounded preview"));

    let _ = fs::remove_dir_all(&data);
}

#[tokio::test]
async fn make_chart_rejects_inspection_samples_as_incomplete_chart_sources() {
    let data = scratch("chart-inspection-sample-data");
    let engine = EngineState::new(&data).unwrap();
    let registry = Registry::standard();
    let inspection = EvidenceItem {
        id: "evidence-1".into(),
        tool: "inspect_table".into(),
        sources: Vec::new(),
        args: serde_json::json!({ "name": "expenses" }),
        note: None,
        sql: None,
        result_summary: "inspected expenses".into(),
        columns: Some(vec!["category".into(), "amount".into()]),
        rows: Some(vec![vec![
            serde_json::json!("Rent"),
            serde_json::json!(1200),
        ]]),
        row_count: Some(1),
        output: Some("first sample row only".into()),
        chart: None,
        result_table: None,
        python_input_trace: None,
        python_queries: None,
        python_queries_complete: None,
        ms: 0,
        error: None,
    };
    let prior = [inspection];
    let context = ToolContext {
        prior_evidence: &prior,
        conversation_id: None,
    };

    let result = registry
        .run_with_context_cancel(
            &engine,
            "make_chart",
            &serde_json::json!({
                "kind": "pie",
                "source_evidence_id": "evidence-1",
                "x_field": "category",
                "value_field": "amount",
                "part_to_whole": true,
                "denominator": "all spending"
            }),
            &context,
            Arc::new(AtomicBool::new(false)),
        )
        .await
        .unwrap();

    let error = match result {
        Ok(_) => panic!("inspection samples must not become chart evidence"),
        Err(error) => error,
    };
    assert!(error.to_string().contains("inspection returns a sample"));
    assert!(error.to_string().contains("complete analytical result"));

    let _ = fs::remove_dir_all(&data);
}

#[tokio::test]
async fn make_chart_respects_a_disabled_visualization_capability() {
    let ws = scratch("chart-capability-ws");
    let data = scratch("chart-capability-data");
    fs::write(ws.join("sales.csv"), "category,amount\nRent,1250\n").unwrap();
    let engine = EngineState::new(&data).unwrap();
    engine.open_workspace(&ws).unwrap();
    let patch = serde_json::json!({
        "capabilities": {
            "table_analysis": true,
            "document_analysis": true,
            "python_analysis": true,
            "visualizations": false
        }
    });
    engine.save_settings(patch.as_object().unwrap()).unwrap();

    let args = serde_json::json!({
        "kind": "bar",
        "sql": "SELECT category, amount FROM sales"
    });
    let registry = Registry::standard();
    match registry.run(&engine, "make_chart", &args).await {
        Some(Err(error)) => assert!(error.to_string().contains("Visualizations are disabled")),
        Some(Ok(_)) => panic!("disabled visualizations unexpectedly ran"),
        None => panic!("make_chart was missing from the default registry"),
    }

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
    assert_eq!(
        forecast.series[0].values,
        vec![Some(100.0), Some(120.0), Some(140.0)]
    );
    assert_eq!(
        forecast.series[1].values,
        vec![Some(80.0), Some(150.0), Some(130.0)]
    );

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
    assert_eq!(
        net.series[0].values,
        vec![Some(-50.0), Some(120.0), Some(-20.0)]
    );

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
    assert_eq!(mood.series[0].values, vec![Some(2.0), Some(4.0), Some(3.0)]);

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
    assert_eq!(
        screen.series[0].values,
        vec![Some(95.0), Some(30.0), Some(15.0)]
    );

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
    assert_eq!(
        costs.series[0].values,
        vec![Some(1200.0), Some(80.0), Some(35.5)]
    );

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
    let mut csv = String::from("category,value\n");
    for category in 1..=13 {
        csv.push_str(&format!("category-{category},{}\n", category * 10));
    }
    fs::write(ws.join("categories.csv"), csv).unwrap();
    let engine = EngineState::new(&data).unwrap();
    engine.open_workspace(&ws).unwrap();

    let out = Registry::standard()
        .run(
            &engine,
            "make_chart",
            &serde_json::json!({
                "kind": "bar",
                "sql": "SELECT category, value FROM categories ORDER BY category"
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
async fn make_chart_allows_more_than_twelve_daily_points() {
    let ws = scratch("chart-time-series-ws");
    let data = scratch("chart-time-series-data");
    let mut csv = String::from("date,value\n");
    for day in 1..=13 {
        csv.push_str(&format!("2024-01-{day:02},{}\n", day * 10));
    }
    fs::write(ws.join("daily.csv"), csv).unwrap();
    let engine = EngineState::new(&data).unwrap();
    engine.open_workspace(&ws).unwrap();

    let chart = Registry::standard()
        .run(
            &engine,
            "make_chart",
            &serde_json::json!({
                "kind": "auto",
                "sql": "SELECT date, value FROM daily ORDER BY date"
            }),
        )
        .await
        .unwrap()
        .unwrap()
        .chart
        .expect("daily time series chart");

    assert_eq!(
        chart.kind,
        fella_lib::engine::analytics::chart::ChartKind::Line
    );
    assert_eq!(chart.labels.len(), 13);
    assert_eq!(chart.series[0].values.last(), Some(&Some(130.0)));

    let _ = fs::remove_dir_all(&ws);
    let _ = fs::remove_dir_all(&data);
}

#[tokio::test]
async fn make_chart_rejects_a_truncated_long_time_series() {
    let ws = scratch("chart-long-time-series-ws");
    let data = scratch("chart-long-time-series-data");
    let mut csv = String::from("date,value\n");
    for day in 0..=1_000 {
        csv.push_str(&format!("day-{day:04},{}\n", day + 1));
    }
    fs::write(ws.join("long_daily.csv"), csv).unwrap();
    let engine = EngineState::new(&data).unwrap();
    engine.open_workspace(&ws).unwrap();

    let out = Registry::standard()
        .run(
            &engine,
            "make_chart",
            &serde_json::json!({
                "kind": "line",
                "sql": "SELECT date, value FROM long_daily ORDER BY date"
            }),
        )
        .await
        .unwrap();
    let error = match out {
        Ok(_) => panic!("a truncated long time series should be rejected"),
        Err(error) => error,
    };
    assert!(error.to_string().contains("raw result limit"), "{error}");

    let _ = fs::remove_dir_all(&ws);
    let _ = fs::remove_dir_all(&data);
}

#[tokio::test]
async fn make_chart_pivots_a_complete_long_form_daily_series_over_the_sql_preview_cap() {
    let ws = scratch("chart-long-form-daily-ws");
    let data = scratch("chart-long-form-daily-data");
    let mut csv = String::from("date,channel,visits\n");
    let channels = ["Direct", "Partner", "Retail", "Search"];
    let month_days_2024 = [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let month_days_2025 = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    for day_index in 0..731 {
        let (year, mut day_of_year, month_days) = if day_index < 366 {
            (2024, day_index, &month_days_2024)
        } else {
            (2025, day_index - 366, &month_days_2025)
        };
        let mut month = 0;
        while day_of_year >= month_days[month] {
            day_of_year -= month_days[month];
            month += 1;
        }
        let date = format!("{year}-{:02}-{:02}", month + 1, day_of_year + 1);
        for (channel_index, channel) in channels.iter().enumerate() {
            let visits = 100 + day_index * 10 + channel_index;
            csv.push_str(&format!("{date},{channel},{visits}\n"));
        }
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
                "sql": "SELECT date, channel, visits FROM daily ORDER BY date, channel",
                "x_field": "date",
                "group_field": "channel",
                "value_field": "visits",
                "missing_treatment": "gap"
            }),
        )
        .await
        .unwrap()
        .unwrap();
    let chart = out.chart.expect("full-period daily chart");

    assert_eq!(out.row_count, Some(2_924));
    assert_eq!(chart.labels.len(), 731);
    assert_eq!(chart.labels.first().map(String::as_str), Some("2024-01-01"));
    assert_eq!(chart.labels.last().map(String::as_str), Some("2025-12-31"));
    assert_eq!(chart.series.len(), 4);
    for (channel_index, channel) in channels.iter().enumerate() {
        let series = chart
            .series
            .iter()
            .find(|series| series.name == *channel)
            .unwrap();
        assert_eq!(series.values.len(), 731, "all daily values for {channel}");
        assert_eq!(
            series.values.first(),
            Some(&Some((100 + channel_index) as f64))
        );
        assert_eq!(
            series.values.last(),
            Some(&Some((100 + 730 * 10 + channel_index) as f64))
        );
    }

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
