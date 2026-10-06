//! Local persistence for backend-owned analytical turns.
//!
//! Conversation archives are a UI transcript projection. This store keeps the
//! runtime record separately so a turn can be inspected without reconstructing
//! it from streamed messages, and so future replay/rerun code has a stable
//! object to load.

use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use crate::engine::error::{EngineError, EngineResult};
use crate::engine::runtime::{
    AnalysisTurn, RunLogEntry, RunLogKind, RunLogModelCall, RunLogOperation, TurnState,
};

const MAX_RECENT_TURNS: usize = 100;

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

pub fn save(data_dir: &Path, turn: &AnalysisTurn) -> EngineResult<String> {
    let path = turn_path(data_dir, &turn.id)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| EngineError::io(format!("create {}", parent.display()), error))?;
    }
    let body = serde_json::to_string_pretty(turn).map_err(EngineError::from)?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, body)
        .map_err(|error| EngineError::io(format!("write {}", tmp.display()), error))?;
    std::fs::rename(&tmp, &path)
        .map_err(|error| EngineError::io(format!("replace {}", path.display()), error))?;
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
    use crate::engine::runtime::{
        AnalysisResult, ContextReference, ExecutionTrace, InteractionMode, LogicalPlan,
        ModelCallTrace, PlanStrategy, TraceStep, TurnState, VerificationReport,
    };

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
    fn saves_and_loads_a_typed_turn_record() {
        let dir = std::env::temp_dir().join(format!("fella-analysis-store-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = save(&dir, &turn()).unwrap();
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
    fn old_turn_without_provenance_still_deserializes() {
        let mut json = serde_json::to_value(turn()).unwrap();
        json.as_object_mut().unwrap().remove("provenance");
        let loaded: AnalysisTurn = serde_json::from_value(json).unwrap();
        assert!(loaded.provenance.is_none());
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
        save(&dir, &private_turn).unwrap();

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
