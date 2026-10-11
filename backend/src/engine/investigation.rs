//! Durable product record for one bounded unit of analyst work.
//!
//! The Investigation deliberately does not embed conversation messages or
//! analysis-turn payloads. Those records remain owned by their existing
//! stores and are linked in later product-model slices.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::engine::error::{EngineError, EngineResult};

const INVESTIGATIONS_DIR: &str = "investigations";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvestigationStatus {
    Active,
    Complete,
    Archived,
}

/// Stable, independently loadable identity for an investigation.
///
/// `workspace_id` is the canonical local folder identity, matching the
/// desktop's existing workspace contract. Conversation and run associations
/// are intentionally not copied into this initial record; subsequent product
/// slices add links without changing ownership of those records.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Investigation {
    pub id: String,
    pub workspace_id: String,
    pub title: String,
    pub status: InvestigationStatus,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
}

fn investigations_dir(data_dir: &Path) -> PathBuf {
    data_dir.join(INVESTIGATIONS_DIR)
}

fn record_path(data_dir: &Path, id: &str) -> EngineResult<PathBuf> {
    if id.is_empty()
        || !id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
    {
        return Err(EngineError::msg("invalid investigation id"));
    }
    Ok(investigations_dir(data_dir).join(format!("{id}.json")))
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

fn read_record(path: &Path, expected_id: &str) -> EngineResult<Investigation> {
    let body = std::fs::read_to_string(path)
        .map_err(|error| EngineError::io(format!("read {}", path.display()), error))?;
    let record: Investigation = serde_json::from_str(&body).map_err(|error| {
        EngineError::msg(format!(
            "investigation {expected_id} is not valid JSON: {error}"
        ))
    })?;
    if record.id != expected_id {
        return Err(EngineError::msg(format!(
            "investigation file {} contains a mismatched id",
            path.display()
        )));
    }
    Ok(record)
}

pub fn create(data_dir: &Path, workspace_id: &str, title: &str) -> EngineResult<Investigation> {
    let workspace_id = workspace_id.trim();
    if workspace_id.is_empty() {
        return Err(EngineError::msg(
            "an investigation requires an explicit workspace id",
        ));
    }

    let title = title.trim();
    let title = if title.is_empty() {
        "New investigation"
    } else {
        title
    };

    let directory = investigations_dir(data_dir);
    std::fs::create_dir_all(&directory)
        .map_err(|error| EngineError::io(format!("create {}", directory.display()), error))?;

    // Random IDs avoid coupling product identity to editable titles, paths,
    // ordering, or wall-clock precision. Retry only on the vanishingly rare
    // collision; never overwrite an existing record.
    for _ in 0..8 {
        let mut random = [0u8; 16];
        getrandom::fill(&mut random)
            .map_err(|error| EngineError::msg(format!("create investigation id: {error}")))?;
        let id = format!(
            "inv_{}",
            random
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        );
        let path = record_path(data_dir, &id)?;
        if path.exists() {
            continue;
        }

        let timestamp = now_ms();
        let record = Investigation {
            id: id.clone(),
            workspace_id: workspace_id.to_owned(),
            title: title.to_owned(),
            status: InvestigationStatus::Active,
            created_at_ms: timestamp,
            updated_at_ms: timestamp,
        };
        let body = serde_json::to_string_pretty(&record).map_err(EngineError::from)?;
        let temporary = path.with_extension("json.tmp");
        let mut file = match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
        {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(EngineError::io(
                    format!("create {}", temporary.display()),
                    error,
                ));
            }
        };
        if let Err(error) = file.write_all(body.as_bytes()) {
            let _ = std::fs::remove_file(&temporary);
            return Err(EngineError::io(
                format!("write {}", temporary.display()),
                error,
            ));
        }
        drop(file);
        match std::fs::hard_link(&temporary, &path) {
            Ok(()) => {
                let _ = std::fs::remove_file(&temporary);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                let _ = std::fs::remove_file(&temporary);
                continue;
            }
            Err(error) => {
                let _ = std::fs::remove_file(&temporary);
                return Err(EngineError::io(format!("write {}", path.display()), error));
            }
        }
        return Ok(record);
    }

    Err(EngineError::msg(
        "could not allocate a unique investigation id",
    ))
}

pub fn load(data_dir: &Path, id: &str) -> EngineResult<Investigation> {
    let path = record_path(data_dir, id)?;
    read_record(&path, id)
}

pub fn list(data_dir: &Path) -> EngineResult<Vec<Investigation>> {
    let directory = investigations_dir(data_dir);
    let entries = match std::fs::read_dir(&directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(EngineError::io(
                format!("read {}", directory.display()),
                error,
            ));
        }
    };

    let mut records = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| {
            EngineError::io(format!("read entries in {}", directory.display()), error)
        })?;
        let path = entry.path();
        if !entry
            .file_type()
            .map_err(|error| EngineError::io(format!("inspect {}", path.display()), error))?
            .is_file()
            || path.extension().and_then(|extension| extension.to_str()) != Some("json")
        {
            continue;
        }
        let id = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .ok_or_else(|| EngineError::msg("investigation filename is not valid UTF-8"))?;
        record_path(data_dir, id)?;
        records.push(read_record(&path, id)?);
    }
    records.sort_by(|left, right| {
        right
            .updated_at_ms
            .cmp(&left.updated_at_ms)
            .then_with(|| left.id.cmp(&right.id))
    });
    Ok(records)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

    fn scratch() -> PathBuf {
        let nonce = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "fella-investigation-store-{}-{nonce}",
            std::process::id()
        ))
    }

    #[test]
    fn creates_loads_and_lists_independent_records() {
        let data_dir = scratch();
        let created = create(&data_dir, "/work/analysis", "  Monthly revenue  ").unwrap();

        assert!(created.id.starts_with("inv_"));
        assert_eq!(created.workspace_id, "/work/analysis");
        assert_eq!(created.title, "Monthly revenue");
        assert_eq!(created.status, InvestigationStatus::Active);
        assert_eq!(created.created_at_ms, created.updated_at_ms);
        assert_eq!(load(&data_dir, &created.id).unwrap(), created);
        assert_eq!(list(&data_dir).unwrap(), vec![created.clone()]);

        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[test]
    fn defaults_blank_title_and_rejects_missing_scope_or_unsafe_ids() {
        let data_dir = scratch();
        let created = create(&data_dir, "/work/analysis", "  ").unwrap();
        assert_eq!(created.title, "New investigation");
        assert!(create(&data_dir, "  ", "No scope").is_err());
        assert!(load(&data_dir, "../conversation").is_err());

        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[test]
    fn missing_store_lists_as_empty() {
        let data_dir = scratch();
        assert!(list(&data_dir).unwrap().is_empty());
    }
}
