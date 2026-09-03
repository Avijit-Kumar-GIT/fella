//! run_python: output capture and (best-effort) isolation.

use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use fella_lib::engine::EngineState;

fn scratch(tag: &str) -> PathBuf {
    let n = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let p = std::env::temp_dir().join(format!("fella-{tag}-{n}"));
    fs::create_dir_all(&p).unwrap();
    p
}

fn have_python() -> bool {
    std::process::Command::new("python3")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[tokio::test]
async fn runs_python_and_captures_stdout_and_stderr() {
    if !have_python() {
        eprintln!("skipping: python3 not installed");
        return;
    }
    let data = scratch("py-data");
    let engine = EngineState::new(&data).unwrap();

    let r = engine
        .run_python("import sys\nprint(6 * 7)\nprint('warned', file=sys.stderr)")
        .await
        .unwrap();

    assert_eq!(r.exit_code, Some(0));
    assert!(!r.timed_out);
    assert!(r.stdout.contains("42"), "stdout: {}", r.stdout);
    assert!(r.stderr.contains("warned"), "stderr: {}", r.stderr);

    let _ = fs::remove_dir_all(&data);
}

#[tokio::test]
async fn python_nonzero_exit_is_reported_not_errored() {
    if !have_python() {
        return;
    }
    let data = scratch("py-data2");
    let engine = EngineState::new(&data).unwrap();

    // A raising snippet should still return a PyResult (exit 1), not Err.
    let r = engine.run_python("raise SystemExit(3)").await.unwrap();
    assert_eq!(r.exit_code, Some(3));
    assert!(!r.timed_out);

    let _ = fs::remove_dir_all(&data);
}
