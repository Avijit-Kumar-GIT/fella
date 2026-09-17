//! run_python: output capture and embedded-WASM isolation.

use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

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

    let _ = fs::remove_dir_all(&data);
    let _ = fs::remove_dir_all(&workspace);
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
