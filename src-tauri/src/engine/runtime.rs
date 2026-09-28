//! The typed runtime spine for an analytical turn.
//!
//! The current agent loop predates these objects and still owns most of the
//! orchestration.  These types make the lifecycle explicit without forcing a
//! rewrite: the direct path can populate a minimal contract and trace today,
//! while semantic interpretation, planning, and acceptance gates attach to
//! the same objects in later slices.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value as Json;

use crate::engine::evidence::{VerificationCheck, VerificationStatus};

/// A stable identifier for one analytical question, distinct from the
/// conversation/tab that contains it.
pub type TurnId = String;

/// A stable identifier for the execution trace produced by one turn.
pub type TraceId = String;

/// Internal function name used when the runtime asks the model to state its
/// interpretation before using data tools. It never reaches the data engine
/// and is omitted from user-facing evidence.
pub const CONTRACT_TOOL_NAME: &str = "__analysis_contract";

/// Explicit lifecycle states for the analytical runtime.  The fast path may
/// move through several states without an extra model call, but it still
/// reports the same protocol as the future structured path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnState {
    Received,
    Interpreting,
    Grounding,
    Planning,
    Executing,
    Verifying,
    Accepted,
    Clarify,
    Retry,
    NeedsReview,
    Unsupported,
    Failed,
    Cancelled,
}

/// How much authority the runtime currently gives an interpretation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InterpretationStatus {
    #[default]
    Unresolved,
    Grounded,
    Assumed,
    Ambiguous,
    Unsupported,
}

/// A measure in the semantic question representation.  These are semantic
/// labels for now; the compiler will later resolve them to physical columns.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractMeasure {
    pub concept: String,
    pub operation: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
}

/// A question filter before or after it has been grounded to observed values.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractFilter {
    pub concept: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    #[serde(default)]
    pub candidate_values: Vec<String>,
    #[serde(default)]
    pub resolved_values: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolution: Option<String>,
}

/// Time semantics kept separate so date-field and range mistakes can be
/// checked without parsing the model's prose answer.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractTime {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub range: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,
}

/// The compact intermediate representation between the user's language and
/// physical execution.  It is intentionally not chain-of-thought.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnalysisContract {
    #[serde(default)]
    pub interpretation: InterpretationStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grain: Option<String>,
    #[serde(default)]
    pub measures: Vec<ContractMeasure>,
    #[serde(default)]
    pub filters: Vec<ContractFilter>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time: Option<ContractTime>,
    #[serde(default)]
    pub group_by: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comparison: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presentation: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assumptions: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unresolved: Vec<String>,
}

impl AnalysisContract {
    /// Parse and normalize a model-proposed contract. The model cannot grant
    /// itself `grounded` status: only later data probes and verification may
    /// promote an interpretation beyond `assumed`.
    pub fn from_tool_args(value: &Json) -> Result<Self, String> {
        let mut contract: Self = serde_json::from_value(value.clone())
            .map_err(|error| format!("invalid analysis contract: {error}"))?;
        if matches!(contract.interpretation, InterpretationStatus::Grounded) {
            contract.interpretation = InterpretationStatus::Assumed;
        }
        if contract.interpretation != InterpretationStatus::Unsupported {
            if !contract.unresolved.is_empty() {
                contract.interpretation = InterpretationStatus::Ambiguous;
            }
            if contract.subject.is_none()
                && contract.measures.is_empty()
                && contract.filters.is_empty()
                && contract.group_by.is_empty()
            {
                return Err("the contract did not identify an analytical subject".into());
            }
        }
        Ok(contract)
    }
}

/// The strategy used to reach the execution layer.  `DirectTools` is the
/// current model-generated SQL/Python path; compiled strategies arrive in a
/// later milestone without changing the result envelope.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanStrategy {
    #[default]
    DirectTools,
    CompiledSql,
    PythonFallback,
    DocumentFallback,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogicalPlan {
    pub strategy: PlanStrategy,
    #[serde(default)]
    pub steps: Vec<String>,
}

/// One completed runtime operation.  This is deliberately smaller than an
/// evidence payload: raw rows and SQL remain on `EvidenceItem`, while this
/// trace gives the runtime a stable, shell-independent execution summary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceStep {
    pub id: String,
    pub operation: String,
    pub duration_ms: u64,
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default)]
    pub sources: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionTrace {
    pub id: TraceId,
    pub turn_id: TurnId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_revision: Option<String>,
    #[serde(default)]
    pub steps: Vec<TraceStep>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerificationReport {
    pub status: VerificationStatus,
    #[serde(default)]
    pub checks: Vec<VerificationCheck>,
}

/// Canonical shape for one question.  Persistence and richer state transitions
/// will be added after the initial protocol is in place.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisTurn {
    pub id: TurnId,
    pub conversation_id: String,
    pub question: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_revision: Option<String>,
    pub state: TurnState,
    pub contract: AnalysisContract,
    pub plan: LogicalPlan,
    pub trace: ExecutionTrace,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification: Option<VerificationReport>,
}

fn new_id(prefix: &str) -> String {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    let sequence = NEXT.fetch_add(1, Ordering::Relaxed);
    format!("{prefix}-{millis:x}-{sequence:x}")
}

pub fn new_turn_id() -> TurnId {
    new_id("turn")
}

pub fn new_trace_id() -> TraceId {
    new_id("trace")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_distinct_and_prefixed() {
        let a = new_turn_id();
        let b = new_turn_id();
        assert!(a.starts_with("turn-"));
        assert!(b.starts_with("turn-"));
        assert_ne!(a, b);
    }

    #[test]
    fn contract_serializes_as_a_small_stable_ir() {
        let contract = AnalysisContract {
            interpretation: InterpretationStatus::Grounded,
            subject: Some("transactions".into()),
            grain: Some("transaction".into()),
            measures: vec![ContractMeasure {
                concept: "amount".into(),
                operation: "sum".into(),
                unit: Some("USD".into()),
            }],
            ..Default::default()
        };
        let value = serde_json::to_value(contract).unwrap();
        assert_eq!(value["interpretation"], "grounded");
        assert_eq!(value["measures"][0]["operation"], "sum");
        assert!(value.get("unresolved").is_none());
    }

    #[test]
    fn state_uses_wire_safe_names() {
        assert_eq!(
            serde_json::to_value(TurnState::NeedsReview).unwrap(),
            "needs_review"
        );
    }

    #[test]
    fn model_cannot_claim_that_a_contract_is_already_grounded() {
        let contract = AnalysisContract::from_tool_args(&serde_json::json!({
            "interpretation": "grounded",
            "subject": "sales",
            "measures": [{"concept": "revenue", "operation": "sum"}],
            "filters": [],
            "group_by": [],
            "assumptions": [],
            "unresolved": []
        }))
        .unwrap();
        assert_eq!(contract.interpretation, InterpretationStatus::Assumed);
    }

    #[test]
    fn ambiguous_contracts_are_preserved_for_the_runtime() {
        let contract = AnalysisContract::from_tool_args(&serde_json::json!({
            "interpretation": "assumed",
            "subject": "sales",
            "measures": [{"concept": "revenue", "operation": "sum"}],
            "filters": [],
            "group_by": [],
            "assumptions": [],
            "unresolved": ["which revenue field?" ]
        }))
        .unwrap();
        assert_eq!(contract.interpretation, InterpretationStatus::Ambiguous);
        assert_eq!(contract.unresolved, vec!["which revenue field?"]);
    }
}
