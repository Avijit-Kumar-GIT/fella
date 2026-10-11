//! Local persistence for backend-owned analytical turns.
//!
//! Conversation archives are a UI transcript projection. This store keeps the
//! runtime record separately so a turn can be inspected without reconstructing
//! it from streamed messages, and so future replay/rerun code has a stable
//! object to load.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::UNIX_EPOCH;

use crate::engine::error::{EngineError, EngineResult};
use crate::engine::runtime::{
    AnalysisTurn, RunLogEntry, RunLogKind, RunLogModelCall, RunLogOperation, TurnState,
};

const MAX_RECENT_TURNS: usize = 100;
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn turns_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("analysis").join("turns")
}

fn turn_path(data_dir: &Path, turn_id: &str) -> EngineResult<PathBuf> {
    if turn_id.is_empty()
        || !turn_id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
    {
        return Err(EngineError::msg("invalid analysis turn id"));
    }
    Ok(turns_dir(data_dir).join(format!("{turn_id}.json")))
}

/// Create one canonical run record without replacing an earlier attempt.
/// The temporary file is linked into place only if the destination is absent,
/// so duplicate IDs fail atomically instead of silently erasing history.
pub fn create(data_dir: &Path, turn: &AnalysisTurn) -> EngineResult<String> {
    let path = turn_path(data_dir, &turn.id)?;
    let parent = path
        .parent()
        .ok_or_else(|| EngineError::msg("analysis turn path has no parent directory"))?;
    std::fs::create_dir_all(parent)
        .map_err(|error| EngineError::io(format!("create {}", parent.display()), error))?;
    let body = serde_json::to_vec_pretty(turn).map_err(EngineError::from)?;
    let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let temporary = parent.join(format!(
        ".{}.{}.{sequence}.tmp",
        turn.id,
        std::process::id()
    ));
    let write_result: EngineResult<()> = (|| {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| EngineError::io(format!("create {}", temporary.display()), error))?;
        file.write_all(&body)
            .map_err(|error| EngineError::io(format!("write {}", temporary.display()), error))?;
        file.sync_all()
            .map_err(|error| EngineError::io(format!("sync {}", temporary.display()), error))?;
        drop(file);
        std::fs::hard_link(&temporary, &path).map_err(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                EngineError::msg(format!("analysis turn {} already exists", turn.id))
            } else {
                EngineError::io(format!("create {}", path.display()), error)
            }
        })?;
        Ok(())
    })();
    let cleanup_result = std::fs::remove_file(&temporary);
    if let Err(error) = cleanup_result {
        if write_result.is_ok() {
            log::warn!(
                "could not remove temporary analysis record {}: {error}",
                temporary.display()
            );
        }
    }
    write_result?;
    Ok(path.display().to_string())
}

pub fn load(data_dir: &Path, turn_id: &str) -> EngineResult<AnalysisTurn> {
    let path = turn_path(data_dir, turn_id)?;
    let body = std::fs::read_to_string(&path)
        .map_err(|error| EngineError::io(format!("read {}", path.display()), error))?;
    serde_json::from_str(&body).map_err(|error| {
        EngineError::msg(format!(
            "analysis turn {} is not valid JSON: {error}",
            turn_id
        ))
    })
}

pub fn exists(data_dir: &Path, turn_id: &str) -> bool {
    turn_path(data_dir, turn_id).is_ok_and(|path| path.is_file())
}

/// Remove every persisted analysis record belonging to a deleted conversation.
/// These records contain the question, answer, and evidence independently of
/// the user-visible transcript archive, so deleting only that archive is not a
/// complete deletion.
pub fn delete_conversation(data_dir: &Path, conversation_id: &str) -> EngineResult<usize> {
    let directory = turns_dir(data_dir);
    let entries = match std::fs::read_dir(&directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(error) => {
            return Err(EngineError::io(
                format!("read {}", directory.display()),
                error,
            ));
        }
    };

    let mut matches = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| {
            EngineError::io(format!("read entries in {}", directory.display()), error)
        })?;
        let file_type = entry.file_type().map_err(|error| {
            EngineError::io(format!("inspect {}", entry.path().display()), error)
        })?;
        if !file_type.is_file()
            || entry.path().extension().and_then(|ext| ext.to_str()) != Some("json")
        {
            continue;
        }
        let path = entry.path();
        let text = std::fs::read_to_string(&path)
            .map_err(|error| EngineError::io(format!("read {}", path.display()), error))?;
        let record: serde_json::Value = serde_json::from_str(&text).map_err(|error| {
            EngineError::msg(format!(
                "inspect analysis record {}: {error}",
                path.display()
            ))
        })?;
        if record
            .get("conversation_id")
            .and_then(serde_json::Value::as_str)
            == Some(conversation_id)
        {
            matches.push(path);
        }
    }

    let count = matches.len();
    for path in matches {
        std::fs::remove_file(&path)
            .map_err(|error| EngineError::io(format!("remove {}", path.display()), error))?;
    }
    Ok(count)
}

/// Read a bounded recent slice and project it to the content-minimized form
/// used by Settings. This is derived from canonical turn records; no second
/// per-run history is written.
pub fn recent(data_dir: &Path, limit: usize) -> Vec<RunLogEntry> {
    let Ok(entries) = std::fs::read_dir(turns_dir(data_dir)) else {
        return Vec::new();
    };
    let mut files: Vec<(u64, PathBuf)> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                return None;
            }
            let metadata = entry.metadata().ok()?;
            if !metadata.is_file() {
                return None;
            }
            let at_ms = metadata
                .modified()
                .ok()?
                .duration_since(UNIX_EPOCH)
                .ok()?
                .as_millis()
                .min(u64::MAX as u128) as u64;
            Some((at_ms, path))
        })
        .collect();
    files.sort_by(|(a_time, a_path), (b_time, b_path)| {
        b_time.cmp(a_time).then_with(|| b_path.cmp(a_path))
    });
    files.truncate(limit.clamp(1, MAX_RECENT_TURNS));

    files
        .into_iter()
        .filter_map(|(at_ms, path)| {
            let body = std::fs::read(path).ok()?;
            let turn: AnalysisTurn = serde_json::from_slice(&body).ok()?;
            Some(project_turn(turn, at_ms))
        })
        .collect()
}

fn project_turn(turn: AnalysisTurn, at_ms: u64) -> RunLogEntry {
    let model_calls = turn
        .trace
        .model_calls
        .into_iter()
        .map(|call| RunLogModelCall {
            model: call.model,
            duration_ms: call.duration_ms,
            success: call.success,
            prompt_tokens: call.prompt_tokens,
            completion_tokens: call.completion_tokens,
        })
        .collect();
    let operations = turn
        .trace
        .steps
        .into_iter()
        .map(|step| RunLogOperation {
            operation: step.operation,
            duration_ms: step.duration_ms,
            success: step.success,
        })
        .collect();
    RunLogEntry {
        id: turn.id,
        at_ms,
        kind: RunLogKind::Turn,
        mode: turn.trace.mode,
        model: turn.trace.model,
        elapsed_ms: turn.trace.elapsed_ms,
        model_calls,
        operations,
        prior_analysis_count: turn.prior_turn_refs.len(),
        context_reference_count: turn.context_refs.len(),
        clarification_continuation: turn.clarification_of.is_some(),
        rerun: turn.rerun_of.is_some(),
        outcome: match turn.state {
            TurnState::Accepted => "completed",
            TurnState::Clarify => "clarification_requested",
            TurnState::NeedsReview => "returned_with_limits",
            TurnState::Failed => "failed",
            TurnState::Unsupported => "unsupported",
            TurnState::Cancelled => "cancelled",
            TurnState::Received => "received",
            TurnState::Interpreting => "interpreting",
            TurnState::Grounding => "grounding",
            TurnState::Planning => "planning",
            TurnState::Executing => "executing",
            TurnState::Verifying => "verifying",
            TurnState::Retry => "retry",
        }
        .into(),
        trigger: None,
        trigger_steps: None,
        trigger_errors: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::context::ContextSection;
    use crate::engine::evidence::{AnswerProvenance, VerificationCheck, VerificationStatus};
    use crate::engine::investigation::{InvestigationContextSnapshot, InvestigationQuestionInput};
    use crate::engine::runtime::{
        AnalysisResult, ContextReference, ExecutionTrace, InteractionMode, LogicalPlan,
        ModelCallTrace, PlanStrategy, TraceStep, TurnState, VerificationReport,
    };
    use crate::engine::state::EngineState;

    fn turn() -> AnalysisTurn {
        AnalysisTurn {
            id: "turn-test-1".into(),
            conversation_id: "conversation-1".into(),
            question: "total sales?".into(),
            context_refs: Vec::new(),
            prior_turn_refs: Vec::new(),
            clarification_of: None,
            clarification_response: None,
            workspace: Some("/tmp/workspace".into()),
            workspace_revision: Some("revision-1".into()),
            workspace_snapshot: None,
            source_scope: None,
            investigation_context: None,
            context_audit: None,
            provenance: Some(AnswerProvenance {
                evidence_ids: vec!["evidence-1".into()],
                context_sections: vec![ContextSection::WorkspaceSchema],
                clarification_of: None,
            }),
            rerun_of: None,
            state: TurnState::Accepted,
            contract: None,
            plan: LogicalPlan {
                strategy: PlanStrategy::DirectTools,
                steps: vec!["run_sql".into()],
            },
            trace: ExecutionTrace {
                id: "trace-test-1".into(),
                turn_id: "turn-test-1".into(),
                workspace_revision: Some("revision-1".into()),
                mode: None,
                model: None,
                model_calls: Vec::new(),
                elapsed_ms: None,
                steps: vec![TraceStep {
                    id: "evidence-1".into(),
                    operation: "run_sql".into(),
                    duration_ms: 4,
                    success: true,
                    summary: Some("one result".into()),
                    sources: vec!["sales.csv".into()],
                }],
            },
            verification: Some(VerificationReport {
                status: VerificationStatus::Verified,
                checks: vec![VerificationCheck {
                    label: "query reruns".into(),
                    ok: true,
                    outcome: None,
                    detail: None,
                    finding: None,
                }],
            }),
            result: AnalysisResult {
                text: "100".into(),
                status: VerificationStatus::Verified,
                usage: None,
                verification: Vec::new(),
                evidence: vec![serde_json::json!({"tool": "run_sql"})],
            },
        }
    }

    #[test]
    fn creates_and_loads_a_typed_turn_record() {
        let dir = std::env::temp_dir().join(format!("fella-analysis-store-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = create(&dir, &turn()).unwrap();
        assert!(path.ends_with("turn-test-1.json"));
        assert!(exists(&dir, "turn-test-1"));
        assert!(!exists(&dir, "missing-turn"));
        assert!(!exists(&dir, "../turn-test-1"));
        let loaded = load(&dir, "turn-test-1").unwrap();
        assert_eq!(loaded.question, "total sales?");
        assert_eq!(loaded.trace.steps[0].operation, "run_sql");
        assert_eq!(loaded.result.status, VerificationStatus::Verified);
        assert_eq!(
            loaded.provenance.as_ref().unwrap().evidence_ids,
            vec!["evidence-1"]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn creating_a_duplicate_turn_id_does_not_replace_the_original_attempt() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("fella-analysis-immutable-{nonce}"));
        let original = turn();
        create(&dir, &original).unwrap();

        let mut conflicting = original.clone();
        conflicting.question = "a different question".into();
        conflicting.result.text = "a replacement answer".into();
        let error = create(&dir, &conflicting).unwrap_err();

        assert!(error.to_string().contains("already exists"));
        let loaded = load(&dir, &original.id).unwrap();
        assert_eq!(loaded.question, original.question);
        assert_eq!(loaded.result.text, original.result.text);
        assert_eq!(
            loaded.trace.steps[0].operation,
            original.trace.steps[0].operation
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn distinct_attempts_keep_their_own_context_method_and_outputs() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("fella-analysis-attempts-{nonce}"));
        let mut first = turn();
        first.source_scope = Some(serde_json::from_value(serde_json::json!({
            "used_sources": [{ "source": "sales.csv", "table": "sales", "evidence_ids": ["evidence-1"] }],
            "skipped_sources": []
        })).unwrap());
        first.investigation_context = Some(serde_json::from_value(serde_json::json!({
            "investigation_id": "investigation-1",
            "question_id": "question-1",
            "question_text": "Compare sales",
            "definitions": [{ "id": "definition-1", "revision": 1, "kind": "definition", "text": "Sales are net of refunds." }],
            "source_selection": { "mode": "selected_sources", "included_paths": ["sales.csv"] }
        })).unwrap());
        create(&dir, &first).unwrap();

        let mut second = first.clone();
        second.id = "turn-test-2".into();
        second.trace.id = "trace-test-2".into();
        second.trace.turn_id = second.id.clone();
        second.question = "Compare sales by month".into();
        second.rerun_of = Some(first.id.clone());
        second.plan.strategy = PlanStrategy::CompiledSql;
        second.source_scope.as_mut().unwrap().used_sources[0].source = "monthly-sales.csv".into();
        second.investigation_context.as_mut().unwrap().definitions[0].revision = 2;
        second.investigation_context.as_mut().unwrap().definitions[0].text =
            "Sales are gross of refunds.".into();
        second.result.text = "The monthly total is 120.".into();
        create(&dir, &second).unwrap();

        let loaded_first = load(&dir, &first.id).unwrap();
        let loaded_second = load(&dir, &second.id).unwrap();
        assert_eq!(
            loaded_first.source_scope.as_ref().unwrap().used_sources[0].source,
            "sales.csv"
        );
        assert_eq!(
            loaded_first
                .investigation_context
                .as_ref()
                .unwrap()
                .definitions[0]
                .revision,
            1
        );
        assert_eq!(loaded_first.plan.strategy, PlanStrategy::DirectTools);
        assert_eq!(loaded_first.result.text, "100");
        assert_eq!(loaded_second.rerun_of.as_deref(), Some(first.id.as_str()));
        assert_eq!(
            loaded_second.source_scope.as_ref().unwrap().used_sources[0].source,
            "monthly-sales.csv"
        );
        assert_eq!(
            loaded_second
                .investigation_context
                .as_ref()
                .unwrap()
                .definitions[0]
                .revision,
            2
        );
        assert_eq!(loaded_second.plan.strategy, PlanStrategy::CompiledSql);
        assert_eq!(loaded_second.result.text, "The monthly total is 120.");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn old_turn_without_provenance_still_deserializes() {
        let mut json = serde_json::to_value(turn()).unwrap();
        json.as_object_mut().unwrap().remove("provenance");
        let loaded: AnalysisTurn = serde_json::from_value(json).unwrap();
        assert!(loaded.provenance.is_none());
    }

    #[test]
    fn investigations_link_existing_turns_from_their_own_workspace_only() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("fella-investigation-runs-{nonce}"));
        let engine = EngineState::new(&dir).unwrap();
        let canonical_turn = turn();
        create(&dir, &canonical_turn).unwrap();

        let mut foreign_turn = turn();
        foreign_turn.id = "turn-foreign".into();
        foreign_turn.trace.turn_id = foreign_turn.id.clone();
        foreign_turn.workspace = Some("/tmp/another-workspace".into());
        create(&dir, &foreign_turn).unwrap();

        let investigation = engine
            .investigation_create(
                "/tmp/workspace",
                "Revenue",
                Some("What were total sales?"),
                Some("message-1"),
                None,
            )
            .unwrap();
        let mixed_ids = ["turn-test-1", "turn-foreign"].map(str::to_owned);
        let error = engine
            .investigation_link_analysis_turns(&investigation.id, &mixed_ids)
            .unwrap_err();
        assert!(error.to_string().contains("different workspace"));
        assert!(engine
            .investigation_load(&investigation.id)
            .unwrap()
            .analysis_turn_ids
            .is_empty());

        assert!(engine
            .investigation_link_analysis_turns(&investigation.id, &["missing-turn".to_string()])
            .is_err());
        let linked = engine
            .investigation_link_analysis_turns(
                &investigation.id,
                &["turn-test-1".to_string(), "turn-test-1".to_string()],
            )
            .unwrap();
        assert_eq!(linked.analysis_turn_ids, vec!["turn-test-1".to_string()]);
        assert_eq!(
            load(&dir, "turn-test-1").unwrap().result.text,
            canonical_turn.result.text,
            "linking stores only the ID and leaves the canonical run unchanged"
        );
        let linked_again = engine
            .investigation_link_analysis_turns(&investigation.id, &["turn-test-1".to_string()])
            .unwrap();
        assert_eq!(linked_again, linked);

        drop(engine);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn saved_runs_link_to_multiple_questions_without_changing_the_canonical_turn() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("fella-investigation-question-runs-{nonce}"));
        let engine = EngineState::new(&dir).unwrap();
        let investigation = engine
            .investigation_create(
                "/tmp/workspace",
                "Refund analysis",
                Some("Explain refund rates"),
                Some("message-refunds"),
                Some("conversation-1"),
            )
            .unwrap();
        let root = investigation.questions.as_ref().unwrap()[0].clone();
        let investigation = engine
            .investigation_update_questions(
                &investigation.id,
                vec![
                    InvestigationQuestionInput {
                        id: root.id.clone(),
                        parent_id: None,
                        text: root.text.clone(),
                        source_selection: None,
                    },
                    InvestigationQuestionInput {
                        id: "question-channel".into(),
                        parent_id: Some(root.id.clone()),
                        text: "Compare refunds by channel".into(),
                        source_selection: None,
                    },
                ],
            )
            .unwrap();
        let mut canonical_turn = turn();
        canonical_turn.investigation_context = Some(InvestigationContextSnapshot {
            investigation_id: investigation.id.clone(),
            question_id: Some(root.id.clone()),
            question_text: Some(root.text),
            definitions: Vec::new(),
            source_selection: Default::default(),
        });
        create(&dir, &canonical_turn).unwrap();

        let linked = engine
            .investigation_set_analysis_turn_questions(
                &investigation.id,
                &canonical_turn.id,
                &[root.id.clone(), "question-channel".into()],
            )
            .unwrap();
        assert_eq!(linked.analysis_turn_ids, vec![canonical_turn.id.clone()]);
        for question in linked.questions.as_ref().unwrap() {
            assert_eq!(question.analysis_turn_ids, vec![canonical_turn.id.clone()]);
        }
        let persisted_turn = load(&dir, &canonical_turn.id).unwrap();
        assert_eq!(persisted_turn.question, canonical_turn.question);
        assert_eq!(
            persisted_turn.investigation_context.unwrap().question_id,
            Some(root.id.clone())
        );

        crate::engine::investigation::question_run_started(
            &dir,
            &investigation.id,
            &root.id,
            &canonical_turn.id,
        )
        .unwrap();
        let unlinked = engine
            .investigation_set_analysis_turn_questions(&investigation.id, &canonical_turn.id, &[])
            .unwrap();
        let root_after = unlinked
            .questions
            .as_ref()
            .unwrap()
            .iter()
            .find(|q| q.id == root.id)
            .unwrap();
        let child_after = unlinked
            .questions
            .as_ref()
            .unwrap()
            .iter()
            .find(|q| q.id == "question-channel")
            .unwrap();
        assert_eq!(
            root_after.analysis_turn_ids,
            vec![canonical_turn.id.clone()]
        );
        assert!(child_after.analysis_turn_ids.is_empty());
        assert_eq!(unlinked.analysis_turn_ids, vec![canonical_turn.id.clone()]);

        let mut foreign_turn = turn();
        foreign_turn.id = "turn-foreign-question-link".into();
        foreign_turn.trace.turn_id = foreign_turn.id.clone();
        foreign_turn.workspace = Some("/tmp/another-workspace".into());
        create(&dir, &foreign_turn).unwrap();
        assert!(engine
            .investigation_set_analysis_turn_questions(
                &investigation.id,
                &foreign_turn.id,
                &["question-channel".into()],
            )
            .unwrap_err()
            .to_string()
            .contains("different workspace"));
        assert!(engine
            .investigation_set_analysis_turn_questions(
                &investigation.id,
                &canonical_turn.id,
                &["not-an-active-question".into()],
            )
            .is_err());

        drop(engine);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn recent_projection_exposes_run_metrics_without_transcript_or_workspace_content() {
        let dir =
            std::env::temp_dir().join(format!("fella-analysis-run-log-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut private_turn = turn();
        private_turn.result.text = "private-answer-marker".into();
        private_turn.trace.mode = Some(InteractionMode::WorkspaceAsk);
        private_turn.trace.model = Some("test-provider/test-model".into());
        private_turn.trace.elapsed_ms = Some(1280);
        private_turn.trace.model_calls = vec![ModelCallTrace {
            model: "test-provider/test-model".into(),
            duration_ms: 900,
            success: true,
            prompt_tokens: Some(125),
            completion_tokens: Some(15),
        }];
        private_turn.prior_turn_refs = vec!["private-prior-turn-id".into()];
        private_turn.context_refs = vec![ContextReference {
            kind: "source".into(),
            key: "private-source-key".into(),
            label: "private source label".into(),
            detail: None,
        }];
        private_turn.clarification_of = Some("private-parent-turn-id".into());
        private_turn.rerun_of = Some("private-rerun-parent-id".into());
        create(&dir, &private_turn).unwrap();

        let entries = recent(&dir, 50);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, "turn-test-1");
        assert_eq!(entries[0].outcome, "completed");
        assert_eq!(entries[0].operations[0].operation, "run_sql");
        assert_eq!(entries[0].mode, Some(InteractionMode::WorkspaceAsk));
        assert_eq!(
            entries[0].model.as_deref(),
            Some("test-provider/test-model")
        );
        assert_eq!(entries[0].elapsed_ms, Some(1280));
        assert_eq!(entries[0].model_calls[0].prompt_tokens, Some(125));
        assert_eq!(entries[0].model_calls[0].completion_tokens, Some(15));
        assert_eq!(entries[0].prior_analysis_count, 1);
        assert_eq!(entries[0].context_reference_count, 1);
        assert!(entries[0].clarification_continuation);
        assert!(entries[0].rerun);

        let serialized = serde_json::to_string(&entries[0]).unwrap();
        for private_content in [
            "total sales?",
            "/tmp/workspace",
            "one result",
            "sales.csv",
            "evidence-1",
            "private-answer-marker",
            "private-prior-turn-id",
            "private-source-key",
            "private source label",
            "private-parent-turn-id",
            "private-rerun-parent-id",
        ] {
            assert!(!serialized.contains(private_content));
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn turn_ids_cannot_escape_the_turn_store() {
        let dir = std::env::temp_dir().join(format!(
            "fella-analysis-store-invalid-{}",
            std::process::id()
        ));
        assert!(load(&dir, "../secrets").is_err());
        assert!(load(&dir, "turn/other").is_err());
    }
}
