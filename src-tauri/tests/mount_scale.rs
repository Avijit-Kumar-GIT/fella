//! Manual mount-scale probe: `cargo test --test mount_scale -- --ignored
//! --nocapture` (optionally set `FELLA_MOUNT_SCALE_FILES` to change the 5,000
//! default for local iteration). It deliberately has no latency threshold; its
//! purpose is to report real end-to-end mount cost and prove complete coverage
//! across nested CSV/TSV/JSON/NDJSON tables, text documents, varied sizes,
//! missing values, and visible unsupported files.

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

#[cfg(target_os = "linux")]
fn peak_rss_bytes() -> Option<u64> {
    std::fs::read_to_string("/proc/self/status")
        .ok()?
        .lines()
        .find_map(|line| line.strip_prefix("VmHWM:"))
        .and_then(|value| value.split_whitespace().next())
        .and_then(|value| value.parse::<u64>().ok())
        .map(|kib| kib.saturating_mul(1024))
}

#[cfg(not(target_os = "linux"))]
fn peak_rss_bytes() -> Option<u64> {
    None
}

fn write_mixed_table(path: &Path, index: usize, rows: usize) {
    let mut writer = BufWriter::new(File::create(path).unwrap());
    match index % 4 {
        0 | 1 => {
            let separator = if index % 4 == 0 { ',' } else { '\t' };
            writeln!(
                writer,
                "record_id{separator}amount{separator}category{separator}day"
            )
            .unwrap();
            for row in 0..rows {
                let amount = if row % 17 == 0 {
                    String::new()
                } else {
                    format!("{}.{:02}", index + row, row % 100)
                };
                let category = ["Food", "food ", "Travel", "misc"][row % 4];
                writeln!(
                    writer,
                    "{index}-{row}{separator}{amount}{separator}{category}{separator}2025-{:02}-01",
                    row % 12 + 1
                )
                .unwrap();
            }
        }
        2 => {
            write!(writer, "[").unwrap();
            for row in 0..rows {
                if row > 0 {
                    write!(writer, ",").unwrap();
                }
                let amount = if row % 17 == 0 {
                    json!(null)
                } else {
                    json!(index + row)
                };
                let category = ["Food", "food ", "Travel", "misc"][row % 4];
                serde_json::to_writer(
                    &mut writer,
                    &json!({
                        "record_id": format!("{index}-{row}"),
                        "amount": amount,
                        "category": category,
                        "day": format!("2025-{:02}-01", row % 12 + 1)
                    }),
                )
                .unwrap();
            }
            writeln!(writer, "]").unwrap();
        }
        _ => {
            for row in 0..rows {
                let amount = if row % 17 == 0 {
                    json!(null)
                } else {
                    json!(index + row)
                };
                let category = ["Food", "food ", "Travel", "misc"][row % 4];
                serde_json::to_writer(
                    &mut writer,
                    &json!({
                        "record_id": format!("{index}-{row}"),
                        "amount": amount,
                        "category": category,
                        "day": format!("2025-{:02}-01", row % 12 + 1)
                    }),
                )
                .unwrap();
                writeln!(writer).unwrap();
            }
        }
    }
    writer.flush().unwrap();
}

#[tokio::test]
#[ignore = "manual 5,000-file performance/coverage probe"]
async fn mounts_five_thousand_nested_sources_without_omissions() {
    const DEFAULT_SOURCE_COUNT: usize = 5_000;
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
        let extension = ["csv", "tsv", "json", "ndjson"][index % 4];
        let rows = if index % 100 == 0 { 1_000 } else { 2 };
        write_mixed_table(&directory.join(format!("records.{extension}")), index, rows);
    }

    for index in 0..100 {
        let side = if index % 2 == 0 { "left" } else { "right" };
        let directory = workspace.path().join("notes").join(side);
        fs::create_dir_all(&directory).unwrap();
        fs::write(
            directory.join(format!("note-{:03}.md", index / 2)),
            format!("# Note {index}\n\nA human-written note with irregular spacing.\n"),
        )
        .unwrap();
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
    let mut inventory_ready_ms = None;
    let catalog = engine
        .open_workspace_with_progress(workspace.path(), |progress| {
            if inventory_ready_ms.is_none()
                && progress.phase == "preparing"
                && progress.prepared_files == 0
            {
                inventory_ready_ms = Some(started.elapsed().as_millis());
            }
        })
        .unwrap();
    let elapsed = started.elapsed();
    let tools = Registry::standard();
    let listing_started = std::time::Instant::now();
    let listing = tools
        .run(&engine, "list_files", &json!({}))
        .await
        .expect("list_files is registered")
        .expect("default inventory page succeeds");
    let listing_elapsed = listing_started.elapsed();
    let expected_inventory = source_count + 200;
    let expected_page_size = expected_inventory.min(40);
    assert!(listing.llm_text.contains(&format!(
        "{expected_page_size} entries shown of {expected_inventory} matching"
    )));
    assert!(listing.llm_text.contains(&format!(
        "{source_count} tables, 100 documents, 100 skipped"
    )));
    if expected_inventory > expected_page_size {
        assert!(listing
            .llm_text
            .contains(&format!("offset={expected_page_size}")));
    }

    let search_started = std::time::Instant::now();
    let last_file = tools
        .run(
            &engine,
            "list_files",
            &json!({
                "search": format!(
                    "group-{:02}/source-{:02}/records.{}",
                    (source_count - 1) / 100,
                    (source_count - 1) % 100,
                    ["csv", "tsv", "json", "ndjson"][(source_count - 1) % 4]
                ),
                "kind": "tables"
            }),
        )
        .await
        .expect("list_files is registered")
        .expect("searching the last nested file succeeds");
    let search_elapsed = search_started.elapsed();
    assert!(last_file.llm_text.contains(&format!(
        "group-{:02}/source-{:02}/records.{}",
        (source_count - 1) / 100,
        (source_count - 1) % 100,
        ["csv", "tsv", "json", "ndjson"][(source_count - 1) % 4]
    )));
    assert!(last_file.llm_text.contains("1 entries shown of 1 matching"));
    let loaded_rows: i64 = catalog
        .sources
        .iter()
        .map(|source| source.row_count.unwrap_or_default())
        .sum();
    let large_source_count = source_count.div_ceil(100);
    let expected_rows =
        (large_source_count * 1_000 + (source_count - large_source_count) * 2) as i64;
    let first_view = catalog
        .sources
        .iter()
        .find_map(|source| source.view.as_deref())
        .expect("at least one table is queryable");
    let sample_started = std::time::Instant::now();
    let first_sample = engine.sample(first_view, 5).unwrap();
    let first_sample_elapsed = sample_started.elapsed();
    assert!(!first_sample.rows.is_empty());
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
    let peak_rss = peak_rss_bytes()
        .map(|bytes| bytes.to_string())
        .unwrap_or_else(|| "unavailable".into());

    assert_eq!(catalog.sources.len(), source_count + 100);
    assert_eq!(loaded_rows, expected_rows);
    assert_eq!(
        catalog
            .sources
            .iter()
            .filter(|source| source.view.is_some())
            .count(),
        source_count
    );
    assert!(catalog
        .sources
        .iter()
        .any(|source| source.path.ends_with("notes/left/note-000.md")));
    assert_eq!(catalog.skipped.len(), 100);
    assert!(catalog
        .sources
        .iter()
        .filter(|source| source.view.is_some())
        .all(|source| source.row_count.is_some()));
    assert!(catalog
        .sources
        .iter()
        .filter(|source| source.view.is_some())
        .all(|source| source
            .columns
            .as_ref()
            .unwrap()
            .iter()
            .all(|column| column.distinct.is_none())));

    let inspected = engine.describe_source(first_view).unwrap();
    assert!(inspected
        .columns
        .unwrap()
        .iter()
        .all(|column| column.distinct.is_some()));

    eprintln!(
        "mount scale sample: tables={} documents={} rows={} skipped={} inventory_ready_ms={} mount_ready_ms={} first_sample_ms={} inventory_page_ms={} path_search_ms={} scratch_bytes={} process_peak_rss_bytes={}",
        source_count,
        catalog.sources.len() - source_count,
        loaded_rows,
        catalog.skipped.len(),
        inventory_ready_ms.unwrap_or_default(),
        elapsed.as_millis(),
        first_sample_elapsed.as_millis(),
        listing_elapsed.as_millis(),
        search_elapsed.as_millis(),
        scratch_bytes
        ,peak_rss
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
