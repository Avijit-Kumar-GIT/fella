//! Local persistence for backend-owned analytical turns.
//!
//! Conversation archives are a UI transcript projection. This store keeps the
//! runtime record separately so a turn can be inspected without reconstructing
//! it from streamed messages, and so future replay/rerun code has a stable
//! object to load.

use std::path::{Path, PathBuf};

use crate::engine::error::{EngineError, EngineResult};
use crate::engine::runtime::AnalysisTurn;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::evidence::{VerificationCheck, VerificationStatus};
    use crate::engine::runtime::{
        AnalysisResult, ExecutionTrace, LogicalPlan, PlanStrategy, TraceStep, TurnState,
        VerificationReport,
    };

    fn turn() -> AnalysisTurn {
        AnalysisTurn {
            id: "turn-test-1".into(),
            conversation_id: "conversation-1".into(),
            question: "total sales?".into(),
            workspace_revision: Some("revision-1".into()),
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
                }],
            }),
            result: AnalysisResult {
                text: "100".into(),
                status: VerificationStatus::Verified,
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
        let loaded = load(&dir, "turn-test-1").unwrap();
        assert_eq!(loaded.question, "total sales?");
        assert_eq!(loaded.trace.steps[0].operation, "run_sql");
        assert_eq!(loaded.result.status, VerificationStatus::Verified);
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
