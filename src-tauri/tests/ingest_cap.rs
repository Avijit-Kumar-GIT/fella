//! Big-file guard: a delimited file past `FELLA_INGEST_ROW_CAP` still loads, but
//! only its first N rows, with a note saying so. Its own test binary so the
//! process-global override can't leak into a sibling test (same reasoning as
//! `tests/sql_timeout.rs`).

use std::fmt::Write as _;
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

#[test]
fn a_huge_csv_loads_its_first_rows_and_says_it_was_truncated() {
    std::env::set_var("FELLA_INGEST_ROW_CAP", "10");
    std::env::set_var("FELLA_SKIP_MODEL_WARMUP", "1");

    let ws = scratch("cap-ws");
    let data = scratch("cap-data");

    let mut csv = String::from("n,label\n");
    for i in 0..500 {
        writeln!(csv, "{i},row-{i}").unwrap();
    }
    fs::write(ws.join("big.csv"), csv).unwrap();

    let engine = EngineState::new(&data).unwrap();
    let catalog = engine.open_workspace(&ws).unwrap();

    let big = catalog.sources.iter().find(|s| s.name == "big.csv").unwrap();
    let loaded = big.row_count.expect("row_count is set");

    // Truncated: far fewer than the 500 rows in the file, and around the cap.
    assert!(
        (1..=20).contains(&loaded),
        "should have stopped near the 10-row cap, loaded {loaded}"
    );
    assert!(
        big.note.as_deref().unwrap_or("").contains("rows were loaded"),
        "truncation is noted: {:?}",
        big.note
    );

    // What did load is real and consistent with row_count.
    let out = engine.run_sql("SELECT count(*) AS c FROM big").unwrap();
    assert_eq!(out.rows[0][0], serde_json::json!(loaded));

    let _ = fs::remove_dir_all(&ws);
    let _ = fs::remove_dir_all(&data);
}
