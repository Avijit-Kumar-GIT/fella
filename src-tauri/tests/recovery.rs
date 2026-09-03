//! A corrupt `fella.db` must not brick the app: it's moved aside and a fresh
//! one is created.

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
fn a_corrupt_fella_db_is_moved_aside_and_recreated() {
    let data = scratch("recover-data");
    // Not a SQLite file at all.
    fs::write(data.join("fella.db"), b"this is not a database, it is garbage").unwrap();

    let engine = EngineState::new(&data).expect("engine should start despite the bad db");

    // A working settings store: reading it doesn't panic, and a write sticks.
    let before = engine.settings();
    assert!(!before.provider.is_empty());
    engine
        .save_settings(
            serde_json::json!({ "model": "recovery-check" }).as_object().unwrap(),
        )
        .unwrap();
    assert_eq!(engine.settings().model, "recovery-check");

    // The bad file was preserved, not deleted.
    let moved = fs::read_dir(&data)
        .unwrap()
        .filter_map(|e| e.ok())
        .any(|e| e.file_name().to_string_lossy().starts_with("fella.db.corrupt-"));
    assert!(moved, "the corrupt db should be kept as fella.db.corrupt-*");

    let _ = fs::remove_dir_all(&data);
}
