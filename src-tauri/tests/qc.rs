//! Robustness of the shared engine: a mangled `auth.json` doesn't stop
//! startup, and concurrent queries (the agent loop runs tool calls in
//! parallel) don't corrupt or deadlock the mutex-guarded state.

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use fella_lib::engine::EngineState;

fn scratch(tag: &str) -> PathBuf {
    let n = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let p = std::env::temp_dir().join(format!("fella-{tag}-{n}"));
    fs::create_dir_all(&p).unwrap();
    p
}

#[test]
fn a_mangled_auth_json_does_not_stop_startup() {
    let data = scratch("qc-auth-data");
    fs::write(data.join("auth.json"), b"{ this is not valid json").unwrap();

    let engine = EngineState::new(&data).expect("engine starts despite a bad auth.json");

    // Settings still work (a corrupt auth.json reads as "no key", not a crash).
    let s = engine.settings();
    assert!(!s.provider.is_empty());

    // And a fresh setting can still be saved over the top.
    engine
        .save_settings(serde_json::json!({ "model": "qc-check" }).as_object().unwrap())
        .unwrap();
    assert_eq!(engine.settings().model, "qc-check");

    let _ = fs::remove_dir_all(&data);
}

#[test]
fn concurrent_queries_do_not_deadlock_or_race() {
    let ws = scratch("qc-conc-ws");
    let data = scratch("qc-conc-data");
    fs::write(ws.join("nums.csv"), "n\n1\n2\n3\n4\n5\n").unwrap();

    let engine = Arc::new(EngineState::new(&data).unwrap());
    engine.open_workspace(&ws).unwrap();

    let mut handles = Vec::new();
    for _ in 0..8 {
        let e = Arc::clone(&engine);
        handles.push(thread::spawn(move || {
            for _ in 0..12 {
                let out = e.run_sql("SELECT sum(n) AS s FROM nums").unwrap();
                assert_eq!(out.rows[0][0], serde_json::json!(15));
                let _ = e.catalog();
                let _ = e.settings();
            }
        }));
    }
    for h in handles {
        h.join().expect("no thread panicked");
    }

    let _ = fs::remove_dir_all(&ws);
    let _ = fs::remove_dir_all(&data);
}
