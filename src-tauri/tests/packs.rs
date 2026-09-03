//! Packs: local install, enable/disable, skill injection, theme tokens, and
//! `fella.md` as workspace context. No network (marketplace install is a
//! later phase).

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use fella_lib::engine::EngineState;

fn scratch(tag: &str) -> PathBuf {
    let n = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let p = std::env::temp_dir().join(format!("fella-{tag}-{n}"));
    fs::create_dir_all(&p).unwrap();
    p
}

fn write_pack(root: &Path, id: &str, manifest: &str, payload_name: &str, payload: &str) -> PathBuf {
    let dir = root.join(id);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("fella-pack.json"), manifest).unwrap();
    fs::write(dir.join(payload_name), payload).unwrap();
    dir
}

fn skill_manifest(id: &str) -> String {
    format!(
        r#"{{"schema":1,"id":"{id}","kind":"skill","name":"{id}","version":"1.0.0","description":"d","payload":"skill.md"}}"#
    )
}

fn theme_manifest(id: &str) -> String {
    format!(
        r#"{{"schema":1,"id":"{id}","kind":"theme","name":"{id}","version":"1.0.0","description":"d","payload":"theme.json"}}"#
    )
}

#[test]
fn local_skill_install_enable_disable_roundtrip() {
    let src = scratch("pk-src");
    let data = scratch("pk-data");
    write_pack(
        &src,
        "finance",
        &skill_manifest("finance"),
        "skill.md",
        "MERCH means the merchant column.",
    );

    let engine = EngineState::new(&data).unwrap();

    let list = engine.packs_add(&src.join("finance")).unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].id, "finance");
    assert_eq!(list[0].kind, "skill");
    assert!(!list[0].enabled, "packs install disabled");
    assert!(!list[0].verified, "a local pack is unverified");
    assert_eq!(list[0].source, "local");

    // Disabled -> not in the prompt context.
    assert!(engine.user_context().is_empty());

    engine.packs_set_enabled("finance", true).unwrap();
    let ctx = engine.user_context();
    assert!(ctx.iter().any(|c| c.contains("MERCH means the merchant column")));

    engine.packs_set_enabled("finance", false).unwrap();
    assert!(engine.user_context().is_empty());

    engine.packs_remove("finance").unwrap();
    assert!(engine.packs_list().is_empty());
    assert!(!data.join("extensions/finance").exists(), "pack dir is removed");

    let _ = fs::remove_dir_all(&src);
    let _ = fs::remove_dir_all(&data);
}

#[test]
fn rejects_an_unsafe_or_malformed_pack() {
    let src = scratch("pk-src2");
    let data = scratch("pk-data2");
    let engine = EngineState::new(&data).unwrap();

    // payload escapes the pack directory
    write_pack(
        &src,
        "evil",
        r#"{"schema":1,"id":"evil","kind":"skill","name":"E","version":"1.0.0","description":"d","payload":"../../etc/passwd"}"#,
        "skill.md",
        "x",
    );
    assert!(engine.packs_add(&src.join("evil")).is_err());

    // unknown kind
    write_pack(
        &src,
        "weird",
        r#"{"schema":1,"id":"weird","kind":"plugin","name":"W","version":"1.0.0","description":"d","payload":"p.md"}"#,
        "p.md",
        "x",
    );
    assert!(engine.packs_add(&src.join("weird")).is_err());

    assert!(engine.packs_list().is_empty());

    let _ = fs::remove_dir_all(&src);
    let _ = fs::remove_dir_all(&data);
}

#[test]
fn only_one_theme_is_active_and_its_tokens_are_filtered() {
    let src = scratch("pk-src3");
    let data = scratch("pk-data3");
    let engine = EngineState::new(&data).unwrap();

    write_pack(
        &src,
        "nord",
        &theme_manifest("nord"),
        "theme.json",
        r##"{"appearance":"dark","--bg":"#2e3440","--text":"#eceff4","--bogus":"nope"}"##,
    );
    write_pack(
        &src,
        "solar",
        &theme_manifest("solar"),
        "theme.json",
        r##"{"--bg":"#002b36","--text":"#93a1a1"}"##,
    );

    engine.packs_add(&src.join("nord")).unwrap();
    engine.packs_add(&src.join("solar")).unwrap();
    assert!(engine.packs_theme().is_none(), "nothing active until enabled");

    engine.packs_set_enabled("nord", true).unwrap();
    engine.packs_set_enabled("solar", true).unwrap();

    let enabled: Vec<_> = engine
        .packs_list()
        .into_iter()
        .filter(|p| p.enabled)
        .map(|p| p.id)
        .collect();
    assert_eq!(enabled, vec!["solar"], "enabling one theme disables the other");

    let tokens = engine.packs_theme().unwrap();
    assert_eq!(tokens.get("--bg").map(String::as_str), Some("#002b36"));
    assert!(!tokens.contains_key("--bogus"), "unknown tokens dropped");

    let _ = fs::remove_dir_all(&src);
    let _ = fs::remove_dir_all(&data);
}

#[test]
fn fella_md_is_workspace_context_not_a_source() {
    let ws = scratch("pk-ws");
    let data = scratch("pk-wsdata");
    fs::write(ws.join("sales.csv"), "amount\n10\n20\n").unwrap();
    fs::write(
        ws.join("fella.md"),
        "Amounts are in euros. Exclude the TRANSFER category.",
    )
    .unwrap();

    let engine = EngineState::new(&data).unwrap();
    let catalog = engine.open_workspace(&ws).unwrap();

    assert!(
        catalog.sources.iter().all(|s| s.name != "fella.md"),
        "fella.md must not be catalogued as a document"
    );
    let ctx = engine.user_context();
    assert!(ctx.iter().any(|c| c.contains("Amounts are in euros")));

    let _ = fs::remove_dir_all(&ws);
    let _ = fs::remove_dir_all(&data);
}
