//! Ingestion completeness: legacy row/byte cap environment variables must not
//! silently make a supported source appear complete with only a prefix loaded.
//! Its own test binary isolates the process environment from sibling tests.

use std::fmt::Write as _;
use std::fs::{self, File};
use std::io::BufWriter;
use std::io::Write as _;
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

    let mut json = String::from("[null,\n");
    let mut ndjson = String::new();
    for i in 0..500 {
        let row = format!(r#"{{"n":{i},"label":"row-{i}"}}"#);
        if i > 0 {
            json.push_str(",\n");
        }
        json.push_str(&row);
        writeln!(ndjson, "{row}").unwrap();
        if i == 249 {
            ndjson.push_str("{ malformed record }\n");
        }
    }
    json.push_str(",\n17\n]\n");
    fs::write(ws.path().join("array-data.json"), json).unwrap();
    fs::write(ws.path().join("line-data.ndjson"), ndjson).unwrap();
    fs::write(
        ws.path().join("object-data.json"),
        r#"{"label":"one row","n":1}"#,
    )
    .unwrap();

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

    for source_name in ["array-data.json", "line-data.ndjson"] {
        let source = catalog
            .sources
            .iter()
            .find(|source| source.name == source_name)
            .unwrap_or_else(|| panic!("{source_name} should be queryable"));
        assert_eq!(source.row_count, Some(500), "{source_name}");
        let table = source.view.as_deref().expect("table view name");
        let count = engine
            .run_sql(&format!("SELECT count(*) FROM \"{table}\""))
            .unwrap();
        assert_eq!(count.rows[0][0], serde_json::json!(500), "{source_name}");
    }
    let array = catalog
        .sources
        .iter()
        .find(|source| source.name == "array-data.json")
        .unwrap();
    assert!(array
        .note
        .as_deref()
        .unwrap_or_default()
        .contains("2 non-object JSON record(s) were skipped"));
    let lines = catalog
        .sources
        .iter()
        .find(|source| source.name == "line-data.ndjson")
        .unwrap();
    assert!(lines
        .note
        .as_deref()
        .unwrap_or_default()
        .contains("1 malformed JSON line(s) were skipped"));
    let object = catalog
        .sources
        .iter()
        .find(|source| source.name == "object-data.json")
        .unwrap();
    assert_eq!(object.row_count, Some(1));
}

#[test]
fn a_large_delimited_source_is_streamed_completely() {
    std::env::set_var("FELLA_INGEST_ROW_CAP", "10");
    std::env::set_var("FELLA_INGEST_BYTE_CAP", "64");
    std::env::set_var("FELLA_SKIP_MODEL_WARMUP", "1");

    const ROWS: usize = 250_000;
    let ws = Scratch::new("large-ingest-ws");
    let data = Scratch::new("large-ingest-data");
    let path = ws.path().join("large.csv");
    let mut writer = BufWriter::new(File::create(&path).unwrap());
    writeln!(writer, "sequence,label").unwrap();
    for sequence in 0..ROWS {
        writeln!(writer, "{sequence},record-{sequence}").unwrap();
    }
    drop(writer);

    let started = std::time::Instant::now();
    let engine = EngineState::new(data.path()).unwrap();
    let catalog = engine.open_workspace(ws.path()).unwrap();
    let elapsed = started.elapsed();
    let source = catalog
        .sources
        .iter()
        .find(|source| source.name == "large.csv")
        .unwrap();

    assert_eq!(source.row_count, Some(ROWS as i64));
    assert!(source
        .columns
        .as_ref()
        .unwrap()
        .iter()
        .all(|column| column.distinct.is_none()));
    let count = engine.run_sql("SELECT count(*) FROM large").unwrap();
    assert_eq!(count.rows[0][0], serde_json::json!(ROWS));
    let last = engine
        .run_sql("SELECT label FROM large ORDER BY rowid DESC LIMIT 1")
        .unwrap();
    assert_eq!(
        last.rows[0][0],
        serde_json::json!(format!("record-{}", ROWS - 1))
    );
    eprintln!(
        "streaming ingest sample: rows={ROWS} bytes={} elapsed_ms={}",
        fs::metadata(path).unwrap().len(),
        elapsed.as_millis()
    );

    let described = engine.describe_source("large").unwrap();
    assert!(described
        .columns
        .unwrap()
        .iter()
        .all(|column| column.distinct.is_some()));
}
