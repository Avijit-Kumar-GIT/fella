//! `grep_files` / `read_file`: read-only, index-free document access.

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

fn open(ws: &std::path::Path, data: &std::path::Path) -> EngineState {
    let engine = EngineState::new(data).unwrap();
    engine.open_workspace(ws).unwrap();
    engine
}

#[test]
fn grep_files_finds_matching_lines_with_source_and_line_number() {
    let ws = scratch("grep-ws");
    let data = scratch("grep-data");
    fs::write(ws.join("a.txt"), "just some prose\nnothing special here\n").unwrap();
    fs::write(ws.join("b.txt"), "line one\na distinctive phrase appears here\nline three\n").unwrap();

    let engine = open(&ws, &data);
    let hits = engine.grep_files("distinctive phrase", 10).unwrap();

    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].source, "b.txt");
    assert_eq!(hits[0].line, 2);
    assert!(hits[0].text.contains("distinctive phrase"));

    let _ = fs::remove_dir_all(&ws);
    let _ = fs::remove_dir_all(&data);
}

#[test]
fn grep_files_is_case_insensitive_and_caps_hits() {
    let ws = scratch("grep-ws2");
    let data = scratch("grep-data2");
    fs::write(ws.join("a.txt"), "Coffee in the morning\ncoffee again at noon\nCOFFEE at night\n").unwrap();

    let engine = open(&ws, &data);
    let hits = engine.grep_files("coffee", 2).unwrap();
    assert_eq!(hits.len(), 2, "should stop at max_hits");

    let all = engine.grep_files("coffee", 10).unwrap();
    assert_eq!(all.len(), 3, "should match regardless of case");

    let _ = fs::remove_dir_all(&ws);
    let _ = fs::remove_dir_all(&data);
}

#[test]
fn read_file_returns_extracted_text() {
    let ws = scratch("read-ws");
    let data = scratch("read-data");
    fs::write(ws.join("note.txt"), "the exact content of this note\n").unwrap();

    let engine = open(&ws, &data);
    let (text, truncated) = engine.read_file("note.txt").unwrap();

    assert_eq!(text, "the exact content of this note\n");
    assert!(!truncated);

    let _ = fs::remove_dir_all(&ws);
    let _ = fs::remove_dir_all(&data);
}

#[tokio::test]
async fn read_file_tool_reads_several_names_in_one_call() {
    use fella_lib::engine::tools::Registry;

    let ws = scratch("readmulti-ws");
    let data = scratch("readmulti-data");
    fs::write(ws.join("one.md"), "alpha content\n").unwrap();
    fs::write(ws.join("two.md"), "beta content\n").unwrap();

    let engine = open(&ws, &data);
    let reg = Registry::standard();
    let out = reg
        .run(
            &engine,
            "read_file",
            &serde_json::json!({ "names": ["one.md", "two.md"] }),
        )
        .await
        .expect("tool exists")
        .expect("tool ok");

    assert!(out.llm_text.contains("alpha content"));
    assert!(out.llm_text.contains("beta content"));
    assert!(out.llm_text.contains("=== one.md ==="));
    assert!(out.summary.contains("one.md") && out.summary.contains("two.md"));

    // The single-name form still works.
    let out1 = reg
        .run(&engine, "read_file", &serde_json::json!({ "name": "two.md" }))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(out1.llm_text, "beta content\n");

    let _ = fs::remove_dir_all(&ws);
    let _ = fs::remove_dir_all(&data);
}

#[test]
fn read_file_truncates_a_very_long_document() {
    let ws = scratch("read-ws2");
    let data = scratch("read-data2");
    let long: String = "x".repeat(20_000);
    fs::write(ws.join("long.txt"), &long).unwrap();

    let engine = open(&ws, &data);
    let (text, truncated) = engine.read_file("long.txt").unwrap();

    assert!(truncated);
    assert!(text.chars().count() < long.chars().count());

    let _ = fs::remove_dir_all(&ws);
    let _ = fs::remove_dir_all(&data);
}

#[test]
fn read_file_and_grep_files_reject_unknown_or_tabular_names() {
    let ws = scratch("read-ws3");
    let data = scratch("read-data3");
    fs::write(ws.join("sales.csv"), "month,amount\n2024-01,100\n").unwrap();
    fs::write(ws.join("note.txt"), "hello\n").unwrap();

    let engine = open(&ws, &data);

    assert!(engine.read_file("does-not-exist.txt").is_err());
    assert!(engine.read_file("sales.csv").is_err(), "tables aren't documents");

    let _ = fs::remove_dir_all(&ws);
    let _ = fs::remove_dir_all(&data);
}
