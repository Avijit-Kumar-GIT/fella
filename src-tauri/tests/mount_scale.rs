//! Manual mount-scale probe: `cargo test --test mount_scale -- --ignored
//! --nocapture` (optionally set `FELLA_MOUNT_SCALE_FILES` to change the 5,000
//! default for local iteration). It deliberately has no latency threshold; its purpose is to
//! report real end-to-end mount cost and prove complete source coverage without
//! turning one machine's timing into a correctness expectation.

use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use fella_lib::engine::{tools::Registry, EngineState};
use serde_json::json;

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

fn tree_size(path: &Path) -> u64 {
    let Ok(entries) = fs::read_dir(path) else {
        return 0;
    };
    entries
        .filter_map(Result::ok)
        .map(|entry| {
            let path = entry.path();
            if path.is_dir() {
                tree_size(&path)
            } else {
                entry.metadata().map(|metadata| metadata.len()).unwrap_or(0)
            }
        })
        .sum()
}

#[tokio::test]
#[ignore = "manual 5,000-file performance/coverage probe"]
async fn mounts_five_thousand_nested_sources_without_omissions() {
    const DEFAULT_SOURCE_COUNT: usize = 5_000;
    const ROWS_PER_SOURCE: usize = 2;
    std::env::set_var("FELLA_SKIP_MODEL_WARMUP", "1");
    let source_count = std::env::var("FELLA_MOUNT_SCALE_FILES")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_SOURCE_COUNT);

    let workspace = Scratch::new("mount-scale-workspace");
    let data = Scratch::new("mount-scale-data");

    for index in 0..source_count {
        let group = index / 100;
        let directory = workspace
            .path()
            .join(format!("group-{group:02}"))
            .join(format!("source-{:02}", index % 100));
        fs::create_dir_all(&directory).unwrap();
        let file = File::create(directory.join("records.csv")).unwrap();
        let mut writer = BufWriter::new(file);
        writeln!(writer, "sequence,amount").unwrap();
        for row in 0..ROWS_PER_SOURCE {
            writeln!(writer, "{row},{}", index + row).unwrap();
        }
    }

    // Human-expected office formats outside Fella's tabular allowlist should
    // remain visible as skipped rather than being opened and parsed.
    let unsupported = workspace.path().join("documents");
    fs::create_dir_all(&unsupported).unwrap();
    for index in 0..100 {
        fs::write(unsupported.join(format!("memo-{index:03}.docx")), []).unwrap();
    }

    let started = std::time::Instant::now();
    let engine = EngineState::new(data.path()).unwrap();
    let catalog = engine.open_workspace(workspace.path()).unwrap();
    let elapsed = started.elapsed();
    let tools = Registry::standard();
    let listing_started = std::time::Instant::now();
    let listing = tools
        .run(&engine, "list_files", &json!({}))
        .await
        .expect("list_files is registered")
        .expect("default inventory page succeeds");
    let listing_elapsed = listing_started.elapsed();
    assert!(listing
        .llm_text
        .contains("40 entries shown of 5100 matching"));
    assert!(listing.llm_text.contains("offset=40"));

    let search_started = std::time::Instant::now();
    let last_file = tools
        .run(
            &engine,
            "list_files",
            &json!({ "search": "group-49/source-99/records.csv", "kind": "tables" }),
        )
        .await
        .expect("list_files is registered")
        .expect("searching the last nested file succeeds");
    let search_elapsed = search_started.elapsed();
    assert!(last_file
        .llm_text
        .contains("group-49/source-99/records.csv"));
    assert!(last_file.llm_text.contains("1 entries shown of 1 matching"));
    let loaded_rows: i64 = catalog
        .sources
        .iter()
        .map(|source| source.row_count.unwrap_or_default())
        .sum();
    let scratch_bytes = fs::read_dir(data.path())
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with(".analysis-")
        })
        .map(|entry| tree_size(&entry.path()))
        .sum::<u64>();

    assert_eq!(catalog.sources.len(), source_count);
    assert_eq!(loaded_rows, (source_count * ROWS_PER_SOURCE) as i64);
    assert_eq!(catalog.skipped.len(), 100);
    assert!(catalog
        .sources
        .iter()
        .all(|source| source.row_count == Some(ROWS_PER_SOURCE as i64)));
    assert!(catalog.sources.iter().all(|source| source
        .columns
        .as_ref()
        .unwrap()
        .iter()
        .all(|column| column.distinct.is_none())));

    let first_view = catalog.sources[0].view.as_deref().unwrap();
    let inspected = engine.describe_source(first_view).unwrap();
    assert!(inspected
        .columns
        .unwrap()
        .iter()
        .all(|column| column.distinct.is_some()));

    eprintln!(
        "mount scale sample: files={} rows={} skipped={} mount_ms={} inventory_page_ms={} path_search_ms={} scratch_bytes={}",
        catalog.sources.len(),
        loaded_rows,
        catalog.skipped.len(),
        elapsed.as_millis(),
        listing_elapsed.as_millis(),
        search_elapsed.as_millis(),
        scratch_bytes
    );

    drop(engine);
    assert!(fs::read_dir(data.path())
        .unwrap()
        .filter_map(Result::ok)
        .all(|entry| !entry
            .file_name()
            .to_string_lossy()
            .starts_with(".analysis-")));
}
