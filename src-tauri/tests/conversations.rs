//! Conversation archiving: one JSON file per ended conversation, under
//! `<data dir>/conversations/`, written once per id.

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
fn archives_a_transcript_to_a_pretty_json_file() {
    let data = scratch("conv-write");
    let engine = EngineState::new(&data).unwrap();

    assert_eq!(engine.conversations_info().count, 0);

    let body = r#"{"id":"abcd1234","saved_at_ms":1,"workspace":null,"messages":[{"role":"user","text":"hi"}]}"#;
    let path = engine.archive_conversation("abcd1234", body).unwrap();

    // named conv_<ms>_<id>.json under conversations/
    let name = std::path::Path::new(&path).file_name().unwrap().to_string_lossy().to_string();
    assert!(name.starts_with("conv_"), "{name}");
    assert!(name.ends_with("_abcd1234.json"), "{name}");
    assert!(path.contains("conversations"));

    // re-serialized pretty (has newlines + indentation), still valid JSON
    let written = fs::read_to_string(&path).unwrap();
    assert!(written.contains("\n  \""));
    let v: serde_json::Value = serde_json::from_str(&written).unwrap();
    assert_eq!(v["messages"][0]["text"], "hi");

    assert_eq!(engine.conversations_info().count, 1);

    let _ = fs::remove_dir_all(&data);
}

#[test]
fn a_second_archive_for_the_same_id_is_a_no_op() {
    let data = scratch("conv-dedupe");
    let engine = EngineState::new(&data).unwrap();

    let first = engine
        .archive_conversation("dup777", r#"{"id":"dup777","messages":[]}"#)
        .unwrap();
    let again = engine
        .archive_conversation("dup777", r#"{"id":"dup777","messages":[{"role":"user","text":"changed"}]}"#)
        .unwrap();

    assert_eq!(first, again);
    assert_eq!(engine.conversations_info().count, 1);
    // the original content is untouched
    let written = fs::read_to_string(&first).unwrap();
    assert!(!written.contains("changed"));

    let _ = fs::remove_dir_all(&data);
}

#[test]
fn lists_conversations_newest_first_with_a_preview() {
    let data = scratch("conv-list");
    let engine = EngineState::new(&data).unwrap();

    engine
        .archive_conversation(
            "older",
            r#"{"id":"older","saved_at_ms":100,"workspace":"/w1",
                "messages":[{"role":"user","text":"How did my spending change?"},
                            {"role":"assistant","text":"Up 12%."}]}"#,
        )
        .unwrap();
    engine
        .archive_conversation(
            "newer",
            r#"{"id":"newer","saved_at_ms":200,"workspace":null,"messages":[]}"#,
        )
        .unwrap();

    let list = engine.conversations_list();
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].id, "newer", "newest saved_at_ms should come first");
    assert_eq!(list[1].id, "older");
    assert_eq!(list[1].preview, "How did my spending change?");
    assert_eq!(list[1].message_count, 2);
    assert_eq!(list[1].workspace.as_deref(), Some("/w1"));
    assert_eq!(list[0].preview, "(empty conversation)");
    assert_eq!(list[0].workspace, None);

    let _ = fs::remove_dir_all(&data);
}

#[test]
fn loads_a_conversation_by_id_and_rejects_an_unknown_one() {
    let data = scratch("conv-load");
    let engine = EngineState::new(&data).unwrap();

    let body = r#"{"id":"loadme","saved_at_ms":1,"workspace":"/w","messages":[{"role":"user","text":"hi"}]}"#;
    engine.archive_conversation("loadme", body).unwrap();

    let loaded = engine.conversation_load("loadme").unwrap();
    let v: serde_json::Value = serde_json::from_str(&loaded).unwrap();
    assert_eq!(v["messages"][0]["text"], "hi");
    assert_eq!(v["workspace"], "/w");

    assert!(engine.conversation_load("no-such-id").is_err());

    let _ = fs::remove_dir_all(&data);
}

#[test]
fn a_weird_id_is_sanitized_and_bad_json_is_rejected() {
    let data = scratch("conv-edge");
    let engine = EngineState::new(&data).unwrap();

    let path = engine
        .archive_conversation("../../etc/passwd", r#"{"messages":[]}"#)
        .unwrap();
    let name = std::path::Path::new(&path).file_name().unwrap().to_string_lossy().to_string();
    assert!(name.ends_with("_etcpasswd.json"), "{name}");
    assert_eq!(std::path::Path::new(&path).parent().unwrap(), data.join("conversations"));

    assert!(engine.archive_conversation("x", "not json at all").is_err());

    let _ = fs::remove_dir_all(&data);
}
