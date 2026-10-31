//! `EngineState` the shared runtime object every command handler borrows.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Instant;

use serde::Serialize;

use crate::engine::agent;
use crate::engine::analysis_store;
use crate::engine::analytics::data::{self, DataEngine, DEFAULT_ROW_CAP};
use crate::engine::analytics::pyexec;
use crate::engine::catalog::{self, Catalog, ColumnInfo, SourceInfo, SourceKind};
use crate::engine::context::{ContextAssembler, ContextAssemblyAudit, ContextPacket};
use crate::engine::error::{EngineError, EngineResult};
use crate::engine::evidence::{
    Answer, AskEvent, EvidenceItem, VerificationCheck, VerificationStatus,
};
use crate::engine::ingest::docs;
use crate::engine::investigation::{
    self, Investigation, InvestigationBrief, InvestigationContextSnapshot, InvestigationDefinition,
    InvestigationDefinitionInput, InvestigationFinding, InvestigationFindingInput,
    InvestigationFindingUpdate, InvestigationQuestion, InvestigationQuestionAnswerStatus,
    InvestigationQuestionInput, InvestigationSourceSelection, QuestionBreakdownProposal,
};
use crate::engine::llm::{LlmClient, ProviderHealth};
use crate::engine::memory::{self, FolderMemory};
use crate::engine::provider::{self, AuthKind, PROVIDERS};
use crate::engine::runtime::{
    AnalysisContract, AnalysisResult, AnalysisSourceScope, AnalysisTurn, AnalysisTurnReplayStatus,
    ClarificationReply, ClarificationRequest, ContextReference, InterpretationStatus, LogicalPlan,
    PlanStrategy, ResolvedClarification, RunLogEntry, RunLogKind, TurnState, VerificationReport,
    WorkspaceColumnSnapshot, WorkspaceRevisionSnapshot, WorkspaceSourceSnapshot,
};
use crate::engine::secrets::Secrets;
use crate::engine::semantic_memory::{
    self, FactAuthority, FactKind, SemanticFact, SemanticFactInput, SemanticMemory,
};
use crate::engine::sqlite::{self, Settings};
use crate::engine::tools::Registry;
use crate::engine::workspace_model::WorkspaceModel;

const MAX_ANALYSIS_TURN_LINKS_PER_UPDATE: usize = 500;
/// Shared by the app-level Investigation readers and workspace-scoped ask
/// runtimes so a live Question run is not mistaken for a process interruption.
pub(crate) type CancelRegistry = Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>;

// Full-table common-value/null/min/max statistics are useful on tiny mounts,
// but not worth rescanning many or large tables before the first question.
// Larger tables keep their full rows and inferred types; inspect_table calls
// describe_source to produce exact statistics lazily when one matters.
const EAGER_PROFILE_TOTAL_SOURCE_BYTES: u64 = 1024 * 1024;
const EAGER_PROFILE_MAX_ROWS: i64 = 10_000;

/// Optional controls for one model-driven analysis turn.
#[derive(Default)]
pub struct AskOptions<'a> {
    pub model: Option<&'a str>,
    pub inspect: bool,
    pub context_refs: &'a [ContextReference],
    pub clarification_reply: Option<ClarificationReply>,
    /// Resolve the current saved context from this canonical Investigation.
    /// Used for ordinary desktop asks; the engine snapshots active revisions.
    pub investigation_id: Option<&'a str>,
    /// Optional durable Question identity for a Question-specific run.
    pub question_id: Option<&'a str>,
    /// Immutable context loaded from an existing turn for a faithful rerun or
    /// clarification continuation. It is not refreshed from current records.
    pub investigation_context: Option<InvestigationContextSnapshot>,
    /// Explicit parent for replayed attempts. Question runs otherwise capture
    /// the latest saved attempt as their parent.
    pub rerun_of: Option<&'a str>,
}

struct PersistAnalysisTurn<'a> {
    conversation_id: &'a str,
    question: &'a str,
    context_refs: &'a [ContextReference],
    investigation_context: Option<&'a InvestigationContextSnapshot>,
    clarification: Option<&'a ResolvedClarification>,
    context_audit: &'a ContextAssemblyAudit,
    catalog: &'a Catalog,
    answer: &'a Answer,
    rerun_of: Option<&'a str>,
}

fn question_answer_status(
    answer: &Answer,
    cancelled: bool,
) -> (InvestigationQuestionAnswerStatus, Option<String>) {
    question_answer_status_parts(
        answer.status,
        answer.contract.as_ref(),
        &answer.verification,
        answer.clarification.as_ref(),
        cancelled,
    )
}

fn question_answer_status_from_turn(
    turn: &AnalysisTurn,
) -> (InvestigationQuestionAnswerStatus, Option<String>) {
    question_answer_status_parts(
        turn.result.status,
        turn.contract.as_ref(),
        &turn.result.verification,
        turn.contract
            .as_ref()
            .and_then(|contract| contract.clarification.as_ref()),
        turn.state == TurnState::Cancelled,
    )
}

fn question_answer_status_parts(
    status: VerificationStatus,
    contract: Option<&AnalysisContract>,
    verification: &[VerificationCheck],
    clarification: Option<&ClarificationRequest>,
    cancelled: bool,
) -> (InvestigationQuestionAnswerStatus, Option<String>) {
    if cancelled {
        return (
            InvestigationQuestionAnswerStatus::Blocked,
            Some("The run was stopped before a complete answer was verified.".to_owned()),
        );
    }
    if contract.is_some_and(|contract| contract.interpretation == InterpretationStatus::Unsupported)
    {
        let reason = contract
            .and_then(|contract| contract.unresolved.first())
            .map(String::as_str)
            .filter(|reason| !reason.trim().is_empty())
            .unwrap_or("The available data or analysis tools do not support this Question.");
        return (
            InvestigationQuestionAnswerStatus::Unsupported,
            Some(compact_status_reason(reason)),
        );
    }
    if let Some(clarification) = clarification {
        let reason = clarification
            .reason
            .as_deref()
            .filter(|reason| !reason.trim().is_empty())
            .unwrap_or(&clarification.question);
        return (
            InvestigationQuestionAnswerStatus::WaitingForInput,
            Some(compact_status_reason(reason)),
        );
    }

    let check_reason = verification
        .iter()
        .find(|check| !check.ok)
        .and_then(|check| check.detail.as_deref())
        .filter(|reason| !reason.trim().is_empty());
    let unresolved_reason = contract
        .and_then(|contract| contract.unresolved.first())
        .map(String::as_str)
        .filter(|reason| !reason.trim().is_empty());
    match status {
        VerificationStatus::Verified => (InvestigationQuestionAnswerStatus::Answered, None),
        VerificationStatus::NeedsReview => (
            InvestigationQuestionAnswerStatus::NeedsReview,
            Some(compact_status_reason(
                check_reason.or(unresolved_reason).unwrap_or(
                    "The run produced an answer, but its verification is not conclusive.",
                ),
            )),
        ),
        VerificationStatus::InsufficientData => (
            InvestigationQuestionAnswerStatus::InsufficientData,
            Some(compact_status_reason(
                check_reason.or(unresolved_reason).unwrap_or(
                    "The run did not produce enough accepted evidence to answer this Question.",
                ),
            )),
        ),
        VerificationStatus::Failed => (
            InvestigationQuestionAnswerStatus::Blocked,
            Some(compact_status_reason(
                check_reason
                    .or(unresolved_reason)
                    .unwrap_or("The analysis could not be verified."),
            )),
        ),
    }
}

fn compact_status_reason(reason: &str) -> String {
    reason.trim().chars().take(240).collect()
}

#[derive(Default)]
struct WorkspaceGateState {
    readers: usize,
    writer: bool,
    waiting_writers: usize,
}

/// Keep each analysis pinned to one workspace revision while allowing a new
/// mount to prepare privately. Publication waits for active analyses to finish,
/// then briefly prevents a new turn from starting during the atomic swap.
#[derive(Default)]
struct WorkspaceGate {
    state: Mutex<WorkspaceGateState>,
    changed: Condvar,
}

struct WorkspaceReadPermit(Arc<WorkspaceGate>);
struct WorkspaceWritePermit(Arc<WorkspaceGate>);

impl WorkspaceGate {
    fn read(self: &Arc<Self>) -> WorkspaceReadPermit {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        while state.writer || state.waiting_writers > 0 {
            state = self.changed.wait(state).unwrap_or_else(|e| e.into_inner());
        }
        state.readers += 1;
        WorkspaceReadPermit(Arc::clone(self))
    }

    fn write(self: &Arc<Self>) -> WorkspaceWritePermit {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.waiting_writers += 1;
        while state.writer || state.readers > 0 {
            state = self.changed.wait(state).unwrap_or_else(|e| e.into_inner());
        }
        state.waiting_writers -= 1;
        state.writer = true;
        WorkspaceWritePermit(Arc::clone(self))
    }
}

impl Drop for WorkspaceReadPermit {
    fn drop(&mut self) {
        let mut state = self.0.state.lock().unwrap_or_else(|e| e.into_inner());
        state.readers = state.readers.saturating_sub(1);
        self.0.changed.notify_all();
    }
}

impl Drop for WorkspaceWritePermit {
    fn drop(&mut self) {
        let mut state = self.0.state.lock().unwrap_or_else(|e| e.into_inner());
        state.writer = false;
        self.0.changed.notify_all();
    }
}

#[cfg(test)]
mod workspace_gate_tests {
    use super::WorkspaceGate;
    use std::sync::{mpsc, Arc};
    use std::thread;
    use std::time::{Duration, Instant};

    #[test]
    fn mount_publication_waits_for_active_analysis_permits() {
        let gate = Arc::new(WorkspaceGate::default());
        let analysis = gate.read();
        let writer_gate = Arc::clone(&gate);
        let (published, published_rx) = mpsc::channel();
        let writer = thread::spawn(move || {
            let _publication = writer_gate.write();
            published.send(()).unwrap();
        });

        let deadline = Instant::now() + Duration::from_secs(1);
        loop {
            let waiting = gate
                .state
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .waiting_writers
                > 0;
            if waiting {
                break;
            }
            assert!(Instant::now() < deadline, "mount writer never queued");
            thread::yield_now();
        }
        assert!(published_rx.try_recv().is_err());

        drop(analysis);
        published_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("mount publishes after the running analysis releases its snapshot");
        writer.join().unwrap();
    }
}

pub struct EngineState {
    workspace: Mutex<WorkspaceState>,
    workspace_gate: Arc<WorkspaceGate>,
    mount_serial: Mutex<()>,
    /// Serialize resolve-or-create so two first-turn requests cannot race
    /// into duplicate durable Investigations for one source message.
    investigation_ensure_lock: Mutex<()>,
    /// Serialize Question status, plan, definition, and scope updates so a
    /// finishing run cannot overwrite a concurrent analyst edit.
    question_state_lock: Mutex<()>,
    sqlite: Mutex<rusqlite::Connection>,
    inner: Mutex<Inner>,
    http: reqwest::Client,
    /// API keys / tokens, keyed by provider. Kept out of the SQLite file.
    secrets: Secrets,
    /// The app data dir. Home to `fella.db`, `auth.json`, and archived
    /// conversations under `conversations/`.
    data_dir: PathBuf,
    /// One stop-flag per in-flight conversation, so `/stop` in one tab doesn't
    /// cancel another tab's run. Keyed by `conversation_id`; an entry is created
    /// when `ask` starts and removed when it finishes.
    cancel: CancelRegistry,
    /// Conversation IDs deleted during this process lifetime. Prevents a late
    /// frontend archive or in-flight analysis save from recreating deleted data.
    deleted_conversations: Mutex<HashSet<String>>,
    /// Extracted PDF text, keyed by path. PDF parsing is slow and `grep_files` /
    /// `read_file` can each hit the same file many times in one run. Entries are
    /// invalidated when the file's mtime changes. Text files aren't cached they
    /// stream.
    doc_cache: Mutex<HashMap<String, (u64, Arc<str>)>>,
}

/// Where past conversations are archived, and how many are there.
fn revision_snapshot(catalog: &Catalog) -> Option<WorkspaceRevisionSnapshot> {
    Some(WorkspaceRevisionSnapshot {
        path: catalog.workspace.clone()?,
        revision: catalog.revision.clone()?,
        sources: catalog
            .sources
            .iter()
            .map(|source| WorkspaceSourceSnapshot {
                name: source.name.clone(),
                view: source.view.clone(),
                kind: serde_json::to_value(source.kind)
                    .ok()
                    .and_then(|value| value.as_str().map(str::to_owned))
                    .unwrap_or_else(|| format!("{:?}", source.kind).to_ascii_lowercase()),
                row_count: source.row_count,
                columns: source
                    .columns
                    .as_deref()
                    .unwrap_or_default()
                    .iter()
                    .map(|column| WorkspaceColumnSnapshot {
                        name: column.name.clone(),
                        type_: column.type_.clone(),
                    })
                    .collect(),
                size_bytes: source.size_bytes,
                mtime: source.mtime,
            })
            .collect(),
        skipped: catalog
            .skipped
            .iter()
            .map(|file| format!("{}: {}", file.name, file.reason))
            .collect(),
    })
}

/// Add mount-time statistics only for small workspaces. On larger mounts the
/// model gets the inferred schema immediately and can request these full-table
/// distributions through `describe_source` when a table is relevant. This
/// avoids repeatedly scanning every newly ingested table just to prepare a
/// catalog the model will not fully consume.
fn profile_loaded_columns(
    data: &dyn DataEngine,
    view: &str,
    ingest_columns: Vec<ColumnInfo>,
) -> Vec<ColumnInfo> {
    let mut profiled = match data.describe(view) {
        Ok(columns) => columns,
        Err(_) => return ingest_columns,
    };
    for column in &mut profiled {
        if column.note.is_none() {
            column.note = ingest_columns
                .iter()
                .find(|ingested| ingested.name == column.name)
                .and_then(|ingested| ingested.note.clone());
        }
    }
    profiled
}

#[derive(Debug, Serialize)]
pub struct ConversationsInfo {
    pub path: String,
    pub count: usize,
}

/// One row of `/history`'s list enough to recognize and pick a past
/// conversation without knowing its id.
#[derive(Debug, Serialize)]
pub struct ConversationSummary {
    pub id: String,
    pub saved_at_ms: i64,
    pub workspace: Option<String>,
    /// ID of the first real user message, used to upgrade older Investigation links.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_question_message_id: Option<String>,
    pub preview: String,
    pub message_count: usize,
    /// A user-given name, if this conversation was renamed. Absent for the
    /// common case, where the UI derives a label from the folder + preview.
    pub title: Option<String>,
}

fn is_actual_question_message(message: &serde_json::Value) -> bool {
    message.get("role").and_then(|role| role.as_str()) == Some("user")
        && message
            .get("text")
            .and_then(|text| text.as_str())
            .map(|text| !text.trim().is_empty() && !text.trim_start().starts_with('/'))
            .unwrap_or(false)
}

fn has_actual_question(value: &serde_json::Value) -> bool {
    value
        .get("messages")
        .and_then(|messages| messages.as_array())
        .map(|messages| messages.iter().any(is_actual_question_message))
        .unwrap_or(false)
}

/// Recover the conversation's original repository from the first completed
/// answer. Older UI archives were rewritten with the currently mounted folder,
/// so their top-level `workspace` can describe a later repository instead of
/// where the conversation began. `None` means the transcript has no usable
/// answer metadata; `Some(None)` is a known no-repository origin.
fn archived_conversation_workspace(value: &serde_json::Value) -> Option<Option<String>> {
    let messages = value.get("messages")?.as_array()?;
    for (index, message) in messages.iter().enumerate() {
        if !is_actual_question_message(message) {
            continue;
        }
        let assistant = messages[index + 1..]
            .iter()
            .take_while(|candidate| !is_actual_question_message(candidate))
            .find(|candidate| {
                candidate.get("role").and_then(|role| role.as_str()) == Some("assistant")
                    && candidate
                        .get("answer")
                        .is_some_and(serde_json::Value::is_object)
            });
        let Some(answer) = assistant.and_then(|message| message.get("answer")) else {
            continue;
        };
        if let Some(workspace) = answer.get("workspace") {
            if workspace.is_null() {
                return Some(None);
            }
            if let Some(path) = workspace.get("path").and_then(|path| path.as_str()) {
                return Some(Some(path.to_string()));
            }
        }
        if let Some(trace) = answer.get("trace").filter(|trace| trace.is_object()) {
            if trace
                .get("workspace_revision")
                .is_some_and(serde_json::Value::is_null)
            {
                return Some(None);
            }
            if trace
                .get("workspace_revision")
                .and_then(|revision| revision.as_str())
                .is_some()
            {
                // A mounted answer from an older schema may not carry its path;
                // leave the archive's existing value as the best available hint.
                return None;
            }
            if trace.get("turn_id").is_some() || trace.get("id").is_some() {
                return Some(None);
            }
        }
    }
    None
}

fn conversational_message_count(messages: &[serde_json::Value]) -> usize {
    messages
        .iter()
        .filter(|message| {
            message.get("role").and_then(|role| role.as_str()) == Some("assistant")
                || is_actual_question_message(message)
        })
        .count()
}

/// One row of the provider list shown by `/login` and `/auth`.
#[derive(Debug, Serialize)]
pub struct ProviderInfo {
    pub id: String,
    pub display: String,
    /// `"key"` all model providers are BYOK.
    pub auth: String,
    pub base_url: String,
    pub get_key_url: String,
    pub embeddings: bool,
    /// A credential is present for this provider.
    pub authed: bool,
    /// This is the currently-selected provider.
    pub current: bool,
}

#[derive(Default)]
struct Inner {
    /// Distilled context from earlier questions, one entry per conversation
    /// (tab). Bounded by `SESSION_CAP`, LRU by `last_used`. Each turn carries
    /// its workspace snapshot so conversational context can survive a mount
    /// change without reusing stale analytical hints. An entry is dropped when
    /// its tab is closed (`forget_conversation`).
    sessions: HashMap<String, SessionMemory>,
    /// Monotonic counter stamped onto `SessionMemory::last_used` so the least
    /// recently asked conversation can be evicted when `sessions` is full.
    session_tick: u64,
}

struct WorkspaceState {
    data: Box<dyn DataEngine>,
    /// Temporary directory for this backend's scratch database. DuckDB uses
    /// memory here, but keeping the directory in the state gives SQLite a
    /// private file that can be built without touching the active engine.
    scratch: Option<WorkspaceScratch>,
    workspace: Option<PathBuf>,
    revision: Option<String>,
    indexed_at_ms: Option<i64>,
    sources: Vec<SourceInfo>,
    /// Contents of `fella.md` at the workspace root, if present. User-written
    /// context fed to the system prompt.
    user_md: Option<String>,
    /// Rendered `schema_block()` for the current sources - it re-samples every
    /// table, so we build it once per workspace and clear it on (re)open or
    /// when `describe_source` refreshes a table's stats.
    schema_cache: Option<String>,
    /// Lowercased `view` names `describe_source` has computed stats for this
    /// workspace session. On a large folder (schema_block's `full` tier, more
    /// than the small-folder cutoff) a table's sample rows only enter the
    /// digest once it's in this set - just-in-time instead of eagerly
    /// sampling every table upfront. Cleared on workspace (re)open, same as
    /// `schema_cache`.
    inspected_tables: HashSet<String>,
    /// Files the last scan/ingest noticed but couldn't load.
    skipped: Vec<catalog::SkippedFile>,
    /// `<data_dir>/memory/<key>.md` for the open workspace (per-folder learned
    /// notes). `None` with no workspace. The file itself is the source of truth
    /// re-read each turn so a hand edit takes effect immediately.
    memory_path: Option<PathBuf>,
    /// Revision-bound semantic projection of the catalog. It is rebuilt only
    /// when the workspace snapshot changes or a source is enriched.
    model: Option<WorkspaceModel>,
}

impl WorkspaceState {
    fn new(data: Box<dyn DataEngine>, scratch: Option<WorkspaceScratch>) -> Self {
        Self {
            data,
            scratch,
            workspace: None,
            revision: None,
            indexed_at_ms: None,
            sources: Vec::new(),
            user_md: None,
            schema_cache: None,
            inspected_tables: HashSet::new(),
            skipped: Vec::new(),
            memory_path: None,
            model: None,
        }
    }
}

fn require_capability(enabled: bool, label: &str) -> EngineResult<()> {
    if enabled {
        Ok(())
    } else {
        Err(EngineError::msg(format!(
            "{label} is disabled in Settings under Experimental analysis capabilities."
        )))
    }
}

struct WorkspaceScratch {
    path: PathBuf,
    users: Arc<AtomicUsize>,
    owner: Arc<AtomicBool>,
}

impl WorkspaceScratch {
    fn new(path: PathBuf) -> Self {
        Self {
            path,
            users: Arc::new(AtomicUsize::new(0)),
            owner: Arc::new(AtomicBool::new(true)),
        }
    }

    fn lease(&self) -> ScratchLease {
        self.users.fetch_add(1, Ordering::AcqRel);
        ScratchLease {
            path: self.path.clone(),
            users: Arc::clone(&self.users),
            owner: Arc::clone(&self.owner),
        }
    }

    fn cleanup(&self) {
        self.owner.store(false, Ordering::Release);
        cleanup_scratch(&self.path, &self.users, &self.owner);
    }
}

impl Drop for WorkspaceScratch {
    fn drop(&mut self) {
        self.cleanup();
    }
}

struct ScratchLease {
    path: PathBuf,
    users: Arc<AtomicUsize>,
    owner: Arc<AtomicBool>,
}

impl Drop for ScratchLease {
    fn drop(&mut self) {
        self.users.fetch_sub(1, Ordering::AcqRel);
        cleanup_scratch(&self.path, &self.users, &self.owner);
    }
}

fn cleanup_scratch(path: &Path, users: &AtomicUsize, owner: &AtomicBool) {
    // The active workspace owns its scratch directory. A Python lease only
    // keeps a replaced workspace alive long enough for its in-flight guest to
    // finish; it must never delete the current directory while another tool
    // is about to open a read-only connection to it.
    if !owner.load(Ordering::Acquire) && users.load(Ordering::Acquire) == 0 {
        let _ = std::fs::remove_dir_all(path);
    }
}

#[cfg(test)]
mod scratch_tests {
    use super::*;

    #[test]
    fn active_workspace_keeps_scratch_after_last_python_lease() {
        let path = std::env::temp_dir().join(format!("fella-scratch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir(&path).unwrap();

        let scratch = WorkspaceScratch::new(path.clone());
        let lease = scratch.lease();
        drop(lease);
        assert!(
            path.exists(),
            "the active workspace still owns its scratch dir"
        );

        drop(scratch);
        assert!(
            !path.exists(),
            "scratch is removed after the workspace owner drops"
        );
    }

    #[test]
    fn replaced_workspace_keeps_scratch_until_its_lease_drops() {
        let path = std::env::temp_dir().join(format!("fella-scratch-old-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir(&path).unwrap();

        let scratch = WorkspaceScratch::new(path.clone());
        let lease = scratch.lease();
        drop(scratch);
        assert!(
            path.exists(),
            "an in-flight Python call still owns the scratch dir"
        );

        drop(lease);
        assert!(
            !path.exists(),
            "replaced scratch is removed after the last lease drops"
        );
    }
}

#[cfg(test)]
mod run_log_tests {
    use super::*;

    #[test]
    fn recent_run_log_includes_existing_content_free_friction_signals() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let data_dir = std::env::temp_dir().join(format!("fella-run-log-{nonce}"));
        let engine = EngineState::new(&data_dir).unwrap();
        std::fs::write(
            data_dir.join("signals.jsonl"),
            r#"{"at_ms":1234,"reason":"repeated_tool_errors","steps":3,"errors":2}"#,
        )
        .unwrap();

        let entries = engine.recent_run_log(10);
        assert_eq!(entries.len(), 1);
        assert!(matches!(entries[0].kind, RunLogKind::Trigger));
        assert_eq!(entries[0].trigger.as_deref(), Some("repeated_tool_errors"));
        assert_eq!(entries[0].trigger_steps, Some(3));
        assert_eq!(entries[0].trigger_errors, Some(2));

        drop(engine);
        let _ = std::fs::remove_dir_all(data_dir);
    }
}

#[cfg(test)]
mod investigation_ensure_tests {
    use super::*;
    use crate::engine::evidence::VerificationStatus;
    use crate::engine::runtime::ExecutionTrace;
    use std::sync::{Arc, Barrier};

    fn saved_turn(id: &str, conversation_id: &str, workspace: &str) -> AnalysisTurn {
        AnalysisTurn {
            id: id.to_owned(),
            conversation_id: conversation_id.to_owned(),
            question: "What were the quarterly sales?".into(),
            context_refs: Vec::new(),
            prior_turn_refs: Vec::new(),
            clarification_of: None,
            clarification_response: None,
            workspace: Some(workspace.to_owned()),
            workspace_revision: Some("legacy-r1".into()),
            workspace_snapshot: None,
            source_scope: None,
            investigation_context: None,
            context_audit: None,
            provenance: None,
            rerun_of: None,
            state: TurnState::Accepted,
            contract: None,
            plan: LogicalPlan::default(),
            trace: ExecutionTrace {
                id: format!("trace-{id}"),
                turn_id: id.to_owned(),
                workspace_revision: Some("legacy-r1".into()),
                mode: None,
                model: None,
                model_calls: Vec::new(),
                elapsed_ms: None,
                steps: Vec::new(),
            },
            verification: None,
            result: AnalysisResult {
                text: "Quarterly sales totaled 1,240.".into(),
                presentation_spec: None,
                status: VerificationStatus::Verified,
                usage: None,
                verification: Vec::new(),
                evidence: vec![serde_json::json!({
                    "id": "legacy-chart-evidence",
                    "tool": "make_chart",
                    "output": {"type": "bar", "values": [310, 290, 330, 310]}
                })],
            },
        }
    }

    #[test]
    fn deleting_a_chat_cascades_its_investigation_but_keeps_blank_work() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let data_dir = std::env::temp_dir().join(format!("fella-delete-chat-{nonce}"));
        let engine = EngineState::new(&data_dir).unwrap();
        let conversation_id = "conversation-delete-cascade";
        let transcript = serde_json::json!({
            "id": conversation_id,
            "workspace": "/work/sales",
            "messages": [{
                "id": "source-message",
                "role": "user",
                "text": "What were quarterly sales?"
            }]
        });
        engine
            .archive_conversation(conversation_id, &transcript.to_string())
            .unwrap();
        let linked = engine
            .investigation_create(
                "/work/sales",
                "Quarterly sales",
                Some("What were quarterly sales?"),
                Some("source-message"),
                Some(conversation_id),
            )
            .unwrap();
        let blank = engine
            .investigation_create(
                "/work/sales",
                "Blank investigation",
                None,
                None,
                Some("conversation-blank"),
            )
            .unwrap();

        engine.delete_conversation(conversation_id).unwrap();
        let remaining = engine.investigations_list(Some(&[])).unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].id, blank.id);
        assert!(remaining.iter().all(|item| item.id != linked.id));
        assert!(engine.conversation_load(conversation_id).is_err());

        drop(engine);
        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[test]
    fn concurrent_first_turns_resolve_to_one_investigation() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let data_dir = std::env::temp_dir().join(format!("fella-investigation-ensure-{nonce}"));
        let engine = Arc::new(EngineState::new(&data_dir).unwrap());
        let barrier = Arc::new(Barrier::new(8));
        let workers: Vec<_> = (0..8)
            .map(|_| {
                let engine = Arc::clone(&engine);
                let barrier = Arc::clone(&barrier);
                std::thread::spawn(move || {
                    barrier.wait();
                    engine
                        .investigation_ensure(
                            "/work/sales",
                            "conversation-first",
                            "Summarize sales trends",
                            "message-first",
                        )
                        .unwrap()
                        .id
                })
            })
            .collect();

        let ids: Vec<_> = workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .collect();
        assert!(ids.iter().all(|id| id == &ids[0]));
        let investigations = engine
            .investigations_list(Some(&["conversation-first".to_owned()]))
            .unwrap();
        assert_eq!(investigations.len(), 1);
        assert!(investigations[0].analysis_turn_ids.is_empty());

        drop(engine);
        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[test]
    fn ensure_migrates_only_matching_archived_runs_without_mutating_history_or_question_coverage() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let data_dir = std::env::temp_dir().join(format!("fella-investigation-history-{nonce}"));
        let engine = EngineState::new(&data_dir).unwrap();
        let conversation_id = "legacy-conversation";
        let workspace_id = "/work/sales";

        let matching = saved_turn("legacy-run", conversation_id, workspace_id);
        let matching_before = serde_json::to_value(&matching).unwrap();
        let foreign_conversation = saved_turn(
            "foreign-conversation-run",
            "other-conversation",
            workspace_id,
        );
        let foreign_workspace = saved_turn("foreign-workspace-run", conversation_id, "/work/other");
        let unreferenced = saved_turn("unreferenced-run", conversation_id, workspace_id);
        for turn in [
            &matching,
            &foreign_conversation,
            &foreign_workspace,
            &unreferenced,
        ] {
            analysis_store::create(&data_dir, turn).unwrap();
        }

        let transcript = serde_json::json!({
            "id": conversation_id,
            "workspace": workspace_id,
            "messages": [
                {"id":"first-message", "role":"user", "text":"What were quarterly sales?"},
                {"id":"legacy-answer", "role":"assistant", "text":"Quarterly sales totaled 1,240.", "answer": {
                    "turn_id":"legacy-run",
                    "evidence":[{"id":"legacy-chart-evidence", "tool":"make_chart", "output":{"type":"bar", "values":[310,290,330,310]}}]
                }},
                {"role":"assistant", "text":"A repeated archive reference.", "answer":{"turn_id":"legacy-run"}},
                {"id":"second-message", "role":"user", "text":"What about costs?"},
                {"role":"assistant", "text":"A different folder's result.", "answer":{"turn_id":"foreign-workspace-run"}},
                {"role":"assistant", "text":"A different conversation's result.", "answer":{"turn_id":"foreign-conversation-run"}},
                {"role":"assistant", "text":"An older result without its canonical record.", "answer":{"turn_id":"missing-canonical-run"}}
            ]
        });
        engine
            .archive_conversation(conversation_id, &transcript.to_string())
            .unwrap();

        let migrated = engine
            .investigation_ensure(
                workspace_id,
                conversation_id,
                "What were quarterly sales?",
                "first-message",
            )
            .unwrap();
        assert_eq!(migrated.analysis_turn_ids, vec!["legacy-run"]);
        assert_eq!(migrated.brief.goal, "What were quarterly sales?");
        let root_question = migrated.questions.as_ref().unwrap().first().unwrap();
        assert_eq!(
            root_question.answer_status,
            InvestigationQuestionAnswerStatus::NotStarted
        );
        assert!(root_question.analysis_turn_ids.is_empty());
        assert_eq!(root_question.latest_analysis_turn_id, None);

        let archived_after: serde_json::Value =
            serde_json::from_str(&engine.conversation_load(conversation_id).unwrap()).unwrap();
        assert_eq!(
            archived_after, transcript,
            "migration must not rewrite transcript messages or chart evidence"
        );
        let canonical_after =
            serde_json::to_value(engine.analysis_turn_load("legacy-run").unwrap()).unwrap();
        assert_eq!(
            canonical_after, matching_before,
            "canonical AnalysisTurn must remain immutable"
        );

        let repeated = engine
            .investigation_ensure(
                workspace_id,
                conversation_id,
                "A later request must not replace the original goal",
                "second-message",
            )
            .unwrap();
        assert_eq!(repeated.id, migrated.id);
        assert_eq!(repeated.analysis_turn_ids, vec!["legacy-run"]);
        assert_eq!(repeated.brief.goal, "What were quarterly sales?");

        drop(engine);
        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[test]
    fn ensure_resumes_archived_run_migration_in_bounded_batches() {
        const RUN_COUNT: usize = MAX_ANALYSIS_TURN_LINKS_PER_UPDATE + 1;
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let data_dir =
            std::env::temp_dir().join(format!("fella-investigation-history-batches-{nonce}"));
        let engine = EngineState::new(&data_dir).unwrap();
        let conversation_id = "large-legacy-conversation";
        let workspace_id = "/work/sales";

        let mut messages = vec![serde_json::json!({
            "id":"first-message",
            "role":"user",
            "text":"Summarize all saved runs"
        })];
        for index in 0..RUN_COUNT {
            let turn_id = format!("legacy-run-{index:03}");
            let turn = saved_turn(&turn_id, conversation_id, workspace_id);
            analysis_store::create(&data_dir, &turn).unwrap();
            messages.push(serde_json::json!({
                "role":"assistant",
                "text":"Saved analysis result.",
                "answer":{"turn_id":turn_id}
            }));
        }
        let transcript = serde_json::json!({
            "id":conversation_id,
            "workspace":workspace_id,
            "messages":messages
        });
        engine
            .archive_conversation(conversation_id, &transcript.to_string())
            .unwrap();

        let migrated = engine
            .investigation_ensure(
                workspace_id,
                conversation_id,
                "Summarize all saved runs",
                "first-message",
            )
            .unwrap();
        assert_eq!(migrated.analysis_turn_ids.len(), RUN_COUNT);
        assert_eq!(
            migrated.analysis_turn_ids.first().unwrap(),
            "legacy-run-000"
        );
        assert_eq!(
            migrated.analysis_turn_ids.last().unwrap(),
            &format!("legacy-run-{:03}", RUN_COUNT - 1)
        );

        let retried = engine
            .investigation_ensure(
                workspace_id,
                conversation_id,
                "A later message",
                "later-message",
            )
            .unwrap();
        assert_eq!(retried.id, migrated.id);
        assert_eq!(retried.analysis_turn_ids, migrated.analysis_turn_ids);

        drop(engine);
        let _ = std::fs::remove_dir_all(data_dir);
    }
}

#[cfg(test)]
static TEST_OPEN_WORKSPACE_TARGET: AtomicUsize = AtomicUsize::new(0);
#[cfg(test)]
static TEST_OPEN_WORKSPACE_PAUSE: AtomicBool = AtomicBool::new(false);
#[cfg(test)]
static TEST_OPEN_WORKSPACE_READY: AtomicBool = AtomicBool::new(false);
#[cfg(test)]
static TEST_OPEN_WORKSPACE_RESUME: AtomicBool = AtomicBool::new(false);

#[cfg(test)]
fn pause_open_workspace_for_test(state: &EngineState) {
    let target = state as *const EngineState as usize;
    if TEST_OPEN_WORKSPACE_TARGET.load(Ordering::SeqCst) != target
        || !TEST_OPEN_WORKSPACE_PAUSE.swap(false, Ordering::SeqCst)
    {
        return;
    }
    TEST_OPEN_WORKSPACE_READY.store(true, Ordering::SeqCst);
    while !TEST_OPEN_WORKSPACE_RESUME.load(Ordering::SeqCst) {
        std::thread::yield_now();
    }
}

/// Most conversations we keep distilled memory for at once.
const SESSION_CAP: usize = 24;

fn workspace_scratch_dir(data_dir: &Path) -> EngineResult<PathBuf> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    loop {
        let suffix = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = data_dir.join(format!(".analysis-{}-{suffix}", std::process::id()));
        match std::fs::create_dir(&path) {
            Ok(()) => return Ok(path),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(EngineError::io(format!("create {}", path.display()), e)),
        }
    }
}

/// A few earlier turns of one conversation, distilled for follow-ups. The
/// conversational text remains useful across workspace switches; analytical
/// hints are only surfaced when their exact source snapshot is mounted.
#[derive(Default)]
struct SessionMemory {
    turns: Vec<TurnDigest>,
    last_used: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum TurnWorkspace {
    NoWorkspace,
    Snapshot {
        path: String,
        revision: String,
    },
    /// Older transcript formats recorded the conversation's folder but not
    /// the indexed revision on each answer. These turns may help resolve
    /// references but their execution evidence is never reusable.
    PathOnly {
        path: String,
    },
    Unknown,
}

struct TurnDigest {
    turn_id: Option<String>,
    question: String,
    headline: String,
    workspace: TurnWorkspace,
    source_selection: Option<InvestigationSourceSelection>,
    frame: Option<String>,
    queries: Vec<String>,
}

fn turn_context_matches(workspace: &TurnWorkspace, catalog: &Catalog) -> bool {
    match workspace {
        TurnWorkspace::NoWorkspace => catalog.workspace.is_none(),
        TurnWorkspace::Snapshot { path, revision } => {
            catalog.workspace.as_deref() == Some(path.as_str())
                && catalog.revision.as_deref() == Some(revision.as_str())
        }
        TurnWorkspace::PathOnly { path } => catalog.workspace.as_deref() == Some(path.as_str()),
        TurnWorkspace::Unknown => false,
    }
}

fn archived_turn_workspace(
    answer: Option<&serde_json::Value>,
    transcript_workspace: Option<&str>,
) -> TurnWorkspace {
    let Some(answer) = answer else {
        return TurnWorkspace::Unknown;
    };
    if let Some(workspace) = answer.get("workspace") {
        if workspace.is_null() {
            return if answer.get("trace").is_some() {
                TurnWorkspace::NoWorkspace
            } else {
                TurnWorkspace::Unknown
            };
        }
        if let (Some(path), Some(revision)) = (
            workspace.get("path").and_then(|value| value.as_str()),
            workspace.get("revision").and_then(|value| value.as_str()),
        ) {
            return TurnWorkspace::Snapshot {
                path: path.to_string(),
                revision: revision.to_string(),
            };
        }
        return TurnWorkspace::Unknown;
    }

    // Current wire answers omit `workspace` when it is absent, while older
    // archived assistant messages may have no typed answer at all. A trace
    // without a workspace revision identifies the former general/no-mount
    // answer; don't infer a source for legacy prose from archive-level metadata.
    if answer.get("trace").is_some()
        && answer
            .pointer("/trace/workspace_revision")
            .is_none_or(serde_json::Value::is_null)
    {
        TurnWorkspace::NoWorkspace
    } else if let Some(path) = transcript_workspace.filter(|path| !path.trim().is_empty()) {
        TurnWorkspace::PathOnly {
            path: path.to_string(),
        }
    } else {
        TurnWorkspace::Unknown
    }
}

fn contract_frame(contract: &AnalysisContract) -> Option<String> {
    let mut parts = Vec::new();
    if let Some(subject) = contract.subject.as_deref() {
        parts.push(format!("subject={subject}"));
    }
    if let Some(population) = contract.population.as_deref() {
        parts.push(format!("population={population}"));
    }
    if let Some(time) = &contract.time {
        if let Some(field) = time.field.as_deref() {
            parts.push(format!("time={field}"));
        }
        if let Some(range) = time.range.as_deref() {
            parts.push(format!("range={range}"));
        }
        if let Some(bucket) = time.bucket {
            parts.push(format!("bucket={bucket:?}"));
        }
    }
    for filter in &contract.filters {
        let values = if filter.resolved_values.is_empty() {
            &filter.candidate_values
        } else {
            &filter.resolved_values
        };
        let value = if values.is_empty() {
            filter.concept.clone()
        } else {
            format!("{}={}", filter.concept, values.join("|"))
        };
        parts.push(if filter.exclude {
            format!("exclude {value}")
        } else {
            value
        });
    }
    if !contract.group_by.is_empty() {
        parts.push(format!("group={}", contract.group_by.join(",")));
    }
    if let Some(value_semantics) = contract.value_semantics.as_deref() {
        parts.push(format!("values={value_semantics}"));
    }
    if let Some(missing_policy) = contract.missing_policy.as_deref() {
        parts.push(format!("missing={missing_policy}"));
    }
    if let Some(denominator) = contract.denominator.as_deref() {
        parts.push(format!("denominator={denominator}"));
    }
    (!parts.is_empty()).then(|| cap_chars(&parts.join("; "), 600))
}

fn reusable_prior_turn_ref(item: &EvidenceItem) -> Option<&str> {
    if item.tool != "read_prior_analysis" || item.error.is_some() {
        return None;
    }
    let freshness = item
        .output
        .as_deref()
        .and_then(|output| serde_json::from_str::<serde_json::Value>(output).ok())
        .and_then(|packet| {
            packet
                .get("freshness")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        });
    (freshness.as_deref() == Some("same_workspace_revision"))
        .then(|| item.args.get("turn_id").and_then(serde_json::Value::as_str))
        .flatten()
}

/// Recover the small backend session projection from a transcript archive.
/// The transcript remains the UI source of truth; this deliberately extracts
/// only the question, answer headline, and successful analytical queries that
/// the prompt has always carried for an in-memory follow-up.
fn archived_turns(value: &serde_json::Value) -> Vec<TurnDigest> {
    let Some(messages) = value.get("messages").and_then(|value| value.as_array()) else {
        return Vec::new();
    };
    let transcript_workspace = value.get("workspace").and_then(|path| path.as_str());

    let mut turns = Vec::new();
    for (index, message) in messages.iter().enumerate() {
        if !is_actual_question_message(message) {
            continue;
        }
        let assistant = messages[index + 1..]
            .iter()
            .take_while(|candidate| !is_actual_question_message(candidate))
            .find(|candidate| {
                candidate.get("role").and_then(|role| role.as_str()) == Some("assistant")
            });
        let Some(assistant) = assistant else {
            continue;
        };
        let answer = assistant.get("answer");
        let turn_id = answer
            .and_then(|answer| answer.get("turn_id"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_string);
        let headline = assistant
            .get("text")
            .and_then(|text| text.as_str())
            .or_else(|| {
                answer
                    .and_then(|answer| answer.get("text"))
                    .and_then(|text| text.as_str())
            })
            .map(|text| {
                text.lines()
                    .map(str::trim)
                    .find(|line| !line.is_empty())
                    .unwrap_or("")
            })
            .filter(|text| !text.is_empty())
            .map(|text| cap_chars(text, 200));
        let Some(headline) = headline else {
            continue;
        };
        let queries = answer
            .and_then(|answer| answer.get("evidence"))
            .and_then(|evidence| evidence.as_array())
            .into_iter()
            .flatten()
            .filter(|item| {
                matches!(
                    item.get("tool").and_then(|tool| tool.as_str()),
                    Some("run_sql" | "make_chart" | "forecast_analysis")
                ) && item
                    .get("error")
                    .map(|error| error.is_null())
                    .unwrap_or(true)
            })
            .filter_map(|item| item.get("sql").and_then(|sql| sql.as_str()))
            .map(|sql| {
                sql.split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
                    .chars()
                    .take(160)
                    .collect()
            })
            .take(3)
            .collect();
        let frame = answer
            .and_then(|answer| answer.get("contract"))
            .and_then(|contract| serde_json::from_value::<AnalysisContract>(contract.clone()).ok())
            .and_then(|contract| contract_frame(&contract));
        let source_selection = answer
            .and_then(|answer| answer.get("investigation_context"))
            .and_then(|context| context.get("source_selection"))
            .and_then(|selection| {
                serde_json::from_value::<InvestigationSourceSelection>(selection.clone()).ok()
            });
        turns.push(TurnDigest {
            turn_id,
            question: message
                .get("text")
                .and_then(|text| text.as_str())
                .map(|text| cap_chars(text, 200))
                .unwrap_or_default(),
            headline,
            workspace: archived_turn_workspace(answer, transcript_workspace),
            source_selection,
            frame,
            queries,
        });
    }
    if turns.len() > 3 {
        turns.drain(0..turns.len() - 3);
    }
    turns
}

#[cfg(test)]
mod conversation_context_tests {
    use super::*;
    use crate::engine::investigation::InvestigationSourceSelectionMode;

    fn scratch(tag: &str) -> PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("fella-conversation-{tag}-{nonce}"));
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn mount_switch_preserves_thread_and_hides_old_workspace_hints() {
        let data_dir = scratch("mount-data");
        let first_workspace = scratch("mount-first");
        let second_workspace = scratch("mount-second");
        std::fs::write(first_workspace.join("values.csv"), "value\n10\n").unwrap();
        std::fs::write(second_workspace.join("values.csv"), "value\n20\n").unwrap();

        let engine = EngineState::new(&data_dir).unwrap();
        engine.inner.lock().unwrap().sessions.insert(
            "same-conversation".into(),
            SessionMemory {
                turns: vec![TurnDigest {
                    turn_id: None,
                    question: "What does variance mean?".into(),
                    headline: "Variance measures spread around a mean.".into(),
                    workspace: TurnWorkspace::NoWorkspace,
                    source_selection: None,
                    frame: None,
                    queries: Vec::new(),
                }],
                last_used: 1,
            },
        );

        let first_catalog = engine.open_workspace(&first_workspace).unwrap();
        let first_revision = first_catalog.revision.clone().unwrap();
        engine
            .inner
            .lock()
            .unwrap()
            .sessions
            .get_mut("same-conversation")
            .unwrap()
            .turns
            .push(TurnDigest {
                turn_id: Some("turn-first-mount".into()),
                question: "What was the total in this folder?".into(),
                headline: "The total was 10.".into(),
                workspace: TurnWorkspace::Snapshot {
                    path: first_workspace.display().to_string(),
                    revision: first_revision,
                },
                source_selection: None,
                frame: Some("measure=value; population=all rows".into()),
                queries: vec!["SELECT SUM(value) FROM values".into()],
            });
        let in_first_mount = engine
            .session_block("same-conversation")
            .expect("conversation survives the first mount");
        assert!(in_first_mount.contains("What does variance mean?"));
        assert!(in_first_mount.contains("prior query shape"));
        assert!(in_first_mount.contains("turn-first-mount"));

        // A changed revision at the same path is stale too; path equality by
        // itself must not keep old analytical hints eligible.
        std::fs::write(first_workspace.join("values.csv"), "value\n11\n").unwrap();
        let changed_catalog = engine.open_workspace(&first_workspace).unwrap();
        assert!(!engine.session_has_reusable_analysis("same-conversation", &changed_catalog));
        let after_reindex = engine
            .session_block("same-conversation")
            .expect("conversation survives a reindex");
        assert!(after_reindex.contains("other or older workspace; not current evidence"));
        assert!(!after_reindex.contains("SELECT SUM(value) FROM values"));

        let second_catalog = engine.open_workspace(&second_workspace).unwrap();
        assert!(!engine.session_has_reusable_analysis("same-conversation", &second_catalog));
        let in_second_mount = engine
            .session_block("same-conversation")
            .expect("conversation survives another mount");
        assert!(in_second_mount.contains("What does variance mean?"));
        assert!(in_second_mount.contains("What was the total in this folder?"));
        assert!(in_second_mount.contains("The total was 10."));
        assert!(in_second_mount.contains("other or older workspace; not current evidence"));
        assert!(!in_second_mount.contains("SELECT SUM(value) FROM values"));
        assert!(!in_second_mount.contains("measure=value"));

        drop(engine);
        let _ = std::fs::remove_dir_all(data_dir);
        let _ = std::fs::remove_dir_all(first_workspace);
        let _ = std::fs::remove_dir_all(second_workspace);
    }

    #[test]
    fn archive_hydration_keeps_cross_mount_context_but_only_current_snapshot_hints() {
        let data_dir = scratch("archive-data");
        let current_workspace = scratch("archive-current");
        std::fs::write(current_workspace.join("values.csv"), "value\n20\n").unwrap();
        let engine = EngineState::new(&data_dir).unwrap();
        let catalog = engine.open_workspace(&current_workspace).unwrap();
        let revision = catalog.revision.unwrap();
        let current_path = current_workspace.display().to_string();
        let transcript = serde_json::json!({
            "id": "cross-mount-thread",
            "workspace": current_path,
            "messages": [
                {"role":"user", "text":"What does variance mean?"},
                {"role":"assistant", "text":"Variance measures spread.", "answer": {
                    "text":"Variance measures spread.", "trace":{"workspace_revision":null}, "evidence":[]
                }},
                {"role":"user", "text":"What was the old folder's total?"},
                {"role":"assistant", "text":"The old total was 10.", "answer": {
                    "text":"The old total was 10.",
                    "workspace":{"path":"/workspace/old", "revision":"old-revision"},
                    "contract":{"subject":"old values"},
                    "evidence":[{"tool":"run_sql", "sql":"SELECT SUM(value) FROM old_values", "error":null}]
                }},
                {"role":"user", "text":"What was the total in this folder?"},
                {"role":"assistant", "text":"The current total was 20.", "answer": {
                    "text":"The current total was 20.",
                    "workspace":{"path":current_path, "revision":revision},
                    "evidence":[{"tool":"run_sql", "sql":"SELECT SUM(value) FROM values", "error":null}]
                }}
            ]
        });
        engine
            .archive_conversation("cross-mount-thread", &transcript.to_string())
            .unwrap();
        engine.hydrate_session_from_archive("cross-mount-thread");

        let prompt_context = engine
            .session_block("cross-mount-thread")
            .expect("archive should hydrate the conversation");
        assert!(prompt_context.contains("What does variance mean?"));
        assert!(prompt_context.contains("Variance measures spread."));
        assert!(prompt_context.contains("The old total was 10."));
        assert!(prompt_context.contains("other or older workspace; not current evidence"));
        assert!(!prompt_context.contains("SELECT SUM(value) FROM old_values"));
        assert!(prompt_context.contains("SELECT SUM(value) FROM values"));

        drop(engine);
        let _ = std::fs::remove_dir_all(data_dir);
        let _ = std::fs::remove_dir_all(current_workspace);
    }

    #[test]
    fn legacy_archive_path_keeps_query_as_a_replay_only_hint() {
        let data_dir = scratch("archive-path-only-data");
        let workspace = scratch("archive-path-only-ws");
        std::fs::write(workspace.join("values.csv"), "value\n20\n").unwrap();
        let engine = EngineState::new(&data_dir).unwrap();
        engine.open_workspace(&workspace).unwrap();
        let path = workspace.display().to_string();
        let transcript = serde_json::json!({
            "id": "legacy-path-only-thread",
            "workspace": path,
            "messages": [
                {"role":"user", "text":"What was the total?"},
                {"role":"assistant", "text":"The total was 20.", "answer": {
                    "text":"The total was 20.",
                    "evidence":[{
                        "tool":"run_sql",
                        "sql":"SELECT SUM(value) FROM values",
                        "error":null
                    }]
                }}
            ]
        });
        engine
            .archive_conversation("legacy-path-only-thread", &transcript.to_string())
            .unwrap();
        engine.hydrate_session_from_archive("legacy-path-only-thread");

        let prompt_context = engine
            .session_block("legacy-path-only-thread")
            .expect("archive should hydrate");
        assert!(prompt_context.contains("archived revision unavailable"));
        assert!(prompt_context.contains("prior evidence is not reusable"));
        assert!(prompt_context.contains("prior query shape"));
        assert!(prompt_context.contains("SELECT SUM(value) FROM values"));

        drop(engine);
        let _ = std::fs::remove_dir_all(data_dir);
        let _ = std::fs::remove_dir_all(workspace);
    }

    #[test]
    fn general_conversation_can_hydrate_without_a_mounted_workspace() {
        let data_dir = scratch("general-data");
        let engine = EngineState::new(&data_dir).unwrap();
        let transcript = serde_json::json!({
            "id": "general-thread",
            "workspace": null,
            "messages": [
                {"role":"user", "text":"What is a median?"},
                {"role":"assistant", "text":"The median is the middle value.", "answer": {
                    "text":"The median is the middle value.", "trace":{"workspace_revision":null}, "evidence":[]
                }}
            ]
        });
        engine
            .archive_conversation("general-thread", &transcript.to_string())
            .unwrap();
        engine.hydrate_session_from_archive("general-thread");

        let prompt_context = engine
            .session_block("general-thread")
            .expect("general thread should hydrate with no folder open");
        assert!(prompt_context.contains("What is a median?"));
        assert!(prompt_context.contains("The median is the middle value."));
        assert!(prompt_context.contains("general turn; no local evidence"));

        drop(engine);
        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[test]
    fn investigation_context_hides_prior_turns_from_other_source_scopes() {
        let data_dir = scratch("investigation-source-scope-memory");
        let engine = EngineState::new(&data_dir).unwrap();
        let sales_scope = InvestigationSourceSelection {
            mode: InvestigationSourceSelectionMode::SelectedSources,
            included_paths: vec!["sales.csv".into()],
        };
        let costs_scope = InvestigationSourceSelection {
            mode: InvestigationSourceSelectionMode::SelectedSources,
            included_paths: vec!["costs.csv".into()],
        };
        engine.inner.lock().unwrap().sessions.insert(
            "investigation-source-scope".into(),
            SessionMemory {
                turns: vec![
                    TurnDigest {
                        turn_id: Some("turn-sales".into()),
                        question: "What are the sales?".into(),
                        headline: "Sales total 30.".into(),
                        workspace: TurnWorkspace::NoWorkspace,
                        source_selection: Some(sales_scope.clone()),
                        frame: None,
                        queries: vec!["SELECT SUM(amount) FROM sales".into()],
                    },
                    TurnDigest {
                        turn_id: Some("turn-costs".into()),
                        question: "What are the costs?".into(),
                        headline: "Costs total 7.".into(),
                        workspace: TurnWorkspace::NoWorkspace,
                        source_selection: Some(costs_scope),
                        frame: None,
                        queries: vec!["SELECT SUM(amount) FROM costs".into()],
                    },
                    TurnDigest {
                        turn_id: Some("turn-general".into()),
                        question: "What is a median?".into(),
                        headline: "The median is the middle value.".into(),
                        workspace: TurnWorkspace::NoWorkspace,
                        source_selection: None,
                        frame: None,
                        queries: Vec::new(),
                    },
                ],
                last_used: 1,
            },
        );

        let context = engine
            .session_block_for_source_selection("investigation-source-scope", Some(&sales_scope))
            .expect("session has earlier turns");
        assert!(context.contains("What are the sales?"));
        assert!(context.contains("SELECT SUM(amount) FROM sales"));
        assert!(!context.contains("What are the costs?"));
        assert!(!context.contains("SELECT SUM(amount) FROM costs"));
        assert!(!context.contains("What is a median?"));

        drop(engine);
        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[test]
    fn selected_source_context_omits_workspace_wide_learned_notes() {
        let data_dir = scratch("investigation-selected-source-notes");
        let engine = EngineState::new(&data_dir).unwrap();
        let memory_path = data_dir.join("workspace-memory.md");
        std::fs::write(
            &memory_path,
            "# Learned notes\n\n## Preferences\n- Excluded source fact: confidential cost center is 42\n",
        )
        .unwrap();
        engine.workspace.lock().unwrap().memory_path = Some(memory_path);

        let selection = InvestigationSourceSelection {
            mode: InvestigationSourceSelectionMode::SelectedSources,
            included_paths: vec!["sales.csv".into()],
        };
        let context = engine.context_packet_for_source_selection(
            "Summarize sales",
            "investigation-selected-source-notes",
            Some(&selection),
            None,
        );

        assert!(context.learned.is_none());

        drop(engine);
        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[test]
    fn archive_hydration_recovers_investigation_scope_from_saved_run() {
        let data_dir = scratch("investigation-source-archive");
        let engine = EngineState::new(&data_dir).unwrap();
        let selection = InvestigationSourceSelection {
            mode: InvestigationSourceSelectionMode::SelectedSources,
            included_paths: vec!["sales.csv".into()],
        };
        let turn_id = "turn-investigation-scope";
        analysis_store::create(
            &data_dir,
            &crate::engine::runtime::AnalysisTurn {
                id: turn_id.into(),
                conversation_id: "investigation-archive".into(),
                question: "What are the sales?".into(),
                context_refs: Vec::new(),
                prior_turn_refs: Vec::new(),
                clarification_of: None,
                clarification_response: None,
                workspace: Some("/workspace".into()),
                workspace_revision: Some("revision-1".into()),
                workspace_snapshot: None,
                source_scope: None,
                investigation_context: Some(InvestigationContextSnapshot {
                    investigation_id: "investigation-1".into(),
                    question_id: None,
                    question_text: None,
                    definitions: Vec::new(),
                    question_plan: Vec::new(),
                    source_selection: selection.clone(),
                }),
                context_audit: None,
                provenance: None,
                rerun_of: None,
                state: TurnState::Accepted,
                contract: None,
                plan: crate::engine::runtime::LogicalPlan::default(),
                trace: crate::engine::runtime::ExecutionTrace {
                    id: "trace-1".into(),
                    turn_id: turn_id.into(),
                    workspace_revision: Some("revision-1".into()),
                    mode: None,
                    model: None,
                    model_calls: Vec::new(),
                    elapsed_ms: None,
                    steps: Vec::new(),
                },
                verification: None,
                result: crate::engine::runtime::AnalysisResult {
                    text: "Sales total 30.".into(),
                    presentation_spec: None,
                    status: crate::engine::evidence::VerificationStatus::Verified,
                    usage: None,
                    verification: Vec::new(),
                    evidence: Vec::new(),
                },
            },
        )
        .unwrap();

        let transcript = serde_json::json!({
            "id": "investigation-archive",
            "workspace": "/workspace",
            "messages": [
                {"role":"user", "text":"What are the sales?"},
                {"role":"assistant", "text":"Sales total 30.", "answer": {
                    "turn_id": turn_id,
                    "workspace":{"path":"/workspace", "revision":"revision-1"},
                    "evidence":[]
                }}
            ]
        });
        engine
            .archive_conversation("investigation-archive", &transcript.to_string())
            .unwrap();
        engine.hydrate_session_from_archive("investigation-archive");

        {
            let inner = engine.inner.lock().unwrap();
            let turns = &inner.sessions.get("investigation-archive").unwrap().turns;
            assert_eq!(turns.len(), 1);
            assert_eq!(turns[0].source_selection.as_ref(), Some(&selection));
        }
        let selected_context = engine
            .session_block_for_source_selection("investigation-archive", Some(&selection))
            .expect("saved conversation should hydrate");
        assert!(selected_context.contains("What are the sales?"));

        drop(engine);
        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[test]
    fn conversation_history_recovers_origin_scope_from_first_answer() {
        let data_dir = scratch("conversation-origin");
        let engine = EngineState::new(&data_dir).unwrap();
        let no_repo = serde_json::json!({
            "id": "general-origin",
            "saved_at_ms": 1,
            "workspace": "/later/repository",
            "messages": [
                {"role":"user", "text":"What is Rust?"},
                {"role":"assistant", "text":"Rust is a programming language.", "answer": {
                    "text":"Rust is a programming language.",
                    "trace":{"id":"trace-general", "turn_id":"turn-general", "steps":[]},
                    "evidence":[]
                }}
            ]
        });
        let repo_origin = serde_json::json!({
            "id": "repo-origin",
            "saved_at_ms": 2,
            "workspace": "/later/repository",
            "messages": [
                {"id":"message-repo-origin", "role":"user", "text":"What is the total?"},
                {"role":"assistant", "text":"The total is 12.", "answer": {
                    "text":"The total is 12.",
                    "workspace":{"path":"/original/repository", "revision":"revision-1"},
                    "trace":{"id":"trace-repo", "turn_id":"turn-repo", "workspace_revision":"revision-1", "steps":[]},
                    "evidence":[]
                }}
            ]
        });
        engine
            .archive_conversation("general-origin", &no_repo.to_string())
            .unwrap();
        engine
            .archive_conversation("repo-origin", &repo_origin.to_string())
            .unwrap();

        let conversations = engine.conversations_list();
        assert_eq!(
            conversations
                .iter()
                .find(|item| item.id == "general-origin")
                .unwrap()
                .workspace,
            None,
            "a later mount must not claim a general conversation"
        );
        assert_eq!(
            conversations
                .iter()
                .find(|item| item.id == "repo-origin")
                .unwrap()
                .workspace
                .as_deref(),
            Some("/original/repository"),
            "the first answer's repository is more reliable than a later archive snapshot"
        );
        assert_eq!(
            conversations
                .iter()
                .find(|item| item.id == "repo-origin")
                .unwrap()
                .first_question_message_id
                .as_deref(),
            Some("message-repo-origin")
        );

        drop(engine);
        let _ = std::fs::remove_dir_all(data_dir);
    }
}

fn normalize_sample_path(path: &str) -> String {
    path.replace('\\', "/")
        .trim_start_matches("./")
        .to_ascii_lowercase()
}

fn normalized_workspace_path(path: &Path) -> String {
    std::fs::canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_ascii_lowercase()
}

fn quote_sql_identifier(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}

#[derive(Debug, Serialize)]
pub struct QueryResult {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<serde_json::Value>>,
    pub row_count: usize,
    pub ms: u64,
    pub truncated: bool,
}

/// One document-search passage with its source and anchor line.
#[derive(Debug, Serialize)]
pub struct GrepHit {
    pub source: String,
    pub line: usize,
    pub text: String,
}

#[derive(Debug)]
pub struct GrepResults {
    pub hits: Vec<GrepHit>,
    /// True when a source could not be read or the bounded scan ended before
    /// all documents were searched. A partial scan is not proof of absence.
    pub incomplete: bool,
}

struct SearchLine {
    number: usize,
    excerpt: String,
    term_frequencies: Vec<u32>,
    token_count: usize,
    regex_match: bool,
}

struct RankedGrepHit {
    hit: GrepHit,
    first_line: usize,
    last_line: usize,
    score: f64,
    phrase_match: bool,
}

/// `read_file` truncates a document past this many characters so one huge
/// PDF can't blow the context window `grep_files` can find a spot in a
/// bigger file first.
const READ_FILE_CHAR_CAP: usize = 12_000;
const DOCUMENT_SEARCH_BYTE_CAP: usize = 64 * 1024 * 1024;
const DOCUMENT_SEARCH_WINDOW_LINES: usize = 5;
const DOCUMENT_SEARCH_LINE_CHARS: usize = 420;

fn document_search_tokens(text: &str) -> Vec<String> {
    text.split(|ch: char| !ch.is_alphanumeric())
        .filter(|token| token.chars().count() >= 2)
        .map(str::to_lowercase)
        .collect()
}

fn document_search_terms(query: &str) -> Vec<String> {
    const STOP_WORDS: &[&str] = &[
        "a", "an", "and", "are", "as", "at", "be", "been", "being", "but", "by", "can", "could",
        "did", "do", "does", "for", "from", "has", "have", "how", "i", "in", "into", "is", "it",
        "its", "me", "of", "on", "or", "please", "that", "the", "their", "them", "there", "these",
        "they", "this", "those", "to", "was", "we", "were", "what", "when", "where", "which",
        "who", "why", "will", "with", "would", "you", "your",
    ];
    let mut seen = HashSet::new();
    document_search_tokens(query)
        .into_iter()
        .filter(|term| !STOP_WORDS.contains(&term.as_str()) && seen.insert(term.clone()))
        .collect()
}

fn has_search_operators(query: &str) -> bool {
    query.chars().any(|ch| {
        matches!(
            ch,
            '\\' | '.' | '^' | '$' | '*' | '+' | '?' | '(' | ')' | '[' | ']' | '{' | '}' | '|'
        )
    })
}

fn excerpt_document_line(line: &str, match_at: Option<usize>) -> String {
    let trimmed = line.trim();
    if trimmed.chars().count() <= DOCUMENT_SEARCH_LINE_CHARS {
        return trimmed.to_string();
    }

    let target = match_at
        .unwrap_or(0)
        .saturating_sub(DOCUMENT_SEARCH_LINE_CHARS / 3);
    let start = trimmed
        .char_indices()
        .find(|(index, _)| *index >= target)
        .map(|(index, _)| index)
        .unwrap_or(0);
    let end = trimmed[start..]
        .char_indices()
        .nth(DOCUMENT_SEARCH_LINE_CHARS)
        .map(|(offset, _)| start + offset)
        .unwrap_or(trimmed.len());
    let mut excerpt = String::new();
    if start > 0 {
        excerpt.push('…');
    }
    excerpt.push_str(&trimmed[start..end]);
    if end < trimmed.len() {
        excerpt.push('…');
    }
    excerpt
}

fn prepare_search_line(
    number: usize,
    line: &str,
    terms: &[String],
    regex: &regex::Regex,
) -> SearchLine {
    let tokens = document_search_tokens(line);
    let term_frequencies = terms
        .iter()
        .map(|term| tokens.iter().filter(|token| *token == term).count() as u32)
        .collect();
    let regex_match = regex.is_match(line);
    let lower = line.to_ascii_lowercase();
    let match_at = terms
        .iter()
        .filter_map(|term| lower.find(term))
        .min()
        .or_else(|| regex.find(line).map(|matched| matched.start()));
    SearchLine {
        number,
        excerpt: excerpt_document_line(line, match_at),
        term_frequencies,
        token_count: tokens.len(),
        regex_match,
    }
}

fn document_window_frequencies(
    window: &VecDeque<SearchLine>,
    term_count: usize,
) -> (Vec<u32>, usize) {
    let mut frequencies = vec![0; term_count];
    let mut token_count = 0;
    for line in window {
        token_count += line.token_count;
        for (index, count) in line.term_frequencies.iter().enumerate() {
            frequencies[index] += count;
        }
    }
    (frequencies, token_count)
}

fn rank_document_window(
    source: &str,
    window: &VecDeque<SearchLine>,
    idf: &[f64],
    average_length: f64,
    phrase: &str,
) -> Option<RankedGrepHit> {
    const BM25_K1: f64 = 1.2;
    const BM25_B: f64 = 0.75;
    let (term_frequencies, token_count) = document_window_frequencies(window, idf.len());
    let regex_match = window.iter().any(|line| line.regex_match);
    let covered_count = term_frequencies.iter().filter(|count| **count > 0).count();
    if covered_count == 0 && !regex_match {
        return None;
    }

    let combined = window
        .iter()
        .map(|line| line.excerpt.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    let phrase_match =
        !phrase.is_empty() && document_search_tokens(&combined).join(" ").contains(phrase);
    let length_norm = if average_length > 0.0 {
        1.0 - BM25_B + BM25_B * token_count as f64 / average_length
    } else {
        1.0
    };
    let score = term_frequencies
        .iter()
        .zip(idf)
        .filter(|(frequency, _)| **frequency > 0)
        .map(|(frequency, idf)| {
            let frequency = *frequency as f64;
            idf * frequency * (BM25_K1 + 1.0) / (frequency + BM25_K1 * length_norm)
        })
        .sum::<f64>()
        + if phrase_match { 2.0 } else { 0.0 }
        + if regex_match { 0.25 } else { 0.0 };
    let anchor = window
        .iter()
        .find(|line| line.term_frequencies.iter().any(|frequency| *frequency > 0))
        .or_else(|| window.iter().find(|line| line.regex_match))?;
    let text = window
        .iter()
        .map(|line| format!("[line {}] {}", line.number, line.excerpt))
        .collect::<Vec<_>>()
        .join("\n");

    Some(RankedGrepHit {
        hit: GrepHit {
            source: source.to_string(),
            line: anchor.number,
            text,
        },
        first_line: window.front()?.number,
        last_line: window.back()?.number,
        score,
        phrase_match,
    })
}

fn sort_ranked_hits(hits: &mut [RankedGrepHit]) {
    hits.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then_with(|| a.hit.source.cmp(&b.hit.source))
            .then_with(|| a.hit.line.cmp(&b.hit.line))
    });
}

fn scan_document_lines(
    engine: &EngineState,
    path: &str,
    kind: SourceKind,
    scanned_bytes: &mut usize,
    mut visit: impl FnMut(usize, &str),
) -> EngineResult<bool> {
    if kind == SourceKind::Pdf {
        let text = engine.pdf_text(path)?;
        for (index, line) in text.lines().enumerate() {
            if scanned_bytes.saturating_add(line.len()) > DOCUMENT_SEARCH_BYTE_CAP {
                return Ok(true);
            }
            *scanned_bytes += line.len();
            visit(index + 1, line);
        }
        return Ok(false);
    }

    let mut hit_cap = false;
    docs::grep_lines(path, |number, line| {
        if scanned_bytes.saturating_add(line.len()) > DOCUMENT_SEARCH_BYTE_CAP {
            hit_cap = true;
            return false;
        }
        *scanned_bytes += line.len();
        visit(number, line);
        true
    })?;
    Ok(hit_cap)
}

impl EngineState {
    /// Internal storage boundary for workspace-scoped analytical outputs.
    pub(crate) fn analytical_data_dir(&self) -> &Path {
        &self.data_dir
    }

    pub fn presentation_results(
        &self,
        workspace: &str,
        turn_id: &str,
    ) -> EngineResult<serde_json::Value> {
        let run = self.analysis_turn_load(turn_id)?;
        if run.workspace.as_deref() != Some(workspace) {
            return Err(EngineError::msg(
                "analysis belongs to a different workspace",
            ));
        }
        let results = crate::engine::artifacts::eligible_results(&run)?;
        Ok(serde_json::json!({ "run": run, "results": results }))
    }

    fn resolve_artifact_context_refs(
        &self,
        references: &[ContextReference],
        catalog: &Catalog,
        selection: Option<&InvestigationSourceSelection>,
    ) -> EngineResult<Vec<ContextReference>> {
        if references.len() > 64 {
            return Err(EngineError::msg("too many selected context references"));
        }
        references.iter().map(|reference| {
            if reference.kind != "artifact" { return Ok(reference.clone()); }
            let workspace = catalog.workspace.as_deref().ok_or(EngineError::NoWorkspace)?;
            let (id, revision) = reference.key.rsplit_once('#').ok_or_else(|| EngineError::msg("select an artifact with an exact revision"))?;
            let revision = revision.parse::<u64>().map_err(|_| EngineError::msg("invalid artifact revision"))?;
            let resolved = crate::engine::artifacts::resolve(&self.data_dir, workspace, id, Some(revision))?;
            if let Some(selection) = selection.filter(|selection| !selection.is_all_sources()) {
                let scope = resolved.run.source_scope.as_ref().ok_or_else(|| EngineError::msg("this saved output has no recorded source scope; it cannot be attached to a narrowed Investigation"))?;
                if scope.used_sources.iter().any(|source| !selection.includes_path(&source.source)) {
                    return Err(EngineError::msg("saved output uses sources outside this Investigation's selection"));
                }
            }
            let detail = serde_json::json!({
                "original_question": resolved.run.question,
                "source_revision": resolved.run.workspace_revision,
                "same_source_revision": resolved.run.workspace_revision == catalog.revision,
                "interpretation": resolved.run.contract,
                "definitions": resolved.run.investigation_context.as_ref().map(|context| &context.definitions),
                "instruction": "User explicitly selected this saved analysis as the target of the current request. Preserve its question and stated meaning except for the requested change. These references explain prior work; inspect current sources and execute a new analysis when changing the calculation. Historical answer prose is not new evidence."
            }).to_string();
            if detail.len() > 24 * 1024 { return Err(EngineError::msg("selected artifact context exceeds its budget")); }
            Ok(ContextReference { kind: "artifact".into(), key: reference.key.clone(), label: resolved.artifact.title, detail: Some(detail) })
        }).collect()
    }

    pub fn new(data_dir: &Path) -> EngineResult<Self> {
        Self::build(data_dir, true, Arc::new(Mutex::new(HashMap::new())))
    }

    /// Construct a workspace-scoped runtime sharing app settings, auth, and
    /// conversation persistence at `data_dir`. Only the app runtime performs
    /// one-time DB migrations and provider reconciliation.
    pub(crate) fn new_workspace_runtime(
        data_dir: &Path,
        cancel: CancelRegistry,
    ) -> EngineResult<Self> {
        Self::build(data_dir, false, cancel)
    }

    fn build(
        data_dir: &Path,
        initialize_app_state: bool,
        cancel: CancelRegistry,
    ) -> EngineResult<Self> {
        // reqwest is built with `rustls-no-provider`; install `ring` once
        // (process-global). Doing it here covers both the app and the tests.
        static CRYPTO: std::sync::Once = std::sync::Once::new();
        CRYPTO.call_once(|| {
            let _ = rustls::crypto::ring::default_provider().install_default();
        });

        std::fs::create_dir_all(data_dir)
            .map_err(|e| EngineError::io(format!("create {}", data_dir.display()), e))?;
        let data = data::open_engine(data_dir)?;
        let sqlite = open_or_recover(&data_dir.join("fella.db"));
        let secrets = Secrets::new(data_dir);
        if initialize_app_state {
            migrate_legacy_key(&sqlite, &secrets);
            reconcile_provider(&sqlite, &secrets);
        }
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(180))
            .connect_timeout(std::time::Duration::from_secs(10))
            // Keep a few idle sockets so a multi-turn question doesn't pay a
            // fresh TLS handshake on every model call. Retire them after 20 s so
            // a hosted LB that silently drops an idle connection isn't hit on
            // reuse (a dropped connection is a retryable `is_connect` error).
            .pool_max_idle_per_host(4)
            .pool_idle_timeout(std::time::Duration::from_secs(20))
            .build()
            // A default client would silently drop the timeouts above and stall
            // the UI on a wedged request; a build failure here is not
            // recoverable, so surface it.
            .expect("build HTTP client");
        Ok(Self {
            workspace: Mutex::new(WorkspaceState::new(data, None)),
            workspace_gate: Arc::new(WorkspaceGate::default()),
            mount_serial: Mutex::new(()),
            investigation_ensure_lock: Mutex::new(()),
            question_state_lock: Mutex::new(()),
            sqlite: Mutex::new(sqlite),
            inner: Mutex::new(Inner::default()),
            http,
            secrets,
            data_dir: data_dir.to_path_buf(),
            cancel,
            deleted_conversations: Mutex::new(HashSet::new()),
            doc_cache: Mutex::new(HashMap::new()),
        })
    }

    pub(crate) fn cancel_registry(&self) -> CancelRegistry {
        Arc::clone(&self.cancel)
    }

    pub(crate) fn invalidate_capability_schema_cache(&self) {
        self.workspace
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .schema_cache = None;
    }

    /// Stop the in-progress `ask()` for one conversation (if any).
    pub fn cancel_run(&self, conversation_id: &str) {
        if let Some(flag) = self
            .cancel
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(conversation_id)
        {
            flag.store(true, Ordering::Relaxed);
        }
    }

    /// Drop a conversation's distilled memory and stop-flag it's a closed tab.
    pub fn forget_conversation(&self, conversation_id: &str) {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .sessions
            .remove(conversation_id);
        self.cancel
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(conversation_id);
    }

    fn allow_conversation_persistence(&self, conversation_id: &str) {
        self.deleted_conversations
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .remove(conversation_id);
    }

    /// Rebuild a conversation's bounded prompt projection after a restart.
    /// Keep prior conversation text for follow-up resolution across mounts;
    /// `session_block` scopes analytical hints to the exact current snapshot.
    fn hydrate_session_from_archive(&self, conversation_id: &str) {
        let already_hydrated = self
            .inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .sessions
            .contains_key(conversation_id);
        if already_hydrated {
            return;
        }
        let Ok(body) = self.conversation_load(conversation_id) else {
            return;
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&body) else {
            return;
        };
        let mut turns = archived_turns(&value);
        // The transcript is a UI projection and does not necessarily embed the
        // canonical Investigation snapshot. Recover the exact selection from
        // the saved run record so a restarted thread retains its source scope.
        for turn in &mut turns {
            if turn.source_selection.is_none() {
                turn.source_selection = turn
                    .turn_id
                    .as_deref()
                    .and_then(|turn_id| analysis_store::load(&self.data_dir, turn_id).ok())
                    .and_then(|record| record.investigation_context)
                    .map(|context| context.source_selection);
            }
        }
        if turns.is_empty() {
            return;
        }
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner
            .sessions
            .entry(conversation_id.to_string())
            .or_insert_with(|| SessionMemory {
                turns,
                last_used: 0,
            });
    }

    /// Write a transcript to `<data_dir>/conversations/`. `body` is the JSON
    /// the UI assembled (`{id, saved_at_ms, workspace, messages, title?}`);
    /// it is re-serialized pretty so the file reads well when opened by
    /// hand. Returns the file path.
    ///
    /// A conversation is archived repeatedly over its life -- as soon as it
    /// has content, again on every later turn, and on a rename -- so this
    /// always writes the latest content. If a file for `id` already exists
    /// its own path (and original timestamp prefix) is reused rather than
    /// creating a new one each time.
    pub fn archive_conversation(&self, id: &str, body: &str) -> EngineResult<String> {
        let deleted = self
            .deleted_conversations
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if deleted.contains(id) {
            return Ok(String::new());
        }
        let value: serde_json::Value = serde_json::from_str(body)
            .map_err(|e| EngineError::msg(format!("conversation body is not JSON: {e}")))?;

        let slug: String = id
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .take(32)
            .collect();
        let slug = if slug.is_empty() {
            "unknown".to_string()
        } else {
            slug
        };
        let suffix = format!("_{slug}.json");

        let dir = self.data_dir.join("conversations");
        std::fs::create_dir_all(&dir)
            .map_err(|e| EngineError::io(format!("create {}", dir.display()), e))?;

        let existing = std::fs::read_dir(&dir).ok().and_then(|entries| {
            entries
                .flatten()
                .find(|e| e.file_name().to_string_lossy().ends_with(&suffix))
                .map(|e| e.path())
        });

        // A command can leave a system note in the transcript, but it is not a
        // conversation. If a previously archived transcript becomes command-only,
        // remove its file instead of leaving an empty row in history.
        if !has_actual_question(&value) {
            if let Some(path) = existing.as_ref() {
                let _ = std::fs::remove_file(path);
            }
            return Ok(String::new());
        }

        let path = existing.unwrap_or_else(|| {
            let ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0);
            dir.join(format!("conv_{ms}{suffix}"))
        });
        let pretty = serde_json::to_string_pretty(&value).unwrap_or_else(|_| body.to_string());

        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, &pretty)
            .map_err(|e| EngineError::io(format!("write {}", tmp.display()), e))?;
        std::fs::rename(&tmp, &path)
            .map_err(|e| EngineError::io(format!("replace {}", path.display()), e))?;

        Ok(path.display().to_string())
    }

    /// Set (or, with an empty/whitespace-only title, clear) a custom display
    /// title for one archived conversation that isn't necessarily open in a
    /// live tab right now -- e.g. renaming a history row from the sidebar
    /// without opening it first. Same slug-suffix lookup as `delete_conversation`.
    pub fn rename_conversation(&self, id: &str, title: &str) -> EngineResult<()> {
        let dir = self.data_dir.join("conversations");
        let slug: String = id
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .take(32)
            .collect();
        let suffix = format!("_{slug}.json");
        let entries = std::fs::read_dir(&dir)
            .map_err(|e| EngineError::io(format!("read {}", dir.display()), e))?;
        let path = entries
            .flatten()
            .find(|e| e.file_name().to_string_lossy().ends_with(&suffix))
            .map(|e| e.path())
            .ok_or_else(|| {
                EngineError::msg("that conversation couldn't be found it may have been deleted")
            })?;

        let text = std::fs::read_to_string(&path)
            .map_err(|e| EngineError::io(format!("read {}", path.display()), e))?;
        let mut value: serde_json::Value = serde_json::from_str(&text)
            .map_err(|e| EngineError::msg(format!("conversation file is not JSON: {e}")))?;
        let trimmed = title.trim();
        if let Some(obj) = value.as_object_mut() {
            if trimmed.is_empty() {
                obj.remove("title");
            } else {
                obj.insert(
                    "title".into(),
                    serde_json::Value::String(trimmed.to_string()),
                );
            }
        }
        let pretty = serde_json::to_string_pretty(&value).unwrap_or(text);

        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, &pretty)
            .map_err(|e| EngineError::io(format!("write {}", tmp.display()), e))?;
        std::fs::rename(&tmp, &path)
            .map_err(|e| EngineError::io(format!("replace {}", path.display()), e))?;

        Ok(())
    }

    /// The conversations directory and how many `*.json` archives it holds.
    pub fn conversations_info(&self) -> ConversationsInfo {
        let dir = self.data_dir.join("conversations");
        let count = std::fs::read_dir(&dir)
            .map(|entries| {
                entries
                    .flatten()
                    .filter(|e| e.file_name().to_string_lossy().ends_with(".json"))
                    .count()
            })
            .unwrap_or(0);
        ConversationsInfo {
            path: dir.display().to_string(),
            count,
        }
    }

    /// Every archived conversation, newest first, with enough to pick one
    /// from without knowing its id: when, what folder it was about, its
    /// first actual question, and how long it ran. Legacy command-only files
    /// are removed while the list is read so they do not linger in history.
    pub fn conversations_list(&self) -> Vec<ConversationSummary> {
        let dir = self.data_dir.join("conversations");
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|x| x.to_str()) != Some("json") {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else {
                continue;
            };
            if !has_actual_question(&v) {
                let _ = std::fs::remove_file(path);
                continue;
            }
            let Some(id) = v
                .get("id")
                .and_then(|value| value.as_str())
                .map(str::to_string)
            else {
                continue;
            };
            let saved_at_ms = v.get("saved_at_ms").and_then(|x| x.as_i64()).unwrap_or(0);
            let archived_workspace = v
                .get("workspace")
                .and_then(|x| x.as_str())
                .map(str::to_string);
            let workspace = archived_conversation_workspace(&v).unwrap_or(archived_workspace);
            let Some(messages) = v.get("messages").and_then(|value| value.as_array()) else {
                continue;
            };
            let Some(first_question) = messages
                .iter()
                .find(|message| is_actual_question_message(message))
            else {
                continue;
            };
            let preview = first_question
                .get("text")
                .and_then(|text| text.as_str())
                .map(|text| cap_chars(text, 80))
                .unwrap_or_default();
            let first_question_message_id = first_question
                .get("id")
                .and_then(|value| value.as_str())
                .map(str::to_owned);
            let title = v.get("title").and_then(|x| x.as_str()).map(str::to_string);
            out.push(ConversationSummary {
                id,
                saved_at_ms,
                workspace,
                first_question_message_id,
                preview,
                message_count: conversational_message_count(messages),
                title,
            });
        }
        out.sort_by_key(|c| std::cmp::Reverse(c.saved_at_ms));
        out
    }

    /// Raw JSON text of one archived conversation `{id, workspace, messages}`,
    /// by id looked up the same slug-suffix way `archive_conversation` writes
    /// it. The frontend parses this itself (same shape it wrote when it was
    /// archived); no need to round-trip every message/evidence field through
    /// a typed Rust struct just to pass it straight back through.
    pub fn conversation_load(&self, id: &str) -> EngineResult<String> {
        let dir = self.data_dir.join("conversations");
        let slug: String = id
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .take(32)
            .collect();
        let suffix = format!("_{slug}.json");
        let entries = std::fs::read_dir(&dir)
            .map_err(|e| EngineError::io(format!("read {}", dir.display()), e))?;
        for entry in entries.flatten() {
            if entry.file_name().to_string_lossy().ends_with(&suffix) {
                return std::fs::read_to_string(entry.path())
                    .map_err(|e| EngineError::io(format!("read {}", entry.path().display()), e));
            }
        }
        Err(EngineError::msg(
            "that conversation couldn't be found it may have been deleted",
        ))
    }

    /// Find canonical runs already present in an archived transcript that are
    /// safe to associate with this workspace's Investigation. The transcript
    /// supplies order and candidate IDs; the immutable run record supplies
    /// conversation and workspace identity. Missing or older non-canonical
    /// runs remain available through the conversation history fallback.
    fn archived_analysis_turn_ids(&self, conversation_id: &str, workspace_id: &str) -> Vec<String> {
        let Ok(body) = self.conversation_load(conversation_id) else {
            return Vec::new();
        };
        let Ok(archive) = serde_json::from_str::<serde_json::Value>(&body) else {
            log::debug!(
                "could not parse archived conversation {conversation_id} for run migration"
            );
            return Vec::new();
        };
        if archive.get("id").and_then(serde_json::Value::as_str) != Some(conversation_id) {
            return Vec::new();
        }

        let Some(messages) = archive
            .get("messages")
            .and_then(serde_json::Value::as_array)
        else {
            return Vec::new();
        };
        let mut seen = HashSet::new();
        let mut linked = Vec::new();
        for message in messages {
            if message.get("role").and_then(serde_json::Value::as_str) != Some("assistant") {
                continue;
            }
            let Some(turn_id) = message
                .pointer("/answer/turn_id")
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|turn_id| !turn_id.is_empty())
            else {
                continue;
            };
            if !seen.insert(turn_id.to_owned()) {
                continue;
            }
            let Ok(turn) = analysis_store::load(&self.data_dir, turn_id) else {
                continue;
            };
            if turn.id == turn_id
                && turn.conversation_id == conversation_id
                && turn.workspace.as_deref() == Some(workspace_id)
            {
                linked.push(turn_id.to_owned());
            }
        }
        linked
    }

    /// Remove one archived conversation's file, by the same slug-suffix
    /// lookup `conversation_load` uses. Used by the sidebar's per-row delete.
    pub fn delete_conversation(&self, id: &str) -> EngineResult<()> {
        let _investigation_guard = self
            .investigation_ensure_lock
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let dir = self.data_dir.join("conversations");
        let slug: String = id
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .take(32)
            .collect();
        let suffix = format!("_{slug}.json");
        let entries = std::fs::read_dir(&dir)
            .map_err(|e| EngineError::io(format!("read {}", dir.display()), e))?;
        let archive = entries.flatten().find_map(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .ends_with(&suffix)
                .then(|| entry.path())
        });
        let Some(archive) = archive else {
            return Err(EngineError::msg(
                "that conversation couldn't be found it may have been deleted",
            ));
        };

        // Serialize deletion with both transcript and canonical-turn writes.
        // The marker stays set after success so late UI persistence cannot
        // resurrect the transcript; it is removed if any deletion step fails.
        self.deleted_conversations
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .insert(id.to_string());
        self.cancel_run(id);
        self.forget_conversation(id);
        if let Err(error) = analysis_store::delete_conversation(&self.data_dir, id) {
            self.allow_conversation_persistence(id);
            return Err(error);
        }
        if let Err(error) = investigation::delete_for_conversation(&self.data_dir, id) {
            self.allow_conversation_persistence(id);
            return Err(error);
        }
        if let Err(error) = std::fs::remove_file(&archive) {
            self.allow_conversation_persistence(id);
            return Err(EngineError::io(
                format!("remove {}", archive.display()),
                error,
            ));
        }
        Ok(())
    }

    /// Load one backend-owned analytical turn. The UI transcript remains the
    /// compact conversational projection; this record is the inspectable
    /// contract/plan/trace/result used by future replay and rerun features.
    pub fn analysis_turn_load(&self, turn_id: &str) -> EngineResult<AnalysisTurn> {
        analysis_store::load(&self.data_dir, turn_id)
    }

    /// Create one durable Investigation record without coupling it to a
    /// transcript or execution record. Automatic conversation association is
    /// handled by the product layer, not by the analytics loop.
    pub fn investigation_create(
        &self,
        workspace_id: &str,
        title: &str,
        initial_request: Option<&str>,
        source_message_id: Option<&str>,
        conversation_id: Option<&str>,
    ) -> EngineResult<Investigation> {
        investigation::create(
            &self.data_dir,
            workspace_id,
            title,
            initial_request,
            source_message_id,
            conversation_id,
        )
    }

    /// Mark an Investigation complete only when every active Question is fully
    /// answered; serialized with Question run-state updates.
    pub fn investigation_mark_complete(&self, id: &str) -> EngineResult<Investigation> {
        let _guard = self
            .question_state_lock
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        investigation::mark_complete(&self.data_dir, id)
    }

    /// Return a completed Investigation to active analyst work.
    pub fn investigation_reopen(&self, id: &str) -> EngineResult<Investigation> {
        let _guard = self
            .question_state_lock
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        investigation::reopen(&self.data_dir, id)
    }

    /// Legacy resolver for conversation-seeded records and archived-run
    /// migration. New chats do not create Investigations through this path.
    pub fn investigation_ensure(
        &self,
        workspace_id: &str,
        conversation_id: &str,
        initial_request: &str,
        source_message_id: &str,
    ) -> EngineResult<Investigation> {
        let _guard = self
            .investigation_ensure_lock
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let investigation = investigation::ensure_for_conversation(
            &self.data_dir,
            workspace_id,
            conversation_id,
            initial_request,
            source_message_id,
        )?;

        let archived_turn_ids = self.archived_analysis_turn_ids(conversation_id, workspace_id);
        let mut linked_investigation = investigation;
        for batch in archived_turn_ids.chunks(MAX_ANALYSIS_TURN_LINKS_PER_UPDATE) {
            match self.investigation_link_analysis_turns(&linked_investigation.id, batch) {
                Ok(updated) => linked_investigation = updated,
                Err(error) => {
                    // Migration is opportunistic: keep the Investigation and
                    // its transcript usable if run-link persistence fails. A
                    // later ensure call can safely resume from any saved batch.
                    log::warn!(
                        "could not associate archived analysis runs with Investigation {}: {error}",
                        linked_investigation.id
                    );
                    break;
                }
            }
        }
        Ok(linked_investigation)
    }

    /// Update the current brief without changing the source Conversation
    /// message or its archived transcript.
    pub fn investigation_update_brief(
        &self,
        id: &str,
        brief: InvestigationBrief,
        reviewed_message_id: Option<&str>,
    ) -> EngineResult<Investigation> {
        investigation::update_brief(&self.data_dir, id, brief, reviewed_message_id)
    }

    /// Naming changes presentation, not the Investigation's analytical state.
    pub fn investigation_rename(&self, id: &str, title: &str) -> EngineResult<Investigation> {
        let _guard = self
            .question_state_lock
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        investigation::rename(&self.data_dir, id, title)
    }

    /// Replace the ordered, analyst-editable Question plan. Existing run
    /// records are independent and remain unchanged.
    pub fn investigation_update_questions(
        &self,
        id: &str,
        questions: Vec<InvestigationQuestionInput>,
    ) -> EngineResult<Investigation> {
        let _guard = self
            .question_state_lock
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        investigation::update_questions(&self.data_dir, id, questions)
    }

    /// Replace an Investigation's active Definitions/Assumptions while keeping
    /// independent records and their revision history.
    pub fn investigation_update_definitions(
        &self,
        id: &str,
        definitions: Vec<InvestigationDefinitionInput>,
    ) -> EngineResult<Investigation> {
        let _guard = self
            .question_state_lock
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        investigation::update_definitions(&self.data_dir, id, definitions)
    }

    /// Update the current source selection for one Investigation. Completed
    /// turns retain their own immutable context snapshot.
    pub fn investigation_update_source_selection(
        &self,
        id: &str,
        selection: InvestigationSourceSelection,
    ) -> EngineResult<Investigation> {
        let _guard = self
            .question_state_lock
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        investigation::update_source_selection(&self.data_dir, id, selection)
    }

    /// Load one Definition record, including its prior analyst-edited values.
    pub fn investigation_definition_load(&self, id: &str) -> EngineResult<InvestigationDefinition> {
        investigation::definition_load(&self.data_dir, id)
    }

    /// Save a concise conclusion with immutable links to an Investigation
    /// Question, canonical AnalysisTurn, and exact evidence IDs from that run.
    pub fn investigation_create_finding(
        &self,
        id: &str,
        input: InvestigationFindingInput,
    ) -> EngineResult<Investigation> {
        let _guard = self
            .question_state_lock
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        investigation::create_finding(&self.data_dir, id, input)
    }

    /// Change only a Finding's claim text. Its source links remain immutable.
    pub fn investigation_update_finding_claim(
        &self,
        id: &str,
        finding_id: &str,
        claim: &str,
    ) -> EngineResult<Investigation> {
        let _guard = self
            .question_state_lock
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        investigation::update_finding_claim(&self.data_dir, id, finding_id, claim)
    }

    /// Edit a Finding's interpretation while preserving its provenance links.
    pub fn investigation_update_finding(
        &self,
        id: &str,
        finding_id: &str,
        update: InvestigationFindingUpdate,
    ) -> EngineResult<Investigation> {
        let _guard = self
            .question_state_lock
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        investigation::update_finding(&self.data_dir, id, finding_id, update)
    }

    /// Load one Finding independently from its parent Investigation.
    pub fn investigation_finding_load(&self, id: &str) -> EngineResult<InvestigationFinding> {
        investigation::finding_load(&self.data_dir, id)
    }

    /// Load an independently persisted Question record by stable identity.
    pub fn investigation_question_load(&self, id: &str) -> EngineResult<InvestigationQuestion> {
        let question = investigation::question_load(&self.data_dir, id)?;
        if question.answer_status == InvestigationQuestionAnswerStatus::Running {
            let _ = self.recover_interrupted_question_runs(&question.investigation_id)?;
        }
        investigation::question_load(&self.data_dir, id)
    }

    /// Ask the configured model for a draft decomposition without adding a
    /// Conversation message or persisting the proposal.
    pub async fn investigation_propose_questions(
        &self,
        id: &str,
        main_question: &str,
    ) -> EngineResult<QuestionBreakdownProposal> {
        let investigation = investigation::load(&self.data_dir, id)?;
        let main_question = main_question.trim();
        if main_question.is_empty() {
            return Err(EngineError::msg(
                "enter a main Question before requesting a breakdown",
            ));
        }
        if main_question.chars().count() > 12_000 {
            return Err(EngineError::msg("main Question is too long"));
        }
        let input = serde_json::json!({
            "main_question": main_question,
            "audience": investigation.brief.audience,
            "desired_output": investigation.brief.desired_output,
            "constraints": investigation.brief.constraints,
        });
        let system = "You are helping a data analyst decompose one main Investigation Question into a small, useful sequence of independently answerable subquestions. Do not answer the question, invent data or findings, or include the main question again. Keep the breakdown grounded in the provided brief, avoid redundant steps, and return only valid JSON with this exact shape: {\"subquestions\":[\"...\"]}. Return between 2 and 6 concise subquestions.";
        let raw = self.ask_once(None, system, &input.to_string()).await?;
        investigation::parse_question_breakdown(&raw, main_question)
    }

    /// Associate canonical, persisted AnalysisTurns without copying their
    /// questions, results, or evidence into the Investigation.
    pub fn investigation_link_analysis_turns(
        &self,
        id: &str,
        analysis_turn_ids: &[String],
    ) -> EngineResult<Investigation> {
        let _guard = self
            .question_state_lock
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if analysis_turn_ids.len() > MAX_ANALYSIS_TURN_LINKS_PER_UPDATE {
            return Err(EngineError::msg("too many analysis runs to link at once"));
        }

        let investigation = investigation::load(&self.data_dir, id)?;
        let mut seen = HashSet::new();
        let mut new_ids = Vec::new();
        for turn_id in analysis_turn_ids {
            let turn_id = turn_id.trim();
            if investigation
                .analysis_turn_ids
                .iter()
                .any(|linked| linked == turn_id)
                || !seen.insert(turn_id.to_owned())
            {
                continue;
            }
            let turn = analysis_store::load(&self.data_dir, turn_id)?;
            if turn.workspace.as_deref() != Some(investigation.workspace_id.as_str()) {
                return Err(EngineError::msg(format!(
                    "analysis turn {turn_id} belongs to a different workspace"
                )));
            }
            new_ids.push(turn_id.to_owned());
        }
        investigation::link_analysis_turns(&self.data_dir, id, &new_ids)
    }

    /// Attach one saved run to the Questions it addresses. The AnalysisTurn
    /// remains canonical and immutable; only Investigation-side references
    /// change.
    pub fn investigation_set_analysis_turn_questions(
        &self,
        id: &str,
        turn_id: &str,
        question_ids: &[String],
    ) -> EngineResult<Investigation> {
        let _guard = self
            .question_state_lock
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        const MAX_QUESTIONS_PER_RUN: usize = 100;
        if question_ids.len() > MAX_QUESTIONS_PER_RUN {
            return Err(EngineError::msg(
                "too many Questions linked to one analysis run",
            ));
        }
        let investigation = investigation::load(&self.data_dir, id)?;
        let turn = analysis_store::load(&self.data_dir, turn_id)?;
        if turn.workspace.as_deref() != Some(investigation.workspace_id.as_str()) {
            return Err(EngineError::msg(format!(
                "analysis turn {turn_id} belongs to a different workspace"
            )));
        }
        investigation::set_analysis_turn_questions(&self.data_dir, id, turn_id, question_ids)
    }

    /// Load an Investigation independently of the conversation UI.
    pub fn investigation_load(&self, id: &str) -> EngineResult<Investigation> {
        self.recover_interrupted_question_runs(id)
    }

    /// List durable Investigation records, newest activity first.
    pub fn investigations_list(
        &self,
        active_conversation_ids: Option<&[String]>,
    ) -> EngineResult<Vec<Investigation>> {
        let _guard = self
            .investigation_ensure_lock
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if let Some(active_conversation_ids) = active_conversation_ids {
            if let Some(mut preserved_ids) = self.archived_conversation_ids() {
                preserved_ids.extend(active_conversation_ids.iter().cloned());
                investigation::delete_orphaned_source_investigations(
                    &self.data_dir,
                    &preserved_ids,
                )?;
            }
        }
        investigation::list(&self.data_dir)?
            .into_iter()
            .map(|item| self.recover_interrupted_question_runs(&item.id))
            .collect()
    }

    /// Return `None` if the archive directory cannot be inspected completely;
    /// callers then skip orphan cleanup so a transient I/O or parse failure
    /// cannot be mistaken for user deletion.
    fn archived_conversation_ids(&self) -> Option<HashSet<String>> {
        let directory = self.data_dir.join("conversations");
        let entries = match std::fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Some(HashSet::new());
            }
            Err(_) => return None,
        };

        let mut ids = HashSet::new();
        for entry in entries {
            let entry = entry.ok()?;
            let path = entry.path();
            if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
                continue;
            }
            let body = std::fs::read_to_string(&path).ok()?;
            let value = serde_json::from_str::<serde_json::Value>(&body).ok()?;
            if !has_actual_question(&value) {
                continue;
            }
            let id = value.get("id")?.as_str()?;
            ids.insert(id.to_owned());
        }
        Some(ids)
    }

    fn recover_interrupted_question_runs(&self, id: &str) -> EngineResult<Investigation> {
        let _guard = self
            .question_state_lock
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let investigation = investigation::load(&self.data_dir, id)?;
        let conversation_is_running =
            investigation
                .conversation_id
                .as_deref()
                .is_some_and(|conversation_id| {
                    self.cancel
                        .lock()
                        .unwrap_or_else(|error| error.into_inner())
                        .contains_key(conversation_id)
                });

        for question in investigation.questions.as_deref().unwrap_or_default() {
            let is_interrupted =
                question.answer_status == InvestigationQuestionAnswerStatus::Running;
            let is_false_blocked = question.answer_status
                == InvestigationQuestionAnswerStatus::Blocked
                && question.answer_status_reason.as_deref()
                    == Some(investigation::INTERRUPTED_QUESTION_RUN_REASON);
            // Older releases collapsed clarification, verification review,
            // and insufficient evidence into `incomplete`. Re-project those
            // terminal records from the canonical AnalysisTurn so stale local
            // Questions stop claiming every such result is a partial answer.
            let is_legacy_incomplete =
                question.answer_status == InvestigationQuestionAnswerStatus::Incomplete;
            if (!is_interrupted && !is_false_blocked && !is_legacy_incomplete)
                || conversation_is_running
            {
                continue;
            }
            if let Some(turn_id) = question.latest_analysis_turn_id.as_deref() {
                if let Some((answer_status, reason)) =
                    self.recover_saved_question_outcome(&investigation, question, turn_id)
                {
                    investigation::question_run_recovered(
                        &self.data_dir,
                        &investigation.id,
                        &question.id,
                        turn_id,
                        answer_status,
                        reason.as_deref(),
                    )?;
                } else if is_interrupted {
                    investigation::question_run_finished(
                        &self.data_dir,
                        &investigation.id,
                        &question.id,
                        turn_id,
                        InvestigationQuestionAnswerStatus::Blocked,
                        Some(investigation::INTERRUPTED_QUESTION_RUN_REASON),
                    )?;
                }
            }
        }
        investigation::load(&self.data_dir, id)
    }

    fn recover_saved_question_outcome(
        &self,
        investigation: &Investigation,
        question: &crate::engine::investigation::InvestigationQuestion,
        turn_id: &str,
    ) -> Option<(InvestigationQuestionAnswerStatus, Option<String>)> {
        let turn = analysis_store::load(&self.data_dir, turn_id).ok()?;
        let context = turn.investigation_context.as_ref()?;
        if turn.id != turn_id
            || investigation.conversation_id.as_deref() != Some(turn.conversation_id.as_str())
            || turn.workspace.as_deref() != Some(investigation.workspace_id.as_str())
            || context.investigation_id != investigation.id
            || context.question_id.as_deref() != Some(question.id.as_str())
        {
            return None;
        }

        let current_context = investigation::context_snapshot_for_question(
            &self.data_dir,
            &investigation.id,
            Some(&question.id),
        )
        .ok()?;
        if &current_context != context {
            return Some((
                InvestigationQuestionAnswerStatus::Outdated,
                Some("The Question, source scope, or definitions changed during this run. Run the current Question again.".to_owned()),
            ));
        }
        Some(question_answer_status_from_turn(&turn))
    }

    /// Recent local run metadata for Settings. This is a projection of saved
    /// canonical turns plus the existing coarse friction log, never a second
    /// transcript or telemetry store.
    pub fn recent_run_log(&self, limit: usize) -> Vec<RunLogEntry> {
        let mut entries = analysis_store::recent(&self.data_dir, limit);
        entries.extend(
            crate::engine::friction::recent(&self.data_dir)
                .into_iter()
                .enumerate()
                .map(|(index, signal)| RunLogEntry {
                    id: format!("trigger-{}-{index}", signal.at_ms),
                    at_ms: signal.at_ms,
                    kind: RunLogKind::Trigger,
                    mode: None,
                    model: None,
                    elapsed_ms: None,
                    model_calls: Vec::new(),
                    operations: Vec::new(),
                    prior_analysis_count: 0,
                    context_reference_count: 0,
                    clarification_continuation: false,
                    rerun: false,
                    outcome: "triggered".into(),
                    trigger: Some(signal.reason),
                    trigger_steps: Some(signal.steps),
                    trigger_errors: Some(signal.errors),
                }),
        );
        entries.sort_by(|a, b| b.at_ms.cmp(&a.at_ms).then_with(|| a.id.cmp(&b.id)));
        entries.truncate(limit.clamp(1, 100));
        entries
    }

    /// Compare a stored turn's source snapshot with the currently mounted
    /// workspace before a replay. A revision hash answers whether the data
    /// changed; the compact source diff explains how it changed when the turn
    /// was written by a revision-aware runtime.
    pub fn analysis_turn_replay_status(
        &self,
        turn_id: &str,
    ) -> EngineResult<AnalysisTurnReplayStatus> {
        let original = self.analysis_turn_load(turn_id)?;
        let current = self.catalog();
        let same_workspace = match (&original.workspace, &current.workspace) {
            (Some(original), Some(current)) => original == current,
            (None, None) => true,
            _ => false,
        };
        let revision_changed = original.workspace_revision != current.revision;
        let current_snapshot = revision_snapshot(&current);
        let source_changes = if same_workspace {
            match (
                original.workspace_snapshot.as_ref(),
                current_snapshot.as_ref(),
            ) {
                (Some(previous), Some(current)) => previous.changes_to(current),
                _ => Vec::new(),
            }
        } else {
            Vec::new()
        };
        Ok(AnalysisTurnReplayStatus {
            turn_id: original.id,
            workspace: original.workspace,
            original_revision: original.workspace_revision,
            current_revision: current.revision,
            same_workspace,
            revision_changed,
            snapshot_available: original.workspace_snapshot.is_some()
                && same_workspace
                && current_snapshot.is_some(),
            can_rerun: same_workspace && current.workspace.is_some(),
            source_changes,
            source_scope: original.source_scope,
        })
    }

    /// Rerun a stored analytical question against the currently mounted
    /// workspace. The original folder is part of the canonical record, so a
    /// rerun cannot silently execute against a different mount. The new turn
    /// is persisted normally and linked back to the original record.
    pub async fn analysis_turn_rerun(
        &self,
        turn_id: &str,
        model: Option<&str>,
        inspect: bool,
        emit: impl Fn(AskEvent) + Send + Sync,
    ) -> EngineResult<Answer> {
        let original = self.analysis_turn_load(turn_id)?;
        let current = self.catalog();
        let Some(current_workspace) = current.workspace.as_deref() else {
            return Err(EngineError::msg(
                "open the original workspace before rerunning this analysis",
            ));
        };
        if let Some(expected_workspace) = original.workspace.as_deref() {
            if expected_workspace != current_workspace {
                return Err(EngineError::msg(format!(
                    "rerun requires the original workspace: {}",
                    expected_workspace
                )));
            }
        }

        let clarification_reply = original
            .clarification_of
            .clone()
            .zip(original.clarification_response.clone())
            .map(|(turn_id, response)| ClarificationReply { turn_id, response });
        let answer = self
            .ask_with_mode_and_context_and_clarification(
                &original.conversation_id,
                &original.question,
                AskOptions {
                    model,
                    inspect,
                    context_refs: &original.context_refs,
                    clarification_reply,
                    investigation_id: None,
                    question_id: None,
                    investigation_context: original.investigation_context.clone(),
                    rerun_of: Some(&original.id),
                },
                emit,
            )
            .await?;
        Ok(answer)
    }

    pub fn catalog(&self) -> Catalog {
        let workspace = self.workspace.lock().unwrap_or_else(|e| e.into_inner());
        Catalog {
            workspace: workspace
                .workspace
                .as_ref()
                .map(|p| p.display().to_string()),
            revision: workspace.revision.clone(),
            indexed_at_ms: workspace.indexed_at_ms,
            sources: workspace.sources.clone(),
            skipped: workspace.skipped.clone(),
        }
    }

    /// Return a small page of the mounted inventory without cloning the
    /// workspace's full source catalog. The caller controls filtering and
    /// pagination; rows are rendered from this revision while the lock is held.
    pub fn list_files_page(
        &self,
        filter: catalog::FileListFilter,
        query: Option<&str>,
        offset: usize,
        limit: usize,
    ) -> EngineResult<catalog::FileListPage> {
        let workspace = self.workspace.lock().unwrap_or_else(|e| e.into_inner());
        let root = workspace
            .workspace
            .as_deref()
            .ok_or(EngineError::NoWorkspace)?;
        Ok(catalog::list_files_page(
            root,
            &workspace.sources,
            &workspace.skipped,
            filter,
            query,
            offset,
            limit,
        ))
    }

    /// Build the same bounded inventory page, limited to the files selected
    /// by an Investigation when it is in selected-sources mode.
    pub fn list_files_page_for_selection(
        &self,
        filter: catalog::FileListFilter,
        query: Option<&str>,
        offset: usize,
        limit: usize,
        selection: Option<&InvestigationSourceSelection>,
    ) -> EngineResult<catalog::FileListPage> {
        if selection.is_none_or(InvestigationSourceSelection::is_all_sources) {
            return self.list_files_page(filter, query, offset, limit);
        }
        let selection = selection.expect("selected mode was checked");
        let workspace = self.workspace.lock().unwrap_or_else(|e| e.into_inner());
        let root = workspace
            .workspace
            .as_deref()
            .ok_or(EngineError::NoWorkspace)?;
        let sources: Vec<SourceInfo> = workspace
            .sources
            .iter()
            .filter(|source| {
                selection.includes_path(&catalog::workspace_relative_path(
                    root,
                    Path::new(&source.path),
                ))
            })
            .cloned()
            .collect();
        let skipped: Vec<_> = workspace
            .skipped
            .iter()
            .filter(|file| selection.includes_path(&file.name.replace('\\', "/")))
            .cloned()
            .collect();
        Ok(catalog::list_files_page(
            root, &sources, &skipped, filter, query, offset, limit,
        ))
    }

    /// Resolve a source's workspace-relative path before exposing it to an
    /// Investigation-scoped tool.
    pub fn source_selection_allows_name(
        &self,
        selection: Option<&InvestigationSourceSelection>,
        name: &str,
    ) -> bool {
        let Some(selection) = selection.filter(|selection| !selection.is_all_sources()) else {
            return true;
        };
        let workspace = self.workspace.lock().unwrap_or_else(|e| e.into_inner());
        let Some(root) = workspace.workspace.as_deref() else {
            return false;
        };
        workspace.sources.iter().any(|source| {
            let path = catalog::workspace_relative_path(root, Path::new(&source.path));
            (source.name == name || source.view.as_deref() == Some(name) || path == name)
                && selection.includes_path(&path)
        })
    }

    fn source_selection_allowed_tables(
        &self,
        selection: &InvestigationSourceSelection,
    ) -> HashSet<String> {
        let workspace = self.workspace.lock().unwrap_or_else(|e| e.into_inner());
        let Some(root) = workspace.workspace.as_deref() else {
            return HashSet::new();
        };
        workspace
            .sources
            .iter()
            .filter(|source| {
                source.view.is_some()
                    && selection.includes_path(&catalog::workspace_relative_path(
                        root,
                        Path::new(&source.path),
                    ))
            })
            .flat_map(|source| [Some(source.name.as_str()), source.view.as_deref()])
            .flatten()
            .map(str::to_ascii_lowercase)
            .collect()
    }

    /// Return the semantic projection for the current workspace. `None` means
    /// there is no mounted workspace or no valid revision yet.
    pub fn workspace_model(&self) -> Option<WorkspaceModel> {
        self.workspace
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .model
            .clone()
    }

    fn workspace_model_prompt_block(&self) -> Option<String> {
        let workspace = self.workspace.lock().unwrap_or_else(|e| e.into_inner());
        workspace.model.as_ref().map(WorkspaceModel::prompt_block)
    }

    fn answer_workspace_is_current(&self, answer: &Answer) -> bool {
        let workspace = self.workspace.lock().unwrap_or_else(|e| e.into_inner());
        match &answer.workspace {
            Some(snapshot) => {
                workspace.workspace.as_deref() == Some(Path::new(&snapshot.path))
                    && workspace.revision.as_deref() == Some(snapshot.revision.as_str())
            }
            None => workspace.workspace.is_none() && workspace.revision.is_none(),
        }
    }

    /// Persist the canonical runtime projection after every completed ask.
    /// This is best-effort so a local disk problem never turns a valid answer
    /// into a failed question; the conversation archive remains an independent
    /// fallback projection.
    fn persist_analysis_turn(&self, turn: PersistAnalysisTurn<'_>) {
        let PersistAnalysisTurn {
            conversation_id,
            question,
            context_refs,
            investigation_context,
            clarification,
            context_audit,
            catalog,
            answer,
            rerun_of,
        } = turn;
        let deleted = self
            .deleted_conversations
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if deleted.contains(conversation_id) {
            return;
        }
        let state = if answer.clarification.is_some() {
            TurnState::Clarify
        } else {
            match answer.status {
                crate::engine::evidence::VerificationStatus::Verified => TurnState::Accepted,
                crate::engine::evidence::VerificationStatus::Failed => TurnState::Failed,
                crate::engine::evidence::VerificationStatus::NeedsReview
                | crate::engine::evidence::VerificationStatus::InsufficientData => {
                    TurnState::NeedsReview
                }
            }
        };
        let evidence = answer
            .evidence
            .iter()
            .filter_map(|item| serde_json::to_value(item).ok())
            .collect();
        let mut prior_turn_refs = Vec::new();
        for turn_id in answer.evidence.iter().filter_map(reusable_prior_turn_ref) {
            if !prior_turn_refs.iter().any(|prior| prior == turn_id) {
                prior_turn_refs.push(turn_id.to_string());
            }
        }
        let workspace_snapshot = revision_snapshot(catalog).filter(|snapshot| {
            answer.workspace.as_ref().is_some_and(|workspace| {
                snapshot.path == workspace.path && snapshot.revision == workspace.revision
            })
        });
        let source_scope = workspace_snapshot.as_ref().map(|snapshot| {
            AnalysisSourceScope::from_evidence(&answer.evidence, &snapshot.skipped)
        });
        let record = AnalysisTurn {
            id: answer.turn_id.clone(),
            conversation_id: conversation_id.to_string(),
            question: question.to_string(),
            context_refs: context_refs.to_vec(),
            prior_turn_refs,
            clarification_of: clarification.map(|reply| reply.turn_id.clone()),
            clarification_response: clarification.map(|reply| reply.response.clone()),
            workspace: answer
                .workspace
                .as_ref()
                .map(|workspace| workspace.path.clone()),
            workspace_revision: answer
                .workspace
                .as_ref()
                .map(|workspace| workspace.revision.clone()),
            workspace_snapshot,
            source_scope,
            investigation_context: investigation_context.cloned(),
            context_audit: Some(context_audit.clone()),
            provenance: Some(answer.provenance.clone()),
            rerun_of: rerun_of.map(str::to_owned),
            state,
            contract: answer.contract.clone(),
            plan: answer.plan.clone().unwrap_or_else(|| LogicalPlan {
                strategy: PlanStrategy::DirectTools,
                steps: answer
                    .trace
                    .steps
                    .iter()
                    .map(|step| step.operation.clone())
                    .collect(),
            }),
            trace: answer.trace.clone(),
            verification: Some(VerificationReport {
                status: answer.status,
                checks: answer.verification.clone(),
            }),
            result: AnalysisResult {
                text: answer.text.clone(),
                presentation_spec: answer.presentation_spec.clone(),
                status: answer.status,
                usage: answer.usage,
                verification: answer.verification.clone(),
                evidence,
            },
        };
        if let Err(error) = analysis_store::create(&self.data_dir, &record) {
            log::warn!(
                "analysis turn {} was not persisted: {error}",
                answer.turn_id
            );
        } else if let Some(context) = investigation_context {
            let _guard = self
                .question_state_lock
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            let turn_ids = [answer.turn_id.clone()];
            if let Err(error) = investigation::link_analysis_turns(
                &self.data_dir,
                &context.investigation_id,
                &turn_ids,
            ) {
                log::warn!(
                    "analysis turn {} was saved but not linked to Investigation {}: {error}",
                    answer.turn_id,
                    context.investigation_id
                );
            }
            if let Some(question_id) = context.question_id.as_ref() {
                if let Err(error) = investigation::link_analysis_turn_questions(
                    &self.data_dir,
                    &context.investigation_id,
                    &answer.turn_id,
                    std::slice::from_ref(question_id),
                ) {
                    log::warn!(
                        "analysis turn {} was saved but not linked to Question {}: {error}",
                        answer.turn_id,
                        question_id
                    );
                }
            }
        }
    }

    // `AnalyticsSource` (see `analytics::AnalyticsSource`) is implemented
    // for this type near the bottom of the file, as a thin delegation to
    // this method and `run_sql` below -- the trait exists so
    // `analytics::verify` (and anything else in `analytics/`) can be handed
    // a narrow capability instead of the whole `EngineState`, not because
    // the logic itself needs to live differently.

    /// Compact one-line-per-table schema: `view("col" TYPE, ...)`. Used to echo
    /// the real schema back to the model after a SQL error.
    pub(crate) fn schema_oneline(&self) -> String {
        let workspace = self.workspace.lock().unwrap_or_else(|e| e.into_inner());
        let mut out = String::new();
        for s in workspace.sources.iter().filter(|s| s.view.is_some()) {
            let cols = s
                .columns
                .as_ref()
                .map(|cs| {
                    cs.iter()
                        .map(|c| format!("\"{}\" {}", c.name, c.type_))
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .unwrap_or_default();
            out.push_str(&format!("  {}({cols})\n", s.view.as_deref().unwrap_or("")));
        }
        out
    }

    /// The table/document digest embedded in the system prompt. Within a modest
    /// budget it lists every column with its type and any ingest note, and a few
    /// sample rows for a small workspace; a large/messy folder falls back to
    /// names + shape only (keeps the prompt small).
    #[cfg(test)]
    pub(crate) fn schema_block(&self) -> String {
        self.schema_block_for_source_selection(None)
    }

    fn schema_block_for_source_selection(
        &self,
        selection: Option<&InvestigationSourceSelection>,
    ) -> String {
        let capabilities = self.settings().capabilities;
        let mut workspace = self.workspace.lock().unwrap_or_else(|e| e.into_inner());
        let selected_only = selection.is_some_and(|selection| !selection.is_all_sources());
        if !selected_only {
            if let Some(cached) = &workspace.schema_cache {
                return cached.clone();
            }
        }
        let workspace_root = workspace.workspace.clone();
        let selected_sources: Vec<&SourceInfo> = workspace
            .sources
            .iter()
            .filter(|source| {
                selection.is_none_or(|selection| {
                    workspace_root.as_deref().is_none_or(|root| {
                        selection.includes_path(&catalog::workspace_relative_path(
                            root,
                            Path::new(&source.path),
                        ))
                    })
                })
            })
            .collect();
        let sources = &selected_sources;
        let inspected = &workspace.inspected_tables;
        let tables: Vec<&SourceInfo> = if capabilities.table_analysis {
            sources
                .iter()
                .copied()
                .filter(|s| s.view.is_some())
                .collect()
        } else {
            Vec::new()
        };
        let total_cols: usize = tables
            .iter()
            .map(|s| s.columns.as_ref().map(|c| c.len()).unwrap_or(0))
            .sum();
        let full = tables.len() <= 12 && total_cols <= 60;
        let small = tables.len() <= 4;

        let mut p = String::new();
        if selected_only {
            p.push_str("Investigation source selection is active. Only the files listed below are available to this run; do not use files outside this selection.\n");
        }
        if !capabilities.table_analysis {
            p.push_str(
                "Table analysis is disabled by the current experimental capability policy.\n",
            );
        } else if tables.is_empty() {
            p.push_str("No tables were detected in the selected sources.\n");
        } else if full {
            p.push_str("Tables (columns and types shown; use inspect_table for values):\n");
            for s in &tables {
                let view = s.view.as_deref().unwrap_or("");
                let rows = s
                    .row_count
                    .map(|n| n.to_string())
                    .unwrap_or_else(|| "?".into());
                let relative_path = workspace_root
                    .as_deref()
                    .map(|root| catalog::workspace_relative_path(root, Path::new(&s.path)))
                    .unwrap_or_else(|| s.name.clone());
                p.push_str(&format!(
                    "  {view}  (file={}, scope={} ; {rows} rows)\n",
                    relative_path,
                    crate::engine::catalog::source_scope(&s.name, &s.path, s.view.as_deref(),)
                        .label()
                ));
                if let Some(note) = &s.note {
                    p.push_str(&format!("    note: {note}\n"));
                }
                if let Some(cols) = &s.columns {
                    for c in cols {
                        let common = (small || inspected.contains(&view.to_lowercase()))
                            .then(|| {
                                c.common_values.as_ref().map(|values| {
                                    let shown = values
                                        .iter()
                                        .take(4)
                                        .map(|value| cap_chars(value, 40))
                                        .collect::<Vec<_>>()
                                        .join(" | ");
                                    format!("  common: {shown}")
                                })
                            })
                            .flatten()
                            .unwrap_or_default();
                        match &c.note {
                            Some(n) => p.push_str(&format!(
                                "    \"{}\" {}  [{}]{}\n",
                                c.name, c.type_, n, common
                            )),
                            None => {
                                p.push_str(&format!("    \"{}\" {}{}\n", c.name, c.type_, common))
                            }
                        }
                    }
                }
                if small || inspected.contains(&view.to_lowercase()) {
                    if let Ok(sample) = query_workspace(
                        &*workspace.data,
                        &format!("SELECT * FROM {} LIMIT 3", data::quote_ident(view)),
                        3,
                    ) {
                        for line in mini_table(&sample) {
                            p.push_str(&format!("    {line}\n"));
                        }
                    }
                }
            }
        } else {
            p.push_str("Tables (use inspect_table for their columns):\n");
            for s in &tables {
                let ncols = s.columns.as_ref().map(|c| c.len()).unwrap_or(0);
                let relative_path = workspace_root
                    .as_deref()
                    .map(|root| catalog::workspace_relative_path(root, Path::new(&s.path)))
                    .unwrap_or_else(|| s.name.clone());
                p.push_str(&format!(
                    "  {}  (file={}, scope={}; {} rows, {ncols} columns)\n",
                    s.view.as_deref().unwrap_or(""),
                    relative_path,
                    crate::engine::catalog::source_scope(&s.name, &s.path, s.view.as_deref(),)
                        .label(),
                    s.row_count
                        .map(|n| n.to_string())
                        .unwrap_or_else(|| "?".into()),
                ));
                let observed = s
                    .columns
                    .as_deref()
                    .unwrap_or_default()
                    .iter()
                    .filter_map(|column| {
                        let values = column.common_values.as_ref()?;
                        let shown = values
                            .iter()
                            .take(4)
                            .map(|value| cap_chars(value, 32))
                            .collect::<Vec<_>>()
                            .join(" | ");
                        Some(format!("{}=[{}]", column.name, shown))
                    })
                    .take(6)
                    .collect::<Vec<_>>();
                if !observed.is_empty() {
                    p.push_str(&format!("    observed values: {}\n", observed.join("; ")));
                }
            }
        }

        if !tables.is_empty() {
            let hints = shared_column_hints(&tables);
            if !hints.is_empty() {
                p.push_str(
                    "Potentially related fields (same names only; a clue, not a join instruction):\n",
                );
                for h in hints {
                    p.push_str(&format!("  {h}\n"));
                }
            }
        }

        let docs: Vec<&SourceInfo> = sources
            .iter()
            .copied()
            .filter(|s| s.view.is_none())
            .collect();
        if !capabilities.document_analysis {
            p.push_str(
                "Document analysis is disabled by the current experimental capability policy.\n",
            );
        } else if !docs.is_empty() {
            p.push_str("Documents (bounded leading previews; use list_files/grep_files/read_file for more):\n");
            for d in docs {
                let relative_path = workspace_root
                    .as_deref()
                    .map(|root| catalog::workspace_relative_path(root, Path::new(&d.path)))
                    .unwrap_or_else(|| d.name.clone());
                match &d.synopsis {
                    Some(syn) => p.push_str(&format!("  {}  {}\n", relative_path, syn)),
                    None => p.push_str(&format!("  {}\n", relative_path)),
                }
            }
        }
        if !selected_only {
            workspace.schema_cache = Some(p.clone());
        }
        p
    }

    /// Assemble one bounded, prompt-ready projection of the current
    /// workspace context. Keeping this policy in the engine gives both
    /// desktop shells the same context budget and provenance behavior.
    #[cfg(test)]
    pub(crate) fn context_packet(&self, question: &str, conversation_id: &str) -> ContextPacket {
        self.context_packet_for_source_selection(question, conversation_id, None, None)
    }

    fn context_packet_for_source_selection(
        &self,
        question: &str,
        conversation_id: &str,
        selection: Option<&InvestigationSourceSelection>,
        investigation_id: Option<&str>,
    ) -> ContextPacket {
        let user_context = self.user_context();
        let schema = self.schema_block_for_source_selection(selection);
        let recent = self.session_block_for_source_selection(conversation_id, selection);
        let selected_only = selection.is_some_and(|selection| !selection.is_all_sources());
        // Learned notes describe the entire folder, not just the sources
        // selected for this Investigation. They may contain facts from an
        // excluded source, so keep them out of selected-source runs.
        let learned = (!selected_only)
            .then(|| self.folder_memory_block(investigation_id))
            .flatten();
        let semantic_model = (!selected_only)
            .then(|| self.workspace_model_prompt_block())
            .flatten();
        ContextAssembler::default().assemble(
            question,
            &user_context,
            &schema,
            recent.as_deref(),
            learned.as_deref(),
            semantic_model.as_deref(),
        )
    }

    /// The "Earlier in this conversation" block for `conversation_id`, or `None`
    /// on that conversation's first turn.
    #[cfg(test)]
    pub(crate) fn session_block(&self, conversation_id: &str) -> Option<String> {
        self.session_block_for_source_selection(conversation_id, None)
    }

    fn session_block_for_source_selection(
        &self,
        conversation_id: &str,
        source_selection: Option<&InvestigationSourceSelection>,
    ) -> Option<String> {
        let catalog = self.catalog();
        let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let turns = match inner.sessions.get(conversation_id) {
            Some(s) if !s.turns.is_empty() => &s.turns,
            _ => return None,
        };
        let mut p = String::from(
            "Earlier in this conversation (for reference resolution only; assistant wording and result values are not evidence):\n",
        );
        for t in turns {
            if source_selection
                .is_some_and(|selection| t.source_selection.as_ref() != Some(selection))
            {
                continue;
            }
            let current_context = turn_context_matches(&t.workspace, &catalog);
            let scope = match (&t.workspace, current_context) {
                (TurnWorkspace::NoWorkspace, _) => "general turn; no local evidence",
                (TurnWorkspace::Snapshot { .. }, true) => {
                    "same workspace revision; prior tool evidence can be retrieved"
                }
                (TurnWorkspace::PathOnly { .. }, true) => {
                    "same folder path; archived revision unavailable, prior evidence is not reusable"
                }
                (TurnWorkspace::Snapshot { .. }, false) => {
                    "other or older workspace; not current evidence"
                }
                (TurnWorkspace::PathOnly { .. }, false) => {
                    "other workspace path; not current evidence"
                }
                (TurnWorkspace::Unknown, _) => "unknown source scope; not evidence",
            };
            let analysis_ref = match (&t.turn_id, &t.workspace, current_context) {
                (Some(id), TurnWorkspace::Snapshot { .. }, true) => {
                    format!(" [analysis turn {id}; retrieve with read_prior_analysis if needed]")
                }
                (Some(id), TurnWorkspace::PathOnly { .. }, true) => {
                    format!(" [analysis turn {id}; revision unknown, do not reuse evidence]")
                }
                (
                    Some(id),
                    TurnWorkspace::Snapshot { .. } | TurnWorkspace::PathOnly { .. },
                    false,
                ) => format!(" [analysis turn {id}; stale for the current workspace]"),
                _ => String::new(),
            };
            p.push_str(&format!(
                "- [{scope}]{analysis_ref} Q: \"{}\"  A: \"{}\"\n",
                t.question, t.headline,
            ));
            if current_context {
                if let Some(frame) = &t.frame {
                    p.push_str(&format!("  prior interpretation (re-check): {frame}\n"));
                }
                for q in &t.queries {
                    p.push_str(&format!(
                        "  prior query shape (not evidence; retrieve its execution if the exact revision still matches, otherwise re-run): {q}\n"
                    ));
                }
            }
        }
        Some(p)
    }

    fn session_has_reusable_analysis(&self, conversation_id: &str, catalog: &Catalog) -> bool {
        let (Some(workspace), Some(revision)) = (&catalog.workspace, &catalog.revision) else {
            return false;
        };
        let workspace = workspace.as_str();
        let candidate_turn_ids: Vec<String> = self
            .inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .sessions
            .get(conversation_id)
            .into_iter()
            .flat_map(|session| &session.turns)
            .filter(|turn| {
                matches!(
                    &turn.workspace,
                    TurnWorkspace::Snapshot {
                        path,
                        revision: turn_revision,
                    } if path == workspace && turn_revision == revision
                )
            })
            .filter_map(|turn| turn.turn_id.clone())
            .collect();
        candidate_turn_ids
            .iter()
            .any(|turn_id| analysis_store::exists(&self.data_dir, turn_id))
    }

    /// User-written context for the system prompt: the workspace `fella.md`.
    pub fn user_context(&self) -> Vec<String> {
        let mut out = Vec::new();
        if let Some(md) = self
            .workspace
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .user_md
            .clone()
        {
            out.push(md);
        }
        out
    }

    // --- per-folder memory ---------------------------------------------------

    fn memory_path(&self) -> Option<PathBuf> {
        self.workspace
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .memory_path
            .clone()
    }

    /// `(file path, contents)` of the current folder's `memory.md` for the
    /// `/memory` command. `contents` is `None` if the file doesn't exist yet.
    /// `None` overall when no folder is open.
    pub fn folder_memory_file(&self) -> Option<(String, Option<String>)> {
        let path = self.memory_path()?;
        let text = std::fs::read_to_string(&path).ok();
        Some((path.display().to_string(), text))
    }

    /// Typed, provenance-preserving facts learned for the current workspace.
    pub fn folder_memory_facts(&self) -> Option<Vec<SemanticFact>> {
        let path = self.memory_path()?;
        let facts = SemanticMemory::load(&semantic_memory::facts_path_for(&path));
        Some(facts.facts().to_vec())
    }

    /// Review or edit one learned fact without changing the workspace data.
    /// An accepted suggestion can stay with one Investigation or be promoted
    /// to workspace scope; its source turn and evidence remain attached.
    pub fn resolve_folder_memory_fact(
        &self,
        fact_id: &str,
        action: &str,
        statement: Option<&str>,
        target_investigation_id: Option<&str>,
    ) -> EngineResult<Vec<SemanticFact>> {
        let memory_path = self.memory_path().ok_or(EngineError::NoWorkspace)?;
        let facts_path = semantic_memory::facts_path_for(&memory_path);
        let mut facts = SemanticMemory::load(&facts_path);
        let original = facts
            .facts()
            .iter()
            .find(|fact| fact.id == fact_id)
            .cloned()
            .ok_or_else(|| EngineError::msg("The learned fact is no longer available."))?;
        if original.state == semantic_memory::FactState::Superseded {
            return Err(EngineError::msg("Superseded knowledge cannot be changed."));
        }
        if let Some(target_id) = target_investigation_id {
            let target = investigation::load(&self.data_dir, target_id)?;
            if target.workspace_id != original.workspace {
                return Err(EngineError::msg(
                    "Knowledge can only be scoped to an Investigation in this workspace.",
                ));
            }
        }

        let at_ms = semantic_memory::now_ms();
        match action {
            "accept" => {
                if original.authority != FactAuthority::ModelSuggestion
                    || original.state != semantic_memory::FactState::Proposed
                {
                    return Err(EngineError::msg("Only a model suggestion can be accepted."));
                }
                if let Some(existing) = facts.fact_mut(fact_id) {
                    existing.state = semantic_memory::FactState::Superseded;
                    existing.updated_at_ms = at_ms;
                }
                let mut accepted = SemanticFact::new(SemanticFactInput {
                    kind: original.kind,
                    key: original.key.clone(),
                    statement: original.statement.clone(),
                    authority: FactAuthority::Confirmed,
                    workspace: original.workspace.clone(),
                    revision: original.revision.clone(),
                    supporting_turn: original.supporting_turn.clone(),
                    evidence_ids: original.evidence_ids.clone(),
                    at_ms,
                });
                accepted.investigation_id = target_investigation_id.map(str::to_string);
                accepted.origin_investigation_id = original
                    .origin_investigation_id
                    .clone()
                    .or_else(|| original.investigation_id.clone());
                accepted.supersedes.push(original.id.clone());
                facts.upsert(accepted);
            }
            "edit" => {
                let statement = statement.unwrap_or_default().trim();
                if statement.is_empty() || statement.chars().count() > 1_000 {
                    return Err(EngineError::msg(
                        "Knowledge text must be non-empty and no longer than 1000 characters.",
                    ));
                }
                if let Some(existing) = facts.fact_mut(fact_id) {
                    existing.state = semantic_memory::FactState::Superseded;
                    existing.updated_at_ms = at_ms;
                }
                let mut edited = SemanticFact::new(SemanticFactInput {
                    kind: original.kind,
                    key: original.key.clone(),
                    statement: statement.to_string(),
                    authority: FactAuthority::User,
                    workspace: original.workspace.clone(),
                    revision: original.revision.clone(),
                    supporting_turn: original.supporting_turn.clone(),
                    evidence_ids: original.evidence_ids.clone(),
                    at_ms,
                });
                edited.investigation_id = target_investigation_id.map(str::to_string);
                edited.origin_investigation_id = original
                    .origin_investigation_id
                    .clone()
                    .or_else(|| original.investigation_id.clone());
                edited.supersedes.push(original.id.clone());
                facts.upsert(edited);
            }
            "remove" => {
                facts.remove(fact_id);
            }
            _ => return Err(EngineError::msg("Unknown learned-knowledge action.")),
        }

        facts.save();
        semantic_memory::sync_markdown_projection(&memory_path, &facts);
        Ok(facts.facts().to_vec())
    }

    /// Delete the current folder's learned notes (and its episode log).
    /// `Ok(false)` if there was nothing to delete; `Err` on no folder open.
    pub fn forget_folder_memory(&self) -> EngineResult<bool> {
        let path = self.memory_path().ok_or(EngineError::NoWorkspace)?;
        let facts_path = semantic_memory::facts_path_for(&path);
        let had = path.exists() || facts_path.exists();
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("episodes.jsonl"));
        let _ = std::fs::remove_file(facts_path);
        Ok(had)
    }

    /// Append one coarse, telemetry-free record to `<data_dir>/signals.jsonl`
    /// when `reason` fires (see `friction::trigger`). Never transmitted; never
    /// contains question/answer text, file paths, or data values. Best-effort
    /// a write failure here must never fail the actual answer.
    pub(crate) fn record_friction_signal(
        &self,
        reason: &str,
        evidence: &[crate::engine::evidence::EvidenceItem],
    ) {
        crate::engine::friction::record(&self.data_dir, reason, evidence);
    }

    /// The learned-notes block for the system prompt, or `None` when memory is
    /// off, no folder is open, or nothing's been learned. Re-read from disk so a
    /// hand edit to `memory.md` lands on the next question.
    pub(crate) fn folder_memory_block(&self, investigation_id: Option<&str>) -> Option<String> {
        if !memory::enabled() {
            return None;
        }
        let path = self.memory_path()?;
        let mut mem = FolderMemory::load(&path);
        mem.prune_tables(&self.known_views());
        let typed = SemanticMemory::load(&semantic_memory::facts_path_for(&path));
        let legacy = if typed.has_kind(FactKind::Vocabulary) {
            mem.semantic_core_without_vocabulary()
        } else {
            mem.semantic_core()
        };
        let typed_block =
            typed.prompt_block(self.workspace_revision().as_deref(), investigation_id);
        match (legacy, typed_block) {
            (Some(legacy), Some(typed)) => Some(format!("{legacy}\n\n{typed}")),
            (Some(legacy), None) => Some(legacy),
            (None, Some(typed)) => Some(typed),
            (None, None) => None,
        }
    }

    fn workspace_revision(&self) -> Option<String> {
        self.workspace
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .revision
            .clone()
    }

    fn known_views(&self) -> Vec<String> {
        self.workspace
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .sources
            .iter()
            .filter_map(|s| s.view.clone())
            .collect()
    }

    /// After a completed turn: append an episode, and if it plainly corrects
    /// the previous answer, learn a vocabulary note from it. Deliberately
    /// does not cache the query itself as a "recipe". Memory holds durable
    /// facts, not code snapshotted against one verifier pass; see
    /// `docs/ARCHITECTURE.md`. `prior_q` is the previous question in this
    /// conversation, if any.
    async fn record_turn_memory(
        &self,
        prior_q: Option<&str>,
        question: &str,
        answer: &Answer,
        investigation_id: Option<&str>,
    ) {
        if !memory::writes_enabled() {
            return;
        }
        let Some(path) = self.memory_path() else {
            return;
        };

        let sqls: Vec<&str> = answer
            .evidence
            .iter()
            .filter(|e| {
                matches!(
                    e.tool.as_str(),
                    "run_sql" | "make_chart" | "forecast_analysis"
                ) && e.error.is_none()
            })
            .filter_map(|e| e.sql.as_deref())
            .collect();
        let corrected = prior_q.is_some() && memory::is_correction(question);

        let at_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        memory::record_episode(
            &path,
            &serde_json::json!({
                "at": at_ms,
                "q": question.chars().take(300).collect::<String>(),
                "headline": answer.text.lines().find(|l| !l.trim().is_empty()).unwrap_or("").chars().take(200).collect::<String>(),
                "queries": sqls.iter().take(3).collect::<Vec<_>>(),
                "verified": matches!(
                    answer.status,
                    crate::engine::evidence::VerificationStatus::Verified
                ),
                "corrected_prior": corrected,
            }),
        );

        let mut mem = FolderMemory::load(&path);
        let facts_path = semantic_memory::facts_path_for(&path);
        let mut facts = SemanticMemory::load(&facts_path);
        let scope = answer
            .workspace
            .as_ref()
            .map(|workspace| workspace.path.clone())
            .unwrap_or_default();
        let revision = answer
            .workspace
            .as_ref()
            .map(|workspace| workspace.revision.clone());
        let evidence_ids: Vec<String> = answer
            .evidence
            .iter()
            .filter(|evidence| evidence.error.is_none())
            .map(|evidence| evidence.id.clone())
            .collect();

        // Upgrade legacy vocabulary notes the first time the typed ledger is
        // touched. Existing users keep their learned definitions, but future
        // updates gain authority, revision, and provenance metadata.
        if !facts.has_kind(FactKind::Vocabulary) {
            for (key, statement) in mem.vocabulary_entries() {
                facts.upsert(SemanticFact::new(SemanticFactInput {
                    kind: FactKind::Vocabulary,
                    key,
                    statement,
                    authority: FactAuthority::User,
                    workspace: scope.clone(),
                    revision: revision.clone(),
                    supporting_turn: None,
                    evidence_ids: Vec::new(),
                    at_ms,
                }));
            }
        }

        if corrected {
            let correction = question.trim();
            let existing = mem.vocabulary_entries();
            match self.reconcile_vocab_key(correction, &existing).await {
                VocabAction::Update(key) | VocabAction::Add(key) => {
                    mem.set_vocab(&key, correction);
                    facts.upsert(SemanticFact::new(SemanticFactInput {
                        kind: FactKind::Vocabulary,
                        key,
                        statement: correction.to_string(),
                        authority: FactAuthority::User,
                        workspace: scope.clone(),
                        revision: revision.clone(),
                        supporting_turn: Some(answer.turn_id.clone()),
                        evidence_ids: evidence_ids.clone(),
                        at_ms,
                    }));
                }
                VocabAction::Noop => {}
            }
        }

        for fact in observed_contract_facts(answer, &scope, revision.clone(), &evidence_ids, at_ms)
        {
            facts.upsert(fact);
        }
        for fact in proposed_contract_facts(
            answer,
            &scope,
            revision,
            &evidence_ids,
            investigation_id,
            at_ms,
        ) {
            facts.upsert(fact);
        }
        mem.prune_tables(&self.known_views());
        mem.save();
        facts.save();
        semantic_memory::sync_markdown_projection(&path, &facts);
    }

    /// Decide whether a new correction updates an existing vocabulary note,
    /// is genuinely new, or restates one already there: supersede rather than
    /// append. The model judges against the small existing list instead of a
    /// keyword/position heuristic, which cannot reliably match reworded
    /// corrections. Uses whichever model is currently active, with no
    /// override, so the memory file stays legible to any model that later
    /// reads it, and costs nothing when there's nothing yet to reconcile
    /// against.
    async fn reconcile_vocab_key(
        &self,
        correction: &str,
        existing: &[(String, String)],
    ) -> VocabAction {
        if existing.is_empty() {
            return VocabAction::Add(default_topic_key(correction));
        }
        let list = existing
            .iter()
            .map(|(k, v)| format!("- {k}: {v}"))
            .collect::<Vec<_>>()
            .join("\n");
        let sys = "You maintain a short list of facts Fella has learned from a user's corrections \
about their own data. Given a new correction and the existing facts, decide: does it UPDATE one of \
them (replace its text with this correction), is it genuinely a NEW fact (ADD), or does it just \
restate one already there with nothing new (NOOP)? Reply with exactly one line, nothing else: \
`UPDATE <key>`, `ADD <short 2-4 word key>`, or `NOOP <key>`. For UPDATE or NOOP, <key> must be copied \
exactly, character for character, from the list below.";
        let user = format!("EXISTING FACTS:\n{list}\n\nNEW CORRECTION:\n{correction}\n\nDecision:");
        match self.ask_once(None, sys, &user).await {
            Ok(reply) => parse_vocab_action(&reply, existing)
                .unwrap_or_else(|| VocabAction::Add(default_topic_key(correction))),
            Err(_) => VocabAction::Add(default_topic_key(correction)),
        }
    }

    /// Return the current workspace context file and its contents.
    /// The file is user-authored context, not an agent write surface.
    pub fn context_file(&self) -> Option<(String, Option<String>)> {
        let ws = self
            .workspace
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .workspace
            .clone()?;
        let path = ws.join("fella.md");
        Some((
            path.display().to_string(),
            std::fs::read_to_string(path).ok(),
        ))
    }

    /// Save the explicit user context file at the root of the open workspace.
    /// This is the one supported workspace write outside the agent tool path.
    pub fn save_context(&self, contents: &str) -> EngineResult<()> {
        let ws = self
            .workspace
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .workspace
            .clone()
            .ok_or(EngineError::NoWorkspace)?;
        let path = ws.join("fella.md");
        let tmp = ws.join(".fella.md.fella-tmp");
        std::fs::write(&tmp, contents)
            .map_err(|e| EngineError::io(format!("write {}", tmp.display()), e))?;
        if let Err(e) = Self::replace_file(&tmp, &path) {
            let _ = std::fs::remove_file(&tmp);
            return Err(EngineError::io(format!("replace {}", path.display()), e));
        }
        let normalized = contents.trim().to_string();
        let mut workspace = self.workspace.lock().unwrap_or_else(|e| e.into_inner());
        workspace.user_md = (!normalized.is_empty()).then_some(normalized);
        Ok(())
    }

    /// Replace a file after writing a sibling temporary file. Unix rename is an
    /// atomic replacement; Windows refuses to rename over an existing file, so the
    /// small fallback removes the old context file before moving the completed
    /// temporary file into place.
    fn replace_file(tmp: &Path, destination: &Path) -> std::io::Result<()> {
        #[cfg(windows)]
        {
            if destination.exists() {
                std::fs::remove_file(destination)?;
            }
        }
        std::fs::rename(tmp, destination)
    }

    pub fn settings(&self) -> Settings {
        let mut s = sqlite::load_settings(&self.sqlite.lock().unwrap_or_else(|e| e.into_inner()));
        let id = provider::normalize_id(&s.provider);
        s.has_credential = self.secrets.has(id);
        s
    }

    pub fn save_settings(
        &self,
        patch: &serde_json::Map<String, serde_json::Value>,
    ) -> EngineResult<Settings> {
        let mut patch = patch.clone();

        // Changing `provider` on its own (the `/model provider <x>` path) should
        // move `base_url` / `model` / `embed_model` to that provider's defaults
        // for any of them not set in the same patch just like `/login` does.
        // Without this you'd be left pointed at the previous provider's URL.
        if let Some(new_id) = patch.get("provider").and_then(|v| v.as_str()) {
            if let Some(p) = provider::get(new_id) {
                let switching = provider::normalize_id(&self.settings().provider) != p.id;
                if !patch.contains_key("base_url") && !p.base_url.is_empty() {
                    patch.insert("base_url".into(), p.base_url.into());
                }
                if !patch.contains_key("model") {
                    if !p.default_model.is_empty() {
                        patch.insert("model".into(), p.default_model.into());
                    } else if switching {
                        patch.insert("model".into(), "".into());
                    }
                }
                if !patch.contains_key("embed_model") && !p.default_embed_model.is_empty() {
                    patch.insert("embed_model".into(), p.default_embed_model.into());
                }
            }
        }

        sqlite::save_settings(
            &self.sqlite.lock().unwrap_or_else(|e| e.into_inner()),
            &patch,
        )?;
        if patch.contains_key("capabilities") {
            self.workspace
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .schema_cache = None;
        }
        // A model or provider change: warm a hosted Ollama-wire model when a
        // credential is available.
        self.warm_model();
        Ok(self.settings())
    }

    /// The providers Fella has built-in support for, and whether each is
    /// signed in. Drives `/login` and `/auth`.
    pub fn list_providers(&self) -> Vec<ProviderInfo> {
        let current = provider::normalize_id(&self.settings().provider).to_string();
        PROVIDERS
            .iter()
            .map(|p| ProviderInfo {
                id: p.id.to_string(),
                display: p.display.to_string(),
                auth: p.auth.as_str().to_string(),
                base_url: p.base_url.to_string(),
                get_key_url: p.get_key_url.to_string(),
                embeddings: p.embeddings,
                authed: self.secrets.has(p.id),
                current: p.id == current,
            })
            .collect()
    }

    /// Store an API key for `provider_id` and make it the active provider,
    /// moving `base_url` / `model` / `embed_model` to that provider's defaults.
    pub fn set_api_key(&self, provider_id: &str, key: &str) -> EngineResult<Settings> {
        let p = provider::get(provider_id)
            .ok_or_else(|| EngineError::msg(format!("unknown provider: {provider_id}")))?;
        if p.auth != AuthKind::ApiKey {
            return Err(EngineError::msg(format!(
                "{} does not use an API key",
                p.display
            )));
        }
        let key = key.trim();
        if key.is_empty() {
            return Err(EngineError::msg("that key is empty"));
        }
        self.secrets.set_api_key(p.id, key)?;

        let switching = provider::normalize_id(&self.settings().provider) != p.id;

        let mut patch = serde_json::Map::new();
        patch.insert("provider".into(), p.id.into());
        if !p.base_url.is_empty() {
            patch.insert("base_url".into(), p.base_url.into());
        }
        if !p.default_model.is_empty() {
            patch.insert("model".into(), p.default_model.into());
        } else if switching {
            // New provider with no default (e.g. a gateway with drifting ids):
            // clear the model so we don't inherit the previous provider's.
            patch.insert("model".into(), "".into());
        }
        if !p.default_embed_model.is_empty() {
            patch.insert("embed_model".into(), p.default_embed_model.into());
        }
        self.save_settings(&patch)
    }

    /// Forget the stored credential for `provider_id`. If it was the provider
    /// Fella is currently on, also drop `base_url` / `model` / `embed_model`
    /// back to the local default so the app isn't left pointed at a service it
    /// can no longer reach. Logging out of any other provider only forgets its
    /// key.
    /// Stop using `provider_id`. If it's the active provider, drop back to the
    /// local default. The saved API key is **kept** so `/login <provider>` can
    /// reuse it unless `forget`, which also clears it from `auth.json`.
    pub fn logout(&self, provider_id: &str, forget: bool) -> EngineResult<Settings> {
        let id = provider::normalize_id(provider_id);
        let is_current = provider::normalize_id(&self.settings().provider) == id;
        let had_key = self.secrets.has(id);

        // Only a genuine typo (not registered, not the active provider, no key
        // on file) is an error otherwise there's something real to do.
        if provider::get(id).is_none() && !is_current && !had_key {
            return Err(EngineError::msg(format!("unknown provider: {provider_id}")));
        }
        if forget {
            self.secrets.clear(id)?;
        }

        if is_current {
            let d = provider::get(provider::DEFAULT_ID)
                .expect("the default provider is always in the registry");
            let mut patch = serde_json::Map::new();
            patch.insert("provider".into(), d.id.into());
            patch.insert("base_url".into(), d.base_url.into());
            patch.insert("model".into(), d.default_model.into());
            patch.insert("embed_model".into(), d.default_embed_model.into());
            return self.save_settings(&patch);
        }

        Ok(self.settings())
    }

    /// Point the engine at a folder: scan it, load every tabular file as a
    /// table, and replace the catalog. The old catalog remains active until
    /// the replacement is fully prepared.
    pub fn open_workspace(&self, path: &Path) -> EngineResult<Catalog> {
        self.open_workspace_with_progress(path, |_| {})
    }

    /// Mount a folder while reporting bounded progress updates. Traversal and
    /// parsing remain local and read-only; the callback is only a status
    /// channel, and no partial catalog is published.
    pub fn open_workspace_with_progress(
        &self,
        path: &Path,
        mut on_progress: impl FnMut(catalog::WorkspaceProgress),
    ) -> EngineResult<Catalog> {
        if !path.is_dir() {
            return Err(EngineError::msg(format!(
                "That doesn't look like a folder: {}",
                path.display()
            )));
        }
        let _mount_serial = self.mount_serial.lock().unwrap_or_else(|e| e.into_inner());
        on_progress(catalog::WorkspaceProgress {
            phase: "scanning",
            visited_files: 0,
            supported_files: 0,
            prepared_files: 0,
            total_supported_files: None,
            skipped_files: 0,
            ingest: None,
        });
        let mut visited_files = 0usize;
        let (scanned, mut skipped) = catalog::scan_with_progress(path, |progress| {
            visited_files = progress.visited_files;
            on_progress(progress);
        })?;
        let total_supported_files = scanned.len();
        on_progress(catalog::WorkspaceProgress {
            phase: "preparing",
            visited_files,
            supported_files: total_supported_files,
            prepared_files: 0,
            total_supported_files: Some(total_supported_files),
            skipped_files: skipped.len(),
            ingest: None,
        });
        let tabular_files: Vec<_> = scanned
            .iter()
            .filter(|file| file.kind.is_tabular())
            .collect();
        let eager_table_profiles = tabular_files.len() <= 4
            && tabular_files
                .iter()
                .map(|file| file.size_bytes)
                .fold(0u64, |total, file_size| total.saturating_add(file_size))
                <= EAGER_PROFILE_TOTAL_SOURCE_BYTES;

        // Build in an isolated backend. The active workspace remains fully
        // queryable until the new catalog and data engine are published below.
        let scratch_dir = workspace_scratch_dir(&self.data_dir)?;
        let mut data = match data::open_engine(&scratch_dir) {
            Ok(data) => data,
            Err(e) => {
                let _ = std::fs::remove_dir_all(&scratch_dir);
                return Err(e);
            }
        };

        let mut sources = Vec::with_capacity(scanned.len());
        {
            let mut used: HashSet<String> = HashSet::new();
            // Each text-doc preview is a file open + short read; cap how many we
            // do so a folder with thousands of notes doesn't pay thousands of
            // extra reads on every open. Beyond the cap, docs list without one.
            let mut synopsis_budget: usize = 250;
            for (prepared_files, f) in scanned.into_iter().enumerate() {
                if prepared_files % 128 == 0 {
                    on_progress(catalog::WorkspaceProgress {
                        phase: "preparing",
                        visited_files,
                        supported_files: total_supported_files,
                        prepared_files,
                        total_supported_files: Some(total_supported_files),
                        skipped_files: skipped.len(),
                        ingest: None,
                    });
                }
                let name = f
                    .path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("source")
                    .to_string();
                let stem = f
                    .path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("source");
                let relative_path = catalog::workspace_relative_path(path, &f.path);
                let path_str = f.path.display().to_string();

                let synopsis = if f.kind == SourceKind::Text && synopsis_budget > 0 {
                    synopsis_budget -= 1;
                    leading_document_preview(&path_str)
                } else {
                    None
                };
                let mut info = SourceInfo {
                    name,
                    path: path_str.clone(),
                    kind: f.kind,
                    view: None,
                    row_count: None,
                    columns: None,
                    size_bytes: f.size_bytes,
                    mtime: f.mtime,
                    synopsis,
                    note: None,
                };

                match f.kind {
                    #[cfg(feature = "xlsx")]
                    catalog::SourceKind::Xlsx => {
                        match crate::engine::ingest::excel::ingest_workbook(
                            &mut *data, &path_str, stem, &mut used,
                        ) {
                            Ok((sheets, _)) if !sheets.is_empty() => {
                                let profile_sheet_stats = eager_table_profiles && sheets.len() <= 4;
                                for sh in sheets {
                                    let columns = if profile_sheet_stats
                                        && sh.row_count <= EAGER_PROFILE_MAX_ROWS
                                    {
                                        profile_loaded_columns(&*data, &sh.view, sh.columns)
                                    } else {
                                        sh.columns
                                    };
                                    sources.push(SourceInfo {
                                        name: format!("{} \u{b7} {}", info.name, sh.sheet),
                                        path: path_str.clone(),
                                        kind: f.kind,
                                        view: Some(sh.view),
                                        row_count: Some(sh.row_count),
                                        columns: Some(columns),
                                        size_bytes: f.size_bytes,
                                        mtime: f.mtime,
                                        synopsis: None,
                                        note: sh.note,
                                    });
                                }
                                continue;
                            }
                            Ok((_, reasons)) => {
                                log::warn!("{path_str}: no usable sheets ({reasons:?})");
                                let reason = if reasons.is_empty() {
                                    "no readable sheets".to_string()
                                } else {
                                    format!("no readable sheets ({})", reasons.join("; "))
                                };
                                skipped.push(catalog::SkippedFile {
                                    name: relative_path.clone(),
                                    reason,
                                });
                                continue;
                            }
                            Err(e) => {
                                log::warn!("skipping {path_str}: {e}");
                                skipped.push(catalog::SkippedFile {
                                    name: relative_path.clone(),
                                    reason: format!("couldn't be opened as a spreadsheet: {e}"),
                                });
                                continue;
                            }
                        }
                    }
                    k if k.is_tabular() && k != SourceKind::Xlsx => {
                        let view = catalog::unique_view_name(stem, &mut used);
                        let load_result = {
                            let mut report_ingest = |progress: data::SourceIngestProgress| {
                                on_progress(catalog::WorkspaceProgress {
                                    phase: "preparing",
                                    visited_files,
                                    supported_files: total_supported_files,
                                    prepared_files,
                                    total_supported_files: Some(total_supported_files),
                                    skipped_files: skipped.len(),
                                    ingest: Some(catalog::WorkspaceIngestProgress {
                                        path: relative_path.clone(),
                                        stage: progress.stage,
                                        bytes_read: progress.bytes_read,
                                        total_bytes: f.size_bytes,
                                    }),
                                });
                            };
                            data.add_source_with_progress(
                                &view,
                                f.kind,
                                &path_str,
                                &mut report_ingest,
                            )
                        };
                        match load_result {
                            Ok(load) => {
                                info.row_count = Some(load.row_count);
                                info.columns = Some(
                                    if eager_table_profiles
                                        && load.row_count <= EAGER_PROFILE_MAX_ROWS
                                    {
                                        profile_loaded_columns(&*data, &view, load.columns)
                                    } else {
                                        load.columns
                                    },
                                );
                                info.note = load.note;
                                info.view = Some(view);
                            }
                            Err(e) => {
                                log::warn!("skipping {path_str}: {e}");
                                skipped.push(catalog::SkippedFile {
                                    name: relative_path.clone(),
                                    reason: "couldn't be read as a table".into(),
                                });
                                continue;
                            }
                        }
                    }
                    _ => {}
                }
                sources.push(info);
            }
        }

        on_progress(catalog::WorkspaceProgress {
            phase: "preparing",
            visited_files,
            supported_files: total_supported_files,
            prepared_files: total_supported_files,
            total_supported_files: Some(total_supported_files),
            skipped_files: skipped.len(),
            ingest: None,
        });

        // `fella.md` at the root is optional user context, not a data file.
        let user_md = std::fs::read_to_string(path.join("fella.md"))
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        skipped.sort_by(|a, b| a.name.cmp(&b.name));
        skipped.dedup_by(|a, b| a.name == b.name);
        let total_skipped_files = skipped.len();
        let revision = Some(catalog::workspace_revision(path, &sources, &skipped));
        let mem_path = memory::path_for(&self.data_dir, path);
        let indexed_at_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis().min(i64::MAX as u128) as i64)
            .unwrap_or(0);
        let model = revision.as_ref().map(|revision| {
            WorkspaceModel::from_sources(
                &path.display().to_string(),
                revision,
                Some(indexed_at_ms),
                &sources,
                &skipped,
            )
        });
        self.persist_sources(path, &sources);

        #[cfg(test)]
        pause_open_workspace_for_test(self);

        on_progress(catalog::WorkspaceProgress {
            phase: "waiting",
            visited_files,
            supported_files: total_supported_files,
            prepared_files: total_supported_files,
            total_supported_files: Some(total_supported_files),
            skipped_files: total_skipped_files,
            ingest: None,
        });
        let publish_permit = self.workspace_gate.write();

        // PDF text belongs to the old folder. Clear the cache when publishing
        // the new snapshot so repeatedly switching folders cannot retain every
        // PDF ever opened in this process. In-flight readers hold their own Arc.
        self.doc_cache
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clear();

        let old_scratch = {
            let mut workspace = self.workspace.lock().unwrap_or_else(|e| e.into_inner());
            let old_scratch = workspace
                .scratch
                .replace(WorkspaceScratch::new(scratch_dir));
            workspace.data = data;
            workspace.workspace = Some(path.to_path_buf());
            workspace.revision = revision;
            workspace.indexed_at_ms = Some(indexed_at_ms);
            workspace.sources = sources;
            workspace.skipped = skipped;
            workspace.user_md = user_md;
            workspace.memory_path = Some(mem_path);
            workspace.schema_cache = None;
            workspace.inspected_tables.clear();
            workspace.model = model;
            old_scratch
        };
        drop(publish_permit);

        // Keep per-conversation turns across a mount change. `session_block`
        // retains their conversational text while suppressing query/contract
        // hints unless their exact workspace revision is still mounted.
        drop(old_scratch);
        // The user is about to ask something: warm the model now so the first
        // question doesn't wait on a cold load.
        self.warm_model();
        on_progress(catalog::WorkspaceProgress {
            phase: "ready",
            visited_files,
            supported_files: total_supported_files,
            prepared_files: total_supported_files,
            total_supported_files: Some(total_supported_files),
            skipped_files: total_skipped_files,
            ingest: None,
        });
        Ok(self.catalog())
    }

    /// The folder from the last session, if it still exists on disk not opened,
    /// just the path so the welcome screen can offer a one-click "reopen".
    /// `None` if there's no history or the folder is gone.
    pub fn last_workspace_path(&self) -> Option<String> {
        let path = {
            let conn = self.sqlite.lock().unwrap_or_else(|e| e.into_inner());
            sqlite::most_recent_workspace(&conn)?
        };
        std::path::Path::new(&path).is_dir().then_some(path)
    }

    /// On launch: reopen the folder from the last session so the user doesn't
    /// re-pick it every time. `None` (and the welcome screen) if there's no
    /// history, the folder is gone, or it won't open no error is surfaced.
    /// A no-op if a workspace is already open.
    pub fn reopen_last_workspace(&self) -> Option<Catalog> {
        if self
            .workspace
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .workspace
            .is_some()
        {
            return None;
        }
        let path = {
            let conn = self.sqlite.lock().unwrap_or_else(|e| e.into_inner());
            sqlite::most_recent_workspace(&conn)?
        };
        let p = std::path::PathBuf::from(&path);
        if !p.is_dir() {
            return None;
        }
        match self.open_workspace(&p) {
            Ok(cat) => Some(cat),
            Err(e) => {
                log::info!("reopen_last_workspace: {path}: {e}");
                None
            }
        }
    }

    /// Re-open the current workspace (used by `/reindex`).
    pub fn reindex(&self) -> EngineResult<Catalog> {
        let ws = {
            let workspace = self.workspace.lock().unwrap_or_else(|e| e.into_inner());
            workspace.workspace.clone()
        };
        match ws {
            Some(path) => self.open_workspace(&path),
            None => Err(EngineError::NoWorkspace),
        }
    }

    /// Full per-column stats for one source.
    pub fn describe_source(&self, name: &str) -> EngineResult<SourceInfo> {
        require_capability(
            self.settings().capabilities.table_analysis,
            "Table analysis",
        )?;
        let mut workspace = self.workspace.lock().unwrap_or_else(|e| e.into_inner());
        let mut info = workspace
            .sources
            .iter()
            .find(|s| s.name == name || s.view.as_deref() == Some(name))
            .cloned()
            .ok_or_else(|| EngineError::UnknownSource(name.to_string()))?;
        let view = info
            .view
            .clone()
            .ok_or_else(|| EngineError::msg(format!("{name} is not a tabular source")))?;

        let mut columns = workspace.data.describe(&view)?;
        if info.row_count.is_none() {
            info.row_count = workspace
                .data
                .query(
                    &format!("SELECT count(*) FROM {}", data::quote_ident(&view)),
                    1,
                )
                .ok()
                .and_then(|o| {
                    o.rows
                        .first()
                        .and_then(|r| r.first())
                        .and_then(|v| v.as_i64())
                });
        }
        // Carry any ingest-time note (coercion, mixed column) onto the
        // freshly-computed stats, keyed by column name.
        if let Some(prior) = &info.columns {
            for c in &mut columns {
                if c.note.is_none() {
                    c.note = prior
                        .iter()
                        .find(|p| p.name == c.name)
                        .and_then(|p| p.note.clone());
                }
            }
        }
        info.columns = Some(columns.clone());

        // Cache the enriched schema back so a later turn's describe / the
        // system-prompt digest is instant and carries the stats.
        if let Some(s) = workspace
            .sources
            .iter_mut()
            .find(|s| s.name == name || s.view.as_deref() == Some(name))
        {
            s.columns = Some(columns);
            if s.row_count.is_none() {
                s.row_count = info.row_count;
            }
        }
        workspace.inspected_tables.insert(view.to_lowercase());
        workspace.schema_cache = None;
        workspace.model = WorkspaceModel::from_catalog(&Catalog {
            workspace: workspace
                .workspace
                .as_ref()
                .map(|p| p.display().to_string()),
            revision: workspace.revision.clone(),
            indexed_at_ms: workspace.indexed_at_ms,
            sources: workspace.sources.clone(),
            skipped: workspace.skipped.clone(),
        });
        Ok(info)
    }

    /// Run a read-only SQL statement (used by `/sql` and the agent).
    pub fn run_sql(&self, sql: &str) -> EngineResult<QueryResult> {
        require_capability(
            self.settings().capabilities.table_analysis,
            "Table analysis",
        )?;
        let workspace = self.workspace.lock().unwrap_or_else(|e| e.into_inner());
        query_workspace(&*workspace.data, sql, DEFAULT_ROW_CAP)
    }

    /// Replay a bounded analytical query with a caller-selected result cap.
    /// The limit is clamped to the chart engine's maximum so verification can
    /// reproduce complete chart inputs without unbounded materialization.
    pub fn run_sql_with_limit(&self, sql: &str, max_rows: usize) -> EngineResult<QueryResult> {
        require_capability(
            self.settings().capabilities.table_analysis,
            "Table analysis",
        )?;
        let workspace = self.workspace.lock().unwrap_or_else(|e| e.into_inner());
        query_workspace(
            &*workspace.data,
            sql,
            max_rows.clamp(1, crate::engine::analytics::chart::MAX_SOURCE_ROWS),
        )
    }

    /// Run a query within the selected files for one Investigation. The
    /// backend enforces the table allowlist at execution time.
    pub fn run_sql_with_source_selection(
        &self,
        sql: &str,
        selection: Option<&InvestigationSourceSelection>,
    ) -> EngineResult<QueryResult> {
        let Some(selection) = selection.filter(|selection| !selection.is_all_sources()) else {
            return self.run_sql(sql);
        };
        require_capability(
            self.settings().capabilities.table_analysis,
            "Table analysis",
        )?;
        let allowed = self.source_selection_allowed_tables(selection);
        let workspace = self.workspace.lock().unwrap_or_else(|e| e.into_inner());
        query_workspace_scoped(&*workspace.data, sql, DEFAULT_ROW_CAP, &allowed)
    }

    /// Run SQL while preserving the stop flag all the way into an interruptible
    /// backend query. This is separate from `run_sql` so command-style callers
    /// keep their small synchronous API.
    pub fn run_sql_cancellable(
        &self,
        sql: &str,
        cancel: Arc<AtomicBool>,
    ) -> EngineResult<QueryResult> {
        require_capability(
            self.settings().capabilities.table_analysis,
            "Table analysis",
        )?;
        let workspace = self.workspace.lock().unwrap_or_else(|e| e.into_inner());
        query_workspace_cancellable(&*workspace.data, sql, DEFAULT_ROW_CAP, cancel)
    }

    pub fn run_sql_cancellable_with_source_selection(
        &self,
        sql: &str,
        cancel: Arc<AtomicBool>,
        selection: Option<&InvestigationSourceSelection>,
    ) -> EngineResult<QueryResult> {
        let Some(selection) = selection.filter(|selection| !selection.is_all_sources()) else {
            return self.run_sql_cancellable(sql, cancel);
        };
        require_capability(
            self.settings().capabilities.table_analysis,
            "Table analysis",
        )?;
        let allowed = self.source_selection_allowed_tables(selection);
        let workspace = self.workspace.lock().unwrap_or_else(|e| e.into_inner());
        query_workspace_cancellable_scoped(&*workspace.data, sql, DEFAULT_ROW_CAP, &allowed, cancel)
    }

    /// Chart queries have a larger bounded source allowance than model-facing
    /// SQL results: tidy data can contain several category rows per plotted
    /// time point before the chart converter pivots it into series.
    pub fn run_chart_sql_cancellable(
        &self,
        sql: &str,
        cancel: Arc<AtomicBool>,
        max_rows: usize,
    ) -> EngineResult<QueryResult> {
        require_capability(
            self.settings().capabilities.table_analysis,
            "Table analysis",
        )?;
        let workspace = self.workspace.lock().unwrap_or_else(|e| e.into_inner());
        query_workspace_cancellable(
            &*workspace.data,
            sql,
            max_rows.clamp(
                DEFAULT_ROW_CAP,
                crate::engine::analytics::chart::MAX_SOURCE_ROWS,
            ),
            cancel,
        )
    }

    pub fn run_chart_sql_cancellable_with_source_selection(
        &self,
        sql: &str,
        cancel: Arc<AtomicBool>,
        max_rows: usize,
        selection: Option<&InvestigationSourceSelection>,
    ) -> EngineResult<QueryResult> {
        let Some(selection) = selection.filter(|selection| !selection.is_all_sources()) else {
            return self.run_chart_sql_cancellable(sql, cancel, max_rows);
        };
        require_capability(
            self.settings().capabilities.table_analysis,
            "Table analysis",
        )?;
        let allowed = self.source_selection_allowed_tables(selection);
        let workspace = self.workspace.lock().unwrap_or_else(|e| e.into_inner());
        query_workspace_cancellable_scoped(
            &*workspace.data,
            sql,
            max_rows.clamp(
                DEFAULT_ROW_CAP,
                crate::engine::analytics::chart::MAX_SOURCE_ROWS,
            ),
            &allowed,
            cancel,
        )
    }

    /// First `n` rows of a source (used by the `inspect_table` tool).
    pub fn sample(&self, name: &str, n: usize) -> EngineResult<QueryResult> {
        require_capability(
            self.settings().capabilities.table_analysis,
            "Table analysis",
        )?;
        let workspace = self.workspace.lock().unwrap_or_else(|e| e.into_inner());
        let view = workspace
            .sources
            .iter()
            .find(|s| s.name == name || s.view.as_deref() == Some(name))
            .and_then(|s| s.view.clone())
            .ok_or_else(|| EngineError::UnknownSource(name.to_string()))?;
        query_workspace(
            &*workspace.data,
            &format!(
                "SELECT * FROM {} LIMIT {}",
                data::quote_ident(&view),
                n.clamp(1, 200)
            ),
            n.clamp(1, 200),
        )
    }

    /// Run a Python snippet against the workspace data (blocking work is moved
    /// off the async executor).
    pub async fn run_python(&self, code: &str) -> EngineResult<pyexec::PyResult> {
        self.run_python_cancellable(code, Arc::new(AtomicBool::new(false)))
            .await
    }

    /// Run Python with a cooperative stop flag. Wasmi resumes the guest in
    /// bounded fuel slices so a pure-Python loop can observe the flag without
    /// leaving a detached blocking task behind.
    pub async fn run_python_cancellable(
        &self,
        code: &str,
        cancel: Arc<AtomicBool>,
    ) -> EngineResult<pyexec::PyResult> {
        require_capability(
            self.settings().capabilities.python_analysis,
            "Python analysis",
        )?;
        let (bridge, scratch_lease) = {
            let workspace = self.workspace.lock().unwrap_or_else(|e| e.into_inner());
            (
                workspace.data.python_bridge(),
                workspace.scratch.as_ref().map(WorkspaceScratch::lease),
            )
        };
        let code = code.to_string();
        tokio::task::spawn_blocking(move || {
            let _scratch_lease = scratch_lease;
            pyexec::run(&code, bridge, Some(cancel))
        })
        .await
        .map_err(|e| EngineError::msg(format!("python task panicked: {e}")))?
    }

    pub async fn run_python_cancellable_with_source_selection(
        &self,
        code: &str,
        cancel: Arc<AtomicBool>,
        selection: Option<&InvestigationSourceSelection>,
    ) -> EngineResult<pyexec::PyResult> {
        let Some(selection) = selection.filter(|selection| !selection.is_all_sources()) else {
            return self.run_python_cancellable(code, cancel).await;
        };
        require_capability(
            self.settings().capabilities.python_analysis,
            "Python analysis",
        )?;
        let (bridge, scratch_lease) = {
            let workspace = self.workspace.lock().unwrap_or_else(|e| e.into_inner());
            let allowed = workspace
                .sources
                .iter()
                .filter(|source| {
                    source.view.is_some()
                        && workspace.workspace.as_deref().is_some_and(|root| {
                            selection.includes_path(&catalog::workspace_relative_path(
                                root,
                                Path::new(&source.path),
                            ))
                        })
                })
                .flat_map(|source| [Some(source.name.as_str()), source.view.as_deref()])
                .flatten()
                .map(str::to_owned)
                .collect::<HashSet<_>>();
            (
                workspace.data.python_bridge().scoped(&allowed),
                workspace.scratch.as_ref().map(WorkspaceScratch::lease),
            )
        };
        let code = code.to_string();
        tokio::task::spawn_blocking(move || {
            let _scratch_lease = scratch_lease;
            pyexec::run(&code, bridge, Some(cancel))
        })
        .await
        .map_err(|e| EngineError::msg(format!("python task panicked: {e}")))?
    }

    /// Run the agent loop for one question, streaming progress through `emit`.
    ///
    /// `conversation_id` scopes the distilled session memory and the stop-flag,
    /// so several conversations (tabs) can run at once without clobbering each
    /// other. Its memory is kept until the tab is closed or the workspace is
    /// (re)opened.
    ///
    /// `model` is this tab's chosen model (of the one signed-in provider);
    /// `None` uses the saved default.
    pub async fn ask(
        &self,
        conversation_id: &str,
        question: &str,
        model: Option<&str>,
        emit: impl Fn(AskEvent) + Send + Sync,
    ) -> EngineResult<Answer> {
        self.ask_with_mode_and_context(conversation_id, question, model, false, &[], emit)
            .await
    }

    /// Run the agent loop with the user's interaction mode. Inspect keeps the
    /// same evidence and verification path but exposes only deterministic
    /// workspace tools.
    pub async fn ask_with_mode(
        &self,
        conversation_id: &str,
        question: &str,
        model: Option<&str>,
        inspect: bool,
        emit: impl Fn(AskEvent) + Send + Sync,
    ) -> EngineResult<Answer> {
        self.ask_with_mode_and_context(conversation_id, question, model, inspect, &[], emit)
            .await
    }

    /// Run the agent loop with structured context-picker references. The raw
    /// question remains the canonical turn question; references are added to
    /// the bounded model prompt and persisted for faithful replay.
    pub async fn ask_with_mode_and_context(
        &self,
        conversation_id: &str,
        question: &str,
        model: Option<&str>,
        inspect: bool,
        context_refs: &[ContextReference],
        emit: impl Fn(AskEvent) + Send + Sync,
    ) -> EngineResult<Answer> {
        self.ask_with_mode_and_context_and_clarification(
            conversation_id,
            question,
            AskOptions {
                model,
                inspect,
                context_refs,
                clarification_reply: None,
                investigation_id: None,
                question_id: None,
                investigation_context: None,
                rerun_of: None,
            },
            emit,
        )
        .await
    }

    /// Continue one pending clarification. `reply.turn_id` is resolved against
    /// the canonical turn store and must belong to this conversation and mount.
    pub async fn ask_with_mode_and_context_and_clarification(
        &self,
        conversation_id: &str,
        question: &str,
        options: AskOptions<'_>,
        emit: impl Fn(AskEvent) + Send + Sync,
    ) -> EngineResult<Answer> {
        let AskOptions {
            model,
            inspect,
            context_refs,
            clarification_reply: reply,
            investigation_id,
            question_id,
            investigation_context: saved_investigation_context,
            rerun_of,
        } = options;
        let _workspace_read_permit = self.workspace_gate.read();
        let settings = self.settings();
        if !settings.has_credential {
            return Err(EngineError::msg(
                "Connect a model service with /login before asking a question.",
            ));
        }
        let effective_model = model
            .map(str::trim)
            .filter(|m| !m.is_empty())
            .map(str::to_string)
            .unwrap_or(settings.model);
        if effective_model.trim().is_empty() {
            return Err(EngineError::msg(
                "No model chosen yet. Run /model to see the options and pick one.",
            ));
        }
        self.hydrate_session_from_archive(conversation_id);
        let turn_catalog = self.catalog();
        let clarification = reply
            .as_ref()
            .map(|reply| self.resolve_clarification_reply(conversation_id, reply, &turn_catalog))
            .transpose()?;
        let prior_context = clarification
            .as_ref()
            .map(|resolved| self.analysis_turn_load(&resolved.turn_id))
            .transpose()?
            .and_then(|turn| turn.investigation_context);
        let investigation_context = if let Some(snapshot) = saved_investigation_context {
            Some(snapshot)
        } else if let Some(snapshot) = prior_context {
            Some(snapshot)
        } else if let Some(id) = investigation_id {
            let investigation = investigation::load(&self.data_dir, id)?;
            if turn_catalog.workspace.as_deref() != Some(investigation.workspace_id.as_str()) {
                return Err(EngineError::msg(
                    "the Investigation does not belong to the mounted workspace",
                ));
            }
            if investigation
                .conversation_id
                .as_deref()
                .is_some_and(|linked| linked != conversation_id)
            {
                return Err(EngineError::msg(
                    "the Investigation is linked to a different conversation",
                ));
            }
            Some(investigation::context_snapshot_for_question(
                &self.data_dir,
                id,
                question_id,
            )?)
        } else if question_id.is_some() {
            return Err(EngineError::msg(
                "a Question-specific run requires its Investigation ID",
            ));
        } else {
            None
        };
        let request_question = clarification
            .as_ref()
            .map(|reply| reply.original_question.as_str())
            .unwrap_or(question);
        let analysis_question = request_question.to_owned();
        let source_selection = investigation_context
            .as_ref()
            .map(|context| &context.source_selection);
        let resolved_context_refs =
            self.resolve_artifact_context_refs(context_refs, &turn_catalog, source_selection)?;
        let context_refs = resolved_context_refs.as_slice();
        let context = self.context_packet_for_source_selection(
            &analysis_question,
            conversation_id,
            source_selection,
            investigation_context
                .as_ref()
                .map(|context| context.investigation_id.as_str()),
        );
        // Touch this conversation's memory slot (create it, mark it most-recently
        // used) and evict the least-recently-used one if we're over the cap.
        {
            let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
            inner.session_tick += 1;
            let tick = inner.session_tick;
            inner
                .sessions
                .entry(conversation_id.to_string())
                .or_default()
                .last_used = tick;
            if inner.sessions.len() > SESSION_CAP {
                if let Some(victim) = inner
                    .sessions
                    .iter()
                    .filter(|(k, _)| k.as_str() != conversation_id)
                    .min_by_key(|(_, v)| v.last_used)
                    .map(|(k, _)| k.clone())
                {
                    inner.sessions.remove(&victim);
                }
            }
        }

        // A fresh stop-flag for this run, registered under the conversation id so
        // `cancel_run(id)` can find it.
        let cancel = Arc::new(AtomicBool::new(false));
        let turn_id = crate::engine::runtime::new_turn_id();
        let question_status_context = investigation_context
            .as_ref()
            .filter(|context| context.question_id.is_some());
        let mut previous_question_turn = None;
        {
            let _guard = self
                .question_state_lock
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            self.cancel
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .insert(conversation_id.to_string(), cancel.clone());
            if let Some(snapshot) = question_status_context {
                let is_active = investigation::question_load(
                    &self.data_dir,
                    snapshot.question_id.as_deref().unwrap_or_default(),
                )
                .is_ok_and(|question| {
                    question.investigation_id == snapshot.investigation_id
                        && question.status == investigation::InvestigationQuestionStatus::Active
                });
                if is_active {
                    match investigation::question_run_started(
                        &self.data_dir,
                        &snapshot.investigation_id,
                        snapshot.question_id.as_deref().unwrap_or_default(),
                        &turn_id,
                    ) {
                        Ok(previous_turn_id) => previous_question_turn = previous_turn_id,
                        Err(error) => {
                            let mut flags = self.cancel.lock().unwrap_or_else(|e| e.into_inner());
                            if flags
                                .get(conversation_id)
                                .is_some_and(|flag| Arc::ptr_eq(flag, &cancel))
                            {
                                flags.remove(conversation_id);
                            }
                            return Err(error);
                        }
                    }
                }
            }
        }

        let llm = self.llm_with_model(&effective_model);
        emit(crate::engine::evidence::AskEvent::TurnState {
            turn_id: turn_id.clone(),
            state: crate::engine::runtime::TurnState::Received,
        });
        let has_reusable_analysis =
            self.session_has_reusable_analysis(conversation_id, &turn_catalog);
        let registry = if inspect {
            Registry::inspect_with_history(settings.capabilities, has_reusable_analysis)
        } else {
            Registry::standard_with_history(settings.capabilities, has_reusable_analysis)
        };
        let answer = agent::run(agent::RunRequest {
            engine: self,
            llm: &llm,
            registry: &registry,
            conversation_id,
            model: &effective_model,
            turn_id: &turn_id,
            question: &analysis_question,
            context: &context,
            clarification: clarification.as_ref(),
            inspect,
            context_refs,
            source_selection,
            cancel: cancel.clone(),
            investigation_context: investigation_context.as_ref(),
            emit: &emit,
        })
        .await;
        let answer = match answer {
            Ok(answer) => {
                self.persist_analysis_turn(PersistAnalysisTurn {
                    conversation_id,
                    question: &analysis_question,
                    context_refs,
                    investigation_context: investigation_context.as_ref(),
                    clarification: clarification.as_ref(),
                    context_audit: &context.audit,
                    catalog: &turn_catalog,
                    answer: &answer,
                    rerun_of: rerun_of.or(previous_question_turn.as_deref()),
                });
                if let Some(snapshot) = question_status_context {
                    let question_id = snapshot.question_id.as_deref().unwrap_or_default();
                    let context_is_current = investigation::context_snapshot_for_question(
                        &self.data_dir,
                        &snapshot.investigation_id,
                        Some(question_id),
                    )
                    .is_ok_and(|current| current == *snapshot);
                    let (status, reason) = if !context_is_current {
                        (
                            InvestigationQuestionAnswerStatus::Outdated,
                            Some("The Question, source scope, or definitions changed during this run. Run the current Question again.".to_owned()),
                        )
                    } else {
                        question_answer_status(&answer, cancel.load(Ordering::Relaxed))
                    };
                    let _guard = self
                        .question_state_lock
                        .lock()
                        .unwrap_or_else(|error| error.into_inner());
                    if let Err(error) = investigation::question_run_finished(
                        &self.data_dir,
                        &snapshot.investigation_id,
                        question_id,
                        &turn_id,
                        status,
                        reason.as_deref(),
                    ) {
                        log::warn!("could not update Question status for run {turn_id}: {error}");
                    }
                }
                let mut flags = self.cancel.lock().unwrap_or_else(|e| e.into_inner());
                if flags
                    .get(conversation_id)
                    .is_some_and(|flag| Arc::ptr_eq(flag, &cancel))
                {
                    flags.remove(conversation_id);
                }
                answer
            }
            Err(error) => {
                if let Some(snapshot) = question_status_context {
                    let _guard = self
                        .question_state_lock
                        .lock()
                        .unwrap_or_else(|state_error| state_error.into_inner());
                    if let Some(question_id) = snapshot.question_id.as_deref() {
                        if let Err(status_error) = investigation::question_run_finished(
                            &self.data_dir,
                            &snapshot.investigation_id,
                            question_id,
                            &turn_id,
                            InvestigationQuestionAnswerStatus::Blocked,
                            Some(&error.to_string()),
                        ) {
                            log::warn!("could not update blocked Question status for run {turn_id}: {status_error}");
                        }
                    }
                }
                let mut flags = self.cancel.lock().unwrap_or_else(|e| e.into_inner());
                if flags
                    .get(conversation_id)
                    .is_some_and(|flag| Arc::ptr_eq(flag, &cancel))
                {
                    flags.remove(conversation_id);
                }
                return Err(error);
            }
        };

        // Fold this turn into the folder's learned notes (a correction becomes
        // a vocabulary note; an ordinary question teaches nothing). Needs the
        // previous question in this conversation for the correction check, so
        // read it before the distil step below pushes this one.
        if clarification.is_none()
            && !cancel.load(Ordering::Relaxed)
            && self.answer_workspace_is_current(&answer)
        {
            let prior_q = {
                let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
                inner
                    .sessions
                    .get(conversation_id)
                    .and_then(|s| s.turns.last())
                    .map(|t| t.question.clone())
            };
            self.record_turn_memory(
                prior_q.as_deref(),
                &analysis_question,
                &answer,
                investigation_context
                    .as_ref()
                    .map(|context| context.investigation_id.as_str()),
            )
            .await;
        }

        // Distil this turn for the next question in the conversation - but not a
        // cancelled or content-free run, which would only evict a useful earlier
        // turn from the 3-slot memory.
        if !cancel.load(Ordering::Relaxed) {
            let headline: String = answer
                .text
                .lines()
                .map(str::trim)
                .find(|l| !l.is_empty())
                .unwrap_or("")
                .chars()
                .take(200)
                .collect();
            let queries: Vec<String> = answer
                .evidence
                .iter()
                .filter(|e| {
                    matches!(
                        e.tool.as_str(),
                        "run_sql" | "make_chart" | "forecast_analysis"
                    ) && e.error.is_none()
                })
                .filter_map(|e| e.sql.clone())
                .map(|s| {
                    s.split_whitespace()
                        .collect::<Vec<_>>()
                        .join(" ")
                        .chars()
                        .take(160)
                        .collect()
                })
                .take(3)
                .collect();
            let uninformative = queries.is_empty()
                && (headline.is_empty()
                    || headline == "Stopped."
                    || headline.starts_with("I ran out of analysis steps"));
            if !uninformative {
                let workspace = self.workspace.lock().unwrap_or_else(|e| e.into_inner());
                let current = match &answer.workspace {
                    Some(snapshot) => {
                        workspace.workspace.as_deref() == Some(Path::new(&snapshot.path))
                            && workspace.revision.as_deref() == Some(snapshot.revision.as_str())
                    }
                    None => workspace.workspace.is_none() && workspace.revision.is_none(),
                };
                if current {
                    let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
                    let entry = inner
                        .sessions
                        .entry(conversation_id.to_string())
                        .or_default();
                    let mut frame = answer.contract.as_ref().and_then(contract_frame);
                    if let Some(reply) = clarification.as_ref() {
                        let resolution = format!(
                            "clarification {} -> {}",
                            reply.request.question, reply.response
                        );
                        frame = Some(match frame {
                            Some(existing) => format!("{existing}; {resolution}"),
                            None => resolution,
                        });
                    }
                    entry.turns.push(TurnDigest {
                        turn_id: Some(answer.turn_id.clone()),
                        question: analysis_question.chars().take(200).collect(),
                        headline,
                        workspace: answer
                            .workspace
                            .as_ref()
                            .map(|snapshot| TurnWorkspace::Snapshot {
                                path: snapshot.path.clone(),
                                revision: snapshot.revision.clone(),
                            })
                            .unwrap_or(TurnWorkspace::NoWorkspace),
                        source_selection: investigation_context
                            .as_ref()
                            .map(|context| context.source_selection.clone()),
                        frame: frame.map(|frame| cap_chars(&frame, 600)),
                        queries,
                    });
                    let n = entry.turns.len();
                    if n > 3 {
                        entry.turns.drain(0..n - 3);
                    }
                }
            }
        }
        Ok(answer)
    }

    fn resolve_clarification_reply(
        &self,
        conversation_id: &str,
        reply: &ClarificationReply,
        catalog: &Catalog,
    ) -> EngineResult<ResolvedClarification> {
        let response = reply.response.trim();
        if response.is_empty() || response.chars().count() > 500 {
            return Err(EngineError::msg(
                "the clarification response must be between 1 and 500 characters",
            ));
        }
        let parent = self.analysis_turn_load(&reply.turn_id)?;
        if parent.conversation_id != conversation_id {
            return Err(EngineError::msg(
                "that clarification belongs to a different conversation",
            ));
        }
        if parent.state != TurnState::Clarify {
            return Err(EngineError::msg(
                "that analysis no longer has a pending clarification",
            ));
        }
        if parent.workspace.as_deref() != catalog.workspace.as_deref() {
            return Err(EngineError::msg(
                "reopen the workspace that asked this clarification before continuing",
            ));
        }
        let request = parent
            .contract
            .as_ref()
            .and_then(|contract| contract.clarification.as_ref())
            .ok_or_else(|| EngineError::msg("the referenced turn has no pending clarification"))?
            .clone();

        Ok(ResolvedClarification {
            turn_id: parent.id,
            original_question: parent.question,
            request,
            response: response.to_string(),
            source_revision_changed: parent.workspace_revision != catalog.revision,
            parent_contract: parent.contract,
        })
    }

    /// Is the configured model provider reachable?
    pub async fn provider_health(&self) -> ProviderHealth {
        let settings = self.settings();
        if !settings.has_credential || settings.base_url.trim().is_empty() {
            return ProviderHealth {
                reachable: false,
                rejected: false,
                models: Vec::new(),
            };
        }
        self.llm().health().await
    }

    /// One model call with no tools or workspace context, using only a system
    /// and user message. Returns the reply text for bounded product proposals
    /// and the eval harness's `--judge` scorer.
    pub async fn ask_once(
        &self,
        model: Option<&str>,
        system: &str,
        user: &str,
    ) -> EngineResult<String> {
        let msgs = [
            crate::engine::llm::ChatMessage::System(system.to_string()),
            crate::engine::llm::ChatMessage::User(user.to_string()),
        ];
        let noop = |_: &str| {};
        let resp = self
            .llm_with_model(model.unwrap_or(""))
            .chat(&msgs, &[], &noop, &noop)
            .await?;
        Ok(resp.content)
    }

    /// Idempotently create the starter Report when the Northstar sample is open.
    pub fn studio_seed_northstar_sample_report(
        &self,
        expected_workspace: &str,
    ) -> EngineResult<crate::engine::artifacts::StudioDocument> {
        let data_dir = self.analytical_data_dir();
        if let Some(document) = crate::engine::artifacts::studio_northstar_sample_seeded(data_dir)?
        {
            return Ok(document);
        }

        let _workspace_read_permit = self.workspace_gate.read();
        let workspace = self
            .workspace
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let root = workspace
            .workspace
            .as_deref()
            .ok_or(EngineError::NoWorkspace)?;
        if normalized_workspace_path(root)
            != normalized_workspace_path(Path::new(expected_workspace))
        {
            return Err(EngineError::msg(
                "The Northstar sample Report can only be seeded from the currently mounted workspace.",
            ));
        }

        const REQUIRED_PATHS: &[&str] = &[
            "catalog/products.csv",
            "commerce/order_lines.csv",
            "commerce/orders/orders_2023.csv",
            "commerce/orders/orders_2024.csv",
            "commerce/orders/orders_2025.csv",
            "docs/metric_policy_current.md",
            "finance/refund_transactions.csv",
        ];
        let mut sources = HashMap::<String, &SourceInfo>::new();
        for source in &workspace.sources {
            let relative = catalog::workspace_relative_path(root, Path::new(&source.path));
            sources.insert(normalize_sample_path(&relative), source);
        }
        if REQUIRED_PATHS
            .iter()
            .any(|path| !sources.contains_key(*path))
        {
            return Err(EngineError::msg(
                "The mounted folder is missing one or more required Northstar sample sources.",
            ));
        }
        let view = |path: &str| -> EngineResult<String> {
            let source = sources.get(path).ok_or_else(|| {
                EngineError::msg(format!("Northstar source {path} is unavailable"))
            })?;
            let view = source.view.as_deref().ok_or_else(|| {
                EngineError::msg(format!("Northstar source {path} is not queryable"))
            })?;
            Ok(quote_sql_identifier(view))
        };
        let orders_2023 = view("commerce/orders/orders_2023.csv")?;
        let orders_2024 = view("commerce/orders/orders_2024.csv")?;
        let orders_2025 = view("commerce/orders/orders_2025.csv")?;
        let order_lines = view("commerce/order_lines.csv")?;
        let products = view("catalog/products.csv")?;
        let refunds = view("finance/refund_transactions.csv")?;
        let policy_source = sources
            .get("docs/metric_policy_current.md")
            .ok_or_else(|| EngineError::msg("Northstar's current metric policy is unavailable"))?;
        if policy_source.size_bytes > 64 * 1024 {
            return Err(EngineError::msg(
                "Northstar's current metric policy is larger than the sample Report limit.",
            ));
        }
        let policy_started = Instant::now();
        let policy_text = std::fs::read_to_string(&policy_source.path)
            .map_err(|error| EngineError::io("read the Northstar metric policy", error))?;
        let revision = workspace
            .revision
            .clone()
            .ok_or_else(|| EngineError::msg("The mounted Northstar workspace has no revision."))?;
        let workspace_path = root.display().to_string();
        let source_refs = |paths: &[&str]| -> Vec<serde_json::Value> {
            paths
                .iter()
                .filter_map(|path| sources.get(*path).map(|source| (path, *source)))
                .map(|(path, source)| {
                    serde_json::json!({"source":path,"table":source.view,"note":null})
                })
                .collect()
        };

        let orders_union = format!(
            r#"
                SELECT order_id, order_date, currency, lower(trim(status)) AS status FROM {orders_2023}
                UNION ALL
                SELECT order_id, order_date, currency, lower(trim(status)) AS status FROM {orders_2024}
                UNION ALL
                SELECT order_id, order_date, currency, lower(trim(status)) AS status FROM {orders_2025}
            "#
        );
        let monthly_sql = format!(
            r#"
                WITH orders AS ({orders_union}),
                order_sales AS (
                    SELECT o.order_id, substr(o.order_date, 1, 7) AS month,
                           SUM(CAST(ROUND((CAST(l.quantity AS REAL) * CAST(l.unit_price_usd AS REAL)
                               - CAST(l.line_discount_usd AS REAL)) * 100, 0) AS INTEGER))
                               AS merchandise_sales_cents
                    FROM orders o
                    JOIN {order_lines} l ON l.order_id = o.order_id
                    WHERE o.status NOT IN ('cancelled', 'canceled')
                      AND upper(trim(o.currency)) = 'USD'
                      AND substr(o.order_date, 1, 4) BETWEEN '2023' AND '2025'
                    GROUP BY o.order_id, substr(o.order_date, 1, 7)
                ),
                monthly_sales AS (
                    SELECT month, SUM(merchandise_sales_cents) AS merchandise_sales_cents,
                           COUNT(*) AS order_count
                    FROM order_sales GROUP BY month
                ),
                posted_refunds AS (
                    SELECT substr(refund_date, 1, 7) AS month,
                           SUM(CAST(ROUND(CAST(amount_usd AS REAL) * 100, 0) AS INTEGER))
                               AS posted_refunds_cents
                    FROM {refunds}
                    WHERE lower(trim(status)) = 'posted'
                      AND substr(refund_date, 1, 4) BETWEEN '2023' AND '2025'
                    GROUP BY substr(refund_date, 1, 7)
                ),
                month_keys AS (
                    SELECT month FROM monthly_sales
                    UNION SELECT month FROM posted_refunds
                )
                SELECT k.month AS month_label, k.month || '-01' AS month_start,
                       CAST(substr(k.month, 1, 4) AS INTEGER) AS year,
                       COALESCE(s.merchandise_sales_cents, 0) / 100.0 AS merchandise_sales,
                       COALESCE(r.posted_refunds_cents, 0) / 100.0 AS posted_refunds,
                       (COALESCE(s.merchandise_sales_cents, 0)
                           - COALESCE(r.posted_refunds_cents, 0)) / 100.0
                           AS net_merchandise_revenue,
                       COALESCE(s.order_count, 0) AS order_count
                FROM month_keys k
                LEFT JOIN monthly_sales s ON s.month = k.month
                LEFT JOIN posted_refunds r ON r.month = k.month
                ORDER BY k.month
            "#
        );
        let quarter_sql = format!(
            r#"
                WITH orders AS (
                    SELECT order_id, order_date, currency, lower(trim(status)) AS status
                    FROM {orders_2025}
                ),
                order_sales AS (
                    SELECT o.order_id, o.order_date,
                           SUM(CAST(ROUND((CAST(l.quantity AS REAL) * CAST(l.unit_price_usd AS REAL)
                               - CAST(l.line_discount_usd AS REAL)) * 100, 0) AS INTEGER))
                               AS merchandise_sales_cents
                    FROM orders o
                    JOIN {order_lines} l ON l.order_id = o.order_id
                    WHERE o.status NOT IN ('cancelled', 'canceled')
                      AND upper(trim(o.currency)) = 'USD'
                    GROUP BY o.order_id, o.order_date
                ),
                sales AS (
                    SELECT 'Q' || CAST((CAST(substr(order_date, 6, 2) AS INTEGER) - 1) / 3 + 1 AS INTEGER) AS quarter,
                           SUM(merchandise_sales_cents) AS merchandise_sales_cents,
                           COUNT(*) AS order_count
                    FROM order_sales
                    WHERE substr(order_date, 1, 4) = '2025'
                    GROUP BY 1
                ),
                refunds AS (
                    SELECT 'Q' || CAST((CAST(substr(refund_date, 6, 2) AS INTEGER) - 1) / 3 + 1 AS INTEGER) AS quarter,
                           SUM(CAST(ROUND(CAST(amount_usd AS REAL) * 100, 0) AS INTEGER))
                               AS posted_refunds_cents
                    FROM {refunds}
                    WHERE lower(trim(status)) = 'posted' AND substr(refund_date, 1, 4) = '2025'
                    GROUP BY 1
                ),
                periods AS (SELECT 'Q3' AS quarter UNION ALL SELECT 'Q4'),
                totals AS (
                    SELECT p.quarter,
                           COALESCE(s.merchandise_sales_cents, 0) AS merchandise_sales_cents,
                           COALESCE(r.posted_refunds_cents, 0) AS posted_refunds_cents,
                           COALESCE(s.order_count, 0) AS order_count,
                           COALESCE(s.merchandise_sales_cents, 0)
                               - COALESCE(r.posted_refunds_cents, 0) AS net_merchandise_revenue_cents
                    FROM periods p
                    LEFT JOIN sales s ON s.quarter = p.quarter
                    LEFT JOIN refunds r ON r.quarter = p.quarter
                )
                SELECT period_totals.quarter,
                       period_totals.merchandise_sales_cents / 100.0 AS merchandise_sales,
                       period_totals.posted_refunds_cents / 100.0 AS posted_refunds,
                       period_totals.net_merchandise_revenue_cents / 100.0 AS net_merchandise_revenue,
                       period_totals.order_count,
                       CASE WHEN period_totals.order_count = 0 THEN NULL
                            ELSE period_totals.merchandise_sales_cents / 100.0 / period_totals.order_count
                       END AS merchandise_aov,
                       CASE WHEN period_totals.quarter = 'Q4'
                            THEN (period_totals.net_merchandise_revenue_cents - q3.net_merchandise_revenue_cents) / 100.0
                            ELSE NULL END AS net_change,
                       CASE WHEN period_totals.quarter = 'Q4' AND q3.net_merchandise_revenue_cents <> 0
                            THEN 1.0 * (period_totals.net_merchandise_revenue_cents - q3.net_merchandise_revenue_cents)
                                 / q3.net_merchandise_revenue_cents
                            ELSE NULL END AS net_change_percent
                FROM totals AS period_totals
                CROSS JOIN (
                    SELECT net_merchandise_revenue_cents FROM totals WHERE quarter = 'Q3'
                ) AS q3
                ORDER BY CASE period_totals.quarter WHEN 'Q3' THEN 0 ELSE 1 END
            "#
        );
        let category_sql = format!(
            r#"
                WITH orders AS (
                    SELECT order_id, order_date, currency, lower(trim(status)) AS status
                    FROM {orders_2025}
                )
                SELECT p.category,
                       'Q' || CAST((CAST(substr(o.order_date, 6, 2) AS INTEGER) - 1) / 3 + 1 AS INTEGER) AS quarter,
                       SUM(CAST(ROUND((CAST(l.quantity AS REAL) * CAST(l.unit_price_usd AS REAL)
                           - CAST(l.line_discount_usd AS REAL)) * 100, 0) AS INTEGER)) / 100.0
                           AS merchandise_sales
                FROM orders o
                JOIN {order_lines} l ON l.order_id = o.order_id
                JOIN {products} p ON p.product_id = l.product_id
                WHERE o.status NOT IN ('cancelled', 'canceled')
                  AND upper(trim(o.currency)) = 'USD'
                  AND substr(o.order_date, 1, 4) = '2025'
                  AND CAST(substr(o.order_date, 6, 2) AS INTEGER) BETWEEN 7 AND 12
                GROUP BY p.category, quarter
                ORDER BY p.category, quarter
            "#
        );
        let refund_sql = format!(
            r#"
                SELECT reason,
                       'Q' || CAST((CAST(substr(refund_date, 6, 2) AS INTEGER) - 1) / 3 + 1 AS INTEGER) AS quarter,
                       SUM(CAST(ROUND(CAST(amount_usd AS REAL) * 100, 0) AS INTEGER)) / 100.0
                           AS posted_refunds
                FROM {refunds}
                WHERE lower(trim(status)) = 'posted' AND substr(refund_date, 1, 4) = '2025'
                  AND CAST(substr(refund_date, 6, 2) AS INTEGER) BETWEEN 7 AND 12
                GROUP BY reason, quarter
                ORDER BY reason, quarter
            "#
        );

        let query_specs = [
            (
                "northstar_months_v2",
                "Monthly merchandise sales, posted refunds, net revenue, and order counts for 2023–2025.",
                monthly_sql,
                vec![
                    "commerce/order_lines.csv",
                    "commerce/orders/orders_2023.csv",
                    "commerce/orders/orders_2024.csv",
                    "commerce/orders/orders_2025.csv",
                    "finance/refund_transactions.csv",
                ],
            ),
            (
                "northstar_quarters_v2",
                "Q3/Q4 2025 discounted merchandise sales, posted refunds, net revenue, order counts, and AOV.",
                quarter_sql,
                vec![
                    "commerce/order_lines.csv",
                    "commerce/orders/orders_2025.csv",
                    "finance/refund_transactions.csv",
                ],
            ),
            (
                "northstar_categories_v2",
                "Q3/Q4 2025 product category merchandise sales after line discounts and before refunds.",
                category_sql,
                vec![
                    "catalog/products.csv",
                    "commerce/order_lines.csv",
                    "commerce/orders/orders_2025.csv",
                ],
            ),
            (
                "northstar_refunds_v2",
                "Q3/Q4 2025 posted refund amounts by reason and refund date.",
                refund_sql,
                vec!["finance/refund_transactions.csv"],
            ),
        ];
        let query_started = Instant::now();
        let mut evidence = Vec::with_capacity(query_specs.len());
        let mut trace_steps = Vec::with_capacity(query_specs.len());
        let mut used_sources = Vec::new();
        for (id, note, sql, paths) in query_specs {
            let result = query_workspace(&*workspace.data, &sql, 2_000)?;
            if result.truncated {
                return Err(EngineError::msg(format!(
                    "The Northstar sample result {id} exceeded the saved-row limit."
                )));
            }
            for path in &paths {
                if let Some(source) = sources.get(*path) {
                    used_sources.push(crate::engine::runtime::AnalysisSourceReference {
                        source: (*path).to_owned(),
                        table: source.view.clone(),
                        evidence_ids: vec![id.to_owned()],
                    });
                }
            }
            let result_summary = format!("{} rows returned from read-only SQL.", result.row_count);
            let source_refs = source_refs(&paths);
            trace_steps.push(crate::engine::runtime::TraceStep {
                id: format!("step_{id}"),
                operation: "run_sql".to_owned(),
                duration_ms: result.ms,
                success: true,
                summary: Some(result_summary.clone()),
                sources: paths.iter().map(|path| (*path).to_owned()).collect(),
            });
            evidence.push(serde_json::json!({
                "id":id,
                "tool":"run_sql",
                "sources":source_refs,
                "args":{"max_rows":2000},
                "note":note,
                "sql":sql,
                "result_summary":result_summary,
                "columns":result.columns,
                "rows":result.rows,
                "row_count":result.row_count,
                "ms":result.ms,
                "truncated":false
            }));
        }
        let policy_result_id = "northstar_policy_v2";
        used_sources.push(crate::engine::runtime::AnalysisSourceReference {
            source: "docs/metric_policy_current.md".to_owned(),
            table: None,
            evidence_ids: vec![policy_result_id.to_owned()],
        });
        trace_steps.push(crate::engine::runtime::TraceStep {
            id: format!("step_{policy_result_id}"),
            operation: "read_file".to_owned(),
            duration_ms: policy_started.elapsed().as_millis() as u64,
            success: true,
            summary: Some("Read the current Northstar finance metric policy.".to_owned()),
            sources: vec!["docs/metric_policy_current.md".to_owned()],
        });
        evidence.push(serde_json::json!({
            "id":policy_result_id,
            "tool":"read_file",
            "args":{"name":"docs/metric_policy_current.md"},
            "note":"Read the current finance metric definition and reporting date rules.",
            "result_summary":"Current finance policy text saved as source evidence.",
            "output":policy_text,
            "ms":policy_started.elapsed().as_millis() as u64
        }));
        let spec = crate::engine::artifacts::northstar_sample_report_spec();
        let turn_id = format!(
            "turn_northstar_sample_{}",
            revision
                .chars()
                .filter(|character| character.is_ascii_alphanumeric())
                .take(32)
                .collect::<String>()
        );
        let trace_id = format!("trace_{turn_id}");
        let checks = vec![
            VerificationCheck {
                label: "Queries ran against the mounted Northstar source revision".to_owned(),
                ok: true,
                outcome: Some(crate::engine::evidence::VerificationOutcome::Passed),
                detail: Some(format!("Source revision {revision}; all four queries were read-only and complete.")),
                finding: None,
            },
            VerificationCheck {
                label: "Metric date basis and category refund limitation are disclosed".to_owned(),
                ok: true,
                outcome: Some(crate::engine::evidence::VerificationOutcome::Passed),
                detail: Some("Refunds use posted date; category values remain before refunds because refund rows have no product-category key.".to_owned()),
                finding: None,
            },
        ];
        let run = AnalysisTurn {
            id: turn_id.clone(),
            conversation_id: "northstar_sample_report_seed".to_owned(),
            question: "Explain Northstar Outfitters’ merchandise revenue trend and compare Q3 with Q4 2025.".to_owned(),
            context_refs: Vec::new(),
            prior_turn_refs: Vec::new(),
            clarification_of: None,
            clarification_response: None,
            workspace: Some(workspace_path),
            workspace_revision: Some(revision.clone()),
            workspace_snapshot: None,
            source_scope: Some(AnalysisSourceScope {
                used_sources,
                skipped_sources: workspace
                    .skipped
                    .iter()
                    .map(|source| format!("{}: {}", source.name, source.reason))
                    .collect(),
            }),
            investigation_context: None,
            context_audit: None,
            provenance: None,
            rerun_of: None,
            state: TurnState::Accepted,
            contract: None,
            plan: LogicalPlan {
                strategy: PlanStrategy::DirectTools,
                steps: vec![
                    "Read the current finance metric policy and its refund date basis.".to_owned(),
                    "Aggregate discounted merchandise sales by order period.".to_owned(),
                    "Subtract only posted refunds by refund transaction period.".to_owned(),
                    "Compare categories before refunds because refunds are not category-attributed.".to_owned(),
                ],
            },
            trace: crate::engine::runtime::ExecutionTrace {
                id: trace_id,
                turn_id: turn_id.clone(),
                workspace_revision: Some(revision),
                mode: Some(crate::engine::runtime::InteractionMode::WorkspaceAsk),
                model: None,
                model_calls: Vec::new(),
                elapsed_ms: Some(query_started.elapsed().as_millis() as u64),
                steps: trace_steps,
            },
            verification: Some(VerificationReport {
                status: VerificationStatus::Verified,
                checks: checks.clone(),
            }),
            result: AnalysisResult {
                text: "Interactive sample results are ready. These saved queries are scoped to the mounted Northstar source revision; review the metric notes and source caveats in the Report before relying on them.".to_owned(),
                presentation_spec: Some(spec),
                status: VerificationStatus::Verified,
                usage: None,
                verification: checks,
                evidence,
            },
        };
        crate::engine::artifacts::studio_seed_northstar_sample_report(data_dir, &run)
    }

    /// Ask the metadata-only Report Design runtime for a preview. The response
    /// cannot execute analysis or persist changes; callers apply the returned
    /// optimistic-versioned update only after user confirmation.
    pub async fn studio_design_propose(
        &self,
        workspace: &str,
        document_id: &str,
        base_version: u64,
        target_placement_id: Option<&str>,
        request: &str,
    ) -> EngineResult<crate::engine::report_design::StudioDesignProposal> {
        if workspace != crate::engine::artifacts::APP_WORKSPACE_ID {
            return Err(EngineError::msg(
                "Reports belong to the Fella app workspace",
            ));
        }
        let document = crate::engine::artifacts::studio_load(
            self.analytical_data_dir(),
            workspace,
            document_id,
        )?;
        if document.version != base_version {
            return Err(EngineError::msg(
                "This Report changed while the design request was being prepared. Refresh it and try again.",
            ));
        }
        let available = crate::engine::artifacts::list_for_reports(self.analytical_data_dir())?;
        let user = crate::engine::report_design::user_prompt(
            &document,
            &available,
            target_placement_id,
            request,
        )?;
        let messages = [
            crate::engine::llm::ChatMessage::System(
                crate::engine::report_design::system_prompt().to_string(),
            ),
            crate::engine::llm::ChatMessage::User(user),
        ];
        let noop = |_: &str| {};
        let response = self.llm().chat_json(&messages, &noop, &noop).await?;
        crate::engine::report_design::propose(
            &document,
            &available,
            target_placement_id,
            request,
            &response.content,
        )
    }

    /// Recompute one saved SQL output with exact, source-scoped Report filters.
    /// The query runs only in the already-mounted engine for the saved source
    /// folder and revision; it never asks a model or opens a different folder.
    pub fn artifact_filter_result(
        &self,
        artifact_id: &str,
        revision: u64,
        result_id: &str,
        filters: &[crate::engine::artifacts::ReportFilterValue],
    ) -> EngineResult<crate::engine::artifacts::BoundResult> {
        require_capability(
            self.settings().capabilities.table_analysis,
            "Table analysis",
        )?;
        let binding = crate::engine::artifacts::report_filter_binding(
            self.analytical_data_dir(),
            artifact_id,
            revision,
            result_id,
            filters,
        )?;
        let workspace = self.workspace.lock().unwrap_or_else(|e| e.into_inner());
        if workspace
            .workspace
            .as_deref()
            .map(|path| path.display().to_string())
            != Some(binding.workspace.as_str().to_owned())
        {
            return Err(EngineError::msg(
                "Mount the source folder for this saved output before filtering it",
            ));
        }
        if workspace.revision.as_deref() != Some(binding.workspace_revision.as_str()) {
            return Err(EngineError::msg(
                "The mounted source has changed since this analysis. Review or rerun it before filtering",
            ));
        }

        let allowed = if binding.source_selection.is_all_sources() {
            None
        } else {
            let Some(root) = workspace.workspace.as_deref() else {
                return Err(EngineError::NoWorkspace);
            };
            Some(
                workspace
                    .sources
                    .iter()
                    .filter(|source| {
                        source.view.is_some()
                            && binding.source_selection.includes_path(
                                &catalog::workspace_relative_path(root, Path::new(&source.path)),
                            )
                    })
                    .flat_map(|source| [Some(source.name.as_str()), source.view.as_deref()])
                    .flatten()
                    .map(str::to_ascii_lowercase)
                    .collect::<HashSet<String>>(),
            )
        };
        let run_report_query = |sql: &str, max_rows: usize| match &allowed {
            Some(tables) => query_workspace_scoped(&*workspace.data, sql, max_rows, tables),
            None => query_workspace(&*workspace.data, sql, max_rows),
        };
        for check in &binding.source_checks {
            let verified = run_report_query(&check.sql, 1)?.rows.iter().any(|row| {
                let Some(value) = row.first() else {
                    return false;
                };
                let value_type = if value.is_null() {
                    "null"
                } else if value.is_string() {
                    "string"
                } else if value.is_number() {
                    "number"
                } else if value.is_boolean() {
                    "boolean"
                } else {
                    ""
                };
                let exact = match value {
                    serde_json::Value::String(value) => value.clone(),
                    value => value.to_string(),
                };
                value_type == check.value_type && exact == check.expected_value
            });
            if !verified {
                return Err(EngineError::msg(
                    "A linked filter value could not be confirmed by its saved source query",
                ));
            }
        }
        let max_rows = crate::engine::analytics::chart::MAX_SOURCE_ROWS;
        let query = run_report_query(&binding.sql, max_rows)?;
        if query.columns != binding.columns {
            return Err(EngineError::msg(
                "The replayed output fields do not match the saved result; saved values remain available",
            ));
        }
        let exact_rows = query
            .rows
            .iter()
            .map(|row| {
                row.iter()
                    .map(|value| match value {
                        serde_json::Value::String(value) => value.clone(),
                        value => value.to_string(),
                    })
                    .collect()
            })
            .collect();
        let complete = !query.truncated && query.row_count == query.rows.len();
        Ok(crate::engine::artifacts::BoundResult {
            id: result_id.to_owned(),
            columns: query.columns,
            rows: query.rows,
            exact_rows,
            row_count: query.row_count,
            complete,
        })
    }

    /// Like [`ask_once`](Self::ask_once) but also returns token usage. The eval
    /// harness's no-harness baseline needs the token count to compare
    /// efficiency against a full run. Not on any product path.
    pub async fn ask_once_usage(
        &self,
        model: Option<&str>,
        system: &str,
        user: &str,
    ) -> EngineResult<(String, Option<crate::engine::evidence::Usage>)> {
        let msgs = [
            crate::engine::llm::ChatMessage::System(system.to_string()),
            crate::engine::llm::ChatMessage::User(user.to_string()),
        ];
        let noop = |_: &str| {};
        let resp = self
            .llm_with_model(model.unwrap_or(""))
            .chat(&msgs, &[], &noop, &noop)
            .await?;
        Ok((resp.content, resp.usage))
    }

    fn llm(&self) -> LlmClient {
        self.llm_with_model("")
    }

    /// Build an LLM client for the signed-in provider, answering with `model`
    /// (empty string = the saved default). Lets each tab pick its own model.
    fn llm_with_model(&self, model: &str) -> LlmClient {
        let mut settings = self.settings();
        if !model.trim().is_empty() {
            settings.model = model.to_string();
        }
        let key = self
            .secrets
            .api_key(provider::normalize_id(&settings.provider));
        LlmClient::new(self.http.clone(), &settings, key)
    }

    /// Fire-and-forget: ask a hosted Ollama-wire provider to load the
    /// configured model now so the next question doesn't stall on a cold load.
    /// No-op without a saved BYOK credential, off a Tokio runtime, for an
    /// OpenAI-wire provider, or when `FELLA_SKIP_MODEL_WARMUP` is set (tests
    /// point at a fixed-count mock server).
    fn warm_model(&self) {
        if std::env::var_os("FELLA_SKIP_MODEL_WARMUP").is_some() {
            return;
        }
        if !self.settings().has_credential {
            return;
        }
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            let llm = self.llm();
            handle.spawn(async move { llm.warm().await });
        }
    }

    fn documents_for_selection(
        &self,
        selection: Option<&InvestigationSourceSelection>,
    ) -> Vec<(String, String, SourceKind)> {
        let workspace = self.workspace.lock().unwrap_or_else(|e| e.into_inner());
        let Some(root) = workspace.workspace.as_deref() else {
            return Vec::new();
        };
        workspace
            .sources
            .iter()
            .filter(|s| s.view.is_none())
            .filter(|s| {
                selection.is_none_or(|selection| {
                    selection
                        .includes_path(&catalog::workspace_relative_path(root, Path::new(&s.path)))
                })
            })
            .map(|s| {
                (
                    catalog::workspace_relative_path(root, Path::new(&s.path)),
                    s.path.clone(),
                    s.kind,
                )
            })
            .collect()
    }

    /// Search extracted document text with workspace-wide BM25 passage
    /// ranking, retaining regex matching for precise lookups. Results carry
    /// nearby line context. The two-pass scan is bounded;
    /// callers must preserve the `incomplete` signal when reporting no hits.
    pub fn search_files(&self, query: &str, max_hits: usize) -> EngineResult<GrepResults> {
        self.search_files_for_selection(query, max_hits, None)
    }

    pub fn search_files_for_selection(
        &self,
        query: &str,
        max_hits: usize,
        selection: Option<&InvestigationSourceSelection>,
    ) -> EngineResult<GrepResults> {
        require_capability(
            self.settings().capabilities.document_analysis,
            "Document analysis",
        )?;
        let re = regex::RegexBuilder::new(query)
            .case_insensitive(true)
            .build()
            .map_err(|e| EngineError::msg(format!("bad pattern: {e}")))?;
        let terms = document_search_terms(query);
        let phrase = document_search_tokens(query).join(" ");
        let prefer_exact_phrase =
            !has_search_operators(query) && document_search_tokens(query).len() > 1;
        let max_hits = max_hits.clamp(1, 50);
        let candidate_cap = max_hits.saturating_mul(16).clamp(48, 800);
        let documents = self.documents_for_selection(selection);

        // First pass computes workspace-wide document frequency and average
        // passage length for BM25. Text stays streamed and is never persisted.
        let mut document_frequencies = vec![0usize; terms.len()];
        let mut passage_count = 0usize;
        let mut total_tokens = 0usize;
        let mut scanned_bytes = 0usize;
        let mut incomplete = false;
        for (name, path, kind) in &documents {
            let mut window = VecDeque::new();
            let scan =
                scan_document_lines(self, path, *kind, &mut scanned_bytes, |number, line| {
                    window.push_back(prepare_search_line(number, line, &terms, &re));
                    if window.len() > DOCUMENT_SEARCH_WINDOW_LINES {
                        window.pop_front();
                    }
                    let (frequencies, length) = document_window_frequencies(&window, terms.len());
                    passage_count += 1;
                    total_tokens += length;
                    for (index, frequency) in frequencies.iter().enumerate() {
                        if *frequency > 0 {
                            document_frequencies[index] += 1;
                        }
                    }
                });
            match scan {
                Ok(true) => {
                    incomplete = true;
                    break;
                }
                Ok(false) => {}
                Err(error) => {
                    log::warn!("search_files: could not scan {}: {error}", name);
                    incomplete = true;
                }
            }
        }

        let average_length = if passage_count == 0 {
            0.0
        } else {
            total_tokens as f64 / passage_count as f64
        };
        let idf: Vec<f64> = document_frequencies
            .iter()
            .map(|frequency| {
                let n = passage_count as f64;
                let df = *frequency as f64;
                (1.0 + (n - df + 0.5) / (df + 0.5)).ln()
            })
            .collect();

        // Second pass applies BM25 with those workspace-wide frequencies,
        // keeping only a bounded pool of the best passages.
        let mut candidates = Vec::new();
        let mut scanned_bytes = 0usize;
        for (name, path, kind) in &documents {
            let mut window = VecDeque::new();
            let scan =
                scan_document_lines(self, path, *kind, &mut scanned_bytes, |number, line| {
                    window.push_back(prepare_search_line(number, line, &terms, &re));
                    if window.len() > DOCUMENT_SEARCH_WINDOW_LINES {
                        window.pop_front();
                    }
                    if let Some(candidate) =
                        rank_document_window(name, &window, &idf, average_length, &phrase)
                    {
                        candidates.push(candidate);
                        if candidates.len() > candidate_cap * 2 {
                            sort_ranked_hits(&mut candidates);
                            candidates.truncate(candidate_cap);
                        }
                    }
                });
            match scan {
                Ok(true) => {
                    incomplete = true;
                    break;
                }
                Ok(false) => {}
                Err(error) => {
                    log::warn!("search_files: could not scan {}: {error}", name);
                    incomplete = true;
                }
            }
        }

        sort_ranked_hits(&mut candidates);
        // A plain multiword query is most often an intentional phrase search.
        // If that phrase exists, don't flood the model with passages that only
        // match a common individual word; if it doesn't, retain BM25's broad
        // multi-term fallback for labels/wording that don't occur together.
        if prefer_exact_phrase && candidates.iter().any(|candidate| candidate.phrase_match) {
            candidates.retain(|candidate| candidate.phrase_match);
        }
        let mut selected: Vec<RankedGrepHit> = Vec::new();
        for candidate in candidates {
            let overlaps = selected.iter().any(|prior| {
                prior.hit.source == candidate.hit.source
                    && candidate.first_line <= prior.last_line.saturating_add(1)
                    && prior.first_line <= candidate.last_line.saturating_add(1)
            });
            if !overlaps {
                selected.push(candidate);
                if selected.len() >= max_hits {
                    break;
                }
            }
        }

        Ok(GrepResults {
            hits: selected
                .into_iter()
                .map(|candidate| candidate.hit)
                .collect(),
            incomplete,
        })
    }

    /// Exact case-insensitive regex lookup retained for callers that need
    /// grep-like line semantics. The agent-facing tool uses `search_files`.
    pub fn grep_files(&self, pattern: &str, max_hits: usize) -> EngineResult<Vec<GrepHit>> {
        self.grep_files_for_selection(pattern, max_hits, None)
    }

    pub fn grep_files_for_selection(
        &self,
        pattern: &str,
        max_hits: usize,
        selection: Option<&InvestigationSourceSelection>,
    ) -> EngineResult<Vec<GrepHit>> {
        require_capability(
            self.settings().capabilities.document_analysis,
            "Document analysis",
        )?;
        let re = regex::RegexBuilder::new(pattern)
            .case_insensitive(true)
            .build()
            .map_err(|e| EngineError::msg(format!("bad pattern: {e}")))?;
        let mut hits = Vec::new();
        for (name, path, kind) in self.documents_for_selection(selection) {
            if kind == SourceKind::Pdf {
                let Ok(text) = self.pdf_text(&path) else {
                    continue;
                };
                for (i, line) in text.lines().enumerate() {
                    if re.is_match(line) {
                        hits.push(GrepHit {
                            source: name.clone(),
                            line: i + 1,
                            text: line.trim().to_string(),
                        });
                        if hits.len() >= max_hits {
                            return Ok(hits);
                        }
                    }
                }
            } else {
                let mut stop = false;
                let _ = docs::grep_lines(&path, |i, line| {
                    if re.is_match(line) {
                        hits.push(GrepHit {
                            source: name.clone(),
                            line: i,
                            text: line.trim().to_string(),
                        });
                        stop = hits.len() >= max_hits;
                    }
                    !stop
                });
                if stop {
                    return Ok(hits);
                }
            }
        }
        Ok(hits)
    }

    /// Full extracted text of one catalogued document, by the workspace-relative
    /// path shown by `list_files`/`grep_files` (not an arbitrary filesystem path). Capped so one huge
    /// document can't blow the context window `grep_files` can find a spot
    /// in a bigger file first.
    pub fn read_file(&self, name: &str) -> EngineResult<(String, bool)> {
        self.read_file_for_selection(name, None)
    }

    pub fn read_file_for_selection(
        &self,
        name: &str,
        selection: Option<&InvestigationSourceSelection>,
    ) -> EngineResult<(String, bool)> {
        require_capability(
            self.settings().capabilities.document_analysis,
            "Document analysis",
        )?;
        let (path, kind) = self
            .documents_for_selection(selection)
            .into_iter()
            .find(|(n, _, _)| n == name)
            .map(|(_, p, k)| (p, k))
            .ok_or_else(|| EngineError::UnknownSource(name.to_string()))?;
        let text = if kind == SourceKind::Pdf {
            let t = self.pdf_text(&path)?;
            if docs::looks_like_no_text_layer(&t) {
                return Ok((
                    "(this PDF has no selectable text it looks like a scan or photos of pages)"
                        .to_string(),
                    false,
                ));
            }
            t.to_string()
        } else {
            // Read only what we might return, not the whole file. The extra
            // margin covers multi-byte characters so the char cap below still
            // has enough to decide "truncated".
            docs::read_text_head(&path, READ_FILE_CHAR_CAP * 4 + 64)?
        };
        Ok(match text.char_indices().nth(READ_FILE_CHAR_CAP) {
            Some((i, _)) => (text[..i].to_string(), true),
            None => (text, false),
        })
    }

    /// Extracted text of a PDF, cached by path and invalidated on mtime change.
    fn pdf_text(&self, path: &str) -> EngineResult<Arc<str>> {
        let mtime = std::fs::metadata(path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);
        if let Some((ts, text)) = self
            .doc_cache
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(path)
        {
            if *ts == mtime {
                return Ok(text.clone());
            }
        }
        let text: Arc<str> = Arc::from(docs::extract_pdf(path)?);
        const DOC_CACHE_CAP_BYTES: usize = 32 * 1024 * 1024;
        let mut cache = self.doc_cache.lock().unwrap_or_else(|e| e.into_inner());
        if text.len() <= DOC_CACHE_CAP_BYTES {
            cache.insert(path.to_string(), (mtime, text.clone()));
            let mut cached_bytes = cache.values().map(|(_, value)| value.len()).sum::<usize>();
            while cached_bytes > DOC_CACHE_CAP_BYTES {
                let Some(key) = cache.keys().next().cloned() else {
                    break;
                };
                if let Some((_, value)) = cache.remove(&key) {
                    cached_bytes = cached_bytes.saturating_sub(value.len());
                }
            }
        }
        Ok(text)
    }

    fn persist_sources(&self, workspace: &Path, sources: &[SourceInfo]) {
        let mut conn = self.sqlite.lock().unwrap_or_else(|e| e.into_inner());
        let Ok(tx) = conn.transaction() else {
            log::warn!("couldn't start workspace source index transaction");
            return;
        };
        let ws = workspace.display().to_string();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let indexed = (|| -> rusqlite::Result<()> {
            tx.execute("DELETE FROM sources WHERE workspace = ?1", [&ws])?;
            {
                let mut insert = tx.prepare_cached(
                    "INSERT OR REPLACE INTO sources (workspace, name, path, kind, view, row_count)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                )?;
                for source in sources {
                    insert.execute(rusqlite::params![
                        ws,
                        source.name,
                        source.path,
                        format!("{:?}", source.kind).to_lowercase(),
                        source.view,
                        source.row_count,
                    ])?;
                }
            }
            tx.execute(
                "INSERT OR REPLACE INTO recent_workspaces (path, opened_at) VALUES (?1, ?2)",
                rusqlite::params![ws, now],
            )?;
            Ok(())
        })();
        if let Err(error) = indexed {
            log::warn!("couldn't persist workspace source index: {error}");
            return;
        }
        if let Err(error) = tx.commit() {
            log::warn!("couldn't commit workspace source index: {error}");
        }
    }
}

/// Promote only revision-bound, verified contract bindings to durable
/// observations. A model proposal or a merely executable query is not enough
/// to teach future turns a definition.
fn observed_contract_facts(
    answer: &Answer,
    scope: &str,
    revision: Option<String>,
    evidence_ids: &[String],
    at_ms: u64,
) -> Vec<SemanticFact> {
    if !matches!(
        answer.status,
        crate::engine::evidence::VerificationStatus::Verified
    ) || evidence_ids.is_empty()
    {
        return Vec::new();
    }
    let Some(contract) = answer.contract.as_ref() else {
        return Vec::new();
    };
    if contract.interpretation != InterpretationStatus::Grounded {
        return Vec::new();
    }

    let make = |key: String, statement: String| {
        SemanticFact::new(SemanticFactInput {
            kind: FactKind::FieldBinding,
            key,
            statement,
            authority: FactAuthority::Observed,
            workspace: scope.to_string(),
            revision: revision.clone(),
            supporting_turn: Some(answer.turn_id.clone()),
            evidence_ids: evidence_ids.to_vec(),
            at_ms,
        })
    };
    let mut facts = Vec::new();
    for measure in &contract.measures {
        let Some(field) = measure
            .field
            .as_deref()
            .filter(|field| !field.trim().is_empty())
        else {
            continue;
        };
        facts.push(make(
            format!("measure:{}", measure.concept),
            format!(
                "{} uses field `{field}` for {}",
                measure.concept, measure.operation
            ),
        ));
    }
    for filter in &contract.filters {
        let Some(field) = filter
            .field
            .as_deref()
            .filter(|field| !field.trim().is_empty())
        else {
            continue;
        };
        facts.push(make(
            format!("filter:{}", filter.concept),
            format!("{} uses field `{field}`", filter.concept),
        ));
    }
    if let Some(time) = &contract.time {
        if let Some(field) = time
            .field
            .as_deref()
            .filter(|field| !field.trim().is_empty())
        {
            facts.push(make(
                "time-field".into(),
                format!("time uses field `{field}`"),
            ));
        }
    }
    for field in &contract.group_by {
        if !field.trim().is_empty() {
            facts.push(make(
                format!("group:{field}"),
                format!("grouping uses field `{field}`"),
            ));
        }
    }
    facts
}

/// Store durable-looking pieces of the model's typed interpretation as
/// proposals. They remain non-promptable until a user accepts them, and the
/// Investigation that produced each proposal remains part of its provenance.
fn proposed_contract_facts(
    answer: &Answer,
    scope: &str,
    revision: Option<String>,
    evidence_ids: &[String],
    investigation_id: Option<&str>,
    at_ms: u64,
) -> Vec<SemanticFact> {
    let Some(contract) = answer.contract.as_ref() else {
        return Vec::new();
    };

    let mut facts = Vec::new();
    let mut propose = |key: String, statement: String| {
        let statement = statement.trim();
        if statement.is_empty() || facts.len() >= 24 {
            return;
        }
        let mut fact = SemanticFact::new(SemanticFactInput {
            kind: FactKind::Definition,
            key,
            statement: statement.chars().take(1_000).collect(),
            authority: FactAuthority::ModelSuggestion,
            workspace: scope.to_string(),
            revision: revision.clone(),
            supporting_turn: Some(answer.turn_id.clone()),
            evidence_ids: evidence_ids.to_vec(),
            at_ms,
        });
        fact.investigation_id = investigation_id.map(str::to_string);
        fact.origin_investigation_id = investigation_id.map(str::to_string);
        facts.push(fact);
    };

    for assumption in contract.assumptions.iter().take(12) {
        let mut key_text = String::new();
        for character in assumption.chars() {
            if character.is_alphanumeric() {
                key_text.extend(character.to_lowercase());
            } else if !key_text.is_empty() && !key_text.ends_with('-') {
                key_text.push('-');
            }
            if key_text.len() >= 64 {
                break;
            }
        }
        let key_text = key_text.trim_end_matches('-');
        propose(
            format!(
                "assumption:{}",
                if key_text.is_empty() {
                    "general"
                } else {
                    key_text
                }
            ),
            assumption.clone(),
        );
    }
    if let Some(value_semantics) = contract.value_semantics.as_deref() {
        propose(
            "value-semantics".into(),
            format!("Value semantics: {value_semantics}"),
        );
    }
    if let Some(missing_policy) = contract.missing_policy.as_deref() {
        propose(
            "missing-value-policy".into(),
            format!("Missing-value policy: {missing_policy}"),
        );
    }
    if let Some(denominator) = contract.denominator.as_deref() {
        propose("denominator".into(), format!("Denominator: {denominator}"));
    }
    for metric in contract.derived_metrics.iter().take(8) {
        let operator = match metric.kind {
            crate::engine::runtime::DerivedMetricKind::Ratio => "divides",
            crate::engine::runtime::DerivedMetricKind::Difference => "subtracts",
        };
        propose(
            format!("derived-metric:{}", metric.concept.to_lowercase()),
            format!(
                "{} {} {} from {}{}",
                metric.concept,
                operator,
                metric.numerator,
                metric.denominator,
                metric
                    .unit
                    .as_deref()
                    .map(|unit| format!(" (unit: {unit})"))
                    .unwrap_or_default()
            ),
        );
    }
    facts
}

#[cfg(test)]
mod proposed_contract_fact_tests {
    use super::*;

    fn answer_with_contract(contract: AnalysisContract) -> Answer {
        Answer {
            turn_id: "analysis-turn-1".into(),
            trace: crate::engine::runtime::ExecutionTrace {
                id: "trace-1".into(),
                turn_id: "analysis-turn-1".into(),
                workspace_revision: Some("sales-r4".into()),
                mode: None,
                model: None,
                model_calls: Vec::new(),
                elapsed_ms: None,
                steps: Vec::new(),
            },
            plan: None,
            contract: Some(contract),
            grounding: None,
            text: "The analysis is complete.".into(),
            presentation_spec: None,
            provenance: crate::engine::evidence::AnswerProvenance::default(),
            evidence: Vec::new(),
            verification: Vec::new(),
            status: VerificationStatus::NeedsReview,
            clarification: None,
            workspace: None,
            usage: None,
        }
    }

    #[test]
    fn analysis_contract_suggestions_keep_investigation_and_evidence_provenance() {
        let answer = answer_with_contract(AnalysisContract {
            assumptions: vec!["Orders exclude test accounts".into()],
            value_semantics: Some("positive values are gross merchandise revenue".into()),
            missing_policy: Some("exclude rows with unknown order dates".into()),
            denominator: Some("completed orders".into()),
            derived_metrics: vec![crate::engine::runtime::ContractDerivedMetric {
                concept: "average order value".into(),
                kind: crate::engine::runtime::DerivedMetricKind::Ratio,
                numerator: "net revenue".into(),
                denominator: "completed orders".into(),
                unit: Some("USD/order".into()),
            }],
            ..Default::default()
        });

        let facts = proposed_contract_facts(
            &answer,
            "/workspace/sales",
            Some("sales-r4".into()),
            &["evidence-1".into()],
            Some("investigation-1"),
            123,
        );

        assert_eq!(facts.len(), 5);
        assert!(facts.iter().all(|fact| {
            fact.authority == FactAuthority::ModelSuggestion
                && fact.state == semantic_memory::FactState::Proposed
                && fact.workspace == "/workspace/sales"
                && fact.investigation_id.as_deref() == Some("investigation-1")
                && fact.origin_investigation_id.as_deref() == Some("investigation-1")
                && fact.revision.as_deref() == Some("sales-r4")
                && fact.supporting_turn.as_deref() == Some("analysis-turn-1")
                && fact.evidence_ids == vec!["evidence-1".to_string()]
        }));
        assert!(facts.iter().any(|fact| {
            fact.key == "assumption:orders-exclude-test-accounts"
                && fact.statement == "Orders exclude test accounts"
        }));
        assert!(facts.iter().any(|fact| {
            fact.key == "derived-metric:average order value"
                && fact.statement.contains("net revenue")
        }));
    }
}

#[cfg(test)]
mod memory_resolution_tests {
    use super::*;

    #[test]
    fn accepting_and_editing_a_suggestion_preserves_its_scope_and_provenance() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let data_dir = std::env::temp_dir().join(format!("fella-memory-resolution-{nonce}"));
        let workspace_path = data_dir.join("workspace");
        let other_workspace_path = data_dir.join("other-workspace");
        std::fs::create_dir_all(&workspace_path).unwrap();
        std::fs::create_dir_all(&other_workspace_path).unwrap();
        let engine = EngineState::new(&data_dir).unwrap();
        let memory_path = data_dir.join("memory/workspace.md");
        {
            let mut workspace = engine.workspace.lock().unwrap();
            workspace.workspace = Some(workspace_path.clone());
            workspace.memory_path = Some(memory_path.clone());
        }
        let local_investigation = investigation::create(
            &data_dir,
            workspace_path.to_str().unwrap(),
            "Local analysis",
            None,
            None,
            None,
        )
        .unwrap();
        let other_investigation = investigation::create(
            &data_dir,
            workspace_path.to_str().unwrap(),
            "Separate analysis",
            None,
            None,
            None,
        )
        .unwrap();
        let other_workspace_investigation = investigation::create(
            &data_dir,
            other_workspace_path.to_str().unwrap(),
            "Different workspace",
            None,
            None,
            None,
        )
        .unwrap();

        let mut proposal = SemanticFact::new(SemanticFactInput {
            kind: FactKind::Definition,
            key: "value-semantics".into(),
            statement: "Net merchandise revenue excludes shipping".into(),
            authority: FactAuthority::ModelSuggestion,
            workspace: workspace_path.to_str().unwrap().into(),
            revision: Some("sales-r4".into()),
            supporting_turn: Some("analysis-turn-1".into()),
            evidence_ids: vec!["evidence-1".into()],
            at_ms: 123,
        });
        proposal.investigation_id = Some(local_investigation.id.clone());
        proposal.origin_investigation_id = Some(local_investigation.id.clone());
        let proposal_id = proposal.id.clone();
        let mut memory = SemanticMemory::load(&semantic_memory::facts_path_for(&memory_path));
        memory.upsert(proposal);
        memory.save();

        let accepted = engine
            .resolve_folder_memory_fact(&proposal_id, "accept", None, Some(&local_investigation.id))
            .unwrap();
        let confirmed = accepted
            .iter()
            .find(|fact| {
                fact.authority == FactAuthority::Confirmed
                    && fact.state == semantic_memory::FactState::Active
            })
            .unwrap();
        assert_eq!(
            confirmed.investigation_id.as_deref(),
            Some(local_investigation.id.as_str())
        );
        assert_eq!(
            confirmed.origin_investigation_id.as_deref(),
            Some(local_investigation.id.as_str())
        );
        assert_eq!(
            confirmed.supporting_turn.as_deref(),
            Some("analysis-turn-1")
        );
        assert_eq!(confirmed.evidence_ids, vec!["evidence-1".to_string()]);
        let confirmed_id = confirmed.id.clone();

        engine
            .resolve_folder_memory_fact(
                &confirmed_id,
                "edit",
                Some("Net merchandise revenue excludes shipping and gift cards"),
                Some(&local_investigation.id),
            )
            .unwrap();
        let edited = engine.folder_memory_facts().unwrap();
        let active = edited
            .iter()
            .find(|fact| fact.state == semantic_memory::FactState::Active)
            .unwrap();
        assert_eq!(active.authority, FactAuthority::User);
        assert_eq!(
            active.investigation_id.as_deref(),
            Some(local_investigation.id.as_str())
        );
        assert_eq!(
            active.origin_investigation_id.as_deref(),
            Some(local_investigation.id.as_str())
        );
        assert_eq!(active.supporting_turn.as_deref(), Some("analysis-turn-1"));
        assert_eq!(active.evidence_ids, vec!["evidence-1".to_string()]);
        let active_id = active.id.clone();

        let rejected = engine.resolve_folder_memory_fact(
            &active_id,
            "edit",
            Some("Cross-workspace change"),
            Some(&other_workspace_investigation.id),
        );
        assert!(rejected.is_err());
        assert!(engine.folder_memory_facts().unwrap().iter().any(|fact| {
            fact.id == active_id && fact.state == semantic_memory::FactState::Active
        }));
        assert!(!engine
            .folder_memory_facts()
            .unwrap()
            .iter()
            .any(|fact| fact.state == semantic_memory::FactState::Active
                && fact.investigation_id.as_deref() == Some(other_investigation.id.as_str())));

        let remaining = engine
            .resolve_folder_memory_fact(&active_id, "remove", None, None)
            .unwrap();
        assert!(!remaining.iter().any(|fact| fact.id == active_id));
        assert!(remaining.iter().any(|fact| {
            fact.id == proposal_id && fact.state == semantic_memory::FactState::Superseded
        }));

        drop(engine);
        let _ = std::fs::remove_dir_all(data_dir);
    }
}

/// What to do with a new correction against the existing vocabulary list.
#[derive(Debug, Clone, PartialEq, Eq)]
enum VocabAction {
    /// Replace this existing entry's text (same key, verbatim from the list).
    Update(String),
    /// A genuinely new fact under this key.
    Add(String),
    /// Restates an existing entry with nothing new; write nothing.
    Noop,
}

/// The pre-reconciliation key heuristic: the first few content words of the
/// correction, filler ("no,", "actually", ...) skipped. Used as the ADD key
/// when there's nothing yet to reconcile against, and as the safety-net
/// fallback if the model's reconciliation reply doesn't parse a correction
/// is never silently dropped, worst case it lands under an approximate key
/// instead of updating the right one.
fn default_topic_key(correction: &str) -> String {
    let topic: String = correction
        .split_whitespace()
        .skip_while(|w| {
            matches!(
                w.trim_end_matches(&[',', ':'][..]).to_lowercase().as_str(),
                "no" | "actually" | "correction" | "wrong" | "that's" | "it's"
            )
        })
        .take(3)
        .collect::<Vec<_>>()
        .join(" ");
    if topic.is_empty() {
        correction.to_string()
    } else {
        topic
    }
}

/// Parses the model's one-line reconciliation reply (`UPDATE <key>` /
/// `ADD <key>` / `NOOP <key>`). `None` on anything that doesn't fit the
/// shape the caller falls back to treating the correction as new. For
/// UPDATE/NOOP the key must exactly (case-insensitively) match one already in
/// `existing` a hallucinated key is not trusted enough to silently overwrite
/// or drop something.
fn parse_vocab_action(reply: &str, existing: &[(String, String)]) -> Option<VocabAction> {
    let line = reply.lines().find(|l| !l.trim().is_empty())?.trim();
    let (verb, rest) = line.split_once(char::is_whitespace)?;
    let key = rest.trim().trim_matches(['`', '"', '\''].as_slice());
    if key.is_empty() {
        return None;
    }
    let exact = |k: &str| {
        existing
            .iter()
            .find(|(ek, _)| ek.eq_ignore_ascii_case(k))
            .map(|(ek, _)| ek.clone())
    };
    match verb.to_ascii_uppercase().as_str() {
        "UPDATE" => exact(key).map(VocabAction::Update),
        "NOOP" => exact(key).map(|_| VocabAction::Noop),
        "ADD" => Some(VocabAction::Add(key.to_string())),
        _ => None,
    }
}

/// Bounded leading text for the system-prompt document listing. Keeping more
/// than a title lets the model see nearby definitions and caveats during its
/// initial inspection, while the cap and short read avoid ingesting full
/// documents into the prompt. Full-text tools remain authoritative.
fn leading_document_preview(path: &str) -> Option<String> {
    const PREVIEW_CHAR_CAP: usize = 800;
    use std::io::Read;
    let mut f = std::fs::File::open(path).ok()?;
    let mut buf = [0u8; 4096];
    let n = f.read(&mut buf).ok()?;
    let text = String::from_utf8_lossy(&buf[..n]);
    let flattened = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flattened.is_empty() {
        None
    } else {
        let mut preview = cap_chars(&flattened, PREVIEW_CHAR_CAP);
        if flattened.chars().count() > PREVIEW_CHAR_CAP {
            preview.push('…');
        }
        Some(preview)
    }
}

/// Truncate to at most `cap` characters (not bytes), for a short preview
/// line no ellipsis added; the caller decides whether one reads better.
pub(crate) fn cap_chars(s: &str, cap: usize) -> String {
    match s.char_indices().nth(cap) {
        Some((i, _)) => s[..i].to_string(),
        None => s.to_string(),
    }
}

/// Render a small `QueryResult` as compact `col=val` sample lines for the
/// system-prompt schema digest.
fn query_workspace(data: &dyn DataEngine, sql: &str, max_rows: usize) -> EngineResult<QueryResult> {
    data::ensure_read_only(sql)?;
    let started = Instant::now();
    let out = data.query(sql, max_rows)?;
    Ok(QueryResult {
        columns: out.columns,
        rows: out.rows,
        row_count: out.row_count,
        ms: started.elapsed().as_millis() as u64,
        truncated: out.truncated,
    })
}

fn query_workspace_cancellable(
    data: &dyn DataEngine,
    sql: &str,
    max_rows: usize,
    cancel: Arc<AtomicBool>,
) -> EngineResult<QueryResult> {
    data::ensure_read_only(sql)?;
    let started = Instant::now();
    let out = data.query_with_cancel(sql, max_rows, cancel)?;
    Ok(QueryResult {
        columns: out.columns,
        rows: out.rows,
        row_count: out.row_count,
        ms: started.elapsed().as_millis() as u64,
        truncated: out.truncated,
    })
}

fn query_workspace_scoped(
    data: &dyn DataEngine,
    sql: &str,
    max_rows: usize,
    allowed_sources: &HashSet<String>,
) -> EngineResult<QueryResult> {
    data::ensure_read_only(sql)?;
    let started = Instant::now();
    let out = data.query_scoped(sql, max_rows, allowed_sources)?;
    Ok(QueryResult {
        columns: out.columns,
        rows: out.rows,
        row_count: out.row_count,
        ms: started.elapsed().as_millis() as u64,
        truncated: out.truncated,
    })
}

fn query_workspace_cancellable_scoped(
    data: &dyn DataEngine,
    sql: &str,
    max_rows: usize,
    allowed_sources: &HashSet<String>,
    cancel: Arc<AtomicBool>,
) -> EngineResult<QueryResult> {
    data::ensure_read_only(sql)?;
    let started = Instant::now();
    let out = data.query_with_cancel_scoped(sql, max_rows, allowed_sources, cancel)?;
    Ok(QueryResult {
        columns: out.columns,
        rows: out.rows,
        row_count: out.row_count,
        ms: started.elapsed().as_millis() as u64,
        truncated: out.truncated,
    })
}

fn mini_table(q: &QueryResult) -> Vec<String> {
    q.rows
        .iter()
        .take(3)
        .map(|row| {
            q.columns
                .iter()
                .zip(row)
                .map(|(c, v)| {
                    let s = match v {
                        serde_json::Value::Null => "".to_string(),
                        serde_json::Value::String(s) => s.clone(),
                        other => other.to_string(),
                    };
                    let s: String = s.chars().take(24).collect();
                    format!("{c}={s}")
                })
                .collect::<Vec<_>>()
                .join(", ")
        })
        .collect()
}

/// One-line candidates for `schema_block`: same-named columns may be related,
/// but name overlap alone does not establish a join key or a need to combine
/// sources. A short stoplist drops names that are too generic to be useful.
fn shared_column_hints(tables: &[&SourceInfo]) -> Vec<String> {
    const STOP: &[&str] = &[
        "id",
        "name",
        "title",
        "description",
        "note",
        "notes",
        "memo",
        "comment",
        "comments",
        "type",
        "status",
        "value",
        "amount",
        "total",
        "subtotal",
        "count",
        "price",
        "cost",
        "qty",
        "quantity",
        "label",
    ];
    let mut by_col: std::collections::BTreeMap<String, Vec<String>> =
        std::collections::BTreeMap::new();
    for t in tables {
        let view = t.view.as_deref().unwrap_or("");
        for c in t.columns.iter().flatten() {
            let low = c.name.to_lowercase();
            if STOP.contains(&low.as_str()) {
                continue;
            }
            by_col
                .entry(low)
                .or_default()
                .push(format!("{view}.\"{}\"", c.name));
        }
    }
    by_col
        .into_values()
        .filter(|refs| refs.len() >= 2)
        .map(|refs| refs.join(" \u{2194} "))
        .take(10)
        .collect()
}

/// Open `fella.db`, or, if it's corrupt (a truncated write, a bad disk),
/// move it aside as `fella.db.corrupt-<unix>` and start fresh. Losing it costs
/// the user their settings and workspace/session metadata, not their API keys
/// (`auth.json` is separate) or their archived conversations. Better than a
/// non-starting app with a stderr-only message.
fn open_or_recover(path: &Path) -> rusqlite::Connection {
    // The app was "Woody" until the rename: carry a `woody.db` (and its WAL
    // sidecars) forward the first time we open at the new name.
    if !path.exists() {
        for suffix in ["", "-wal", "-shm"] {
            let old = PathBuf::from(format!(
                "{}{suffix}",
                path.with_file_name("woody.db").display()
            ));
            let new = PathBuf::from(format!("{}{suffix}", path.display()));
            if old.exists() {
                let _ = std::fs::rename(&old, &new);
            }
        }
    }
    match sqlite::open(path) {
        Ok(c) => c,
        Err(e) => {
            log::warn!("fella.db unusable ({e}); moving it aside and starting fresh");
            let ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            for suffix in ["", "-wal", "-shm"] {
                let from = PathBuf::from(format!("{}{suffix}", path.display()));
                let to = PathBuf::from(format!("{}.corrupt-{ts}{suffix}", path.display()));
                let _ = std::fs::rename(&from, &to);
            }
            sqlite::open(path).expect("recreate fella.db after moving the corrupt one aside")
        }
    }
}

/// One-shot: move a plaintext `api_key` left in the settings table by a
/// pre-keychain build into the credential store, then drop the row.
fn migrate_legacy_key(conn: &rusqlite::Connection, secrets: &Secrets) {
    let Some(key) = sqlite::take_legacy_api_key(conn) else {
        return;
    };
    let stored = sqlite::load_settings(conn).provider;
    let id = provider::normalize_id(&stored);
    // A pre-registry key almost always belonged to the "openai-compatible"
    // (now `custom`) path; if the provider still reads as the BYOK default,
    // park it there rather than guessing a hosted provider.
    let target: &str = if id == provider::DEFAULT_ID {
        "custom"
    } else {
        id
    };
    if secrets.set_api_key(target, &key).is_ok() {
        let _ = sqlite::clear_legacy_api_key(conn);
        log::info!("migrated a stored API key out of the settings table into auth.json");
    }
}

/// On startup: if the stored provider needs an API key but none is saved (a
/// past `/logout`, a deleted `auth.json`), fall back to the BYOK default so
/// the app doesn't come up pointed at a service it can't use with a stale
/// model still showing in the status bar.
fn reconcile_provider(conn: &rusqlite::Connection, secrets: &Secrets) {
    let stored = sqlite::load_settings(conn).provider;
    match provider::get(&stored) {
        // Registered provider with a saved key needs no reconciliation.
        Some(p) if secrets.has(p.id) => return,
        // Registered but keyless: fall through and reset.
        Some(_) => {}
        // An id no build knows (a stray `/model provider x`, a removed registry
        // row). Its key, if any, is unreachable through the UI reset too.
        None => {}
    }
    let d = provider::get(provider::DEFAULT_ID).expect("the default provider is in the registry");
    let mut patch = serde_json::Map::new();
    patch.insert("provider".into(), d.id.into());
    patch.insert("base_url".into(), d.base_url.into());
    patch.insert("model".into(), d.default_model.into());
    patch.insert("embed_model".into(), d.default_embed_model.into());
    if sqlite::save_settings(conn, &patch).is_ok() {
        log::info!(
            "no usable credential for provider {stored:?}; reset to {}",
            d.id
        );
    }
}

// `AnalyticsSource` is a thin delegation to the methods just above -- the
// trait exists so `analytics::verify` (and anything else in `analytics/`)
// can be handed a narrow capability instead of the whole `EngineState`, not
// because the logic itself needs to live differently.
impl crate::engine::analytics::AnalyticsSource for EngineState {
    fn catalog(&self) -> Catalog {
        EngineState::catalog(self)
    }
    fn run_sql(&self, sql: &str) -> EngineResult<QueryResult> {
        EngineState::run_sql(self, sql)
    }
    fn run_sql_with_limit(&self, sql: &str, max_rows: usize) -> EngineResult<QueryResult> {
        EngineState::run_sql_with_limit(self, sql, max_rows)
    }
}

#[cfg(test)]
mod schema_hint_tests {
    use super::*;
    use crate::engine::catalog::{ColumnInfo, SourceInfo, SourceKind};

    fn tbl(view: &str, cols: &[&str]) -> SourceInfo {
        SourceInfo {
            name: format!("{view}.csv"),
            path: String::new(),
            kind: SourceKind::Csv,
            view: Some(view.into()),
            row_count: Some(1),
            columns: Some(cols.iter().map(|c| ColumnInfo::bare(*c, "TEXT")).collect()),
            size_bytes: 0,
            mtime: 0,
            synopsis: None,
            note: None,
        }
    }

    #[test]
    fn shared_columns_become_join_hints() {
        let orders = tbl("orders", &["order_id", "customer_id", "amount"]);
        let customers = tbl("customers", &["Customer_ID", "name", "region"]);
        let hints = shared_column_hints(&[&orders, &customers]);
        // matched case-insensitively; each side shown with its own casing
        assert_eq!(
            hints,
            vec![r#"orders."customer_id" ↔ customers."Customer_ID""#]
        );
    }

    #[test]
    fn stoplist_and_singletons_are_dropped() {
        let a = tbl("a", &["id", "name", "amount", "sku"]);
        let b = tbl("b", &["id", "name", "amount", "note"]);
        // id/name/amount are stoplisted; sku/note appear in only one table
        assert!(shared_column_hints(&[&a, &b]).is_empty());
    }
}

#[cfg(test)]
mod vocab_reconcile_tests {
    use super::*;

    #[test]
    fn default_topic_key_skips_filler_words() {
        assert_eq!(
            default_topic_key("no, actually rent should include housing"),
            "rent should include"
        );
        assert_eq!(default_topic_key("gym is under health"), "gym is under");
    }

    #[test]
    fn default_topic_key_falls_back_to_the_whole_text_if_all_filler() {
        assert_eq!(default_topic_key("no actually wrong"), "no actually wrong");
    }

    #[test]
    fn parses_update_against_an_exact_existing_key() {
        let existing = vec![("for rent totals".to_string(), "old text".to_string())];
        assert_eq!(
            parse_vocab_action("UPDATE for rent totals", &existing),
            Some(VocabAction::Update("for rent totals".into()))
        );
        // case-insensitive match, but the returned key is copied from `existing`
        assert_eq!(
            parse_vocab_action("UPDATE FOR RENT TOTALS", &existing),
            Some(VocabAction::Update("for rent totals".into()))
        );
    }

    #[test]
    fn parses_add_with_a_fresh_key() {
        let existing = vec![("for rent totals".to_string(), "old text".to_string())];
        assert_eq!(
            parse_vocab_action("ADD gym category", &existing),
            Some(VocabAction::Add("gym category".into()))
        );
    }

    #[test]
    fn parses_noop_against_an_exact_existing_key() {
        let existing = vec![("for rent totals".to_string(), "old text".to_string())];
        assert_eq!(
            parse_vocab_action("NOOP for rent totals", &existing),
            Some(VocabAction::Noop)
        );
    }

    #[test]
    fn a_hallucinated_key_on_update_or_noop_is_not_trusted() {
        let existing = vec![("for rent totals".to_string(), "old text".to_string())];
        // the model named a key that isn't actually in the list -- caller
        // falls back to ADD rather than silently overwriting/dropping.
        assert_eq!(parse_vocab_action("UPDATE some other key", &existing), None);
        assert_eq!(parse_vocab_action("NOOP some other key", &existing), None);
    }

    #[test]
    fn unparseable_replies_fall_back_to_none() {
        let existing = vec![("for rent totals".to_string(), "old text".to_string())];
        assert_eq!(
            parse_vocab_action("I think this updates the rent note.", &existing),
            None
        );
        assert_eq!(parse_vocab_action("", &existing), None);
    }
}

#[cfg(test)]
mod jit_schema_tests {
    use std::ops::Deref;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    struct ScratchDir(PathBuf);

    impl ScratchDir {
        fn new(tag: &str) -> Self {
            let n = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!("fella-jit-{tag}-{n}"));
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Deref for ScratchDir {
        type Target = Path;

        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }

    impl Drop for ScratchDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn samples_only_a_large_folders_inspected_tables() {
        let ws = ScratchDir::new("ws");
        let data = ScratchDir::new("data");
        // 6 tables: past the <=4 small-folder cutoff (samples always shown),
        // still under the <=12 tables / <=60 columns "full" ceiling (columns
        // are shown eagerly either way).
        for i in 0..6 {
            std::fs::write(
                ws.join(format!("t{i}.csv")),
                format!("id,city\n1,town{i}\n2,town{i}\n"),
            )
            .unwrap();
        }
        let engine = EngineState::new(&data).unwrap();
        engine.open_workspace(&ws).unwrap();

        let before = engine.schema_block();
        assert!(before.contains("t0"), "table names still listed");
        assert!(before.contains("\"city\""), "columns still listed eagerly");
        assert!(
            !before.contains("town0"),
            "no sample rows before any inspection"
        );
        assert!(
            !before.contains("town3"),
            "no sample rows before any inspection"
        );

        engine.describe_source("t0").unwrap();
        let after = engine.schema_block();
        assert!(
            after.contains("town0"),
            "t0's sample rows enter the digest once inspected"
        );
        assert!(
            !after.contains("town3"),
            "t3 wasn't inspected, still no samples for it"
        );
    }

    #[test]
    fn small_folders_always_show_samples() {
        let ws = ScratchDir::new("ws-small");
        let data = ScratchDir::new("data-small");
        // Only 2 tables: at or under the <=4 cutoff, samples show up-front.
        for i in 0..2 {
            std::fs::write(
                ws.join(format!("s{i}.csv")),
                format!("id,city\n1,town{i}\n2,town{i}\n"),
            )
            .unwrap();
        }
        let engine = EngineState::new(&data).unwrap();
        engine.open_workspace(&ws).unwrap();

        let block = engine.schema_block();
        assert!(block.contains("town0"));
        assert!(block.contains("town1"));
    }

    #[test]
    fn initial_workspace_map_includes_bounded_document_definitions() {
        let ws = ScratchDir::new("document-preview-ws");
        let data = ScratchDir::new("document-preview-data");
        std::fs::write(
            ws.join("analysis-notes.md"),
            "# Analysis notes\n\nThe broad operating-cost measure has no recorded definition. Fees and maintenance are separate labels, and the source does not say whether either belongs in that measure.\n",
        )
        .unwrap();

        let engine = EngineState::new(&data).unwrap();
        engine.open_workspace(&ws).unwrap();

        let question = "What is the operating-cost measure?";
        let packet = engine.context_packet(question, "document-preview-test");
        assert!(
            packet.schema.contains("has no recorded definition"),
            "the initial model context should expose relevant document caveats, not only a title"
        );
        assert!(
            packet
                .schema
                .contains("Fees and maintenance are separate labels"),
            "the preview should preserve nearby source terminology"
        );
        assert!(
            packet.schema.contains("bounded leading previews"),
            "the prompt should distinguish previews from full document contents"
        );
    }

    #[test]
    fn read_file_caps_document_text_and_reports_truncation() {
        let ws = ScratchDir::new("read-file-cap-ws");
        let data = ScratchDir::new("read-file-cap-data");
        let contents = format!("{}TAIL", "x".repeat(READ_FILE_CHAR_CAP));
        std::fs::write(ws.join("long-notes.txt"), &contents).unwrap();

        let engine = EngineState::new(&data).unwrap();
        engine.open_workspace(&ws).unwrap();
        let (text, truncated) = engine.read_file("long-notes.txt").unwrap();

        assert!(truncated);
        assert_eq!(text.chars().count(), READ_FILE_CHAR_CAP);
        assert!(!text.contains("TAIL"));
    }

    #[test]
    fn sql_row_cap_keeps_total_count_and_truncation_signal() {
        let ws = ScratchDir::new("sql-row-cap-ws");
        let data = ScratchDir::new("sql-row-cap-data");
        let rows = (0..1_001)
            .map(|value| value.to_string())
            .collect::<Vec<_>>();
        std::fs::write(
            ws.join("measurements.csv"),
            format!("value\n{}\n", rows.join("\n")),
        )
        .unwrap();

        let engine = EngineState::new(&data).unwrap();
        engine.open_workspace(&ws).unwrap();
        let result = engine.run_sql("SELECT value FROM measurements").unwrap();

        assert_eq!(
            result.rows.len(),
            crate::engine::analytics::data::DEFAULT_ROW_CAP
        );
        assert_eq!(result.row_count, 1_001);
        assert!(result.truncated);
    }
}

#[cfg(test)]
mod workspace_swap_tests {
    use std::sync::Arc;
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let n = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("fella-atomic-{tag}-{n}"));
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn replacement_never_exposes_catalog_without_its_data() {
        let old_workspace = scratch("old-workspace");
        let new_workspace = scratch("new-workspace");
        let data_dir = scratch("data");
        std::fs::write(old_workspace.join("sales.csv"), "value\n1\n").unwrap();
        std::fs::write(new_workspace.join("sales.csv"), "value\n2\n").unwrap();

        let engine = Arc::new(EngineState::new(&data_dir).unwrap());
        engine.open_workspace(&old_workspace).unwrap();

        TEST_OPEN_WORKSPACE_TARGET.store(Arc::as_ptr(&engine) as usize, Ordering::SeqCst);
        TEST_OPEN_WORKSPACE_READY.store(false, Ordering::SeqCst);
        TEST_OPEN_WORKSPACE_RESUME.store(false, Ordering::SeqCst);
        TEST_OPEN_WORKSPACE_PAUSE.store(true, Ordering::SeqCst);

        let opening = {
            let engine = Arc::clone(&engine);
            let new_workspace = new_workspace.clone();
            std::thread::spawn(move || engine.open_workspace(&new_workspace))
        };

        let deadline = Instant::now() + Duration::from_secs(5);
        while !TEST_OPEN_WORKSPACE_READY.load(Ordering::SeqCst) && Instant::now() < deadline {
            std::thread::yield_now();
        }
        let paused = TEST_OPEN_WORKSPACE_READY.load(Ordering::SeqCst);
        let observed_catalog = engine.catalog();
        let observed_query = engine.run_sql("SELECT value FROM sales");
        TEST_OPEN_WORKSPACE_RESUME.store(true, Ordering::SeqCst);
        TEST_OPEN_WORKSPACE_TARGET.store(0, Ordering::SeqCst);
        let opened = opening.join().unwrap().unwrap();

        assert!(paused, "workspace replacement did not reach its test pause");
        assert_eq!(
            observed_catalog.workspace.as_deref(),
            Some(old_workspace.to_str().unwrap())
        );
        let query = observed_query.expect("the old catalog must still have its old table");
        assert_eq!(query.rows[0][0], serde_json::json!(1));
        assert_eq!(
            opened.workspace.as_deref(),
            Some(new_workspace.to_str().unwrap())
        );
        assert_eq!(
            engine.run_sql("SELECT value FROM sales").unwrap().rows[0][0],
            serde_json::json!(2)
        );

        let _ = std::fs::remove_dir_all(old_workspace);
        let _ = std::fs::remove_dir_all(new_workspace);
        let _ = std::fs::remove_dir_all(data_dir);
    }
}

#[cfg(test)]
mod investigation_answer_status_tests {
    use super::*;

    fn temp_data_dir(label: &str) -> PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("fella-{label}-{nonce}"))
    }

    fn saved_question_turn(
        turn_id: &str,
        investigation: &Investigation,
        question: &crate::engine::investigation::InvestigationQuestion,
        context: InvestigationContextSnapshot,
    ) -> AnalysisTurn {
        let verification = vec![VerificationCheck {
            label: "date coverage".into(),
            ok: false,
            outcome: Some(crate::engine::evidence::VerificationOutcome::Flagged),
            detail: Some("Two source rows have no transaction date.".into()),
            finding: None,
        }];
        AnalysisTurn {
            id: turn_id.to_owned(),
            conversation_id: investigation.conversation_id.clone().unwrap(),
            question: question.text.clone(),
            context_refs: Vec::new(),
            prior_turn_refs: Vec::new(),
            clarification_of: None,
            clarification_response: None,
            workspace: Some(investigation.workspace_id.clone()),
            workspace_revision: Some("workspace-r1".into()),
            workspace_snapshot: None,
            source_scope: None,
            investigation_context: Some(context),
            context_audit: None,
            provenance: None,
            rerun_of: None,
            state: TurnState::NeedsReview,
            contract: None,
            plan: LogicalPlan::default(),
            trace: crate::engine::runtime::ExecutionTrace {
                id: format!("trace-{turn_id}"),
                turn_id: turn_id.to_owned(),
                workspace_revision: Some("workspace-r1".into()),
                mode: None,
                model: None,
                model_calls: Vec::new(),
                elapsed_ms: None,
                steps: Vec::new(),
            },
            verification: None,
            result: AnalysisResult {
                text: "The rows total 1,240, but two dates are missing.".into(),
                presentation_spec: None,
                status: VerificationStatus::NeedsReview,
                usage: None,
                verification,
                evidence: Vec::new(),
            },
        }
    }

    #[test]
    fn unsupported_interpretation_is_a_distinct_coverage_outcome() {
        let answer = Answer {
            turn_id: "turn-unsupported".to_owned(),
            trace: crate::engine::runtime::ExecutionTrace {
                id: "trace-unsupported".to_owned(),
                turn_id: "turn-unsupported".to_owned(),
                workspace_revision: None,
                mode: None,
                model: None,
                model_calls: Vec::new(),
                elapsed_ms: None,
                steps: Vec::new(),
            },
            plan: None,
            contract: Some(crate::engine::runtime::AnalysisContract {
                interpretation: crate::engine::runtime::InterpretationStatus::Unsupported,
                unresolved: vec!["The files do not contain cohort dates.".to_owned()],
                ..Default::default()
            }),
            grounding: None,
            text: "This cannot be answered from these files.".to_owned(),
            presentation_spec: None,
            provenance: crate::engine::evidence::AnswerProvenance::default(),
            evidence: Vec::new(),
            verification: Vec::new(),
            status: crate::engine::evidence::VerificationStatus::Failed,
            clarification: None,
            workspace: None,
            usage: None,
        };

        assert_eq!(
            question_answer_status(&answer, false),
            (
                InvestigationQuestionAnswerStatus::Unsupported,
                Some("The files do not contain cohort dates.".to_owned())
            )
        );
        assert_eq!(
            question_answer_status(&answer, true).0,
            InvestigationQuestionAnswerStatus::Blocked,
            "explicit cancellation takes precedence over the model's interpretation"
        );
    }

    #[test]
    fn question_answer_status_separates_review_insufficient_data_and_clarification() {
        let no_checks = Vec::new();
        assert_eq!(
            question_answer_status_parts(
                VerificationStatus::NeedsReview,
                None,
                &no_checks,
                None,
                false,
            ),
            (
                InvestigationQuestionAnswerStatus::NeedsReview,
                Some("The run produced an answer, but its verification is not conclusive.".into())
            ),
            "a run-wide verification concern is not proof of partial coverage"
        );
        assert_eq!(
            question_answer_status_parts(
                VerificationStatus::InsufficientData,
                None,
                &no_checks,
                None,
                false,
            ),
            (
                InvestigationQuestionAnswerStatus::InsufficientData,
                Some(
                    "The run did not produce enough accepted evidence to answer this Question."
                        .into()
                )
            )
        );

        let clarification = crate::engine::runtime::ClarificationRequest {
            question: "Which date range should I use?".into(),
            options: vec!["Calendar year".into(), "Fiscal year".into()],
            reason: Some("The source includes both calendar and fiscal periods.".into()),
        };
        assert_eq!(
            question_answer_status_parts(
                VerificationStatus::NeedsReview,
                None,
                &no_checks,
                Some(&clarification),
                false,
            ),
            (
                InvestigationQuestionAnswerStatus::WaitingForInput,
                Some("The source includes both calendar and fiscal periods.".into())
            ),
            "a decision requested from the user should not be presented as a partial answer"
        );
    }

    #[test]
    fn app_investigation_reads_do_not_mark_a_workspace_runtime_run_interrupted() {
        let data_dir = temp_data_dir("shared-question-run");
        let app = EngineState::new(&data_dir).unwrap();
        let workspace =
            EngineState::new_workspace_runtime(&data_dir, app.cancel_registry()).unwrap();
        let created = investigation::create(
            &data_dir,
            "/work/analysis",
            "Revenue analysis",
            Some("Explain revenue movement"),
            None,
            Some("conversation-analysis"),
        )
        .unwrap();
        let question = created.questions.as_ref().unwrap()[0].clone();
        investigation::question_run_started(
            &data_dir,
            &created.id,
            &question.id,
            "turn-live-question",
        )
        .unwrap();
        workspace.cancel.lock().unwrap().insert(
            "conversation-analysis".into(),
            Arc::new(AtomicBool::new(false)),
        );

        let while_running = app.investigation_load(&created.id).unwrap();
        assert_eq!(
            while_running.questions.as_ref().unwrap()[0].answer_status,
            InvestigationQuestionAnswerStatus::Running,
            "the app runtime must see the active cancellation entry owned by the workspace runtime"
        );

        workspace
            .cancel
            .lock()
            .unwrap()
            .remove("conversation-analysis");
        let after_interruption = app.investigation_load(&created.id).unwrap();
        let question = &after_interruption.questions.as_ref().unwrap()[0];
        assert_eq!(
            question.answer_status,
            InvestigationQuestionAnswerStatus::Blocked
        );
        assert_eq!(
            question.answer_status_reason.as_deref(),
            Some(investigation::INTERRUPTED_QUESTION_RUN_REASON)
        );

        drop(workspace);
        drop(app);
        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[test]
    fn recovery_restores_a_saved_review_outcome_from_the_matching_turn() {
        let data_dir = temp_data_dir("recover-question-answer");
        let app = EngineState::new(&data_dir).unwrap();
        let created = investigation::create(
            &data_dir,
            "/work/analysis",
            "Revenue analysis",
            Some("Explain revenue movement"),
            None,
            Some("conversation-analysis"),
        )
        .unwrap();
        let question = created.questions.as_ref().unwrap()[0].clone();
        let turn_id = "turn-saved-question-answer";
        investigation::question_run_started(&data_dir, &created.id, &question.id, turn_id).unwrap();
        let context = investigation::context_snapshot_for_question(
            &data_dir,
            &created.id,
            Some(&question.id),
        )
        .unwrap();
        let turn = saved_question_turn(turn_id, &created, &question, context);
        analysis_store::create(&data_dir, &turn).unwrap();

        // This reproduces the stale marker written by the earlier recovery
        // path even though the canonical run already contains a reviewable answer.
        investigation::question_run_finished(
            &data_dir,
            &created.id,
            &question.id,
            turn_id,
            InvestigationQuestionAnswerStatus::Blocked,
            Some(investigation::INTERRUPTED_QUESTION_RUN_REASON),
        )
        .unwrap();

        let recovered = app.investigation_load(&created.id).unwrap();
        let question = &recovered.questions.as_ref().unwrap()[0];
        assert_eq!(
            question.answer_status,
            InvestigationQuestionAnswerStatus::NeedsReview
        );
        assert_eq!(
            question.answer_status_reason.as_deref(),
            Some("Two source rows have no transaction date.")
        );
        assert_eq!(question.latest_analysis_turn_id.as_deref(), Some(turn_id));

        drop(app);
        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[test]
    fn legacy_incomplete_question_status_is_reclassified_from_its_saved_turn() {
        let data_dir = temp_data_dir("migrate-legacy-incomplete-question");
        let app = EngineState::new(&data_dir).unwrap();
        let created = investigation::create(
            &data_dir,
            "/work/analysis",
            "Revenue analysis",
            Some("Explain revenue movement"),
            None,
            Some("conversation-analysis"),
        )
        .unwrap();
        let question = created.questions.as_ref().unwrap()[0].clone();
        let turn_id = "turn-legacy-incomplete-question";
        investigation::question_run_started(&data_dir, &created.id, &question.id, turn_id).unwrap();
        let context = investigation::context_snapshot_for_question(
            &data_dir,
            &created.id,
            Some(&question.id),
        )
        .unwrap();
        let turn = saved_question_turn(turn_id, &created, &question, context);
        analysis_store::create(&data_dir, &turn).unwrap();
        investigation::question_run_finished(
            &data_dir,
            &created.id,
            &question.id,
            turn_id,
            InvestigationQuestionAnswerStatus::Incomplete,
            Some("The run returned without fully answering this Question."),
        )
        .unwrap();

        let migrated = app.investigation_load(&created.id).unwrap();
        let question = &migrated.questions.as_ref().unwrap()[0];
        assert_eq!(
            question.answer_status,
            InvestigationQuestionAnswerStatus::NeedsReview
        );
        assert_eq!(
            question.answer_status_reason.as_deref(),
            Some("Two source rows have no transaction date.")
        );

        drop(app);
        let _ = std::fs::remove_dir_all(data_dir);
    }
}
