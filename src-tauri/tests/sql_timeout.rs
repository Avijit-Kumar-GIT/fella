//! A runaway `run_sql` is interrupted rather than hanging the agent loop.
//! Its own test binary so the `FELLA_QUERY_TIMEOUT_SECS` override can't leak
//! into the other integration tests.

use std::fs;
use std::path::PathBuf;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use fella_lib::engine::EngineState;

fn scratch(tag: &str) -> PathBuf {
    let n = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let p = std::env::temp_dir().join(format!("fella-{tag}-{n}"));
    fs::create_dir_all(&p).unwrap();
    p
}

#[test]
fn runaway_query_is_interrupted() {
    std::env::set_var("FELLA_QUERY_TIMEOUT_SECS", "1");

    let ws = scratch("timeout-ws");
    let data = scratch("timeout-data");
    fs::write(ws.join("t.csv"), "x\n1\n2\n3\n").unwrap();

    let engine = EngineState::new(&data).unwrap();
    engine.open_workspace(&ws).unwrap();

    // An unbounded recursive CTE: it never finishes on its own.
    let started = Instant::now();
    let err = engine
        .run_sql("WITH RECURSIVE r(n) AS (SELECT 1 UNION ALL SELECT n + 1 FROM r) SELECT count(*) FROM r")
        .expect_err("a non-terminating query should have been interrupted");
    let elapsed = started.elapsed();

    assert!(
        err.to_string().contains("query stopped after"),
        "unexpected error: {err}"
    );
    assert!(
        elapsed.as_secs() < 8,
        "interrupt took too long: {elapsed:?}"
    );

    // The engine still works after an interrupted query.
    let ok = engine.run_sql("SELECT count(*) AS n FROM t").unwrap();
    assert_eq!(ok.row_count, 1);

    let _ = fs::remove_dir_all(&ws);
    let _ = fs::remove_dir_all(&data);
}
