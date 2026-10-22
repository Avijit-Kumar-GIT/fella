//! Workspace-owned analytical outputs and ordered Studio documents.
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
use super::runtime::AnalysisTurn;

/// Version assigned to newly-created or newly-revised presentation artifacts.
pub const LIBRARY_VERSION: &str = "fella-openui-v2";
/// Historical version retained on existing revisions and portable snapshots.
pub const LEGACY_LIBRARY_VERSION: &str = "fella-openui-v1";

/// Whether this build can preserve and load a stored presentation revision.
/// This does not imply that new revisions may be written using that version.
pub fn supports_library_version(version: &str) -> bool {
    matches!(version, LIBRARY_VERSION | LEGACY_LIBRARY_VERSION)
}
const MAX_RECORD_BYTES: usize = 4 * 1024 * 1024;
const MAX_RUN_BYTES: u64 = 64 * 1024 * 1024;
const MAX_PROGRAM_BYTES: usize = 256 * 1024;
const MAX_BODY_BYTES: usize = 512 * 1024;
const MAX_REVISIONS: usize = 256;
const MAX_PLACEMENTS: usize = 256;
const MAX_CHECKPOINTS: usize = 64;
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArtifactRevision {
    pub revision: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_revision: Option<u64>,
    pub run_id: String,
    pub program: String,
    pub library_version: String,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StudioPlacement {
    pub id: String,
    pub artifact_id: String,
    pub revision: u64,
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

fn validate_program(program: &str) -> EngineResult<()> {
    // This is storage validation, not executable-program validation. The
    // application renderer still validates its component/binding allowlist.
    if program.len() > MAX_PROGRAM_BYTES || program.contains('\0') {
        return Err(EngineError::msg(
            "presentation program exceeds its storage limit",
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
         CREATE INDEX IF NOT EXISTS studio_workspace ON studio_documents(workspace);",
    )?;
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
        validate_program(&revision.program)?;
        if revision.revision <= previous
            || !seen.insert(revision.revision)
            || !supports_library_version(&revision.library_version)
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
                        Some("table" | "result_table" | "openui" | "presentation")
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
    program: &str,
) -> EngineResult<ArtifactRecord> {
    validate_workspace(workspace)?;
    let title = validate_title(title)?;
    validate_program(program)?;
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
            program: program.to_owned(),
            library_version: LIBRARY_VERSION.to_owned(),
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
    let revision = revision(&artifact, number.unwrap_or(artifact.current_revision))?.clone();
    let run = load_run(data_dir, workspace, &revision.run_id)?;
    let results = eligible_results(&run)?;
    Ok(ResolvedArtifact {
        artifact,
        revision,
        run,
        results,
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
    program: Option<&str>,
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
    let program = program.unwrap_or(&base.program);
    validate_program(program)?;
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
        program: program.to_owned(),
        library_version: LIBRARY_VERSION.to_owned(),
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
            || !placement.view_state.is_object()
            || serde_json::to_vec(&placement.view_state)?.len() > 16 * 1024
        {
            return Err(EngineError::msg("invalid Studio placement or view state"));
        }
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
    put_studio_unlocked(&store_unlocked(data_dir)?, &document)?;
    Ok(document)
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
    next_version(&mut document, base_version)?;
    document.title = validate_title(&update.title)?;
    document.body = update.body;
    document.placements = update.placements;
    validate_studio(&document)?;
    // Resolve every pin before updating anything. Retention writes and document
    // placement changes then commit atomically, even across an application crash.
    let mut artifact_ids = HashSet::new();
    for placement in &document.placements {
        resolve_unlocked(
            data_dir,
            &tx,
            workspace,
            &placement.artifact_id,
            Some(placement.revision),
        )?;
        artifact_ids.insert(placement.artifact_id.clone());
    }
    for id in artifact_ids {
        let mut artifact = load_artifact_unlocked(&tx, workspace, &id)?;
        for revision in &artifact.revisions {
            load_run(data_dir, workspace, &revision.run_id)?;
        }
        artifact.saved = true;
        artifact.updated_at_ms = now_ms();
        put_artifact_unlocked(&tx, &artifact)?;
    }
    put_studio_unlocked(&tx, &document)?;
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
    let db = store_unlocked(data_dir)?;
    let mut document = load_studio_unlocked(&db, workspace, document_id)?;
    next_version(&mut document, base_version)?;
    document.checkpoints.push(StudioCheckpoint {
        id: new_id("checkpoint")?,
        title,
        body: document.body.clone(),
        placements: document.placements.clone(),
        created_at_ms: now_ms(),
    });
    put_studio_unlocked(&db, &document)?;
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
    let db = store_unlocked(data_dir)?;
    let mut document = load_studio_unlocked(&db, workspace, document_id)?;
    next_version(&mut document, base_version)?;
    let checkpoint = document
        .checkpoints
        .iter()
        .find(|c| c.id == checkpoint_id)
        .ok_or_else(|| EngineError::msg("Studio checkpoint does not exist"))?
        .clone();
    for placement in &checkpoint.placements {
        resolve_unlocked(
            data_dir,
            &db,
            workspace,
            &placement.artifact_id,
            Some(placement.revision),
        )?;
    }
    // Restoring moves presentation references only; it does not adopt artifact
    // revisions, recompute analysis, or erase the available checkpoint history.
    document.body = checkpoint.body;
    document.placements = checkpoint.placements;
    put_studio_unlocked(&db, &document)?;
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
        create_from_run(&profile.0, WORKSPACE, run, "Revenue", "root = Report()").unwrap()
    }

    fn placement(artifact: &ArtifactRecord, rev: u64, id: &str) -> StudioPlacement {
        StudioPlacement {
            id: id.into(),
            artifact_id: artifact.id.clone(),
            revision: rev,
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
            "Metric(999999)",
        )
        .unwrap();
        assert!(!item.saved);
        assert_eq!(item.revisions[0].library_version, "fella-openui-v2");
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
        assert!(create_from_run(&profile.0, "/other", "run-1", "Other", "").is_err());
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
        assert!(create_from_run(&profile.0, WORKSPACE, &bad.id, "Bad scope", "").is_err());
        bad.id = "bad-revision".into();
        bad.investigation_context = None;
        bad.trace.workspace_revision = Some("different-revision".into());
        analysis_store::create(&profile.0, &bad).unwrap();
        assert!(create_from_run(&profile.0, WORKSPACE, &bad.id, "Bad revision", "").is_err());
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
            Some("NewView()"),
            "Exclude refunds",
        )
        .unwrap();
        assert_eq!(candidate.current_revision, 1);
        assert_eq!(candidate.revisions[0], first.revisions[0]);
        assert_eq!(candidate.revisions[1].parent_revision, Some(1));
        assert_eq!(candidate.revisions[1].library_version, "fella-openui-v2");
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
    fn legacy_artifact_revisions_load_and_new_revisions_use_current_library_version() {
        let profile = Profile::new();
        seed(&profile, "legacy-run");
        seed(&profile, "next-run");
        let mut legacy = artifact(&profile, "legacy-run");
        legacy.revisions[0].library_version = LEGACY_LIBRARY_VERSION.to_owned();

        // Seed a serialized v1 record to exercise the normal load path.
        let db = store_unlocked(&profile.0).unwrap();
        put_artifact_unlocked(&db, &legacy).unwrap();
        drop(db);
        let loaded = load(&profile.0, WORKSPACE, &legacy.id).unwrap();
        assert_eq!(loaded.revisions[0].library_version, LEGACY_LIBRARY_VERSION);

        // Preserve the old revision byte-for-byte while ensuring all newly
        // authored revisions use the current v2 library.
        let revised = revise(
            &profile.0,
            WORKSPACE,
            &legacy.id,
            1,
            Some("next-run"),
            Some("root = Report()"),
            "Refresh presentation",
        )
        .unwrap();
        assert_eq!(revised.revisions[0], loaded.revisions[0]);
        assert_eq!(revised.revisions[1].library_version, LIBRARY_VERSION);
        assert_eq!(load(&profile.0, WORKSPACE, &legacy.id).unwrap(), revised);

        let mut unsupported = revised;
        unsupported.revisions[0].library_version = "fella-openui-v3".into();
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
        let foreign = create_from_run(&profile.0, "/other", "foreign", "Other", "").unwrap();
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
        assert!(create_from_run(&profile.0, WORKSPACE, "run-1", "", "").is_err());
        assert!(create_from_run(
            &profile.0,
            WORKSPACE,
            "run-1",
            "Title",
            &"x".repeat(MAX_PROGRAM_BYTES + 1)
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
