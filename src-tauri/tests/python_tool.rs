//! run_python: output capture and embedded-WASM isolation.

use std::fs;
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

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
async fn runs_python_and_captures_stdout_and_stderr() {
    let data = scratch("py-data");
    let engine = EngineState::new(&data).unwrap();

    let r = engine
        .run_python("import sys\nprint(6 * 7)\nprint('warned', file=sys.stderr)")
        .await
        .unwrap();

    assert_eq!(r.exit_code, Some(0), "stderr: {}", r.stderr);
    assert!(!r.timed_out);
    assert!(r.stdout.contains("42"), "stdout: {}", r.stdout);
    assert!(r.stderr.contains("warned"), "stderr: {}", r.stderr);

    let _ = fs::remove_dir_all(&data);
}

#[tokio::test]
async fn capability_policy_blocks_python_analysis() {
    let data = scratch("py-capability-data");
    let engine = EngineState::new(&data).unwrap();
    let patch = serde_json::json!({
        "capabilities": {
            "table_analysis": true,
            "document_analysis": true,
            "python_analysis": false,
            "visualizations": true
        }
    });
    engine.save_settings(patch.as_object().unwrap()).unwrap();

    match engine.run_python("print(42)").await {
        Err(error) => assert!(error.to_string().contains("Python analysis is disabled")),
        Ok(_) => panic!("disabled Python analysis unexpectedly ran"),
    }

    let _ = fs::remove_dir_all(&data);
}

#[tokio::test]
async fn python_nonzero_exit_is_reported_not_errored() {
    let data = scratch("py-data2");
    let engine = EngineState::new(&data).unwrap();

    // A raising snippet should still return a PyResult (nonzero), not Err.
    let r = engine.run_python("raise SystemExit(3)").await.unwrap();
    assert!(
        r.exit_code.is_some_and(|code| code != 0),
        "stderr: {}",
        r.stderr
    );
    assert!(!r.timed_out);

    let _ = fs::remove_dir_all(&data);
}

#[tokio::test]
async fn python_sql_bridge_returns_workspace_rows() {
    let data = scratch("py-sql-data");
    let workspace = scratch("py-sql-workspace");
    fs::write(
        workspace.join("sales.csv"),
        "name,amount\nalpha,10\nbeta,20\n",
    )
    .unwrap();
    let engine = EngineState::new(&data).unwrap();
    engine.open_workspace(&workspace).unwrap();

    let r = engine
        .run_python(
            "rows = sql('SELECT name, amount FROM sales ORDER BY amount')\n\
             print(rows[0]['name'])\n\
             print(sum(row['amount'] for row in rows))",
        )
        .await
        .unwrap();

    assert_eq!(r.exit_code, Some(0), "stderr: {}", r.stderr);
    assert!(r.stdout.contains("alpha"), "stdout: {}", r.stdout);
    assert!(r.stdout.contains("30"), "stdout: {}", r.stdout);
    assert_eq!(r.queries.len(), 1);
    assert!(r.query_trace_complete);
    assert_eq!(
        r.queries[0].sql,
        "SELECT name, amount FROM sales ORDER BY amount"
    );
    assert_eq!(r.queries[0].row_count, 2);

    let _ = fs::remove_dir_all(&data);
    let _ = fs::remove_dir_all(&workspace);
}

#[tokio::test]
async fn python_can_publish_a_typed_chartable_result_without_leaking_wire_data() {
    let data = scratch("py-published-table");
    let engine = EngineState::new(&data).unwrap();

    let result = engine
        .run_python(
            "fella_table(['period', 'value', 'note'], [\n\
             ['Jan', 1.5, True],\n\
             ['Fév', None, 'He said \"hello\"']\n\
             ])\n\
             print('analysis complete')",
        )
        .await
        .unwrap();

    assert_eq!(result.exit_code, Some(0), "stderr: {}", result.stderr);
    let table = result
        .result_table
        .expect("explicitly published typed table");
    assert_eq!(table.columns, vec!["period", "value", "note"]);
    assert_eq!(
        table.rows[0],
        vec![
            serde_json::json!("Jan"),
            serde_json::json!(1.5),
            serde_json::json!(true)
        ]
    );
    assert_eq!(
        table.rows[1],
        vec![
            serde_json::json!("Fév"),
            serde_json::Value::Null,
            serde_json::json!("He said \"hello\"")
        ]
    );
    assert!(
        result.stdout.contains("analysis complete"),
        "stdout: {}",
        result.stdout
    );
    assert!(
        !result.stdout.contains("__FELLA_TABLE__"),
        "internal table record leaked: {}",
        result.stdout
    );

    let _ = fs::remove_dir_all(&data);
}

#[tokio::test]
async fn python_has_built_in_stats_helpers() {
    let data = scratch("py-stats");
    let engine = EngineState::new(&data).unwrap();

    let r = engine
        .run_python("print(median([1, 4, 2]))\nprint(stdev([1, 2, 3]))")
        .await
        .unwrap();

    assert_eq!(r.exit_code, Some(0), "stderr: {}", r.stderr);
    assert!(r.stdout.contains("2"), "stdout: {}", r.stdout);
    assert!(r.stdout.contains("1.0"), "stdout: {}", r.stdout);

    let _ = fs::remove_dir_all(&data);
}

#[tokio::test]
async fn forecast_helpers_offer_reusable_methods_without_hiding_the_formula() {
    let data = scratch("py-forecast-methods");
    let engine = EngineState::new(&data).unwrap();

    let r = engine
        .run_python(
            "values = [1, 2, 3, 4, 5]\n\
             print(forecast_series(values, 2, 'naive'))\n\
             print(forecast_series(values, 2, 'mean'))\n\
             print(forecast_series(values, 2, 'drift'))\n\
             print(forecast_series(values, 2, 'linear_trend'))\n\
             print(forecast_series([10, 12, 20, 22], 3, 'seasonal_naive', 2))",
        )
        .await
        .unwrap();

    assert_eq!(r.exit_code, Some(0), "stderr: {}", r.stderr);
    let lines = r.stdout.lines().collect::<Vec<_>>();
    assert_eq!(lines[0], "[5.0, 5.0]");
    assert_eq!(lines[1], "[3.0, 3.0]");
    assert_eq!(lines[2], "[6.0, 7.0]");
    assert_eq!(lines[3], "[6.0, 7.0]");
    assert_eq!(lines[4], "[20.0, 22.0, 20.0]");

    let _ = fs::remove_dir_all(&data);
}

#[tokio::test]
async fn rolling_backtest_uses_only_each_origin_prefix_and_compares_with_naive() {
    let data = scratch("py-forecast-backtest");
    let engine = EngineState::new(&data).unwrap();

    // The holdout value 100 is not part of the five-point training prefix.
    // Linear trend predicts 6; last-value naive predicts 5.
    let r = engine
        .run_python(
            "print(rolling_origin_backtest([1, 2, 3, 4, 5, 100], 'linear_trend', \
             horizon=1, min_train=5))",
        )
        .await
        .unwrap();

    assert_eq!(r.exit_code, Some(0), "stderr: {}", r.stderr);
    assert!(r.stdout.contains("'mae': 94.0"), "stdout: {}", r.stdout);
    assert!(
        r.stdout.contains("'baseline_mae': 95.0"),
        "stdout: {}",
        r.stdout
    );
    assert!(r.stdout.contains("'origins': 1"), "stdout: {}", r.stdout);
    assert!(
        r.stdout.contains("'n_forecasts': 1"),
        "stdout: {}",
        r.stdout
    );
    assert!(
        r.stdout.contains("'available_origins': 1"),
        "stdout: {}",
        r.stdout
    );

    let _ = fs::remove_dir_all(&data);
}

#[tokio::test]
async fn forecast_error_bands_withhold_insufficient_or_degenerate_uncertainty() {
    let data = scratch("py-forecast-bands");
    let engine = EngineState::new(&data).unwrap();

    let short = engine
        .run_python(
            "print(forecast_error_bands([1, 2, 3, 2, 5, 4, 7, 6, 9], \
             'linear_trend', 1))",
        )
        .await
        .unwrap();
    assert_eq!(short.exit_code, Some(0), "stderr: {}", short.stderr);
    assert!(
        short.stdout.contains("'n_errors': 6"),
        "stdout: {}",
        short.stdout
    );
    assert!(
        short.stdout.contains("'lower': None"),
        "stdout: {}",
        short.stdout
    );
    assert!(
        short.stdout.contains("too few rolling-origin errors"),
        "stdout: {}",
        short.stdout
    );

    let degenerate = engine
        .run_python(
            "print(forecast_error_bands([2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2], \
             'naive', 1, min_errors=8))",
        )
        .await
        .unwrap();
    assert_eq!(
        degenerate.exit_code,
        Some(0),
        "stderr: {}",
        degenerate.stderr
    );
    assert!(
        degenerate.stdout.contains("'lower': None"),
        "stdout: {}",
        degenerate.stdout
    );
    assert!(
        degenerate.stdout.contains("no observed spread"),
        "stdout: {}",
        degenerate.stdout
    );

    let _ = fs::remove_dir_all(&data);
}

#[tokio::test]
async fn forecast_helpers_do_not_silently_coerce_missing_values() {
    let data = scratch("py-forecast-missing");
    let engine = EngineState::new(&data).unwrap();

    let r = engine
        .run_python("print(forecast_series([10, None, 14], 1, 'mean'))")
        .await
        .unwrap();

    assert!(r.exit_code.is_some_and(|code| code != 0));
    assert!(
        r.stderr.contains("contains a missing value"),
        "stderr: {}",
        r.stderr
    );

    let _ = fs::remove_dir_all(&data);
}

#[tokio::test]
async fn forecast_helpers_bound_long_horizons_and_backtest_work() {
    let data = scratch("py-forecast-bounds");
    let engine = EngineState::new(&data).unwrap();

    // Keep each operation in a separate run: every Python tool invocation
    // gets its own bounded Wasm fuel budget in the actual app.
    let r = engine
        .run_python("print(rolling_origin_backtest(list(range(53)), 'naive', min_train=2))")
        .await
        .unwrap();

    assert_eq!(r.exit_code, Some(0), "stderr: {}", r.stderr);
    assert!(r.stdout.contains("'origins': 10"), "stdout: {}", r.stdout);
    assert!(
        r.stdout.contains("'available_origins': 51"),
        "stdout: {}",
        r.stdout
    );

    let long_horizon = engine
        .run_python(
            "print(rolling_origin_backtest(list(range(505)), 'naive', horizon=501, min_train=2))",
        )
        .await
        .unwrap();
    assert_eq!(
        long_horizon.exit_code,
        Some(0),
        "stderr: {}",
        long_horizon.stderr
    );
    assert!(
        long_horizon.stdout.contains("'horizon': 501"),
        "stdout: {}",
        long_horizon.stdout
    );
    assert!(
        long_horizon.stdout.contains("'origins': 1"),
        "stdout: {}",
        long_horizon.stdout
    );
    assert!(
        long_horizon.stdout.contains("'n_forecasts': 501"),
        "stdout: {}",
        long_horizon.stdout
    );

    let excessive_horizon = engine
        .run_python(
            "try:\n    forecast_series([1, 2], 1001)\nexcept ValueError as error:\n    print(error)",
        )
        .await
        .unwrap();
    assert_eq!(
        excessive_horizon.exit_code,
        Some(0),
        "stderr: {}",
        excessive_horizon.stderr
    );
    assert!(
        excessive_horizon
            .stdout
            .contains("horizon exceeds the helper limit of 1000"),
        "stdout: {}",
        excessive_horizon.stdout
    );

    let _ = fs::remove_dir_all(&data);
}

#[tokio::test]
async fn python_cannot_open_a_host_file() {
    let data = scratch("py-isolation");
    let engine = EngineState::new(&data).unwrap();

    let r = engine
        .run_python("open('/etc/passwd').read()")
        .await
        .unwrap();

    assert!(
        r.exit_code.is_some_and(|code| code != 0),
        "stderr: {}",
        r.stderr
    );
    assert!(!r.timed_out);

    let _ = fs::remove_dir_all(&data);
}

#[tokio::test]
async fn python_output_is_capped() {
    let data = scratch("py-output-cap");
    let engine = EngineState::new(&data).unwrap();

    let r = engine.run_python("print('x' * 100_000)").await.unwrap();

    assert_eq!(r.exit_code, Some(0), "stderr: {}", r.stderr);
    assert!(
        r.stdout.contains("output truncated"),
        "stdout_len={} stderr={:?} timed_out={} exit={:?}",
        r.stdout.len(),
        r.stderr,
        r.timed_out,
        r.exit_code
    );
    assert!(r.stdout.len() <= 64 * 1024 + 32);

    let _ = fs::remove_dir_all(&data);
}

#[tokio::test]
async fn python_loop_is_stopped_by_fuel() {
    let data = scratch("py-fuel");
    let engine = EngineState::new(&data).unwrap();

    let r = engine.run_python("while True:\n    pass").await.unwrap();

    assert!(r.timed_out, "expected the fuel limit, got: {:?}", r.stderr);
    assert!(r.exit_code.is_none());

    let _ = fs::remove_dir_all(&data);
}

#[tokio::test]
async fn python_loop_can_be_stopped_by_the_user() {
    let data = scratch("py-cancel");
    let engine = Arc::new(EngineState::new(&data).unwrap());
    let cancel = Arc::new(AtomicBool::new(false));
    let running = Arc::clone(&engine);
    let worker_cancel = Arc::clone(&cancel);

    let task = tokio::spawn(async move {
        running
            .run_python_cancellable(
                "while True:\n    total = 0\n    for i in range(100_000):\n        total += i * i",
                worker_cancel,
            )
            .await
            .unwrap()
    });
    // The heavier body keeps the release build alive long enough for this
    // test to exercise user cancellation before the finite fuel budget ends.
    tokio::time::sleep(Duration::from_millis(50)).await;
    cancel.store(true, Ordering::Relaxed);

    let result = tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .expect("the Python worker did not respond to cancellation")
        .unwrap();
    assert!(
        result.cancelled,
        "expected user cancellation: stderr={}",
        result.stderr
    );
    assert!(!result.timed_out);

    let _ = fs::remove_dir_all(&data);
}

#[tokio::test]
async fn forecast_tool_returns_a_point_backtest_and_replayable_source() {
    let data = scratch("forecast-tool-data");
    let workspace = scratch("forecast-tool-workspace");
    fs::write(
        workspace.join("observations.csv"),
        "month,value\n2011-01,10\n2011-02,20\n2011-03,30\n2011-04,40\n2011-05,50\n2011-06,60\n",
    )
    .unwrap();
    let engine = EngineState::new(&data).unwrap();
    engine.open_workspace(&workspace).unwrap();
    let registry = Registry::standard();

    let result = match registry
        .run(
            &engine,
            "forecast_analysis",
            &serde_json::json!({
                "sql": "SELECT month AS period, SUM(value) AS value FROM observations GROUP BY month ORDER BY month",
                "method": "mean",
                "horizon": 1
            }),
        )
        .await
    {
        Some(Ok(result)) => result,
        Some(Err(error)) => panic!("forecast tool failed: {error}"),
        None => panic!("the standard Ask registry did not expose forecasts"),
    };

    let output = result.output.as_deref().unwrap_or_default();
    assert!(output.contains("forecast_values=[35.0]"), "{output}");
    assert!(output.contains("'origins': 3"), "{output}");
    assert!(output.contains("'mae': 25.0"), "{output}");
    assert!(output.contains("'baseline_mae': 10.0"), "{output}");
    assert!(output.contains("too few rolling-origin errors"), "{output}");
    assert_eq!(
        result.columns.as_ref().unwrap(),
        &vec!["period".to_string(), "value".to_string()]
    );
    assert_eq!(result.row_count, Some(6));
    assert_eq!(result.python_queries_complete, Some(true));
    let chart_table = result
        .result_table
        .as_ref()
        .expect("forecast should publish its projected series for chart reuse");
    assert_eq!(
        chart_table.columns,
        vec!["period", "observed", "forecast", "lower", "upper"]
    );
    assert_eq!(chart_table.rows.len(), 7);
    assert_eq!(chart_table.rows[0][1], serde_json::json!(10.0));
    assert_eq!(chart_table.rows[0][2], serde_json::Value::Null);
    assert_eq!(chart_table.rows[6][0], serde_json::json!("Forecast +1"));
    assert_eq!(chart_table.rows[6][2], serde_json::json!(35.0));
    let trace = result.python_queries.as_deref().unwrap();
    assert_eq!(trace.len(), 1);
    assert_eq!(trace[0].row_count, 6);
    assert_eq!(trace[0].sql, "SELECT month AS period, SUM(value) AS value FROM observations GROUP BY month ORDER BY month");

    let short = match registry
        .run(
            &engine,
            "forecast_analysis",
            &serde_json::json!({
                "sql": "SELECT month AS period, SUM(value) AS value FROM observations GROUP BY month ORDER BY month LIMIT 2",
                "method": "mean",
                "horizon": 1
            }),
        )
        .await
    {
        Some(Ok(result)) => result,
        Some(Err(error)) => panic!("short-history forecast failed: {error}"),
        None => panic!("the forecast tool disappeared from the standard registry"),
    };
    let short_output = short.output.as_deref().unwrap_or_default();
    assert!(
        short_output.contains("forecast_values=[15.0]"),
        "{short_output}"
    );
    assert!(short_output.contains("backtest=None"), "{short_output}");
    assert!(
        short_output.contains("not enough history for a chronological holdout"),
        "{short_output}"
    );

    let duplicate_periods = match registry
        .run(
            &engine,
            "forecast_analysis",
            &serde_json::json!({
                "sql": "SELECT 'same' AS period, value AS value FROM observations ORDER BY value LIMIT 2",
                "method": "mean",
                "horizon": 1
            }),
        )
        .await
    {
        Some(Err(error)) => error,
        Some(Ok(_)) => panic!("duplicate forecast periods unexpectedly succeeded"),
        None => panic!("the forecast tool disappeared from the standard registry"),
    };
    assert!(
        duplicate_periods.to_string().contains("duplicate period"),
        "{duplicate_periods}"
    );

    let _ = fs::remove_dir_all(&data);
    let _ = fs::remove_dir_all(&workspace);
}
