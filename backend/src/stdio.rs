//! Line-delimited JSON bridge used by the Electron shell.
//!
//! Electron sends app commands as JSON lines over stdin and receives results
//! and stream events over stdout. stdout is reserved for protocol messages;
//! diagnostics stay on stderr.

use std::collections::HashMap;
use std::io::{self, BufRead, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::engine::state::CancelRegistry;
use crate::engine::{
    AskOptions, Catalog, ClarificationReply, ContextReference, EngineError, EngineResult,
    EngineState, InvestigationDefinitionInput, InvestigationFindingInput,
    InvestigationFindingUpdate, InvestigationQuestionInput, InvestigationSourceSelection,
    WorkspaceProgress,
};

type Output = Arc<Mutex<BufWriter<io::Stdout>>>;

#[derive(Debug, Deserialize)]
struct Request {
    id: u64,
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Debug, Serialize)]
struct Response {
    id: u64,
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<EngineError>,
}

fn emit(output: &Output, value: Value) {
    let Ok(mut output) = output.lock() else {
        return;
    };
    if serde_json::to_writer(&mut *output, &value).is_ok() {
        let _ = output.write_all(b"\n");
        let _ = output.flush();
    }
}

fn respond(output: &Output, id: u64, result: EngineResult<Value>) {
    match result {
        Ok(result) => emit(
            output,
            serde_json::to_value(Response {
                id,
                ok: true,
                result: Some(result),
                error: None,
            })
            .unwrap_or(Value::Null),
        ),
        Err(error) => emit(
            output,
            serde_json::to_value(Response {
                id,
                ok: false,
                result: None,
                error: Some(error),
            })
            .unwrap_or(Value::Null),
        ),
    }
}

fn event(output: &Output, id: u64, value: impl Serialize) {
    emit(
        output,
        serde_json::json!({
            "id": id,
            "event": value,
        }),
    );
}

fn serialized<T: Serialize>(value: T) -> EngineResult<Value> {
    serde_json::to_value(value).map_err(EngineError::from)
}

fn required<T: DeserializeOwned>(params: &Value, name: &str) -> EngineResult<T> {
    let value = params
        .get(name)
        .cloned()
        .ok_or_else(|| EngineError::msg(format!("missing IPC argument: {name}")))?;
    serde_json::from_value(value).map_err(EngineError::from)
}

fn expand_tilde(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix('~') {
        if rest.is_empty() || rest.starts_with('/') || rest.starts_with('\\') {
            if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))
            {
                return Path::new(&home).join(rest.trim_start_matches(['/', '\\']));
            }
        }
    }
    PathBuf::from(path)
}

fn value_result<T: Serialize>(result: EngineResult<T>) -> EngineResult<Value> {
    result.and_then(serialized)
}

const MAX_OPEN_WORKSPACES: usize = 4;

/// Owns one independent Rust runtime/catalog per open repository while sharing
/// the app database, provider settings, and credential store. Workspace IDs
/// are canonical local folder paths; display names are never used as keys.
struct WorkspaceRegistry {
    data_dir: PathBuf,
    cancel: CancelRegistry,
    open_serial: Mutex<()>,
    engines: Mutex<HashMap<PathBuf, Arc<EngineState>>>,
}

impl WorkspaceRegistry {
    fn new(data_dir: PathBuf, cancel: CancelRegistry) -> Self {
        Self {
            data_dir,
            cancel,
            open_serial: Mutex::new(()),
            engines: Mutex::new(HashMap::new()),
        }
    }

    fn canonical_path(path: &Path) -> EngineResult<PathBuf> {
        let canonical = path.canonicalize().map_err(|error| {
            EngineError::io(format!("resolve workspace {}", path.display()), error)
        })?;
        if !canonical.is_dir() {
            return Err(EngineError::msg(format!(
                "That doesn't look like a folder: {}",
                canonical.display()
            )));
        }
        Ok(canonical)
    }

    fn open_workspace_with_progress(
        &self,
        path: &Path,
        on_progress: impl FnMut(WorkspaceProgress),
    ) -> EngineResult<Catalog> {
        let canonical = Self::canonical_path(path)?;
        let _serial = self
            .open_serial
            .lock()
            .unwrap_or_else(|error| error.into_inner());

        if let Some(engine) = self
            .engines
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .get(&canonical)
            .cloned()
        {
            return Ok(engine.catalog());
        }

        let count = self
            .engines
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .len();
        if count >= MAX_OPEN_WORKSPACES {
            return Err(EngineError::msg(format!(
                "You can have up to {MAX_OPEN_WORKSPACES} repository workspaces open. Close one before opening another."
            )));
        }

        let engine = Arc::new(EngineState::new_workspace_runtime(
            &self.data_dir,
            Arc::clone(&self.cancel),
        )?);
        let catalog = engine.open_workspace_with_progress(&canonical, on_progress)?;
        self.engines
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .insert(canonical, engine);
        Ok(catalog)
    }

    fn engine(&self, workspace_id: &str) -> EngineResult<Arc<EngineState>> {
        let path = Path::new(workspace_id);
        let engines = self
            .engines
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if let Some(engine) = engines.get(path) {
            // Keep a live workspace addressable for cancellation and in-flight
            // runs even if its directory is temporarily unavailable.
            return Ok(Arc::clone(engine));
        }
        let canonical = Self::canonical_path(path)?;
        engines.get(&canonical).cloned().ok_or_else(|| {
            EngineError::msg("That repository workspace is not open in this session.")
        })
    }

    fn engine_for_request(
        &self,
        params: &Value,
        app_engine: &Arc<EngineState>,
    ) -> EngineResult<Arc<EngineState>> {
        match params.get("workspaceId") {
            None | Some(Value::Null) => Ok(Arc::clone(app_engine)),
            Some(Value::String(workspace_id)) if !workspace_id.trim().is_empty() => {
                self.engine(workspace_id)
            }
            Some(Value::String(_)) => Err(EngineError::msg("workspaceId must not be empty.")),
            Some(_) => Err(EngineError::msg(
                "workspaceId must be a folder identity string.",
            )),
        }
    }

    fn close_workspace(&self, workspace_id: &str) -> bool {
        // A closed or renamed folder may no longer canonicalize. The ID was
        // emitted as the canonical path when mounted, so remove that exact key.
        self.engines
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .remove(Path::new(workspace_id))
            .is_some()
    }

    fn forget_conversation(&self, conversation_id: &str) {
        let engines: Vec<_> = self
            .engines
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .values()
            .cloned()
            .collect();
        for engine in engines {
            engine.forget_conversation(conversation_id);
        }
    }

    fn invalidate_capability_caches(&self) {
        let engines: Vec<_> = self
            .engines
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .values()
            .cloned()
            .collect();
        for engine in engines {
            engine.invalidate_capability_schema_cache();
        }
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.engines
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .len()
    }
}

async fn dispatch(
    request: Request,
    engine: Arc<EngineState>,
    workspaces: Arc<WorkspaceRegistry>,
    output: Output,
    started: Instant,
) -> EngineResult<()> {
    let id = request.id;
    let result = async {
        let workspace_scoped = matches!(
            request.method.as_str(),
            "get_catalog"
                | "get_workspace_model"
                | "describe"
                | "sample_source"
                | "preview_file"
                | "run_sql_direct"
                | "reindex"
                | "memory_file"
                | "memory_facts"
                | "memory_fact_resolve"
                | "forget_memory"
                | "cancel"
                | "context_file"
                | "save_context"
                | "analysis_turn_replay_status"
                | "analysis_turn_rerun"
                | "artifact_compare"
                | "studio_export_package"
                | "ask"
        );
        let workspace_engine = if workspace_scoped {
            workspaces.engine_for_request(&request.params, &engine)?
        } else {
            Arc::clone(&engine)
        };
        match request.method.as_str() {
            "ping" => Ok(Value::String("pong".into())),
            "app_info" => serialized(serde_json::json!({
                "name": "fella",
                "version": env!("CARGO_PKG_VERSION"),
                "uptime_ms": started.elapsed().as_millis(),
            })),
            "app_ready" => {
                let ms = started.elapsed().as_millis();
                eprintln!("fella: interactive in {ms} ms");
                serialized(ms)
            }
            "open_workspace" => {
                let path: String = required(&request.params, "path")?;
                let workspaces = Arc::clone(&workspaces);
                let progress_output = Arc::clone(&output);
                let path = expand_tilde(&path);
                let mounted = tokio::task::spawn_blocking(move || {
                    workspaces.open_workspace_with_progress(&path, |item| {
                        event(&progress_output, id, item);
                    })
                })
                .await
                .map_err(|error| {
                    EngineError::msg(format!("workspace mount task failed: {error}"))
                })?;
                value_result(mounted)
            }
            "close_workspace" => {
                let workspace_id: String = required(&request.params, "workspaceId")?;
                workspaces.close_workspace(&workspace_id);
                Ok(Value::Null)
            }
            "get_catalog" => serialized(workspace_engine.catalog()),
            "get_workspace_model" => serialized(workspace_engine.workspace_model()),
            "last_workspace_path" => serialized(engine.last_workspace_path()),
            "describe" => {
                let name: String = required(&request.params, "name")?;
                value_result(workspace_engine.describe_source(&name))
            }
            "sample_source" => {
                let name: String = required(&request.params, "name")?;
                let rows = request
                    .params
                    .get("rows")
                    .and_then(Value::as_u64)
                    .and_then(|rows| usize::try_from(rows).ok())
                    .unwrap_or(5)
                    .clamp(1, 50);
                value_result(workspace_engine.sample(&name, rows))
            }
            "preview_file" => {
                let name: String = required(&request.params, "name")?;
                let (text, truncated) = workspace_engine.read_file(&name)?;
                serialized(serde_json::json!({ "text": text, "truncated": truncated }))
            }
            "run_sql_direct" => {
                let sql: String = required(&request.params, "sql")?;
                value_result(workspace_engine.run_sql(&sql))
            }
            "reindex" => value_result(workspace_engine.reindex()),
            "memory_file" => serialized(workspace_engine.folder_memory_file()),
            "memory_facts" => serialized(workspace_engine.folder_memory_facts()),
            "memory_fact_resolve" => {
                let fact_id: String = required(&request.params, "factId")?;
                let action: String = required(&request.params, "action")?;
                let statement = request.params.get("statement").and_then(Value::as_str);
                let target_investigation_id = request
                    .params
                    .get("targetInvestigationId")
                    .and_then(Value::as_str);
                value_result(workspace_engine.resolve_folder_memory_fact(
                    &fact_id,
                    &action,
                    statement,
                    target_investigation_id,
                ))
            }
            "forget_memory" => value_result(workspace_engine.forget_folder_memory()),
            "get_settings" => serialized(engine.settings()),
            "set_settings" => {
                let settings = request
                    .params
                    .get("settings")
                    .and_then(Value::as_object)
                    .ok_or_else(|| EngineError::msg("settings must be an object"))?;
                let invalidate = settings.contains_key("capabilities");
                let saved = engine.save_settings(settings)?;
                if invalidate {
                    workspaces.invalidate_capability_caches();
                }
                serialized(saved)
            }
            "list_providers" => serialized(engine.list_providers()),
            "set_api_key" => {
                let provider: String = required(&request.params, "provider")?;
                let key: String = required(&request.params, "key")?;
                value_result(engine.set_api_key(&provider, &key))
            }
            "logout" => {
                let provider: String = required(&request.params, "provider")?;
                let forget = request
                    .params
                    .get("forget")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                value_result(engine.logout(&provider, forget))
            }
            "provider_health" => value_result(Ok(engine.provider_health().await)),
            "cancel" => {
                let conversation_id: String = required(&request.params, "conversationId")?;
                workspace_engine.cancel_run(&conversation_id);
                Ok(Value::Null)
            }
            "forget_conversation" => {
                let conversation_id: String = required(&request.params, "conversationId")?;
                engine.forget_conversation(&conversation_id);
                workspaces.forget_conversation(&conversation_id);
                Ok(Value::Null)
            }
            "context_file" => serialized(workspace_engine.context_file()),
            "save_context" => {
                let contents: String = required(&request.params, "contents")?;
                value_result(workspace_engine.save_context(&contents))
            }
            "archive_conversation" => {
                let id: String = required(&request.params, "id")?;
                let body: String = required(&request.params, "body")?;
                value_result(engine.archive_conversation(&id, &body))
            }
            "conversations_info" => serialized(engine.conversations_info()),
            "conversations_list" => serialized(engine.conversations_list()),
            "conversation_load" => {
                let id: String = required(&request.params, "id")?;
                value_result(engine.conversation_load(&id))
            }
            "run_log_recent" => {
                let limit = request
                    .params
                    .get("limit")
                    .and_then(Value::as_u64)
                    .and_then(|value| usize::try_from(value).ok())
                    .unwrap_or(50);
                serialized(engine.recent_run_log(limit))
            }
            "analysis_turn_load" => {
                let turn_id: String = required(&request.params, "turnId")?;
                value_result(engine.analysis_turn_load(&turn_id))
            }
            "artifact_create" => {
                let workspace: String = required(&request.params, "workspaceId")?;
                let turn_id: String = required(&request.params, "turnId")?;
                let title: String = required(&request.params, "title")?;
                let spec: Value = required(&request.params, "spec")?;
                value_result(crate::engine::artifacts::create_from_run(
                    engine.analytical_data_dir(),
                    &workspace,
                    &turn_id,
                    &title,
                    &spec,
                ))
            }
            "presentation_results" => {
                let workspace: String = required(&request.params, "workspaceId")?;
                let turn_id: String = required(&request.params, "turnId")?;
                engine.presentation_results(&workspace, &turn_id)
            }
            "artifact_list" => {
                let workspace: String = required(&request.params, "workspaceId")?;
                value_result(crate::engine::artifacts::list(
                    engine.analytical_data_dir(),
                    &workspace,
                ))
            }
            "artifact_list_for_reports" => value_result(
                crate::engine::artifacts::list_for_reports(engine.analytical_data_dir()),
            ),
            "artifact_resolve" => {
                let workspace: String = required(&request.params, "workspaceId")?;
                let artifact_id: String = required(&request.params, "artifactId")?;
                value_result(crate::engine::artifacts::resolve(
                    engine.analytical_data_dir(),
                    &workspace,
                    &artifact_id,
                    request.params.get("revision").and_then(Value::as_u64),
                ))
            }
            "artifact_resolve_saved" => {
                let artifact_id: String = required(&request.params, "artifactId")?;
                value_result(crate::engine::artifacts::resolve_saved(
                    engine.analytical_data_dir(),
                    &artifact_id,
                    request.params.get("revision").and_then(Value::as_u64),
                ))
            }
            "artifact_revise" => {
                let workspace: String = required(&request.params, "workspaceId")?;
                let artifact_id: String = required(&request.params, "artifactId")?;
                let base: u64 = required(&request.params, "baseRevision")?;
                let note: String = required(&request.params, "changeNote")?;
                let artifact = crate::engine::artifacts::revise(
                    engine.analytical_data_dir(),
                    &workspace,
                    &artifact_id,
                    base,
                    request.params.get("runId").and_then(Value::as_str),
                    request.params.get("spec").cloned(),
                    &note,
                )?;
                let revision = artifact.revisions.last().map(|revision| revision.revision);
                value_result(crate::engine::artifacts::resolve(
                    engine.analytical_data_dir(),
                    &workspace,
                    &artifact_id,
                    revision,
                ))
            }
            "artifact_adopt" => {
                let workspace: String = required(&request.params, "workspaceId")?;
                let artifact_id: String = required(&request.params, "artifactId")?;
                let base: u64 = required(&request.params, "baseRevision")?;
                let revision: u64 = required(&request.params, "revision")?;
                crate::engine::artifacts::adopt(
                    engine.analytical_data_dir(),
                    &workspace,
                    &artifact_id,
                    base,
                    revision,
                )?;
                value_result(crate::engine::artifacts::resolve(
                    engine.analytical_data_dir(),
                    &workspace,
                    &artifact_id,
                    Some(revision),
                ))
            }
            "artifact_compare" => {
                let workspace: String = required(&request.params, "workspaceId")?;
                let artifact_id: String = required(&request.params, "artifactId")?;
                let before_revision: u64 = required(&request.params, "beforeRevision")?;
                let after_revision: u64 = required(&request.params, "afterRevision")?;
                let before = crate::engine::artifacts::resolve(
                    engine.analytical_data_dir(),
                    &workspace,
                    &artifact_id,
                    Some(before_revision),
                )?;
                let after = crate::engine::artifacts::resolve(
                    engine.analytical_data_dir(),
                    &workspace,
                    &artifact_id,
                    Some(after_revision),
                )?;
                value_result(crate::engine::artifact_exchange::compare_with_dependencies(
                    engine.analytical_data_dir(),
                    &before,
                    &after,
                ))
            }
            "studio_list" => {
                let workspace: String = required(&request.params, "workspaceId")?;
                value_result(crate::engine::artifacts::studio_list(
                    engine.analytical_data_dir(),
                    &workspace,
                ))
            }
            "studio_create" => {
                let workspace: String = required(&request.params, "workspaceId")?;
                let title: String = required(&request.params, "title")?;
                value_result(crate::engine::artifacts::studio_create(
                    engine.analytical_data_dir(),
                    &workspace,
                    &title,
                ))
            }
            "studio_load" => {
                let workspace: String = required(&request.params, "workspaceId")?;
                let document_id: String = required(&request.params, "documentId")?;
                value_result(crate::engine::artifacts::studio_load(
                    engine.analytical_data_dir(),
                    &workspace,
                    &document_id,
                ))
            }
            "studio_save" => {
                let workspace: String = required(&request.params, "workspaceId")?;
                let document_id: String = required(&request.params, "documentId")?;
                let base: u64 = required(&request.params, "baseVersion")?;
                let update = required(&request.params, "update")?;
                value_result(crate::engine::artifacts::studio_save(
                    engine.analytical_data_dir(),
                    &workspace,
                    &document_id,
                    base,
                    update,
                ))
            }
            "studio_design_propose" => {
                let workspace: String = required(&request.params, "workspaceId")?;
                let document_id: String = required(&request.params, "documentId")?;
                let base: u64 = required(&request.params, "baseVersion")?;
                let target_placement_id = request
                    .params
                    .get("targetPlacementId")
                    .and_then(Value::as_str);
                let prompt: String = required(&request.params, "request")?;
                value_result(
                    engine
                        .studio_design_propose(
                            &workspace,
                            &document_id,
                            base,
                            target_placement_id,
                            &prompt,
                        )
                        .await,
                )
            }
            "studio_checkpoint" => {
                let workspace: String = required(&request.params, "workspaceId")?;
                let document_id: String = required(&request.params, "documentId")?;
                let base: u64 = required(&request.params, "baseVersion")?;
                let title: String = required(&request.params, "title")?;
                value_result(crate::engine::artifacts::studio_checkpoint(
                    engine.analytical_data_dir(),
                    &workspace,
                    &document_id,
                    base,
                    &title,
                ))
            }
            "studio_history" => {
                let workspace: String = required(&request.params, "workspaceId")?;
                let document_id: String = required(&request.params, "documentId")?;
                value_result(crate::engine::artifacts::studio_history(
                    engine.analytical_data_dir(),
                    &workspace,
                    &document_id,
                ))
            }
            "studio_restore_revision" => {
                let workspace: String = required(&request.params, "workspaceId")?;
                let document_id: String = required(&request.params, "documentId")?;
                let base: u64 = required(&request.params, "baseVersion")?;
                let revision: u64 = required(&request.params, "revisionVersion")?;
                value_result(crate::engine::artifacts::studio_restore_revision(
                    engine.analytical_data_dir(),
                    &workspace,
                    &document_id,
                    base,
                    revision,
                ))
            }
            "studio_restore" => {
                let workspace: String = required(&request.params, "workspaceId")?;
                let document_id: String = required(&request.params, "documentId")?;
                let base: u64 = required(&request.params, "baseVersion")?;
                let checkpoint_id: String = required(&request.params, "checkpointId")?;
                value_result(crate::engine::artifacts::studio_restore(
                    engine.analytical_data_dir(),
                    &workspace,
                    &document_id,
                    base,
                    &checkpoint_id,
                ))
            }
            "studio_export_package" => {
                let workspace: String = required(&request.params, "workspaceId")?;
                let document_id: String = required(&request.params, "documentId")?;
                value_result(crate::engine::artifact_exchange::export_studio(
                    engine.analytical_data_dir(),
                    &workspace,
                    &document_id,
                ))
            }
            "report_package_preview" => {
                let json: String = required(&request.params, "json")?;
                value_result(crate::engine::artifact_exchange::package_preview(&json))
            }
            "report_package_import" => {
                let json: String = required(&request.params, "json")?;
                value_result(crate::engine::artifact_exchange::package_import(
                    engine.analytical_data_dir(),
                    &json,
                ))
            }
            "report_package_load" => {
                let import_id: String = required(&request.params, "importId")?;
                value_result(crate::engine::artifact_exchange::package_load(
                    engine.analytical_data_dir(),
                    &import_id,
                ))
            }
            "report_package_list" => value_result(crate::engine::artifact_exchange::package_list(
                engine.analytical_data_dir(),
            )),
            "investigation_create" => {
                let workspace_id: String = required(&request.params, "workspaceId")?;
                let title = request
                    .params
                    .get("title")
                    .and_then(Value::as_str)
                    .unwrap_or("New investigation");
                let initial_request = request.params.get("initialRequest").and_then(Value::as_str);
                let source_message_id = request
                    .params
                    .get("sourceMessageId")
                    .and_then(Value::as_str);
                let conversation_id = request.params.get("conversationId").and_then(Value::as_str);
                value_result(engine.investigation_create(
                    &workspace_id,
                    title,
                    initial_request,
                    source_message_id,
                    conversation_id,
                ))
            }
            "investigation_mark_complete" => {
                let investigation_id: String = required(&request.params, "investigationId")?;
                value_result(engine.investigation_mark_complete(&investigation_id))
            }
            "investigation_reopen" => {
                let investigation_id: String = required(&request.params, "investigationId")?;
                value_result(engine.investigation_reopen(&investigation_id))
            }
            "investigation_ensure" => {
                let workspace_id: String = required(&request.params, "workspaceId")?;
                let conversation_id: String = required(&request.params, "conversationId")?;
                let initial_request: String = required(&request.params, "initialRequest")?;
                let source_message_id: String = required(&request.params, "sourceMessageId")?;
                value_result(engine.investigation_ensure(
                    &workspace_id,
                    &conversation_id,
                    &initial_request,
                    &source_message_id,
                ))
            }
            "investigation_rename" => {
                let investigation_id: String = required(&request.params, "investigationId")?;
                let title: String = required(&request.params, "title")?;
                value_result(engine.investigation_rename(&investigation_id, &title))
            }
            "investigation_update_brief" => {
                let investigation_id: String = required(&request.params, "investigationId")?;
                let brief = required(&request.params, "brief")?;
                let reviewed_message_id = request
                    .params
                    .get("reviewedMessageId")
                    .and_then(Value::as_str);
                value_result(engine.investigation_update_brief(
                    &investigation_id,
                    brief,
                    reviewed_message_id,
                ))
            }
            "investigation_update_questions" => {
                let investigation_id: String = required(&request.params, "investigationId")?;
                let questions: Vec<InvestigationQuestionInput> =
                    required(&request.params, "questions")?;
                value_result(engine.investigation_update_questions(&investigation_id, questions))
            }
            "investigation_update_definitions" => {
                let investigation_id: String = required(&request.params, "investigationId")?;
                let definitions: Vec<InvestigationDefinitionInput> =
                    required(&request.params, "definitions")?;
                value_result(
                    engine.investigation_update_definitions(&investigation_id, definitions),
                )
            }
            "investigation_update_source_selection" => {
                let investigation_id: String = required(&request.params, "investigationId")?;
                let source_selection: InvestigationSourceSelection =
                    required(&request.params, "sourceSelection")?;
                value_result(
                    engine
                        .investigation_update_source_selection(&investigation_id, source_selection),
                )
            }
            "investigation_question_load" => {
                let question_id: String = required(&request.params, "questionId")?;
                value_result(engine.investigation_question_load(&question_id))
            }
            "investigation_definition_load" => {
                let definition_id: String = required(&request.params, "definitionId")?;
                value_result(engine.investigation_definition_load(&definition_id))
            }
            "investigation_create_finding" => {
                let investigation_id: String = required(&request.params, "investigationId")?;
                let finding: InvestigationFindingInput = required(&request.params, "finding")?;
                value_result(engine.investigation_create_finding(&investigation_id, finding))
            }
            "investigation_update_finding_claim" => {
                let investigation_id: String = required(&request.params, "investigationId")?;
                let finding_id: String = required(&request.params, "findingId")?;
                let claim: String = required(&request.params, "claim")?;
                value_result(engine.investigation_update_finding_claim(
                    &investigation_id,
                    &finding_id,
                    &claim,
                ))
            }
            "investigation_update_finding" => {
                let investigation_id: String = required(&request.params, "investigationId")?;
                let finding_id: String = required(&request.params, "findingId")?;
                let update: InvestigationFindingUpdate = required(&request.params, "update")?;
                value_result(engine.investigation_update_finding(
                    &investigation_id,
                    &finding_id,
                    update,
                ))
            }
            "investigation_finding_load" => {
                let finding_id: String = required(&request.params, "findingId")?;
                value_result(engine.investigation_finding_load(&finding_id))
            }
            "investigation_propose_questions" => {
                let investigation_id: String = required(&request.params, "investigationId")?;
                let main_question: String = required(&request.params, "mainQuestion")?;
                value_result(
                    engine
                        .investigation_propose_questions(&investigation_id, &main_question)
                        .await,
                )
            }
            "investigation_link_analysis_turns" => {
                let investigation_id: String = required(&request.params, "investigationId")?;
                let analysis_turn_ids: Vec<String> = required(&request.params, "analysisTurnIds")?;
                value_result(
                    engine.investigation_link_analysis_turns(&investigation_id, &analysis_turn_ids),
                )
            }
            "investigation_set_analysis_turn_questions" => {
                let investigation_id: String = required(&request.params, "investigationId")?;
                let analysis_turn_id: String = required(&request.params, "analysisTurnId")?;
                let question_ids: Vec<String> = required(&request.params, "questionIds")?;
                value_result(engine.investigation_set_analysis_turn_questions(
                    &investigation_id,
                    &analysis_turn_id,
                    &question_ids,
                ))
            }
            "investigation_load" => {
                let investigation_id: String = required(&request.params, "investigationId")?;
                value_result(engine.investigation_load(&investigation_id))
            }
            "investigations_list" => {
                let active_conversation_ids = request
                    .params
                    .get("activeConversationIds")
                    .map(|value| {
                        serde_json::from_value::<Vec<String>>(value.clone())
                            .map_err(EngineError::from)
                    })
                    .transpose()?;
                value_result(engine.investigations_list(active_conversation_ids.as_deref()))
            }
            "analysis_turn_replay_status" => {
                let turn_id: String = required(&request.params, "turnId")?;
                value_result(workspace_engine.analysis_turn_replay_status(&turn_id))
            }
            "analysis_turn_rerun" => {
                let turn_id: String = required(&request.params, "turnId")?;
                let model = request
                    .params
                    .get("model")
                    .and_then(Value::as_str)
                    .map(str::to_owned);
                let inspect = request.params.get("mode").and_then(Value::as_str) == Some("inspect");
                let events = output.clone();
                let answer = workspace_engine
                    .analysis_turn_rerun(&turn_id, model.as_deref(), inspect, move |item| {
                        event(&events, id, item)
                    })
                    .await;
                value_result(answer)
            }
            "delete_conversation" => {
                let id: String = required(&request.params, "id")?;
                engine.forget_conversation(&id);
                workspaces.forget_conversation(&id);
                value_result(engine.delete_conversation(&id))
            }
            "rename_conversation" => {
                let id: String = required(&request.params, "id")?;
                let title: String = required(&request.params, "title")?;
                value_result(engine.rename_conversation(&id, &title))
            }
            "ask" => {
                let conversation_id: String = required(&request.params, "conversationId")?;
                let question: String = required(&request.params, "question")?;
                let model = request
                    .params
                    .get("model")
                    .and_then(Value::as_str)
                    .map(str::to_owned);
                let inspect = request.params.get("mode").and_then(Value::as_str) == Some("inspect");
                let context_refs = request
                    .params
                    .get("contextRefs")
                    .cloned()
                    .map(serde_json::from_value::<Vec<ContextReference>>)
                    .transpose()?
                    .unwrap_or_default();
                let clarification_reply = request
                    .params
                    .get("clarificationReply")
                    .cloned()
                    .filter(|value| !value.is_null())
                    .map(serde_json::from_value::<ClarificationReply>)
                    .transpose()?;
                let investigation_id = request
                    .params
                    .get("investigationId")
                    .and_then(Value::as_str)
                    .map(str::to_owned);
                let question_id = request
                    .params
                    .get("questionId")
                    .and_then(Value::as_str)
                    .map(str::to_owned);
                let events = output.clone();
                let answer = workspace_engine
                    .ask_with_mode_and_context_and_clarification(
                        &conversation_id,
                        &question,
                        AskOptions {
                            model: model.as_deref(),
                            inspect,
                            context_refs: &context_refs,
                            clarification_reply,
                            investigation_id: investigation_id.as_deref(),
                            question_id: question_id.as_deref(),
                            investigation_context: None,
                            rerun_of: None,
                        },
                        move |item| event(&events, id, item),
                    )
                    .await;
                value_result(answer)
            }
            method => Err(EngineError::msg(format!("unknown IPC command: {method}"))),
        }
    }
    .await;
    respond(&output, id, result);
    Ok(())
}

/// Run the engine as a long-lived JSON-lines child process for Electron.
pub fn run(data_dir: &Path) -> Result<(), String> {
    let engine = Arc::new(EngineState::new(data_dir).map_err(|error| error.to_string())?);
    let workspaces = Arc::new(WorkspaceRegistry::new(
        data_dir.to_path_buf(),
        engine.cancel_registry(),
    ));
    let output = Arc::new(Mutex::new(BufWriter::new(io::stdout())));
    let started = Instant::now();

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .worker_threads(2)
        .build()
        .map_err(|error| format!("create engine runtime: {error}"))?;

    runtime.block_on(async move {
        let (requests, mut incoming) = tokio::sync::mpsc::unbounded_channel::<String>();
        std::thread::spawn(move || {
            let stdin = io::stdin();
            for line in stdin.lock().lines() {
                match line {
                    Ok(line) => {
                        if requests.send(line).is_err() {
                            break;
                        }
                    }
                    Err(error) => {
                        eprintln!("fella engine stdin: {error}");
                        break;
                    }
                }
            }
        });

        let mut tasks = Vec::new();
        while let Some(line) = incoming.recv().await {
            let request = match serde_json::from_str::<Request>(&line) {
                Ok(request) => request,
                Err(error) => {
                    eprintln!("fella engine request: {error}");
                    continue;
                }
            };
            tasks.push(tokio::spawn(dispatch(
                request,
                Arc::clone(&engine),
                Arc::clone(&workspaces),
                Arc::clone(&output),
                started,
            )));
        }

        for task in tasks {
            let _ = task.await;
        }
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root() -> PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("fella-workspace-registry-{nonce}"))
    }

    fn write_workspace(root: &Path, amounts: &[u64]) {
        std::fs::create_dir_all(root).unwrap();
        let mut csv = String::from("amount\n");
        for amount in amounts {
            csv.push_str(&format!("{amount}\n"));
        }
        std::fs::write(root.join("facts.csv"), csv).unwrap();
    }

    #[test]
    fn registry_keeps_catalogs_independent_and_enforces_four_open_workspaces() {
        let root = temp_root();
        let data_dir = root.join("app-data");
        let paths: Vec<_> = (0..5)
            .map(|index| root.join(format!("repo-{index}")))
            .collect();
        write_workspace(&paths[0], &[3, 7]);
        write_workspace(&paths[1], &[100, 200, 300]);
        for path in &paths[2..] {
            write_workspace(path, &[1]);
        }

        let app_engine = EngineState::new(&data_dir).unwrap();
        let registry = WorkspaceRegistry::new(data_dir.clone(), app_engine.cancel_registry());
        let first = registry
            .open_workspace_with_progress(&paths[0], |_| {})
            .unwrap();
        let second = registry
            .open_workspace_with_progress(&paths[1], |_| {})
            .unwrap();
        let first_id = first.workspace.clone().unwrap();
        let second_id = second.workspace.clone().unwrap();
        assert_ne!(first_id, second_id);

        let first_view = first.sources[0].view.as_deref().unwrap();
        let second_view = second.sources[0].view.as_deref().unwrap();
        let first_engine = registry.engine(&first_id).unwrap();
        let second_engine = registry.engine(&second_id).unwrap();
        assert_eq!(
            first_engine
                .run_sql(&format!("SELECT SUM(amount) FROM \"{first_view}\""))
                .unwrap()
                .rows[0][0],
            serde_json::json!(10)
        );
        assert_eq!(
            second_engine
                .run_sql(&format!("SELECT SUM(amount) FROM \"{second_view}\""))
                .unwrap()
                .rows[0][0],
            serde_json::json!(600)
        );
        assert_eq!(
            first_engine.catalog().workspace.as_deref(),
            Some(first_id.as_str())
        );
        assert_eq!(
            second_engine.catalog().workspace.as_deref(),
            Some(second_id.as_str())
        );

        // Reopening a path focuses its existing runtime instead of consuming a
        // second slot or replacing another workspace's catalog.
        let reopened = registry
            .open_workspace_with_progress(&paths[0], |_| {})
            .unwrap();
        assert_eq!(reopened.workspace.as_deref(), Some(first_id.as_str()));
        assert_eq!(registry.len(), 2);

        registry
            .open_workspace_with_progress(&paths[2], |_| {})
            .unwrap();
        registry
            .open_workspace_with_progress(&paths[3], |_| {})
            .unwrap();
        let capacity_error = registry
            .open_workspace_with_progress(&paths[4], |_| {})
            .unwrap_err();
        assert!(capacity_error.to_string().contains("up to 4"));
        assert_eq!(registry.len(), 4);

        assert!(registry.close_workspace(&second_id));
        registry
            .open_workspace_with_progress(&paths[4], |_| {})
            .unwrap();
        assert_eq!(registry.len(), 4);
        assert!(registry.engine(&second_id).is_err());

        drop(first_engine);
        drop(second_engine);
        drop(registry);
        let _ = std::fs::remove_dir_all(root);
    }
}
