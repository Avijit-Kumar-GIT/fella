//! Ingestion completeness: legacy row/byte cap environment variables must not
//! silently make a supported source appear complete with only a prefix loaded.
//! Its own test binary isolates the process environment from sibling tests.

use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use fella_lib::engine::EngineState;

struct Scratch(PathBuf);

impl Scratch {
    fn new(tag: &str) -> Self {
        let n = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p = std::env::temp_dir().join(format!("fella-{tag}-{n}"));
        fs::create_dir_all(&p).unwrap();
        Self(p)
    }

    fn path(&self) -> &std::path::Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn a_source_is_complete_even_when_legacy_ingest_caps_are_set() {
    std::env::set_var("FELLA_INGEST_ROW_CAP", "10");
    std::env::set_var("FELLA_INGEST_BYTE_CAP", "64");
    std::env::set_var("FELLA_SKIP_MODEL_WARMUP", "1");

    // This changes the intended contract from the old capped-ingest behavior:
    // every generated row is part of the dataset, not a tuning target for the
    // candidate. The low legacy values make the old implementation fail here.
    let ws = Scratch::new("complete-ws");
    let data = Scratch::new("complete-data");

    let mut csv = String::from("n,label\n");
    for i in 0..500 {
        writeln!(csv, "{i},row-{i}").unwrap();
    }
    fs::write(ws.path().join("big.csv"), csv).unwrap();

    let engine = EngineState::new(data.path()).unwrap();
    let catalog = engine.open_workspace(ws.path()).unwrap();

    let big = catalog
        .sources
        .iter()
        .find(|s| s.name == "big.csv")
        .unwrap();
    let loaded = big.row_count.expect("row_count is set");

    assert_eq!(loaded, 500, "all source rows must be queryable");
    assert!(big.note.is_none(), "complete input has no truncation note");

    // The SQL table must agree with the catalog and include the final input row.
    let out = engine.run_sql("SELECT count(*) AS c FROM big").unwrap();
    assert_eq!(out.rows[0][0], serde_json::json!(500));
    let last = engine
        .run_sql("SELECT label FROM big ORDER BY n DESC LIMIT 1")
        .unwrap();
    assert_eq!(last.rows[0][0], serde_json::json!("row-499"));
}
