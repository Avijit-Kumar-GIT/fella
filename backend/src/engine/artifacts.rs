//! Source-scoped analytical outputs and app-workspace-owned Reports.
//!
//! These records reference immutable canonical runs; they never persist values
//! supplied by a renderer as analytical evidence. SQLite makes metadata updates,
//! optimistic version checks, and Studio retention markers one transaction.
//! Every public store operation takes the analysis lifecycle lock BEFORE opening
//! this database. Never call a public function from inside another locked call.

use std::collections::HashSet;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value as Json;

use super::analysis_store;
use super::error::{EngineError, EngineResult};
use super::investigation::InvestigationSourceSelection;
use super::runtime::AnalysisTurn;

/// Schema attached to new JSON-render artifacts. Legacy expression-based
/// artifact rows are removed by the one-time store migration below.
pub const SCHEMA_VERSION: &str = "fella-json-render-v1";
/// Reports belong to the Fella window's content workspace, not a mounted data
/// folder. Artifact records retain their own mounted source workspace.
pub const APP_WORKSPACE_ID: &str = "fella-app-workspace";
const STUDIO_OWNER_MIGRATION: &str = "reports-app-workspace-v1";
const NORTHSTAR_SAMPLE_REPORT_ID: &str = "studio_northstar_sample_revenue_v1";
const NORTHSTAR_SAMPLE_ARTIFACT_PREFIX: &str = "artifact_northstar_sample_revenue_";
const NORTHSTAR_SAMPLE_REPORT_SEED_VERSION: &str = "northstar-saved-analysis-v2";
const NORTHSTAR_SAMPLE_REPORT_LEGACY_BODY: &str = "**Status: In progress**\n\n## Objective\nExplain how Northstar Outfitters’ net merchandise revenue changed between Q3 and Q4 2025.\n\n## Work plan\n- Confirm the current finance metric policy, source coverage, and date basis.\n- Compare revenue, order volume, and average order value across both quarters.\n- Identify the product, refund, or source-quality factors that explain the change.\n\n## Evidence to add\nNo findings are attached yet. Add verified analysis from Chat as the investigation progresses. Check `docs/metric_policy_current.md` before accepting a revenue definition; archived exports may use older rules.";
const MAX_RECORD_BYTES: usize = 4 * 1024 * 1024;
const MAX_RUN_BYTES: u64 = 64 * 1024 * 1024;
const MAX_SPEC_BYTES: usize = 256 * 1024;
const MAX_BODY_BYTES: usize = 512 * 1024;
const MAX_REVISIONS: usize = 256;
const MAX_PLACEMENTS: usize = 256;
const MAX_CHECKPOINTS: usize = 64;
const MAX_STUDIO_HISTORY: usize = 100;
const MAX_STUDIO_HISTORY_BYTES: i64 = 8 * 1024 * 1024;
const MAX_RESULTS: usize = 256;
const MAX_RESULT_ROWS: usize = 100_000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BoundResult {
    pub id: String,
    pub columns: Vec<String>,
    /// Convenience display values. Consumers requiring precision use exact_rows.
    pub rows: Vec<Vec<Json>>,
    /// Canonical saved scalar representations; null is "null", not zero.
    pub exact_rows: Vec<Vec<String>>,
    pub row_count: usize,
    pub complete: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReportFilterValue {
    pub source_artifact_id: String,
    pub source_revision: u64,
    pub result_id: String,
    pub field: String,
    pub interaction_group: String,
    pub value: String,
    pub value_type: String,
}

#[derive(Debug, Clone)]
pub struct ReportFilterBinding {
    pub workspace: String,
    pub workspace_revision: String,
    pub sql: String,
    pub columns: Vec<String>,
    pub source_selection: InvestigationSourceSelection,
    pub source_checks: Vec<ReportFilterSourceCheck>,
}

#[derive(Debug, Clone)]
pub struct ReportFilterSourceCheck {
    pub sql: String,
    pub expected_value: String,
    pub value_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArtifactRevision {
    pub revision: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_revision: Option<u64>,
    pub run_id: String,
    pub spec: Json,
    pub schema_version: String,
    pub change_note: String,
    pub created_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArtifactRecord {
    pub id: String,
    pub workspace: String,
    pub title: String,
    pub current_revision: u64,
    pub saved: bool,
    pub revisions: Vec<ArtifactRevision>,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedArtifact {
    pub artifact: ArtifactRecord,
    pub revision: ArtifactRevision,
    pub run: AnalysisTurn,
    pub results: Vec<BoundResult>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StudioPlacementWidth {
    #[default]
    Full,
    Half,
}

fn is_full_width(width: &StudioPlacementWidth) -> bool {
    *width == StudioPlacementWidth::Full
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StudioPlacement {
    pub id: String,
    pub artifact_id: String,
    pub revision: u64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub label: String,
    #[serde(default, skip_serializing_if = "is_full_width")]
    pub width: StudioPlacementWidth,
    pub view_state: Json,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StudioCheckpoint {
    pub id: String,
    pub title: String,
    pub body: String,
    pub placements: Vec<StudioPlacement>,
    pub created_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StudioRevisionKind {
    Created,
    Presentation,
    Analytical,
    Checkpoint,
    Restored,
    Imported,
}

/// Compact Report snapshot. Placements retain artifact/revision references;
/// result data and source files are never copied into Report history.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StudioRevision {
    pub document_id: String,
    pub workspace: String,
    pub version: u64,
    pub title: String,
    pub body: String,
    pub placements: Vec<StudioPlacement>,
    pub kind: StudioRevisionKind,
    pub summary: String,
    pub created_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StudioDocument {
    pub id: String,
    pub workspace: String,
    pub title: String,
    pub version: u64,
    pub body: String,
    pub placements: Vec<StudioPlacement>,
    pub checkpoints: Vec<StudioCheckpoint>,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StudioUpdate {
    pub title: String,
    pub body: String,
    pub placements: Vec<StudioPlacement>,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}

fn new_id(prefix: &str) -> EngineResult<String> {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes)
        .map_err(|error| EngineError::msg(format!("generate {prefix} identity: {error}")))?;
    Ok(format!(
        "{prefix}_{}",
        bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()
    ))
}

fn validate_id(id: &str) -> EngineResult<()> {
    if id.is_empty()
        || id.len() > 128
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(EngineError::msg("invalid analytical record id"));
    }
    Ok(())
}

fn validate_workspace(workspace: &str) -> EngineResult<()> {
    if workspace.is_empty()
        || workspace.trim() != workspace
        || workspace == "general"
        || workspace.len() > 8192
        || workspace.chars().any(char::is_control)
    {
        return Err(EngineError::msg(
            "an analytical record requires an explicit workspace",
        ));
    }
    Ok(())
}

fn validate_title(title: &str) -> EngineResult<String> {
    let title = title.trim();
    if title.is_empty() || title.chars().count() > 120 || title.chars().any(char::is_control) {
        return Err(EngineError::msg(
            "title must contain 1–120 printable characters",
        ));
    }
    Ok(title.to_owned())
}

fn schema_type_matches(value: &Json, kind: &str) -> bool {
    match kind {
        "object" => value.is_object(),
        "array" => value.is_array(),
        "string" => value.is_string(),
        "integer" => {
            value.as_i64().is_some()
                || value.as_u64().is_some()
                || value.as_f64().is_some_and(|n| n.fract() == 0.0)
        }
        "number" => value.is_number(),
        "boolean" => value.is_boolean(),
        "null" => value.is_null(),
        _ => true,
    }
}

/// Validate the catalog's generated JSON Schema subset. The schemas come from
/// the same Zod component definitions as the Svelte renderer and use only
/// standard structural keywords; keep Rust from persisting a tree the UI will
/// reject on reopen.
fn validate_component_schema(value: &Json, schema: &Json, path: &str) -> EngineResult<()> {
    if let Some(expected) = schema.get("type") {
        let matches = if let Some(kind) = expected.as_str() {
            schema_type_matches(value, kind)
        } else {
            expected.as_array().is_some_and(|kinds| {
                kinds
                    .iter()
                    .filter_map(Json::as_str)
                    .any(|kind| schema_type_matches(value, kind))
            })
        };
        if !matches {
            return Err(EngineError::msg(format!("{path} has the wrong JSON type")));
        }
    }
    if let Some(values) = schema.get("enum").and_then(Json::as_array) {
        if !values.contains(value) {
            return Err(EngineError::msg(format!("{path} has an unsupported value")));
        }
    }
    if schema
        .get("const")
        .is_some_and(|expected| expected != value)
    {
        return Err(EngineError::msg(format!("{path} has an unsupported value")));
    }
    if let Some(choices) = schema.get("oneOf").and_then(Json::as_array) {
        let matches = choices
            .iter()
            .filter(|choice| validate_component_schema(value, choice, path).is_ok())
            .count();
        if matches != 1 {
            return Err(EngineError::msg(format!(
                "{path} must match exactly one supported form"
            )));
        }
    }
    if let Some(choices) = schema.get("anyOf").and_then(Json::as_array) {
        if !choices
            .iter()
            .any(|choice| validate_component_schema(value, choice, path).is_ok())
        {
            return Err(EngineError::msg(format!(
                "{path} does not match a supported form"
            )));
        }
    }
    if let Some(string) = value.as_str() {
        let length = string.chars().count();
        if schema
            .get("minLength")
            .and_then(Json::as_u64)
            .is_some_and(|min| length < min as usize)
            || schema
                .get("maxLength")
                .and_then(Json::as_u64)
                .is_some_and(|max| length > max as usize)
        {
            return Err(EngineError::msg(format!(
                "{path} exceeds its text length limits"
            )));
        }
    }
    if let Some(array) = value.as_array() {
        if schema
            .get("minItems")
            .and_then(Json::as_u64)
            .is_some_and(|min| array.len() < min as usize)
            || schema
                .get("maxItems")
                .and_then(Json::as_u64)
                .is_some_and(|max| array.len() > max as usize)
        {
            return Err(EngineError::msg(format!(
                "{path} exceeds its item count limits"
            )));
        }
        if let Some(items) = schema.get("items") {
            for (index, item) in array.iter().enumerate() {
                validate_component_schema(item, items, &format!("{path}[{index}]"))?;
            }
        }
    }
    if let Some(number) = value.as_f64() {
        if schema
            .get("minimum")
            .and_then(Json::as_f64)
            .is_some_and(|min| number < min)
            || schema
                .get("maximum")
                .and_then(Json::as_f64)
                .is_some_and(|max| number > max)
            || schema
                .get("exclusiveMinimum")
                .and_then(Json::as_f64)
                .is_some_and(|min| number <= min)
        {
            return Err(EngineError::msg(format!(
                "{path} is outside its numeric limits"
            )));
        }
    }
    if let Some(object) = value.as_object() {
        if let Some(required) = schema.get("required").and_then(Json::as_array) {
            if required
                .iter()
                .filter_map(Json::as_str)
                .any(|key| !object.contains_key(key))
            {
                return Err(EngineError::msg(format!(
                    "{path} is missing a required property"
                )));
            }
        }
        let properties = schema.get("properties").and_then(Json::as_object);
        if let Some(name_schema) = schema.get("propertyNames") {
            for key in object.keys() {
                validate_component_schema(
                    &Json::String(key.clone()),
                    name_schema,
                    &format!("{path} property name"),
                )?;
            }
        }
        for (key, child) in object {
            if let Some(child_schema) = properties.and_then(|properties| properties.get(key)) {
                validate_component_schema(child, child_schema, &format!("{path}.{key}"))?;
                continue;
            }
            match schema.get("additionalProperties") {
                Some(Json::Bool(false)) => {
                    return Err(EngineError::msg(format!(
                        "{path}.{key} is not a supported property"
                    )))
                }
                Some(additional) if additional.is_object() => {
                    validate_component_schema(child, additional, &format!("{path}.{key}"))?
                }
                _ => {}
            }
        }
    }
    Ok(())
}

pub fn validate_spec(spec: &Json) -> EngineResult<()> {
    let object = spec
        .as_object()
        .ok_or_else(|| EngineError::msg("presentation spec must be a JSON object"))?;
    if object.len() != 2 || !object.contains_key("root") || !object.contains_key("elements") {
        return Err(EngineError::msg(
            "presentation spec must contain only root and elements",
        ));
    }
    if serde_json::to_vec(spec).map_or(true, |bytes| bytes.len() > MAX_SPEC_BYTES) {
        return Err(EngineError::msg(
            "presentation spec exceeds its storage limit",
        ));
    }
    let root = spec["root"]
        .as_str()
        .filter(|root| !root.is_empty())
        .ok_or_else(|| EngineError::msg("presentation root is missing"))?;
    let elements = spec["elements"]
        .as_object()
        .filter(|elements| !elements.is_empty() && elements.len() <= 240)
        .ok_or_else(|| {
            EngineError::msg("presentation elements are missing or exceed their limit")
        })?;
    if elements
        .get(root)
        .and_then(|element| element.get("type"))
        .and_then(Json::as_str)
        != Some("Document")
    {
        return Err(EngineError::msg("presentation root must be Document"));
    }
    let catalog: Json =
        serde_json::from_str(include_str!("../../resources/presentation-catalog.json"))?;
    let components = catalog["componentNames"]
        .as_array()
        .ok_or_else(|| EngineError::msg("presentation component catalog is unavailable"))?;
    let mut parents = HashSet::new();
    for (key, element) in elements {
        if key.len() > 80
            || !key.chars().enumerate().all(|(index, ch)| {
                ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' && index > 0
            })
            || !key
                .chars()
                .next()
                .is_some_and(|ch| ch.is_ascii_alphabetic())
        {
            return Err(EngineError::msg(
                "presentation contains an invalid element key",
            ));
        }
        let kind = element
            .get("type")
            .and_then(Json::as_str)
            .ok_or_else(|| EngineError::msg("presentation element is missing a type"))?;
        if !components.iter().any(|name| name.as_str() == Some(kind))
            || !element.get("props").is_some_and(Json::is_object)
        {
            return Err(EngineError::msg(
                "presentation uses an unknown component or invalid props",
            ));
        }
        let prop_schema = catalog["componentSchemas"][kind]
            .as_object()
            .map(|_| &catalog["componentSchemas"][kind])
            .ok_or_else(|| EngineError::msg("presentation component schemas are unavailable"))?;
        validate_component_schema(
            &element["props"],
            prop_schema,
            &format!("elements.{key}.props"),
        )?;
        let children = element
            .get("children")
            .and_then(Json::as_array)
            .ok_or_else(|| EngineError::msg("presentation children must be an array"))?;
        for child in children {
            let child = child
                .as_str()
                .ok_or_else(|| EngineError::msg("presentation child references must be strings"))?;
            if !elements.contains_key(child) || !parents.insert(child.to_owned()) {
                return Err(EngineError::msg(
                    "presentation contains a missing or multiply attached child",
                ));
            }
        }
        if element
            .get("repeat")
            .is_some_and(|repeat| !repeat.is_null())
        {
            return Err(EngineError::msg("presentation repetition is not enabled"));
        }
    }
    if parents.contains(root) {
        return Err(EngineError::msg("presentation root cannot have a parent"));
    }
    let mut visited = HashSet::new();
    let mut pending = vec![root.to_owned()];
    while let Some(key) = pending.pop() {
        if !visited.insert(key.clone()) {
            return Err(EngineError::msg("presentation contains a cycle"));
        }
        pending.extend(
            elements[&key]["children"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Json::as_str)
                .map(str::to_owned),
        );
    }
    if visited.len() != elements.len() {
        return Err(EngineError::msg(
            "every presentation element must be reachable from the root",
        ));
    }
    Ok(())
}

fn store_unlocked(data_dir: &Path) -> EngineResult<Connection> {
    let database = data_dir.join("analysis").join("artifacts.sqlite");
    let db = Connection::open(database)?;
    db.busy_timeout(std::time::Duration::from_secs(5))?;
    db.execute_batch(
        "PRAGMA foreign_keys = ON;
         CREATE TABLE IF NOT EXISTS artifacts (
           id TEXT PRIMARY KEY, workspace TEXT NOT NULL, body TEXT NOT NULL);
         CREATE INDEX IF NOT EXISTS artifacts_workspace ON artifacts(workspace);
         CREATE TABLE IF NOT EXISTS studio_documents (
           id TEXT PRIMARY KEY, workspace TEXT NOT NULL, body TEXT NOT NULL);
         CREATE INDEX IF NOT EXISTS studio_workspace ON studio_documents(workspace);
         CREATE TABLE IF NOT EXISTS studio_revisions (
           document_id TEXT NOT NULL, version INTEGER NOT NULL, body TEXT NOT NULL,
           PRIMARY KEY(document_id, version));
         CREATE TABLE IF NOT EXISTS artifact_metadata (key TEXT PRIMARY KEY, value TEXT NOT NULL);",
    )?;
    let version = db
        .query_row(
            "SELECT value FROM artifact_metadata WHERE key='schema_version'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    if version.as_deref() != Some(SCHEMA_VERSION) {
        let mut statement = db.prepare("SELECT id FROM artifacts")?;
        let old_ids = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<HashSet<_>, _>>()?;
        drop(statement);
        db.execute("DELETE FROM artifacts", [])?;
        if !old_ids.is_empty() {
            let mut query = db.prepare("SELECT id, body FROM studio_documents")?;
            let documents = query
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            drop(query);
            for (id, body) in documents {
                if let Ok(mut document) = serde_json::from_str::<StudioDocument>(&body) {
                    document
                        .placements
                        .retain(|placement| !old_ids.contains(&placement.artifact_id));
                    for checkpoint in &mut document.checkpoints {
                        checkpoint
                            .placements
                            .retain(|placement| !old_ids.contains(&placement.artifact_id));
                    }
                    db.execute(
                        "UPDATE studio_documents SET body=?2 WHERE id=?1",
                        params![id, encode(&document)?],
                    )?;
                }
            }
        }
        db.execute("INSERT INTO artifact_metadata(key,value) VALUES('schema_version',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value", [SCHEMA_VERSION])?;
    }
    let owner_migration = db
        .query_row(
            "SELECT value FROM artifact_metadata WHERE key='studio_owner_migration'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    if owner_migration.as_deref() != Some(STUDIO_OWNER_MIGRATION) {
        let mut db = db;
        let tx = db.transaction()?;
        let mut query = tx.prepare("SELECT id, body FROM studio_documents")?;
        let documents = query
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        drop(query);
        for (id, body) in documents {
            let mut document: StudioDocument = decode(&body)?;
            document.workspace = APP_WORKSPACE_ID.to_owned();
            tx.execute(
                "UPDATE studio_documents SET workspace=?2, body=?3 WHERE id=?1",
                params![id, APP_WORKSPACE_ID, encode(&document)?],
            )?;
        }
        tx.execute(
            "INSERT INTO artifact_metadata(key,value) VALUES('studio_owner_migration',?1)
             ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            [STUDIO_OWNER_MIGRATION],
        )?;
        tx.commit()?;
        return Ok(db);
    }
    Ok(db)
}

fn encode<T: Serialize>(value: &T) -> EngineResult<String> {
    let body = serde_json::to_string(value)?;
    if body.len() > MAX_RECORD_BYTES {
        return Err(EngineError::msg(
            "analytical record exceeds its storage limit",
        ));
    }
    Ok(body)
}

fn decode<T: serde::de::DeserializeOwned>(body: &str) -> EngineResult<T> {
    if body.len() > MAX_RECORD_BYTES {
        return Err(EngineError::msg(
            "stored analytical record exceeds its size limit",
        ));
    }
    serde_json::from_str(body).map_err(EngineError::from)
}

fn load_artifact_unlocked(
    db: &Connection,
    workspace: &str,
    id: &str,
) -> EngineResult<ArtifactRecord> {
    validate_id(id)?;
    let body: String = db
        .query_row(
            "SELECT body FROM artifacts WHERE id = ?1 AND workspace = ?2",
            params![id, workspace],
            |r| r.get(0),
        )
        .optional()?
        .ok_or_else(|| EngineError::msg("artifact is not available in this workspace"))?;
    let record: ArtifactRecord = decode(&body)?;
    validate_artifact(&record)?;
    if record.id != id || record.workspace != workspace {
        return Err(EngineError::msg(
            "artifact identity does not match its stored owner",
        ));
    }
    Ok(record)
}

/// Resolve an artifact by its opaque, globally unique ID. The artifact record
/// supplies the mounted source scope; callers cannot substitute another path.
fn load_artifact_by_id_unlocked(db: &Connection, id: &str) -> EngineResult<ArtifactRecord> {
    validate_id(id)?;
    let (workspace, body): (String, String) = db
        .query_row(
            "SELECT workspace, body FROM artifacts WHERE id = ?1",
            [id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?
        .ok_or_else(|| EngineError::msg("saved analytical output is unavailable"))?;
    let record: ArtifactRecord = decode(&body)?;
    validate_artifact(&record)?;
    if record.id != id || record.workspace != workspace {
        return Err(EngineError::msg(
            "artifact identity does not match its stored source scope",
        ));
    }
    Ok(record)
}

fn validate_artifact(record: &ArtifactRecord) -> EngineResult<()> {
    validate_id(&record.id)?;
    validate_workspace(&record.workspace)?;
    validate_title(&record.title)?;
    if record.revisions.is_empty() || record.revisions.len() > MAX_REVISIONS {
        return Err(EngineError::msg("invalid artifact revision history"));
    }
    let mut seen = HashSet::new();
    let mut previous = 0;
    for revision in &record.revisions {
        validate_id(&revision.run_id)?;
        validate_spec(&revision.spec)?;
        if revision.revision <= previous
            || !seen.insert(revision.revision)
            || revision.schema_version != SCHEMA_VERSION
            || revision.change_note.len() > 4096
            || (previous == 0 && (revision.revision != 1 || revision.parent_revision.is_some()))
            || (previous > 0
                && !revision
                    .parent_revision
                    .is_some_and(|p| seen.contains(&p) && p < revision.revision))
        {
            return Err(EngineError::msg("invalid artifact revision lineage"));
        }
        previous = revision.revision;
    }
    if !seen.contains(&record.current_revision) {
        return Err(EngineError::msg("artifact has no current revision"));
    }
    Ok(())
}

fn put_artifact_unlocked(db: &Connection, record: &ArtifactRecord) -> EngineResult<()> {
    validate_artifact(record)?;
    db.execute(
        "INSERT INTO artifacts(id,workspace,body) VALUES(?1,?2,?3)
         ON CONFLICT(id) DO UPDATE SET body=excluded.body WHERE workspace=excluded.workspace",
        params![record.id, record.workspace, encode(record)?],
    )?;
    Ok(())
}

fn load_run(data_dir: &Path, workspace: &str, id: &str) -> EngineResult<AnalysisTurn> {
    validate_id(id)?;
    let path = data_dir
        .join("analysis")
        .join("turns")
        .join(format!("{id}.json"));
    let metadata = std::fs::symlink_metadata(&path)
        .map_err(|e| EngineError::io("inspect canonical analysis run", e))?;
    if !metadata.is_file() || metadata.len() > MAX_RUN_BYTES {
        return Err(EngineError::msg(
            "canonical analysis run is not a bounded regular file",
        ));
    }
    let run = analysis_store::load(data_dir, id)?;
    if run.id != id || run.workspace.as_deref() != Some(workspace) {
        return Err(EngineError::msg(
            "analysis run belongs to a different workspace",
        ));
    }
    validate_run_scope(&run)?;
    Ok(run)
}

fn validate_run_scope(run: &AnalysisTurn) -> EngineResult<()> {
    if let Some(snapshot) = &run.workspace_snapshot {
        if run.workspace.as_deref() != Some(snapshot.path.as_str()) {
            return Err(EngineError::msg(
                "analysis run workspace snapshot is inconsistent",
            ));
        }
    }
    if let (Some(revision), Some(trace_revision)) =
        (&run.workspace_revision, &run.trace.workspace_revision)
    {
        if revision != trace_revision {
            return Err(EngineError::msg(
                "analysis run source revision is inconsistent",
            ));
        }
    }
    if let Some(context) = &run.investigation_context {
        if !context.source_selection.is_all_sources() {
            if let Some(scope) = &run.source_scope {
                for source in &scope.used_sources {
                    if !context.source_selection.includes_path(&source.source) {
                        return Err(EngineError::msg(
                            "analysis run contains evidence outside its recorded source scope",
                        ));
                    }
                }
            }
        }
    }
    Ok(())
}

/// Resolve only saved, non-excluded table data. Presentation-provided numbers,
/// chart series, prose, and Python stdout are not result tables. Completeness
/// refers to this saved result, never to underlying source coverage.
pub fn eligible_results(run: &AnalysisTurn) -> EngineResult<Vec<BoundResult>> {
    validate_run_scope(run)?;
    let mut results = Vec::new();
    let mut seen = HashSet::new();
    for item in &run.result.evidence {
        // Schema inspection and source previews are useful context, but not
        // computation outputs. A sample's row_count can equal its stored rows
        // without describing the full source; never expose it as an answer.
        if !matches!(
            item.get("tool").and_then(Json::as_str),
            Some("run_sql" | "run_python" | "forecast_analysis")
        ) {
            continue;
        }
        if item.get("error").is_some_and(|v| !v.is_null()) {
            continue;
        }
        if let Some(disposition) = item.get("verifier_disposition").filter(|v| !v.is_null()) {
            match disposition.get("state").and_then(Json::as_str) {
                Some("excluded") => continue,
                Some("artifact_withheld") => {
                    if matches!(
                        disposition.get("artifact").and_then(Json::as_str),
                        Some("table" | "result_table" | "presentation")
                    ) {
                        continue;
                    }
                }
                _ => continue,
            }
        }
        let derived = item.get("result_table").filter(|v| v.is_object());
        let table = derived.unwrap_or(item);
        let (Some(columns), Some(rows)) = (table.get("columns"), table.get("rows")) else {
            continue;
        };
        if columns.is_null() || rows.is_null() {
            continue;
        }
        let Some(id) = item.get("id").and_then(Json::as_str) else {
            return Err(EngineError::msg(
                "saved result is missing its evidence identity",
            ));
        };
        validate_id(id)?;
        if !seen.insert(id.to_owned()) || results.len() >= MAX_RESULTS {
            return Err(EngineError::msg(
                "duplicate or excessive saved result identities",
            ));
        }
        let columns: Vec<String> = serde_json::from_value(columns.clone())?;
        let rows: Vec<Vec<Json>> = serde_json::from_value(rows.clone())?;
        if columns.is_empty()
            || columns.len() > 1024
            || rows.len() > MAX_RESULT_ROWS
            || columns.iter().any(|c| c.is_empty() || c.len() > 4096)
            || columns.iter().collect::<HashSet<_>>().len() != columns.len()
            || rows.iter().any(|row| {
                row.len() != columns.len() || row.iter().any(|v| v.is_array() || v.is_object())
            })
        {
            return Err(EngineError::msg(
                "saved result has an invalid or excessive table shape",
            ));
        }
        if let Some(context) = &run.investigation_context {
            if !context.source_selection.is_all_sources() {
                if let Some(sources) = item.get("sources").and_then(Json::as_array) {
                    for source in sources {
                        if let Some(path) = source.get("source").and_then(Json::as_str) {
                            if !context.source_selection.includes_path(path) {
                                return Err(EngineError::msg(
                                    "saved result is outside the Investigation source scope",
                                ));
                            }
                        }
                    }
                }
            }
        }
        let count = if derived.is_some() {
            Some(rows.len())
        } else {
            item.get("row_count")
                .and_then(Json::as_u64)
                .and_then(|n| usize::try_from(n).ok())
        };
        let summary = item
            .get("result_summary")
            .and_then(Json::as_str)
            .unwrap_or("")
            .to_ascii_lowercase();
        let truncated = item.get("truncated").and_then(Json::as_bool) == Some(true)
            || summary.contains("capped")
            || summary.contains("truncated");
        let exact_rows = rows
            .iter()
            .map(|row| {
                row.iter()
                    .map(|v| match v {
                        Json::String(s) => s.clone(),
                        value => value.to_string(),
                    })
                    .collect()
            })
            .collect();
        results.push(BoundResult {
            id: id.to_owned(),
            columns,
            row_count: count.unwrap_or(rows.len()).max(rows.len()),
            complete: count == Some(rows.len()) && !truncated,
            rows,
            exact_rows,
        });
    }
    Ok(results)
}

pub fn create_from_run(
    data_dir: &Path,
    workspace: &str,
    run_id: &str,
    title: &str,
    spec: &Json,
) -> EngineResult<ArtifactRecord> {
    validate_workspace(workspace)?;
    let title = validate_title(title)?;
    validate_spec(spec)?;
    let _guard = analysis_store::lifecycle_lock(data_dir)?;
    let run = load_run(data_dir, workspace, run_id)?;
    eligible_results(&run)?;
    let db = store_unlocked(data_dir)?;
    let now = now_ms();
    let artifact = ArtifactRecord {
        id: new_id("artifact")?,
        workspace: workspace.to_owned(),
        title,
        current_revision: 1,
        saved: false,
        revisions: vec![ArtifactRevision {
            revision: 1,
            parent_revision: None,
            run_id: run_id.to_owned(),
            spec: spec.clone(),
            schema_version: SCHEMA_VERSION.to_owned(),
            change_note: "Created from saved analysis".into(),
            created_at_ms: now,
        }],
        created_at_ms: now,
        updated_at_ms: now,
    };
    put_artifact_unlocked(&db, &artifact)?;
    Ok(artifact)
}

pub fn list(data_dir: &Path, workspace: &str) -> EngineResult<Vec<ArtifactRecord>> {
    validate_workspace(workspace)?;
    let _guard = analysis_store::lifecycle_lock(data_dir)?;
    let db = store_unlocked(data_dir)?;
    let mut query = db.prepare("SELECT id FROM artifacts WHERE workspace=?1 ORDER BY id")?;
    let ids = query
        .query_map([workspace], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    ids.iter()
        .map(|id| load_artifact_unlocked(&db, workspace, id))
        .collect()
}

/// List locally saved outputs for report authoring. Each returned record keeps
/// its own source workspace; this does not grant the presentation runtime data
/// access or change any analysis scope.
pub fn list_for_reports(data_dir: &Path) -> EngineResult<Vec<ArtifactRecord>> {
    let _guard = analysis_store::lifecycle_lock(data_dir)?;
    let db = store_unlocked(data_dir)?;
    let mut query = db.prepare("SELECT id FROM artifacts ORDER BY id")?;
    let ids = query
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    let mut records = ids
        .iter()
        .map(|id| load_artifact_by_id_unlocked(&db, id))
        .collect::<EngineResult<Vec<_>>>()?;
    records.sort_by_key(|record| std::cmp::Reverse(record.updated_at_ms));
    Ok(records)
}

pub fn load(data_dir: &Path, workspace: &str, artifact_id: &str) -> EngineResult<ArtifactRecord> {
    validate_workspace(workspace)?;
    let _guard = analysis_store::lifecycle_lock(data_dir)?;
    load_artifact_unlocked(&store_unlocked(data_dir)?, workspace, artifact_id)
}

fn revision(record: &ArtifactRecord, number: u64) -> EngineResult<&ArtifactRevision> {
    record
        .revisions
        .iter()
        .find(|r| r.revision == number)
        .ok_or_else(|| EngineError::msg("artifact revision does not exist"))
}

fn resolve_unlocked(
    data_dir: &Path,
    db: &Connection,
    workspace: &str,
    artifact_id: &str,
    number: Option<u64>,
) -> EngineResult<ResolvedArtifact> {
    let artifact = load_artifact_unlocked(db, workspace, artifact_id)?;
    resolve_record_unlocked(data_dir, db, artifact, number)
}

fn resolve_saved_unlocked(
    data_dir: &Path,
    db: &Connection,
    artifact_id: &str,
    number: Option<u64>,
) -> EngineResult<ResolvedArtifact> {
    let artifact = load_artifact_by_id_unlocked(db, artifact_id)?;
    resolve_record_unlocked(data_dir, db, artifact, number)
}

fn resolve_record_unlocked(
    data_dir: &Path,
    _db: &Connection,
    artifact: ArtifactRecord,
    number: Option<u64>,
) -> EngineResult<ResolvedArtifact> {
    let revision = revision(&artifact, number.unwrap_or(artifact.current_revision))?.clone();
    let run = load_run(data_dir, &artifact.workspace, &revision.run_id)?;
    let results = eligible_results(&run)?;
    Ok(ResolvedArtifact {
        artifact,
        revision,
        run,
        results,
    })
}

pub fn resolve_saved(
    data_dir: &Path,
    artifact_id: &str,
    number: Option<u64>,
) -> EngineResult<ResolvedArtifact> {
    let _guard = analysis_store::lifecycle_lock(data_dir)?;
    resolve_saved_unlocked(data_dir, &store_unlocked(data_dir)?, artifact_id, number)
}

fn declares_interaction_field(spec: &Json, result_id: &str, group: &str, field: &str) -> bool {
    spec.get("elements")
        .and_then(Json::as_object)
        .is_some_and(|elements| {
            elements.values().any(|element| {
                let Some(props) = element.get("props") else {
                    return false;
                };
                if props.get("resultId").and_then(Json::as_str) != Some(result_id)
                    || props.get("interactionGroup").and_then(Json::as_str) != Some(group)
                {
                    return false;
                }
                if element.get("type").and_then(Json::as_str) == Some("ResultFilter") {
                    return props.get("field").and_then(Json::as_str) == Some(field);
                }
                props
                    .get("linkBy")
                    .and_then(Json::as_array)
                    .is_some_and(|fields| {
                        fields
                            .iter()
                            .any(|candidate| candidate.as_str() == Some(field))
                    })
            })
        })
}

fn filter_literal(value: &str, value_type: &str) -> EngineResult<String> {
    match value_type {
        "string" => Ok(format!("'{}'", value.replace('\'', "''"))),
        "number" => {
            let parsed: serde_json::Number = serde_json::from_str(value)
                .map_err(|_| EngineError::msg("A numeric Report filter value is invalid"))?;
            let canonical = parsed.to_string();
            if canonical != value {
                return Err(EngineError::msg(
                    "A numeric Report filter value is not in canonical form",
                ));
            }
            Ok(canonical)
        }
        "boolean" if value == "true" => Ok("TRUE".into()),
        "boolean" if value == "false" => Ok("FALSE".into()),
        "null" if value == "null" => Ok("NULL".into()),
        _ => Err(EngineError::msg("A Report filter type is invalid")),
    }
}

/// Build a read-only replay of a saved SQL result with exact filters selected
/// from saved Results in the same source snapshot and Investigation boundary.
/// The caller executes this only against the already-mounted source engine.
pub fn report_filter_binding(
    data_dir: &Path,
    artifact_id: &str,
    revision: u64,
    result_id: &str,
    filters: &[ReportFilterValue],
) -> EngineResult<ReportFilterBinding> {
    if filters.is_empty() || filters.len() > 8 {
        return Err(EngineError::msg(
            "A Report filter query needs 1–8 active filters",
        ));
    }
    validate_id(artifact_id)?;
    validate_id(result_id)?;
    let target = resolve_saved(data_dir, artifact_id, Some(revision))?;
    let target_result = target
        .results
        .iter()
        .find(|result| result.id == result_id)
        .ok_or_else(|| EngineError::msg("The saved result is unavailable for filtering"))?;
    let evidence = target
        .run
        .result
        .evidence
        .iter()
        .find(|item| item.get("id").and_then(Json::as_str) == Some(result_id))
        .ok_or_else(|| EngineError::msg("The saved result has no replayable evidence"))?;
    if evidence.get("tool").and_then(Json::as_str) != Some("run_sql") {
        return Err(EngineError::msg(
            "This saved result was not produced by a replayable SQL query",
        ));
    }
    let query = evidence
        .get("sql")
        .and_then(Json::as_str)
        .filter(|query| !query.trim().is_empty())
        .ok_or_else(|| EngineError::msg("The saved SQL query is unavailable"))?;
    super::analytics::data::ensure_read_only(query)?;

    let workspace_revision = target
        .run
        .workspace_revision
        .clone()
        .filter(|revision| !revision.is_empty())
        .ok_or_else(|| EngineError::msg("This result has no recorded source revision"))?;
    let target_selection = target
        .run
        .investigation_context
        .as_ref()
        .map(|context| context.source_selection.clone())
        .unwrap_or_default();
    let mut predicates = Vec::with_capacity(filters.len());
    let mut source_checks = Vec::new();
    for filter in filters {
        validate_id(&filter.source_artifact_id)?;
        validate_id(&filter.result_id)?;
        if filter.source_revision == 0
            || filter.field.is_empty()
            || filter.field.chars().count() > 240
            || filter.value.len() > 16_000
            || filter.interaction_group.is_empty()
            || filter.interaction_group.len() > 80
            || !filter
                .interaction_group
                .chars()
                .enumerate()
                .all(|(index, ch)| {
                    if index == 0 {
                        ch.is_ascii_alphabetic()
                    } else {
                        ch.is_ascii_alphanumeric() || matches!(ch, '_' | '.' | '-')
                    }
                })
        {
            return Err(EngineError::msg("A Report filter value is invalid"));
        }
        let source = resolve_saved(
            data_dir,
            &filter.source_artifact_id,
            Some(filter.source_revision),
        )?;
        if source.artifact.workspace != target.artifact.workspace
            || source.run.workspace_revision.as_deref() != Some(&workspace_revision)
        {
            return Err(EngineError::msg(
                "Linked Report outputs must use the same mounted source revision",
            ));
        }
        let source_selection = source
            .run
            .investigation_context
            .as_ref()
            .map(|context| context.source_selection.clone())
            .unwrap_or_default();
        if source_selection != target_selection {
            return Err(EngineError::msg(
                "Linked Report outputs must use the same source selection",
            ));
        }
        let source_result = source
            .results
            .iter()
            .find(|result| result.id == filter.result_id)
            .ok_or_else(|| EngineError::msg("A linked filter's source result is unavailable"))?;
        if !declares_interaction_field(
            &source.revision.spec,
            &source_result.id,
            &filter.interaction_group,
            &filter.field,
        ) || !declares_interaction_field(
            &target.revision.spec,
            &target_result.id,
            &filter.interaction_group,
            &filter.field,
        ) {
            return Err(EngineError::msg(
                "A saved presentation does not declare this linked filter field",
            ));
        }
        let identifier = format!("\"{}\"", filter.field.replace('"', "\"\""));
        let literal = filter_literal(&filter.value, &filter.value_type)?;
        let source_column = source_result
            .columns
            .iter()
            .position(|column| column == &filter.field)
            .ok_or_else(|| EngineError::msg("A linked filter field is unavailable"))?;
        let exact_value_is_valid = source_result.rows.iter().enumerate().any(|(row, values)| {
            let Some(raw) = values.get(source_column) else {
                return false;
            };
            let kind = if raw.is_null() {
                "null"
            } else if raw.is_string() {
                "string"
            } else if raw.is_number() {
                "number"
            } else if raw.is_boolean() {
                "boolean"
            } else {
                ""
            };
            kind == filter.value_type
                && source_result
                    .exact_rows
                    .get(row)
                    .and_then(|values| values.get(source_column))
                    .is_some_and(|value| value == &filter.value)
        });
        if !exact_value_is_valid {
            let source_evidence = source
                .run
                .result
                .evidence
                .iter()
                .find(|item| item.get("id").and_then(Json::as_str) == Some(&filter.result_id))
                .filter(|item| item.get("tool").and_then(Json::as_str) == Some("run_sql"))
                .and_then(|item| item.get("sql").and_then(Json::as_str))
                .filter(|query| !query.trim().is_empty())
                .ok_or_else(|| {
                    EngineError::msg(
                        "A Report filter must use an exact typed value from its saved result",
                    )
                })?;
            super::analytics::data::ensure_read_only(source_evidence)?;
            let source_query = source_evidence.trim().trim_end_matches(';').trim();
            let source_predicate = if filter.value_type == "null" {
                format!("{identifier} IS NULL")
            } else {
                format!("{identifier} = {literal}")
            };
            let check_sql = format!("SELECT {identifier} AS \"fella_filter_value\" FROM ({source_query}) AS \"fella_filter_source\" WHERE {source_predicate} LIMIT 1");
            super::analytics::data::ensure_read_only(&check_sql)?;
            source_checks.push(ReportFilterSourceCheck {
                sql: check_sql,
                expected_value: filter.value.clone(),
                value_type: filter.value_type.clone(),
            });
        }
        if !target_result
            .columns
            .iter()
            .any(|column| column == &filter.field)
        {
            return Err(EngineError::msg(
                "This saved output does not contain a field required by its linked filter",
            ));
        }
        let predicate = match filter.value_type.as_str() {
            "null" if filter.value == "null" => format!("{identifier} IS NULL"),
            _ => format!("{identifier} = {literal}"),
        };
        predicates.push(predicate);
    }

    let query = query.trim().trim_end_matches(';').trim();
    let sql = format!(
        "SELECT * FROM ({query}) AS \"fella_report_result\" WHERE {}",
        predicates.join(" AND ")
    );
    super::analytics::data::ensure_read_only(&sql)?;
    Ok(ReportFilterBinding {
        workspace: target.artifact.workspace,
        workspace_revision,
        sql,
        columns: target_result.columns.clone(),
        source_selection: target_selection,
        source_checks,
    })
}

pub fn resolve(
    data_dir: &Path,
    workspace: &str,
    artifact_id: &str,
    number: Option<u64>,
) -> EngineResult<ResolvedArtifact> {
    validate_workspace(workspace)?;
    let _guard = analysis_store::lifecycle_lock(data_dir)?;
    resolve_unlocked(
        data_dir,
        &store_unlocked(data_dir)?,
        workspace,
        artifact_id,
        number,
    )
}

/// Append a candidate without adopting it or mutating its producing run.
pub fn revise(
    data_dir: &Path,
    workspace: &str,
    artifact_id: &str,
    base_revision: u64,
    run_id: Option<&str>,
    spec: Option<Json>,
    change_note: &str,
) -> EngineResult<ArtifactRecord> {
    validate_workspace(workspace)?;
    if change_note.len() > 4096 || change_note.contains('\0') {
        return Err(EngineError::msg("revision note exceeds its storage limit"));
    }
    let _guard = analysis_store::lifecycle_lock(data_dir)?;
    let db = store_unlocked(data_dir)?;
    let mut record = load_artifact_unlocked(&db, workspace, artifact_id)?;
    if record.current_revision != base_revision {
        return Err(EngineError::msg(
            "artifact changed; reload before creating a revision",
        ));
    }
    if record.revisions.len() >= MAX_REVISIONS {
        return Err(EngineError::msg("artifact revision storage limit reached"));
    }
    let base = revision(&record, base_revision)?;
    let run_id = run_id.unwrap_or(&base.run_id);
    let spec = spec.unwrap_or_else(|| base.spec.clone());
    validate_spec(&spec)?;
    eligible_results(&load_run(data_dir, workspace, run_id)?)?;
    let next = record
        .revisions
        .last()
        .unwrap()
        .revision
        .checked_add(1)
        .ok_or_else(|| EngineError::msg("artifact revision overflow"))?;
    let candidate = ArtifactRevision {
        revision: next,
        parent_revision: Some(base_revision),
        run_id: run_id.to_owned(),
        spec,
        schema_version: SCHEMA_VERSION.to_owned(),
        change_note: change_note.to_owned(),
        created_at_ms: now_ms(),
    };
    record.updated_at_ms = candidate.created_at_ms;
    record.revisions.push(candidate);
    put_artifact_unlocked(&db, &record)?;
    Ok(record)
}

/// Optimistically move the current pointer. Historical pinned revisions stay put.
pub fn adopt(
    data_dir: &Path,
    workspace: &str,
    artifact_id: &str,
    base_revision: u64,
    number: u64,
) -> EngineResult<ArtifactRecord> {
    validate_workspace(workspace)?;
    let _guard = analysis_store::lifecycle_lock(data_dir)?;
    let db = store_unlocked(data_dir)?;
    let mut record = load_artifact_unlocked(&db, workspace, artifact_id)?;
    if record.current_revision != base_revision {
        return Err(EngineError::msg(
            "artifact changed; reload before adopting a revision",
        ));
    }
    let candidate = revision(&record, number)?;
    eligible_results(&load_run(data_dir, workspace, &candidate.run_id)?)?;
    record.current_revision = number;
    record.updated_at_ms = now_ms();
    put_artifact_unlocked(&db, &record)?;
    Ok(record)
}

/// Explicit retention is monotonic. Removing a placement does not erase saved
/// history; deleting saved work requires a separate, deliberately designed flow.
pub fn mark_saved(
    data_dir: &Path,
    workspace: &str,
    artifact_id: &str,
) -> EngineResult<ArtifactRecord> {
    validate_workspace(workspace)?;
    let _guard = analysis_store::lifecycle_lock(data_dir)?;
    let db = store_unlocked(data_dir)?;
    let mut record = load_artifact_unlocked(&db, workspace, artifact_id)?;
    for revision in &record.revisions {
        load_run(data_dir, workspace, &revision.run_id)?;
    }
    record.saved = true;
    record.updated_at_ms = now_ms();
    put_artifact_unlocked(&db, &record)?;
    Ok(record)
}

/// Caller holds the lifecycle lock. Corrupt metadata fails closed: conversation
/// deletion must never interpret an unreadable retention record as "not saved".
pub(crate) fn retained_run_ids_unlocked(data_dir: &Path) -> EngineResult<HashSet<String>> {
    if !data_dir.join("analysis").join("artifacts.sqlite").exists() {
        return Ok(HashSet::new());
    }
    let db = store_unlocked(data_dir)?;
    let mut query = db.prepare("SELECT id,workspace,body FROM artifacts")?;
    let mut rows = query.query([])?;
    let mut retained = HashSet::new();
    while let Some(row) = rows.next()? {
        let id: String = row.get(0)?;
        let workspace: String = row.get(1)?;
        let body: String = row.get(2)?;
        let record: ArtifactRecord = decode(&body)?;
        validate_artifact(&record)?;
        if id != record.id || workspace != record.workspace {
            return Err(EngineError::msg(
                "stored artifact retention identity is inconsistent",
            ));
        }
        if record.saved {
            retained.extend(record.revisions.into_iter().map(|r| r.run_id));
        }
    }
    Ok(retained)
}

fn load_studio_unlocked(
    db: &Connection,
    workspace: &str,
    id: &str,
) -> EngineResult<StudioDocument> {
    validate_id(id)?;
    let body: String = db
        .query_row(
            "SELECT body FROM studio_documents WHERE id=?1 AND workspace=?2",
            params![id, workspace],
            |r| r.get(0),
        )
        .optional()?
        .ok_or_else(|| EngineError::msg("Studio document is not available in this workspace"))?;
    let document: StudioDocument = decode(&body)?;
    if document.id != id || document.workspace != workspace || document.version == 0 {
        return Err(EngineError::msg("Studio document identity is inconsistent"));
    }
    validate_studio(&document)?;
    Ok(document)
}

fn validate_studio(document: &StudioDocument) -> EngineResult<()> {
    validate_id(&document.id)?;
    validate_workspace(&document.workspace)?;
    validate_title(&document.title)?;
    if document.body.len() > MAX_BODY_BYTES
        || document.body.contains('\0')
        || document.checkpoints.len() > MAX_CHECKPOINTS
    {
        return Err(EngineError::msg(
            "Studio document exceeds its storage limit",
        ));
    }
    validate_placement_shapes(&document.placements)?;
    let mut ids = HashSet::new();
    for checkpoint in &document.checkpoints {
        validate_id(&checkpoint.id)?;
        validate_title(&checkpoint.title)?;
        if !ids.insert(&checkpoint.id)
            || checkpoint.body.len() > MAX_BODY_BYTES
            || checkpoint.body.contains('\0')
        {
            return Err(EngineError::msg("invalid Studio checkpoint"));
        }
        validate_placement_shapes(&checkpoint.placements)?;
    }
    Ok(())
}

fn validate_placement_shapes(placements: &[StudioPlacement]) -> EngineResult<()> {
    if placements.len() > MAX_PLACEMENTS {
        return Err(EngineError::msg("Studio placement limit reached"));
    }
    let mut ids = HashSet::new();
    for placement in placements {
        validate_id(&placement.id)?;
        validate_id(&placement.artifact_id)?;
        if !ids.insert(&placement.id)
            || placement.revision == 0
            || placement.label.chars().count() > 160
            || placement.label.contains('\0')
            || !placement.view_state.is_object()
            || serde_json::to_vec(&placement.view_state)?.len() > 16 * 1024
        {
            return Err(EngineError::msg("invalid Studio placement or view state"));
        }
    }
    Ok(())
}

fn studio_revision(
    document: &StudioDocument,
    kind: StudioRevisionKind,
    summary: &str,
) -> StudioRevision {
    StudioRevision {
        document_id: document.id.clone(),
        workspace: document.workspace.clone(),
        version: document.version,
        title: document.title.clone(),
        body: document.body.clone(),
        placements: document.placements.clone(),
        kind,
        summary: summary.chars().take(240).collect(),
        created_at_ms: document.updated_at_ms,
    }
}

fn classify_studio_change(
    before: &StudioDocument,
    after: &StudioDocument,
) -> (StudioRevisionKind, String) {
    let adopted = before.placements.iter().any(|old| {
        after.placements.iter().any(|new| {
            old.id == new.id && old.artifact_id == new.artifact_id && old.revision != new.revision
        })
    });
    let mut changes = Vec::new();
    if before.title != after.title {
        changes.push("report title changed");
    }
    if before.body != after.body {
        changes.push("report notes changed");
    }
    if before.placements.len() != after.placements.len() {
        changes.push("saved outputs added or removed");
    } else if before
        .placements
        .iter()
        .map(|placement| &placement.id)
        .ne(after.placements.iter().map(|placement| &placement.id))
    {
        changes.push("saved outputs rearranged");
    }
    if before.placements.iter().any(|old| {
        after.placements.iter().any(|new| {
            old.id == new.id
                && old.artifact_id == new.artifact_id
                && old.revision == new.revision
                && (old.label != new.label || old.width != new.width)
        })
    }) {
        changes.push("output layout changed");
    }
    let kind = if adopted {
        StudioRevisionKind::Analytical
    } else {
        StudioRevisionKind::Presentation
    };
    let summary = if adopted {
        "A new analytical revision was pinned to this Report."
    } else if changes.is_empty() {
        "Report saved."
    } else {
        return (
            kind,
            format!("{}.", changes.join("; ")).replace("report", "Report"),
        );
    };
    (kind, summary.into())
}

fn store_studio_revision_unlocked(
    db: &Connection,
    document: &StudioDocument,
    kind: StudioRevisionKind,
    summary: &str,
) -> EngineResult<()> {
    let revision = studio_revision(document, kind, summary);
    let version = i64::try_from(revision.version)
        .map_err(|_| EngineError::msg("Report revision exceeds the storage range"))?;
    db.execute(
        "INSERT OR IGNORE INTO studio_revisions(document_id,version,body) VALUES(?1,?2,?3)",
        params![revision.document_id, version, encode(&revision)?],
    )?;
    db.execute(
        "DELETE FROM studio_revisions WHERE document_id=?1 AND version NOT IN
         (SELECT version FROM studio_revisions WHERE document_id=?1 ORDER BY version DESC LIMIT ?2)",
        params![revision.document_id, MAX_STUDIO_HISTORY as i64],
    )?;
    loop {
        let bytes: i64 = db.query_row(
            "SELECT COALESCE(SUM(length(body)),0) FROM studio_revisions WHERE document_id=?1",
            [&revision.document_id],
            |row| row.get(0),
        )?;
        if bytes <= MAX_STUDIO_HISTORY_BYTES {
            break;
        }
        let oldest: Option<i64> = db.query_row(
            "SELECT MIN(version) FROM studio_revisions WHERE document_id=?1",
            [&revision.document_id],
            |row| row.get(0),
        )?;
        let Some(oldest) = oldest else { break };
        if oldest == version {
            break;
        }
        db.execute(
            "DELETE FROM studio_revisions WHERE document_id=?1 AND version=?2",
            params![revision.document_id, oldest],
        )?;
    }
    Ok(())
}

fn validate_studio_revision(revision: &StudioRevision, document_id: &str) -> EngineResult<()> {
    validate_id(&revision.document_id)?;
    validate_workspace(&revision.workspace)?;
    validate_title(&revision.title)?;
    validate_placement_shapes(&revision.placements)?;
    if revision.document_id != document_id
        || revision.version == 0
        || revision.body.len() > MAX_BODY_BYTES
        || revision.body.contains('\0')
        || revision.summary.chars().count() > 240
    {
        return Err(EngineError::msg("Report revision is invalid"));
    }
    Ok(())
}

fn put_studio_unlocked(db: &Connection, document: &StudioDocument) -> EngineResult<()> {
    validate_studio(document)?;
    db.execute(
        "INSERT INTO studio_documents(id,workspace,body) VALUES(?1,?2,?3)
         ON CONFLICT(id) DO UPDATE SET body=excluded.body WHERE workspace=excluded.workspace",
        params![document.id, document.workspace, encode(document)?],
    )?;
    Ok(())
}

fn next_version(document: &mut StudioDocument, expected: u64) -> EngineResult<()> {
    if expected != document.version {
        return Err(EngineError::msg(
            "Studio document changed; reload before saving",
        ));
    }
    document.version = document
        .version
        .checked_add(1)
        .ok_or_else(|| EngineError::msg("Studio version overflow"))?;
    document.updated_at_ms = now_ms();
    Ok(())
}

pub fn studio_create(
    data_dir: &Path,
    workspace: &str,
    title: &str,
) -> EngineResult<StudioDocument> {
    validate_workspace(workspace)?;
    let title = validate_title(title)?;
    let _guard = analysis_store::lifecycle_lock(data_dir)?;
    let now = now_ms();
    let document = StudioDocument {
        id: new_id("studio")?,
        workspace: workspace.to_owned(),
        title,
        version: 1,
        body: String::new(),
        placements: vec![],
        checkpoints: vec![],
        created_at_ms: now,
        updated_at_ms: now,
    };
    let db = store_unlocked(data_dir)?;
    let tx = db.unchecked_transaction()?;
    put_studio_unlocked(&tx, &document)?;
    store_studio_revision_unlocked(
        &tx,
        &document,
        StudioRevisionKind::Created,
        "Report created.",
    )?;
    tx.commit()?;
    Ok(document)
}

/// Avoid repeating the sample queries once its saved Report seed is complete.
pub fn studio_northstar_sample_seeded(data_dir: &Path) -> EngineResult<Option<StudioDocument>> {
    let _guard = analysis_store::lifecycle_lock(data_dir)?;
    let db = store_unlocked(data_dir)?;
    let version = db
        .query_row(
            "SELECT value FROM artifact_metadata WHERE key='northstar_sample_report_seed'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    if version.as_deref() == Some(NORTHSTAR_SAMPLE_REPORT_SEED_VERSION) {
        return load_studio_unlocked(&db, APP_WORKSPACE_ID, NORTHSTAR_SAMPLE_REPORT_ID).map(Some);
    }
    Ok(None)
}

/// Create or upgrade the starter Report using a canonical, query-backed run.
/// Existing Report edits are preserved; this only adds the sample placement.
pub fn studio_seed_northstar_sample_report(
    data_dir: &Path,
    run: &AnalysisTurn,
) -> EngineResult<StudioDocument> {
    let workspace = run
        .workspace
        .as_deref()
        .ok_or_else(|| EngineError::msg("Northstar sample analysis has no mounted workspace"))?;
    let revision = run
        .workspace_revision
        .as_deref()
        .ok_or_else(|| EngineError::msg("Northstar sample analysis has no source revision"))?;
    let spec = run
        .result
        .presentation_spec
        .as_ref()
        .ok_or_else(|| EngineError::msg("Northstar sample analysis has no Report layout"))?;
    validate_spec(spec)?;
    eligible_results(run)?;

    let _guard = analysis_store::lifecycle_lock(data_dir)?;
    let mut db = store_unlocked(data_dir)?;
    let seed_version = db
        .query_row(
            "SELECT value FROM artifact_metadata WHERE key='northstar_sample_report_seed'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    if seed_version.as_deref() == Some(NORTHSTAR_SAMPLE_REPORT_SEED_VERSION) {
        return load_studio_unlocked(&db, APP_WORKSPACE_ID, NORTHSTAR_SAMPLE_REPORT_ID);
    }

    if analysis_store::exists(data_dir, &run.id) {
        let existing = analysis_store::load(data_dir, &run.id)?;
        if existing.workspace.as_deref() != Some(workspace)
            || existing.workspace_revision.as_deref() != Some(revision)
        {
            return Err(EngineError::msg(
                "Northstar sample analysis identity conflicts with a different source revision",
            ));
        }
    } else {
        analysis_store::create_unlocked(data_dir, run)?;
    }

    let tx = db.transaction()?;
    let document_exists = tx
        .query_row(
            "SELECT 1 FROM studio_documents WHERE id=?1 AND workspace=?2",
            params![NORTHSTAR_SAMPLE_REPORT_ID, APP_WORKSPACE_ID],
            |_| Ok(()),
        )
        .optional()?
        .is_some();

    let suffix: String = revision
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .take(32)
        .collect();
    if suffix.is_empty() {
        return Err(EngineError::msg("Northstar source revision is invalid"));
    }
    let artifact_id = format!("{NORTHSTAR_SAMPLE_ARTIFACT_PREFIX}{suffix}");
    let artifact_exists = tx
        .query_row(
            "SELECT 1 FROM artifacts WHERE id=?1",
            [&artifact_id],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    let artifact = if artifact_exists {
        let mut artifact = load_artifact_by_id_unlocked(&tx, &artifact_id)?;
        if artifact.workspace != workspace {
            return Err(EngineError::msg(
                "Northstar sample artifact belongs to a different mounted workspace",
            ));
        }
        if !artifact
            .revisions
            .iter()
            .any(|revision| revision.run_id == run.id)
        {
            return Err(EngineError::msg(
                "Northstar sample artifact is already bound to another analysis run",
            ));
        }
        artifact.saved = true;
        artifact.updated_at_ms = now_ms();
        put_artifact_unlocked(&tx, &artifact)?;
        artifact
    } else {
        let now = now_ms();
        let artifact = ArtifactRecord {
            id: artifact_id.clone(),
            workspace: workspace.to_owned(),
            title: "Northstar revenue explorer".to_owned(),
            current_revision: 1,
            saved: true,
            revisions: vec![ArtifactRevision {
                revision: 1,
                parent_revision: None,
                run_id: run.id.clone(),
                spec: spec.clone(),
                schema_version: SCHEMA_VERSION.to_owned(),
                change_note: "Seeded interactive sample Report from read-only Northstar analysis."
                    .to_owned(),
                created_at_ms: now,
            }],
            created_at_ms: now,
            updated_at_ms: now,
        };
        put_artifact_unlocked(&tx, &artifact)?;
        artifact
    };

    let now = now_ms();
    let mut document = if document_exists {
        load_studio_unlocked(&tx, APP_WORKSPACE_ID, NORTHSTAR_SAMPLE_REPORT_ID)?
    } else {
        StudioDocument {
            id: NORTHSTAR_SAMPLE_REPORT_ID.to_owned(),
            workspace: APP_WORKSPACE_ID.to_owned(),
            title: "Northstar Outfitters — Revenue review".to_owned(),
            version: 1,
            body: String::new(),
            placements: Vec::new(),
            checkpoints: Vec::new(),
            created_at_ms: now,
            updated_at_ms: now,
        }
    };

    let placement_id = "placement_northstar_revenue_explorer";
    if !document
        .placements
        .iter()
        .any(|placement| placement.artifact_id == artifact.id)
    {
        document.placements.push(StudioPlacement {
            id: placement_id.to_owned(),
            artifact_id: artifact.id,
            revision: artifact.current_revision,
            label: "Revenue explorer".to_owned(),
            width: StudioPlacementWidth::Full,
            view_state: serde_json::json!({}),
        });
    }

    let sample_intro = "**Status: In progress**\n\nA working Northstar revenue analysis, seeded with saved results so you can explore the charts, filters, and linked tables. Change a selector or click a chart category to filter the related view.";
    let methodology = "## Metric notes\n\n- Monthly net merchandise revenue uses merchandise sales after line discounts, grouped by order month, less refunds posted in each refund month.\n- The Q3/Q4 comparison uses order-date merchandise sales less refunds posted in each quarter. AOV is discounted merchandise sales divided by non-cancelled orders.\n- Category sales are shown before refunds because the refund source has no product-category attribution.\n- Only USD orders and posted refunds are included. Pending refunds, cancelled orders, tax, and shipping are excluded; values are not currency-converted. See `docs/metric_policy_current.md` for the current policy.";
    if document.body == NORTHSTAR_SAMPLE_REPORT_LEGACY_BODY || document.body.trim().is_empty() {
        document.body = format!("{sample_intro}\n\n{methodology}");
    } else if !document.body.contains("working Northstar revenue analysis") {
        document.body = format!(
            "{}\n\n---\n\n{sample_intro}\n\n{methodology}",
            document.body.trim()
        );
    }

    if document_exists {
        let base_version = document.version;
        next_version(&mut document, base_version)?;
    }
    put_studio_unlocked(&tx, &document)?;
    store_studio_revision_unlocked(
        &tx,
        &document,
        if document_exists {
            StudioRevisionKind::Presentation
        } else {
            StudioRevisionKind::Created
        },
        "Added a saved, interactive Northstar sample analysis to the starter Report.",
    )?;
    tx.execute(
        "INSERT INTO artifact_metadata(key,value) VALUES('northstar_sample_report_seed',?1)
         ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        [NORTHSTAR_SAMPLE_REPORT_SEED_VERSION],
    )?;
    tx.commit()?;
    Ok(document)
}

/// The starter Report uses the same json-render components and saved-result
/// bindings as model-generated analytical presentations.
pub fn northstar_sample_report_spec() -> Json {
    let mut elements = serde_json::Map::new();
    let mut add = |key: &str, kind: &str, props: Json, children: Vec<&str>| {
        elements.insert(
            key.to_owned(),
            serde_json::json!({"type":kind,"props":props,"children":children}),
        );
    };

    add(
        "report",
        "Document",
        serde_json::json!({}),
        vec![
            "headline",
            "intro",
            "metric_grid",
            "quarter_detail_section",
            "trend_section",
            "category_section",
            "refund_section",
            "notes",
        ],
    );
    add(
        "headline",
        "Title",
        serde_json::json!({"content":"Northstar revenue, made explorable","level":1}),
        vec![],
    );
    add(
        "intro",
        "Text",
        serde_json::json!({"content":"A seeded working analysis of 2023–2025 merchandise revenue, with quarter comparisons and source caveats. Use the controls to explore the saved results."}),
        vec![],
    );
    add(
        "metric_grid",
        "Grid",
        serde_json::json!({"columns":4}),
        vec!["q3_net", "q4_net", "net_change", "net_change_pct"],
    );
    add(
        "q3_net",
        "Metric",
        serde_json::json!({"resultId":"northstar_quarters_v2","field":"net_merchandise_revenue","row":0,"title":"Q3 2025 net revenue","unit":"USD","format":"currency","detail":"Order-date sales less refunds posted in Q3."}),
        vec![],
    );
    add(
        "q4_net",
        "Metric",
        serde_json::json!({"resultId":"northstar_quarters_v2","field":"net_merchandise_revenue","row":1,"title":"Q4 2025 net revenue","unit":"USD","format":"currency","detail":"Same period basis as Q3."}),
        vec![],
    );
    add(
        "net_change",
        "Metric",
        serde_json::json!({"resultId":"northstar_quarters_v2","field":"net_change","row":1,"title":"Quarter-over-quarter change","unit":"USD","format":"currency","detail":"Q4 less Q3."}),
        vec![],
    );
    add(
        "net_change_pct",
        "Metric",
        serde_json::json!({"resultId":"northstar_quarters_v2","field":"net_change_percent","row":1,"title":"Change from Q3","format":"percent","detail":"Relative to Q3 net merchandise revenue."}),
        vec![],
    );

    add(
        "quarter_detail_section",
        "Section",
        serde_json::json!({"title":"Quarterly detail · Q3 vs Q4 2025"}),
        vec!["quarter_detail_table"],
    );
    add(
        "quarter_detail_table",
        "Table",
        serde_json::json!({"resultId":"northstar_quarters_v2","title":"Sales, refunds, and order value","fields":["quarter","merchandise_sales","posted_refunds","net_merchandise_revenue","order_count","merchandise_aov"],"columnFormats":{"merchandise_sales":{"label":"Sales before refunds","format":"currency","unit":"USD","decimals":2},"posted_refunds":{"label":"Refunds posted","format":"currency","unit":"USD","decimals":2},"net_merchandise_revenue":{"label":"Net merchandise revenue","format":"currency","unit":"USD","decimals":2},"order_count":{"label":"Non-cancelled orders","format":"number","decimals":0},"merchandise_aov":{"label":"Avg. discounted value per order","format":"currency","unit":"USD","decimals":2}}}),
        vec![],
    );

    add(
        "trend_section",
        "Section",
        serde_json::json!({"title":"Monthly net merchandise revenue"}),
        vec!["trend_caption", "year_filter", "trend_chart", "trend_table"],
    );
    add(
        "trend_caption",
        "Caption",
        serde_json::json!({"content":"Sales are grouped by order month; posted refunds are grouped by refund month, in line with the current finance policy."}),
        vec![],
    );
    add(
        "year_filter",
        "ResultFilter",
        serde_json::json!({"resultId":"northstar_months_v2","field":"year","name":"Trend year","title":"Year","interactionGroup":"northstar_trend"}),
        vec![],
    );
    add(
        "trend_chart",
        "Chart",
        serde_json::json!({"resultId":"northstar_months_v2","title":"Revenue by month","interactionGroup":"northstar_trend","linkBy":["year"],"spec":{"mark":{"type":"line","point":true},"encoding":{"x":{"field":"month_start","type":"temporal","title":"Month","axis":{"format":"%b %y","labelAngle":-35}},"y":{"field":"net_merchandise_revenue","type":"quantitative","title":"Net merchandise revenue (USD)","axis":{"format":"$,.0f"}},"color":{"field":"year","type":"nominal","title":"Year"},"tooltip":[{"field":"month_label","type":"nominal","title":"Month"},{"field":"net_merchandise_revenue","type":"quantitative","title":"Net revenue","format":"$,.2f"},{"field":"posted_refunds","type":"quantitative","title":"Posted refunds","format":"$,.2f"}]}}}),
        vec![],
    );
    add(
        "trend_table",
        "Table",
        serde_json::json!({"resultId":"northstar_months_v2","title":"Monthly values","fields":["month_label","year","net_merchandise_revenue","posted_refunds","order_count"],"columnFormats":{"net_merchandise_revenue":{"label":"Net revenue","format":"currency","unit":"USD","decimals":2},"posted_refunds":{"label":"Posted refunds","format":"currency","unit":"USD","decimals":2},"order_count":{"label":"Orders","format":"number","decimals":0}},"interactionGroup":"northstar_trend","linkBy":["year"]}),
        vec![],
    );

    add(
        "category_section",
        "Section",
        serde_json::json!({"title":"Product category comparison · Q3 vs Q4 2025"}),
        vec![
            "category_caption",
            "quarter_filter",
            "category_chart",
            "category_table",
        ],
    );
    add(
        "category_caption",
        "Caption",
        serde_json::json!({"content":"Merchandise sales after line discounts, before refunds. Refunds cannot be attributed to product categories from the available source data."}),
        vec![],
    );
    add(
        "quarter_filter",
        "ResultFilter",
        serde_json::json!({"resultId":"northstar_categories_v2","field":"quarter","name":"Category quarter","title":"Quarter","interactionGroup":"northstar_categories"}),
        vec![],
    );
    add(
        "category_chart",
        "Chart",
        serde_json::json!({"resultId":"northstar_categories_v2","title":"Sales by category","interactionGroup":"northstar_categories","linkBy":["category","quarter"],"spec":{"mark":"bar","encoding":{"x":{"field":"category","type":"nominal","title":"Product category","axis":{"labelAngle":-25}},"xOffset":{"field":"quarter","type":"nominal"},"y":{"field":"merchandise_sales","type":"quantitative","title":"Sales before refunds (USD)","axis":{"format":"$,.0f"}},"color":{"field":"quarter","type":"nominal","title":"Quarter"},"tooltip":[{"field":"category","type":"nominal"},{"field":"quarter","type":"nominal"},{"field":"merchandise_sales","type":"quantitative","title":"Merchandise sales","format":"$,.2f"}]}}}),
        vec![],
    );
    add(
        "category_table",
        "Table",
        serde_json::json!({"resultId":"northstar_categories_v2","title":"Category values","fields":["category","quarter","merchandise_sales"],"columnFormats":{"merchandise_sales":{"label":"Sales before refunds","format":"currency","unit":"USD","decimals":2}},"interactionGroup":"northstar_categories","linkBy":["category","quarter"]}),
        vec![],
    );

    add(
        "refund_section",
        "Section",
        serde_json::json!({"title":"Posted refunds by reason"}),
        vec![
            "refund_caption",
            "refund_quarter_filter",
            "refund_chart",
            "refund_table",
        ],
    );
    add(
        "refund_caption",
        "Caption",
        serde_json::json!({"content":"Refunds are grouped by the date they were posted, not by the original order date. Pending refunds are excluded."}),
        vec![],
    );
    add(
        "refund_quarter_filter",
        "ResultFilter",
        serde_json::json!({"resultId":"northstar_refunds_v2","field":"quarter","name":"Refund quarter","title":"Quarter","interactionGroup":"northstar_refunds"}),
        vec![],
    );
    add(
        "refund_chart",
        "Chart",
        serde_json::json!({"resultId":"northstar_refunds_v2","title":"Posted refund amount by reason","interactionGroup":"northstar_refunds","linkBy":["reason","quarter"],"spec":{"mark":"bar","encoding":{"x":{"field":"reason","type":"nominal","title":"Refund reason","axis":{"labelAngle":-30}},"xOffset":{"field":"quarter","type":"nominal"},"y":{"field":"posted_refunds","type":"quantitative","title":"Posted refunds (USD)","axis":{"format":"$,.0f"}},"color":{"field":"quarter","type":"nominal","title":"Quarter"},"tooltip":[{"field":"reason","type":"nominal"},{"field":"quarter","type":"nominal"},{"field":"posted_refunds","type":"quantitative","title":"Posted refunds","format":"$,.2f"}]}}}),
        vec![],
    );
    add(
        "refund_table",
        "Table",
        serde_json::json!({"resultId":"northstar_refunds_v2","title":"Refund values","fields":["reason","quarter","posted_refunds"],"columnFormats":{"posted_refunds":{"format":"currency","unit":"USD","decimals":2}},"interactionGroup":"northstar_refunds","linkBy":["reason","quarter"]}),
        vec![],
    );

    add(
        "notes",
        "Callout",
        serde_json::json!({"title":"Scope and data notes","tone":"caution","content":"This is a starter analysis, not a finished business conclusion. USD orders and posted refunds are included; cancelled orders, tax, shipping, and pending refunds are excluded. Values are not currency-converted. Monthly and quarterly net values pair order-period merchandise sales with refunds posted during that period. Product category values are before refunds because refund rows do not identify a product category. Review the current finance metric policy and source revisions before using these figures externally."}),
        vec![],
    );

    serde_json::json!({"root":"report","elements":elements})
}

pub fn studio_list(data_dir: &Path, workspace: &str) -> EngineResult<Vec<StudioDocument>> {
    validate_workspace(workspace)?;
    let _guard = analysis_store::lifecycle_lock(data_dir)?;
    let db = store_unlocked(data_dir)?;
    let mut query = db.prepare("SELECT id FROM studio_documents WHERE workspace=?1 ORDER BY id")?;
    let ids = query
        .query_map([workspace], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    ids.iter()
        .map(|id| load_studio_unlocked(&db, workspace, id))
        .collect()
}

pub fn studio_list_all(data_dir: &Path) -> EngineResult<Vec<StudioDocument>> {
    let _guard = analysis_store::lifecycle_lock(data_dir)?;
    let db = store_unlocked(data_dir)?;
    let mut query = db.prepare("SELECT id FROM studio_documents ORDER BY id")?;
    let ids = query
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    let mut documents = ids
        .iter()
        .map(|id| load_studio_by_id_unlocked(&db, id))
        .collect::<EngineResult<Vec<_>>>()?;
    documents.sort_by_key(|document| std::cmp::Reverse(document.updated_at_ms));
    Ok(documents)
}

fn load_studio_by_id_unlocked(db: &Connection, id: &str) -> EngineResult<StudioDocument> {
    validate_id(id)?;
    let body: String = db
        .query_row(
            "SELECT body FROM studio_documents WHERE id=?1",
            [id],
            |row| row.get(0),
        )
        .optional()?
        .ok_or_else(|| EngineError::msg("Report does not exist"))?;
    let document: StudioDocument = decode(&body)?;
    validate_studio(&document)?;
    if document.id != id {
        return Err(EngineError::msg(
            "Report identity does not match its stored record",
        ));
    }
    Ok(document)
}

pub fn studio_load(
    data_dir: &Path,
    workspace: &str,
    document_id: &str,
) -> EngineResult<StudioDocument> {
    validate_workspace(workspace)?;
    let _guard = analysis_store::lifecycle_lock(data_dir)?;
    load_studio_unlocked(&store_unlocked(data_dir)?, workspace, document_id)
}

pub fn studio_save(
    data_dir: &Path,
    workspace: &str,
    document_id: &str,
    base_version: u64,
    update: StudioUpdate,
) -> EngineResult<StudioDocument> {
    validate_workspace(workspace)?;
    validate_placement_shapes(&update.placements)?;
    let _guard = analysis_store::lifecycle_lock(data_dir)?;
    let mut db = store_unlocked(data_dir)?;
    let tx = db.transaction()?;
    let mut document = load_studio_unlocked(&tx, workspace, document_id)?;
    let before = document.clone();
    next_version(&mut document, base_version)?;
    document.title = validate_title(&update.title)?;
    document.body = update.body;
    document.placements = update.placements;
    validate_studio(&document)?;
    // Resolve every pin before updating anything. Retention writes and document
    // placement changes then commit atomically, even across an application crash.
    let mut artifact_ids = HashSet::new();
    for placement in &document.placements {
        resolve_saved_unlocked(
            data_dir,
            &tx,
            &placement.artifact_id,
            Some(placement.revision),
        )?;
        artifact_ids.insert(placement.artifact_id.clone());
    }
    for id in artifact_ids {
        let mut artifact = load_artifact_by_id_unlocked(&tx, &id)?;
        for revision in &artifact.revisions {
            load_run(data_dir, &artifact.workspace, &revision.run_id)?;
        }
        artifact.saved = true;
        artifact.updated_at_ms = now_ms();
        put_artifact_unlocked(&tx, &artifact)?;
    }
    put_studio_unlocked(&tx, &document)?;
    let (kind, summary) = classify_studio_change(&before, &document);
    store_studio_revision_unlocked(&tx, &document, kind, &summary)?;
    tx.commit()?;
    Ok(document)
}

pub fn studio_checkpoint(
    data_dir: &Path,
    workspace: &str,
    document_id: &str,
    base_version: u64,
    title: &str,
) -> EngineResult<StudioDocument> {
    validate_workspace(workspace)?;
    let title = validate_title(title)?;
    let _guard = analysis_store::lifecycle_lock(data_dir)?;
    let mut db = store_unlocked(data_dir)?;
    let tx = db.transaction()?;
    let mut document = load_studio_unlocked(&tx, workspace, document_id)?;
    next_version(&mut document, base_version)?;
    document.checkpoints.push(StudioCheckpoint {
        id: new_id("checkpoint")?,
        title,
        body: document.body.clone(),
        placements: document.placements.clone(),
        created_at_ms: now_ms(),
    });
    put_studio_unlocked(&tx, &document)?;
    store_studio_revision_unlocked(
        &tx,
        &document,
        StudioRevisionKind::Checkpoint,
        "Named Report checkpoint saved.",
    )?;
    tx.commit()?;
    Ok(document)
}

pub fn studio_restore(
    data_dir: &Path,
    workspace: &str,
    document_id: &str,
    base_version: u64,
    checkpoint_id: &str,
) -> EngineResult<StudioDocument> {
    validate_workspace(workspace)?;
    validate_id(checkpoint_id)?;
    let _guard = analysis_store::lifecycle_lock(data_dir)?;
    let mut db = store_unlocked(data_dir)?;
    let tx = db.transaction()?;
    let mut document = load_studio_unlocked(&tx, workspace, document_id)?;
    next_version(&mut document, base_version)?;
    let checkpoint = document
        .checkpoints
        .iter()
        .find(|c| c.id == checkpoint_id)
        .ok_or_else(|| EngineError::msg("Studio checkpoint does not exist"))?
        .clone();
    for placement in &checkpoint.placements {
        resolve_saved_unlocked(
            data_dir,
            &tx,
            &placement.artifact_id,
            Some(placement.revision),
        )?;
    }
    // Restoring moves presentation references only; it does not adopt artifact
    // revisions, recompute analysis, or erase the available checkpoint history.
    document.body = checkpoint.body;
    document.placements = checkpoint.placements;
    put_studio_unlocked(&tx, &document)?;
    store_studio_revision_unlocked(
        &tx,
        &document,
        StudioRevisionKind::Restored,
        "Report restored from a named checkpoint.",
    )?;
    tx.commit()?;
    Ok(document)
}

pub fn studio_history(
    data_dir: &Path,
    workspace: &str,
    document_id: &str,
) -> EngineResult<Vec<StudioRevision>> {
    validate_workspace(workspace)?;
    let _guard = analysis_store::lifecycle_lock(data_dir)?;
    let mut db = store_unlocked(data_dir)?;
    let tx = db.transaction()?;
    let document = load_studio_unlocked(&tx, workspace, document_id)?;
    let history_count: i64 = tx.query_row(
        "SELECT COUNT(*) FROM studio_revisions WHERE document_id=?1",
        [document_id],
        |row| row.get(0),
    )?;
    if history_count == 0 {
        let (kind, summary) = if document.version == 1 {
            (StudioRevisionKind::Created, "Report created.")
        } else {
            (
                StudioRevisionKind::Imported,
                "Current Report version retained when history tracking was added.",
            )
        };
        store_studio_revision_unlocked(&tx, &document, kind, summary)?;
    }
    tx.commit()?;
    let mut query = db.prepare(
        "SELECT body FROM studio_revisions WHERE document_id=?1 ORDER BY version DESC LIMIT ?2",
    )?;
    let bodies = query
        .query_map(params![document_id, MAX_STUDIO_HISTORY as i64], |row| {
            row.get::<_, String>(0)
        })?
        .collect::<Result<Vec<_>, _>>()?;
    bodies
        .iter()
        .map(|body| {
            let revision: StudioRevision = decode(body)?;
            validate_studio_revision(&revision, document_id)?;
            if revision.workspace != workspace {
                return Err(EngineError::msg("Report history owner is inconsistent"));
            }
            Ok(revision)
        })
        .collect()
}

pub fn studio_restore_revision(
    data_dir: &Path,
    workspace: &str,
    document_id: &str,
    base_version: u64,
    revision_version: u64,
) -> EngineResult<StudioDocument> {
    validate_workspace(workspace)?;
    validate_id(document_id)?;
    let revision_number = i64::try_from(revision_version)
        .map_err(|_| EngineError::msg("Report revision exceeds the storage range"))?;
    let _guard = analysis_store::lifecycle_lock(data_dir)?;
    let mut db = store_unlocked(data_dir)?;
    let tx = db.transaction()?;
    let mut document = load_studio_unlocked(&tx, workspace, document_id)?;
    if document.version == revision_version {
        return Err(EngineError::msg("That Report revision is already current"));
    }
    let body: String = tx
        .query_row(
            "SELECT body FROM studio_revisions WHERE document_id=?1 AND version=?2",
            params![document_id, revision_number],
            |row| row.get(0),
        )
        .optional()?
        .ok_or_else(|| EngineError::msg("That Report revision is no longer available"))?;
    let prior: StudioRevision = decode(&body)?;
    validate_studio_revision(&prior, document_id)?;
    if prior.workspace != workspace {
        return Err(EngineError::msg("Report history owner is inconsistent"));
    }
    next_version(&mut document, base_version)?;
    document.title = validate_title(&prior.title)?;
    document.body = prior.body;
    document.placements = prior.placements;
    validate_studio(&document)?;
    let mut artifact_ids = HashSet::new();
    for placement in &document.placements {
        resolve_saved_unlocked(
            data_dir,
            &tx,
            &placement.artifact_id,
            Some(placement.revision),
        )?;
        artifact_ids.insert(placement.artifact_id.clone());
    }
    for id in artifact_ids {
        let mut artifact = load_artifact_by_id_unlocked(&tx, &id)?;
        for revision in &artifact.revisions {
            load_run(data_dir, &artifact.workspace, &revision.run_id)?;
        }
        artifact.saved = true;
        artifact.updated_at_ms = now_ms();
        put_artifact_unlocked(&tx, &artifact)?;
    }
    put_studio_unlocked(&tx, &document)?;
    store_studio_revision_unlocked(
        &tx,
        &document,
        StudioRevisionKind::Restored,
        &format!("Restored Report revision {revision_version}; newer history remains available."),
    )?;
    tx.commit()?;
    Ok(document)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::path::PathBuf;
    use std::sync::{Arc, Barrier};

    const WORKSPACE: &str = "/not-mounted/artifact-fixture";

    struct Profile(PathBuf);

    impl Profile {
        fn new() -> Self {
            let path = std::env::temp_dir().join(new_id("fella-artifacts-test").unwrap());
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for Profile {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn run(id: &str) -> AnalysisTurn {
        serde_json::from_value(json!({
            "id":id,"conversation_id":"chat-1","question":"Compare net revenue",
            "workspace":WORKSPACE,"workspace_revision":"source-v1",
            "state":"accepted","plan":{"strategy":"direct_tools","steps":[]},
            "trace":{"id":"trace-1","turn_id":id,"workspace_revision":"source-v1","steps":[]},
            "result":{"text":"Revenue grew.","status":"verified","evidence":[{
                "id":"evidence-1","tool":"run_sql","sql":"SELECT period, net FROM sales",
                "sources":[{"source":"sales.csv","table":"sales"}],
                "columns":["period","net"],"rows":[["Q3",9007199254740993_u64],["Q4",100.25]],
                "row_count":2,"result_summary":"2 rows"
            }]}
        }))
        .unwrap()
    }

    fn seed(profile: &Profile, id: &str) -> AnalysisTurn {
        let run = run(id);
        analysis_store::create(&profile.0, &run).unwrap();
        run
    }

    fn artifact(profile: &Profile, run: &str) -> ArtifactRecord {
        create_from_run(&profile.0, WORKSPACE, run, "Revenue", &sample_spec()).unwrap()
    }

    fn sample_spec() -> Json {
        serde_json::json!({"root":"doc","elements":{"doc":{"type":"Document","props":{},"children":["title"]},"title":{"type":"Title","props":{"content":"Revenue"},"children":[]}}})
    }

    #[test]
    fn json_render_specs_must_match_the_generated_component_prop_schemas() {
        let mut valid = sample_spec();
        valid["elements"]["doc"]["children"] = json!(["card"]);
        valid["elements"]["card"] = json!({
            "type":"Card", "props":{"tone":"quiet"}, "children":["title"]
        });
        assert!(validate_spec(&valid).is_ok());

        let mut invalid_enum = valid.clone();
        invalid_enum["elements"]["card"]["props"]["tone"] = json!("success");
        assert!(validate_spec(&invalid_enum)
            .unwrap_err()
            .to_string()
            .contains("unsupported value"));

        let mut invalid_property = valid;
        invalid_property["elements"]["card"]["props"]["className"] = json!("custom");
        assert!(validate_spec(&invalid_property)
            .unwrap_err()
            .to_string()
            .contains("not a supported property"));
    }

    #[test]
    fn first_json_render_open_removes_legacy_artifact_rows_and_their_studio_pins() {
        let profile = Profile::new();
        let analysis = profile.0.join("analysis");
        std::fs::create_dir_all(&analysis).unwrap();
        let database = analysis.join("artifacts.sqlite");
        let db = Connection::open(&database).unwrap();
        db.execute_batch(
            "CREATE TABLE artifacts (id TEXT PRIMARY KEY, workspace TEXT NOT NULL, body TEXT NOT NULL);
             CREATE TABLE studio_documents (id TEXT PRIMARY KEY, workspace TEXT NOT NULL, body TEXT NOT NULL);",
        ).unwrap();
        db.execute(
            "INSERT INTO artifacts(id, workspace, body) VALUES(?1, ?2, ?3)",
            params![
                "old-format-artifact",
                WORKSPACE,
                r#"{"legacy_payload":"discard on schema upgrade"}"#
            ],
        )
        .unwrap();
        let placement = StudioPlacement {
            id: "old-pin".into(),
            artifact_id: "old-format-artifact".into(),
            revision: 1,
            label: String::new(),
            width: StudioPlacementWidth::Full,
            view_state: Json::Object(Default::default()),
        };
        let document = StudioDocument {
            id: "saved-notes".into(),
            workspace: WORKSPACE.into(),
            title: "Notes".into(),
            version: 1,
            body: "Keep user-authored notes.".into(),
            placements: vec![placement.clone()],
            checkpoints: vec![StudioCheckpoint {
                id: "checkpoint".into(),
                title: "Before".into(),
                body: "Keep this note too.".into(),
                placements: vec![placement],
                created_at_ms: 1,
            }],
            created_at_ms: 1,
            updated_at_ms: 1,
        };
        db.execute(
            "INSERT INTO studio_documents(id, workspace, body) VALUES(?1, ?2, ?3)",
            params![
                document.id,
                WORKSPACE,
                serde_json::to_string(&document).unwrap()
            ],
        )
        .unwrap();
        drop(db);

        let migrated = store_unlocked(&profile.0).unwrap();
        let count: i64 = migrated
            .query_row("SELECT COUNT(*) FROM artifacts", [], |row| row.get(0))
            .unwrap();
        assert_eq!(
            count, 0,
            "legacy rendered artifacts are intentionally discarded"
        );
        drop(migrated);
        let retained = studio_load(&profile.0, WORKSPACE, "saved-notes").unwrap();
        assert_eq!(retained.body, "Keep user-authored notes.");
        assert!(retained.placements.is_empty());
        assert!(retained.checkpoints[0].placements.is_empty());
    }

    fn placement(artifact: &ArtifactRecord, rev: u64, id: &str) -> StudioPlacement {
        StudioPlacement {
            id: id.into(),
            artifact_id: artifact.id.clone(),
            revision: rev,
            label: String::new(),
            width: StudioPlacementWidth::Full,
            view_state: json!({"filter":"Q4"}),
        }
    }

    fn update(
        document: &StudioDocument,
        body: &str,
        placements: Vec<StudioPlacement>,
    ) -> StudioUpdate {
        StudioUpdate {
            title: document.title.clone(),
            body: body.into(),
            placements,
        }
    }

    #[test]
    fn artifact_resolves_exact_canonical_values_and_never_changes_the_run() {
        let profile = Profile::new();
        let source = seed(&profile, "run-1");
        let path = profile.0.join("analysis/turns/run-1.json");
        let original = std::fs::read(&path).unwrap();
        let item = create_from_run(
            &profile.0,
            WORKSPACE,
            &source.id,
            " Revenue ",
            &sample_spec(),
        )
        .unwrap();
        assert!(!item.saved);
        assert_eq!(item.revisions[0].schema_version, SCHEMA_VERSION);
        assert_eq!(item.title, "Revenue");
        let resolved = resolve(&profile.0, WORKSPACE, &item.id, None).unwrap();
        assert_eq!(resolved.run.question, source.question);
        assert_eq!(resolved.results.len(), 1);
        assert_eq!(resolved.results[0].exact_rows[0][1], "9007199254740993");
        assert_eq!(resolved.results[0].exact_rows[1][1], "100.25");
        assert_eq!(resolved.results[0].rows[0][1], json!(9007199254740993_u64));
        assert!(resolved.results[0].complete);
        assert_eq!(std::fs::read(path).unwrap(), original);
        assert_eq!(list(&profile.0, WORKSPACE).unwrap(), vec![item]);
    }

    #[test]
    fn presentation_storage_does_not_promote_run_status_or_remove_verification_caveats() {
        let profile = Profile::new();
        let mut source = run("limited-run");
        source.state = super::super::runtime::TurnState::NeedsReview;
        source.result.status = super::super::evidence::VerificationStatus::NeedsReview;
        source.result.verification = serde_json::from_value(json!([{
            "label":"Missing refund export","ok":false,"outcome":"flagged",
            "detail":"The saved result excludes returns not present in the source."
        }]))
        .unwrap();
        analysis_store::create(&profile.0, &source).unwrap();
        let artifact = artifact(&profile, &source.id);
        let resolved = resolve(&profile.0, WORKSPACE, &artifact.id, None).unwrap();
        assert_eq!(resolved.run.state, source.state);
        assert_eq!(resolved.run.result.status, source.result.status);
        assert_eq!(resolved.run.result.verification, source.result.verification);
        assert!(
            resolved.results[0].complete,
            "a complete saved computation is not a claim of complete source coverage"
        );
    }

    #[test]
    fn bindings_exclude_errors_withheld_tables_and_excluded_evidence_but_not_unrelated_charts() {
        let mut run = run("run-1");
        let base = run.result.evidence[0].clone();
        let mut excluded = base.clone();
        excluded["id"] = json!("excluded");
        excluded["verifier_disposition"] = json!({"state":"excluded","reason":"replay mismatch"});
        let mut failed = base.clone();
        failed["id"] = json!("failed");
        failed["error"] = json!("query failed");
        let mut withheld = base.clone();
        withheld["id"] = json!("withheld");
        withheld["verifier_disposition"] =
            json!({"state":"artifact_withheld","artifact":"result_table","reason":"bad table"});
        let mut chart_withheld = base.clone();
        chart_withheld["id"] = json!("chart-withheld");
        chart_withheld["verifier_disposition"] =
            json!({"state":"artifact_withheld","artifact":"chart","reason":"wrong labels"});
        run.result.evidence = vec![base, excluded, failed, withheld, chart_withheld];
        let results = eligible_results(&run).unwrap();
        assert_eq!(
            results.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
            vec!["evidence-1", "chart-withheld"]
        );
    }

    #[test]
    fn completeness_distinguishes_saved_tables_samples_capped_and_unknown_counts() {
        let mut run = run("run-1");
        run.result.evidence[0]["row_count"] = json!(200);
        let result = eligible_results(&run).unwrap().remove(0);
        assert!(!result.complete);
        assert_eq!(result.row_count, 200);
        run.result.evidence[0]["row_count"] = Json::Null;
        assert!(!eligible_results(&run).unwrap()[0].complete);
        run.result.evidence[0]["row_count"] = json!(2);
        run.result.evidence[0]["result_summary"] = json!("2 rows (capped)");
        assert!(!eligible_results(&run).unwrap()[0].complete);
        run.result.evidence = vec![json!({
            "id":"derived-table","tool":"run_python","result_summary":"python finished",
            "result_table":{"columns":["amount","missing"],"rows":[["10.0000",null]]}
        })];
        let result = eligible_results(&run).unwrap().remove(0);
        assert!(result.complete);
        assert_eq!(result.exact_rows, vec![vec!["10.0000", "null"]]);
    }

    #[test]
    fn matching_preview_row_counts_never_make_inspection_outputs_analytical_results() {
        let mut run = run("run-1");
        let analytical = run.result.evidence[0].clone();
        for tool in [
            "inspect_table",
            "sample_source",
            "make_chart",
            "lookup_prior_analysis",
        ] {
            let mut preview = analytical.clone();
            preview["id"] = json!(tool);
            preview["tool"] = json!(tool);
            run.result.evidence.push(preview);
        }
        let results = eligible_results(&run).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "evidence-1");
        assert!(results[0].complete);
    }

    #[test]
    fn malformed_tables_and_duplicate_evidence_ids_fail_without_repairing_values() {
        let mut run = run("run-1");
        run.result.evidence[0]["rows"] = json!([["Q3"]]);
        assert!(eligible_results(&run).is_err());
        let mut run = super::tests::run("run-1");
        run.result.evidence.push(run.result.evidence[0].clone());
        assert!(eligible_results(&run).is_err());
    }

    #[test]
    fn workspace_identity_and_recorded_source_scope_are_enforced_without_mounting() {
        let profile = Profile::new();
        seed(&profile, "run-1");
        let item = artifact(&profile, "run-1");
        assert!(create_from_run(&profile.0, "/other", "run-1", "Other", &sample_spec()).is_err());
        assert!(resolve(&profile.0, "/other", &item.id, None).is_err());
        assert!(list(&profile.0, "/other").unwrap().is_empty());
        assert!(resolve(&profile.0, WORKSPACE, "../escape", None).is_err());
        assert!(list(&profile.0, "general").is_err());
        assert!(list(&profile.0, " /not-mounted/artifact-fixture").is_err());
        let mut bad = run("bad-scope");
        bad.investigation_context = Some(
            serde_json::from_value(json!({
                "investigation_id":"investigation-1","definitions":[],
                "source_selection":{"mode":"selected_sources","included_paths":["allowed.csv"]}
            }))
            .unwrap(),
        );
        analysis_store::create(&profile.0, &bad).unwrap();
        assert!(
            create_from_run(&profile.0, WORKSPACE, &bad.id, "Bad scope", &sample_spec()).is_err()
        );
        bad.id = "bad-revision".into();
        bad.investigation_context = None;
        bad.trace.workspace_revision = Some("different-revision".into());
        analysis_store::create(&profile.0, &bad).unwrap();
        assert!(create_from_run(
            &profile.0,
            WORKSPACE,
            &bad.id,
            "Bad revision",
            &sample_spec()
        )
        .is_err());
    }

    #[test]
    fn candidates_are_immutable_and_adoption_is_optimistic_not_automatic() {
        let profile = Profile::new();
        seed(&profile, "run-1");
        seed(&profile, "run-2");
        let first = artifact(&profile, "run-1");
        let candidate = revise(
            &profile.0,
            WORKSPACE,
            &first.id,
            1,
            Some("run-2"),
            Some(sample_spec()),
            "Exclude refunds",
        )
        .unwrap();
        assert_eq!(candidate.current_revision, 1);
        assert_eq!(candidate.revisions[0], first.revisions[0]);
        assert_eq!(candidate.revisions[1].parent_revision, Some(1));
        assert_eq!(candidate.revisions[1].schema_version, SCHEMA_VERSION);
        let adopted = adopt(&profile.0, WORKSPACE, &first.id, 1, 2).unwrap();
        assert_eq!(adopted.current_revision, 2);
        assert!(adopt(&profile.0, WORKSPACE, &first.id, 1, 1).is_err());
        assert!(revise(&profile.0, WORKSPACE, &first.id, 1, None, None, "stale").is_err());
        assert_eq!(
            resolve(&profile.0, WORKSPACE, &first.id, Some(1))
                .unwrap()
                .run
                .id,
            "run-1"
        );
        assert_eq!(
            resolve(&profile.0, WORKSPACE, &first.id, None)
                .unwrap()
                .run
                .id,
            "run-2"
        );
        assert_eq!(
            load(&profile.0, WORKSPACE, &first.id).unwrap().revisions,
            candidate.revisions
        );
    }

    #[test]
    fn unsupported_legacy_presentation_revisions_are_rejected() {
        let profile = Profile::new();
        seed(&profile, "legacy-run");
        let mut unsupported = artifact(&profile, "legacy-run");
        unsupported.revisions[0].schema_version = "old-expression-runtime".into();
        assert!(validate_artifact(&unsupported).is_err());
    }

    #[test]
    fn studio_save_pins_ordered_revisions_and_retention_in_one_transaction() {
        let profile = Profile::new();
        seed(&profile, "run-1");
        seed(&profile, "run-2");
        let first = artifact(&profile, "run-1");
        let second = artifact(&profile, "run-2");
        let doc = studio_create(&profile.0, WORKSPACE, "Quarterly review").unwrap();
        let placements = vec![
            placement(&second, 1, "second"),
            placement(&first, 1, "first"),
        ];
        let saved = studio_save(
            &profile.0,
            WORKSPACE,
            &doc.id,
            1,
            update(&doc, "# Revenue", placements.clone()),
        )
        .unwrap();
        assert_eq!(saved.version, 2);
        assert_eq!(saved.placements, placements);
        assert!(load(&profile.0, WORKSPACE, &first.id).unwrap().saved);
        assert!(load(&profile.0, WORKSPACE, &second.id).unwrap().saved);
        assert_eq!(studio_load(&profile.0, WORKSPACE, &doc.id).unwrap(), saved);
        assert_eq!(
            studio_list(&profile.0, WORKSPACE).unwrap(),
            vec![saved.clone()]
        );
        assert!(studio_load(&profile.0, "/other", &doc.id).is_err());
        assert!(studio_save(
            &profile.0,
            WORKSPACE,
            &doc.id,
            1,
            update(&saved, "stale", vec![])
        )
        .is_err());
        assert_eq!(studio_load(&profile.0, WORKSPACE, &doc.id).unwrap(), saved);
    }

    #[test]
    fn invalid_studio_pins_do_not_partially_save_or_retain_artifacts() {
        let profile = Profile::new();
        seed(&profile, "run-1");
        let first = artifact(&profile, "run-1");
        let doc = studio_create(&profile.0, WORKSPACE, "Review").unwrap();
        let bad = vec![
            placement(&first, 1, "valid"),
            placement(&first, 99, "invalid"),
        ];
        assert!(studio_save(
            &profile.0,
            WORKSPACE,
            &doc.id,
            1,
            update(&doc, "should not save", bad)
        )
        .is_err());
        assert_eq!(studio_load(&profile.0, WORKSPACE, &doc.id).unwrap(), doc);
        assert!(!load(&profile.0, WORKSPACE, &first.id).unwrap().saved);
        let duplicate = vec![
            placement(&first, 1, "duplicate"),
            placement(&first, 1, "duplicate"),
        ];
        assert!(studio_save(
            &profile.0,
            WORKSPACE,
            &doc.id,
            1,
            update(&doc, "", duplicate)
        )
        .is_err());
        let mut foreign_run = run("foreign");
        foreign_run.workspace = Some("/other".into());
        analysis_store::create(&profile.0, &foreign_run).unwrap();
        let foreign =
            create_from_run(&profile.0, "/other", "foreign", "Other", &sample_spec()).unwrap();
        assert!(studio_save(
            &profile.0,
            WORKSPACE,
            &doc.id,
            1,
            update(&doc, "", vec![placement(&foreign, 1, "foreign")])
        )
        .is_err());
    }

    #[test]
    fn checkpoint_restore_pins_history_without_adopting_or_recomputing() {
        let profile = Profile::new();
        seed(&profile, "run-1");
        seed(&profile, "run-2");
        let first = artifact(&profile, "run-1");
        let doc = studio_create(&profile.0, WORKSPACE, "Review").unwrap();
        let old_placements = vec![placement(&first, 1, "result")];
        let doc = studio_save(
            &profile.0,
            WORKSPACE,
            &doc.id,
            1,
            update(&doc, "Original", old_placements.clone()),
        )
        .unwrap();
        let doc = studio_checkpoint(&profile.0, WORKSPACE, &doc.id, 2, "Before refunds").unwrap();
        revise(
            &profile.0,
            WORKSPACE,
            &first.id,
            1,
            Some("run-2"),
            None,
            "Refunds",
        )
        .unwrap();
        adopt(&profile.0, WORKSPACE, &first.id, 1, 2).unwrap();
        let doc = studio_save(
            &profile.0,
            WORKSPACE,
            &doc.id,
            3,
            update(&doc, "Changed", vec![placement(&first, 2, "result")]),
        )
        .unwrap();
        let restored =
            studio_restore(&profile.0, WORKSPACE, &doc.id, 4, &doc.checkpoints[0].id).unwrap();
        assert_eq!(restored.version, 5);
        assert_eq!(restored.body, "Original");
        assert_eq!(restored.placements, old_placements);
        assert_eq!(restored.checkpoints, doc.checkpoints);
        assert_eq!(
            load(&profile.0, WORKSPACE, &first.id)
                .unwrap()
                .current_revision,
            2
        );
        assert!(studio_restore(&profile.0, WORKSPACE, &doc.id, 4, &doc.checkpoints[0].id).is_err());
    }

    #[test]
    fn deleting_chat_retains_only_saved_artifact_support_including_candidate_history() {
        let profile = Profile::new();
        for id in ["run-1", "run-2", "run-3", "run-4"] {
            seed(&profile, id);
        }
        let saved = artifact(&profile, "run-1");
        revise(
            &profile.0,
            WORKSPACE,
            &saved.id,
            1,
            Some("run-2"),
            None,
            "Candidate",
        )
        .unwrap();
        let unsaved = artifact(&profile, "run-3");
        mark_saved(&profile.0, WORKSPACE, &saved.id).unwrap();
        assert_eq!(
            analysis_store::delete_conversation(&profile.0, "chat-1").unwrap(),
            2
        );
        assert!(analysis_store::exists(&profile.0, "run-1"));
        assert!(analysis_store::exists(&profile.0, "run-2"));
        assert!(!analysis_store::exists(&profile.0, "run-3"));
        assert!(!analysis_store::exists(&profile.0, "run-4"));
        assert!(resolve(&profile.0, WORKSPACE, &saved.id, Some(2)).is_ok());
        assert!(resolve(&profile.0, WORKSPACE, &unsaved.id, None).is_err());
        assert!(mark_saved(&profile.0, WORKSPACE, &unsaved.id).is_err());
    }

    #[test]
    fn studio_checkpoint_support_survives_chat_deletion_after_removing_placements() {
        let profile = Profile::new();
        seed(&profile, "run-1");
        let first = artifact(&profile, "run-1");
        let doc = studio_create(&profile.0, WORKSPACE, "Review").unwrap();
        let doc = studio_save(
            &profile.0,
            WORKSPACE,
            &doc.id,
            1,
            update(&doc, "saved", vec![placement(&first, 1, "plot")]),
        )
        .unwrap();
        let doc = studio_checkpoint(&profile.0, WORKSPACE, &doc.id, 2, "Original").unwrap();
        let doc = studio_save(
            &profile.0,
            WORKSPACE,
            &doc.id,
            3,
            update(&doc, "empty", vec![]),
        )
        .unwrap();
        assert_eq!(
            analysis_store::delete_conversation(&profile.0, "chat-1").unwrap(),
            0
        );
        let restored =
            studio_restore(&profile.0, WORKSPACE, &doc.id, 4, &doc.checkpoints[0].id).unwrap();
        assert_eq!(restored.placements.len(), 1);
    }

    #[test]
    fn corrupt_retention_metadata_fails_closed_before_any_run_is_deleted() {
        let profile = Profile::new();
        seed(&profile, "run-1");
        let saved = artifact(&profile, "run-1");
        mark_saved(&profile.0, WORKSPACE, &saved.id).unwrap();
        let db = store_unlocked(&profile.0).unwrap();
        db.execute(
            "UPDATE artifacts SET body='not JSON' WHERE id=?1",
            [&saved.id],
        )
        .unwrap();
        drop(db);
        assert!(analysis_store::delete_conversation(&profile.0, "chat-1").is_err());
        assert!(analysis_store::exists(&profile.0, "run-1"));
    }

    #[test]
    fn competing_studio_saves_reject_stale_version_instead_of_overwriting() {
        let profile = Profile::new();
        let doc = studio_create(&profile.0, WORKSPACE, "Concurrent").unwrap();
        let barrier = Arc::new(Barrier::new(3));
        let handles: Vec<_> = ["first", "second"]
            .into_iter()
            .map(|body| {
                let profile = profile.0.clone();
                let doc = doc.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    studio_save(&profile, WORKSPACE, &doc.id, 1, update(&doc, body, vec![]))
                })
            })
            .collect();
        barrier.wait();
        let outcomes: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        assert_eq!(outcomes.iter().filter(|r| r.is_ok()).count(), 1);
        assert_eq!(outcomes.iter().filter(|r| r.is_err()).count(), 1);
        assert_eq!(
            studio_load(&profile.0, WORKSPACE, &doc.id).unwrap().version,
            2
        );
    }

    #[test]
    fn saving_and_deleting_concurrently_never_creates_a_saved_dangling_reference() {
        let profile = Profile::new();
        seed(&profile, "run-1");
        let item = artifact(&profile, "run-1");
        let barrier = Arc::new(Barrier::new(3));
        let save_profile = profile.0.clone();
        let id = item.id.clone();
        let save_barrier = barrier.clone();
        let save = std::thread::spawn(move || {
            save_barrier.wait();
            mark_saved(&save_profile, WORKSPACE, &id)
        });
        let delete_profile = profile.0.clone();
        let delete_barrier = barrier.clone();
        let delete = std::thread::spawn(move || {
            delete_barrier.wait();
            analysis_store::delete_conversation(&delete_profile, "chat-1")
        });
        barrier.wait();
        let saved = save.join().unwrap();
        let deleted = delete.join().unwrap().unwrap();
        if saved.is_ok() {
            assert_eq!(deleted, 0);
            assert!(resolve(&profile.0, WORKSPACE, &item.id, None).is_ok());
        } else {
            assert_eq!(deleted, 1);
            assert!(!load(&profile.0, WORKSPACE, &item.id).unwrap().saved);
        }
    }

    #[test]
    fn oversized_or_invalid_metadata_cannot_change_the_document_or_artifact() {
        let profile = Profile::new();
        seed(&profile, "run-1");
        assert!(create_from_run(&profile.0, WORKSPACE, "run-1", "", &sample_spec()).is_err());
        assert!(create_from_run(
            &profile.0,
            WORKSPACE,
            "run-1",
            "Title",
            &serde_json::json!({"root":"x","elements":{},"padding":"x".repeat(MAX_SPEC_BYTES + 1)})
        )
        .is_err());
        let doc = studio_create(&profile.0, WORKSPACE, "Review").unwrap();
        assert!(studio_save(
            &profile.0,
            WORKSPACE,
            &doc.id,
            1,
            update(&doc, &"x".repeat(MAX_BODY_BYTES + 1), vec![])
        )
        .is_err());
        assert_eq!(studio_load(&profile.0, WORKSPACE, &doc.id).unwrap(), doc);
    }
}
