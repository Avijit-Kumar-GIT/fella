//! Workspace inventory pagination and stable relative document paths.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use fella_lib::engine::{tools::Registry, EngineState};
use serde_json::{json, Value};

struct Scratch(PathBuf);

impl Scratch {
    fn new(tag: &str) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("fella-{tag}-{nonce}"));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

async fn tool(engine: &EngineState, name: &str, args: Value) -> String {
    Registry::standard()
        .run(engine, name, &args)
        .await
        .expect("expected built-in tool")
        .expect("tool should succeed")
        .llm_text
}

fn inventory_entries(output: &str) -> Vec<&str> {
    output
        .lines()
        .filter(|line| {
            line.starts_with("table ")
                || line.starts_with("document ")
                || line.starts_with("skipped ")
        })
        .collect()
}

#[tokio::test]
async fn large_inventory_is_paged_searchable_and_includes_skipped_files() {
    // The expected inventory is implementation-independent: 65 supported
    // tables, three readable documents, and one unsupported document.
    const TABLES: usize = 65;
    let workspace = Scratch::new("inventory-workspace");
    let data = Scratch::new("inventory-data");
    for index in 0..TABLES {
        let directory = workspace
            .path()
            .join("tables")
            .join(format!("partition-{index:02}"));
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join("records.csv"), format!("amount\n{index}\n")).unwrap();
    }
    fs::create_dir_all(workspace.path().join("docs/left")).unwrap();
    fs::create_dir_all(workspace.path().join("docs/right")).unwrap();
    fs::create_dir_all(workspace.path().join("docs/misc")).unwrap();
    fs::write(workspace.path().join("docs/left/notes.txt"), "left note\n").unwrap();
    fs::write(
        workspace.path().join("docs/right/notes.txt"),
        "right note\n",
    )
    .unwrap();
    fs::write(
        workspace.path().join("docs/misc/readme.txt"),
        "workspace notes\n",
    )
    .unwrap();
    fs::create_dir_all(workspace.path().join("archive")).unwrap();
    fs::write(workspace.path().join("archive/unsupported.docx"), []).unwrap();

    let engine = EngineState::new(data.path()).unwrap();
    let mut progress = Vec::new();
    engine
        .open_workspace_with_progress(workspace.path(), |update| progress.push(update))
        .unwrap();
    assert_eq!(
        progress.first().map(|update| update.phase),
        Some("scanning")
    );
    let ready = progress.last().expect("mount reports completion");
    assert_eq!(ready.phase, "ready");
    assert_eq!(ready.visited_files, TABLES + 4);
    assert_eq!(ready.supported_files, TABLES + 3);
    assert_eq!(ready.prepared_files, TABLES + 3);
    assert_eq!(ready.total_supported_files, Some(TABLES + 3));
    assert_eq!(ready.skipped_files, 1);

    let first = tool(&engine, "list_files", json!({})).await;
    assert!(first.contains("40 entries shown of 69 matching"), "{first}");
    assert!(
        first.contains("65 tables, 3 documents, 1 skipped"),
        "{first}"
    );
    assert!(first.contains("offset=40"), "{first}");
    assert_eq!(inventory_entries(&first).len(), 40);

    let second = tool(&engine, "list_files", json!({ "offset": 40 })).await;
    assert!(
        second.contains("29 entries shown of 69 matching"),
        "{second}"
    );
    assert_eq!(inventory_entries(&second).len(), 29);
    let first_entries = inventory_entries(&first);
    let second_entries = inventory_entries(&second);
    assert!(first_entries
        .iter()
        .all(|entry| !second_entries.contains(entry)));

    let searched = tool(
        &engine,
        "list_files",
        json!({ "search": "partition-64/records.csv", "kind": "tables" }),
    )
    .await;
    assert!(
        searched.contains("tables/partition-64/records.csv"),
        "{searched}"
    );
    assert_eq!(inventory_entries(&searched).len(), 1);

    let capped = tool(&engine, "list_files", json!({ "limit": 5_000 })).await;
    assert_eq!(inventory_entries(&capped).len(), 50);
    assert!(capped.contains("offset=50"), "{capped}");

    let skipped = tool(
        &engine,
        "list_files",
        json!({ "kind": "skipped", "search": "unsupported.docx" }),
    )
    .await;
    assert!(skipped.contains("archive/unsupported.docx"), "{skipped}");
    assert_eq!(inventory_entries(&skipped).len(), 1);

    let invalid_kind = match Registry::standard()
        .run(&engine, "list_files", &json!({ "kind": "everything" }))
        .await
    {
        Some(Err(error)) => error,
        _ => panic!("invalid inventory kind must return an error"),
    };
    assert!(invalid_kind.to_string().contains("unknown inventory kind"));
}

#[tokio::test]
async fn nested_duplicate_document_names_use_relative_paths_for_reading() {
    let workspace = Scratch::new("relative-docs-workspace");
    let data = Scratch::new("relative-docs-data");
    fs::create_dir_all(workspace.path().join("left")).unwrap();
    fs::create_dir_all(workspace.path().join("right")).unwrap();
    fs::write(
        workspace.path().join("left/notes.txt"),
        "left-only phrase\n",
    )
    .unwrap();
    fs::write(
        workspace.path().join("right/notes.txt"),
        "right-only phrase\n",
    )
    .unwrap();

    let engine = EngineState::new(data.path()).unwrap();
    engine.open_workspace(workspace.path()).unwrap();

    let listing = tool(
        &engine,
        "list_files",
        json!({ "kind": "documents", "search": "right/notes.txt" }),
    )
    .await;
    assert!(listing.contains("right/notes.txt"), "{listing}");
    assert!(!listing.contains("left/notes.txt"), "{listing}");

    let content = tool(&engine, "read_file", json!({ "name": "right/notes.txt" })).await;
    assert!(content.contains("right-only phrase"), "{content}");
    assert!(!content.contains("left-only phrase"), "{content}");

    let matches = tool(
        &engine,
        "grep_files",
        json!({ "pattern": "right-only phrase" }),
    )
    .await;
    assert!(matches.contains("right/notes.txt"), "{matches}");
}
