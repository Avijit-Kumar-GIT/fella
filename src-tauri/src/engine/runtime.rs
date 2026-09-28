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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimeBucket {
    Year,
    Month,
    Week,
    Day,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortDirection {
    Asc,
    Desc,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractOrder {
    pub by: String,
    pub direction: SortDirection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DerivedMetricKind {
    Ratio,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractDerivedMetric {
    pub concept: String,
    pub kind: DerivedMetricKind,
    pub numerator: String,
    pub denominator: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JoinKind {
    Inner,
    Left,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractJoin {
    pub left_source: String,
    pub left_field: String,
    pub right_source: String,
    pub right_field: String,
    pub kind: JoinKind,
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
    pub bucket: Option<TimeBucket>,
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
    pub order_by: Option<ContractOrder>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u16>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub derived_metrics: Vec<ContractDerivedMetric>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub joins: Vec<ContractJoin>,
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
            if contract
                .order_by
                .as_ref()
                .is_some_and(|order| order.by.trim().is_empty())
            {
                return Err("the order_by clause did not identify a field or measure".into());
            }
            if contract
                .limit
                .is_some_and(|limit| !(1..=1000).contains(&limit))
            {
                return Err("the analytical limit must be between 1 and 1000".into());
            }
            for derived in &contract.derived_metrics {
                if derived.concept.trim().is_empty()
                    || derived.numerator.trim().is_empty()
                    || derived.denominator.trim().is_empty()
                {
                    return Err(
                        "a derived metric needs a concept, numerator, and denominator".into(),
                    );
                }
            }
            for join in &contract.joins {
                if join.left_source.trim().is_empty()
                    || join.left_field.trim().is_empty()
                    || join.right_source.trim().is_empty()
                    || join.right_field.trim().is_empty()
                {
                    return Err("a join needs left/right sources and fields".into());
                }
                if join.left_source.eq_ignore_ascii_case(&join.right_source)
                    && join.left_field.eq_ignore_ascii_case(&join.right_field)
                {
                    return Err("a join cannot join a field to itself".into());
                }
            }
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

/// The result projection kept in the backend-owned turn record. Raw evidence
/// stays JSON because the evidence envelope is intentionally allowed to grow
/// with new read-only tools without changing the runtime record schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisResult {
    pub text: String,
    pub status: VerificationStatus,
    #[serde(default)]
    pub verification: Vec<VerificationCheck>,
    #[serde(default)]
    pub evidence: Vec<Json>,
}

/// Canonical shape for one question.  Persistence and richer state transitions
/// attach to this object so the UI can remain a projection of the backend
/// record instead of becoming another source of truth.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisTurn {
    pub id: TurnId,
    pub conversation_id: String,
    pub question: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_revision: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rerun_of: Option<TurnId>,
    pub state: TurnState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contract: Option<AnalysisContract>,
    pub plan: LogicalPlan,
    pub trace: ExecutionTrace,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification: Option<VerificationReport>,
    pub result: AnalysisResult,
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
                field: None,
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

    #[test]
    fn ranking_contracts_validate_the_target_and_limit() {
        let contract = AnalysisContract::from_tool_args(&serde_json::json!({
            "interpretation": "assumed",
            "subject": "sales",
            "measures": [{"concept": "revenue", "operation": "sum"}],
            "filters": [],
            "group_by": ["category"],
            "order_by": {"by": "revenue", "direction": "desc"},
            "limit": 10,
            "assumptions": [],
            "unresolved": []
        }))
        .unwrap();
        assert_eq!(contract.order_by.unwrap().direction, SortDirection::Desc);
        assert_eq!(contract.limit, Some(10));

        let invalid = AnalysisContract::from_tool_args(&serde_json::json!({
            "interpretation": "assumed",
            "subject": "sales",
            "measures": [{"concept": "revenue", "operation": "sum"}],
            "filters": [],
            "group_by": [],
            "order_by": {"by": "revenue", "direction": "desc"},
            "limit": 0,
            "assumptions": [],
            "unresolved": []
        }));
        assert!(invalid.is_err());
    }

    #[test]
    fn ratio_contracts_validate_the_declared_operands() {
        let contract = AnalysisContract::from_tool_args(&serde_json::json!({
            "interpretation": "assumed",
            "subject": "sales",
            "measures": [
                {"concept": "revenue", "operation": "sum"},
                {"concept": "orders", "operation": "count"}
            ],
            "filters": [],
            "group_by": [],
            "derived_metrics": [{
                "concept": "revenue per order",
                "kind": "ratio",
                "numerator": "revenue",
                "denominator": "orders"
            }],
            "assumptions": [],
            "unresolved": []
        }))
        .unwrap();
        assert_eq!(contract.derived_metrics[0].kind, DerivedMetricKind::Ratio);

        let invalid = AnalysisContract::from_tool_args(&serde_json::json!({
            "interpretation": "assumed",
            "subject": "sales",
            "measures": [{"concept": "revenue", "operation": "sum"}],
            "filters": [],
            "group_by": [],
            "derived_metrics": [{
                "concept": "revenue per order",
                "kind": "ratio",
                "numerator": "revenue",
                "denominator": ""
            }],
            "assumptions": [],
            "unresolved": []
        }));
        assert!(invalid.is_err());
    }

    #[test]
    fn join_contracts_validate_both_sides() {
        let contract = AnalysisContract::from_tool_args(&serde_json::json!({
            "interpretation": "assumed",
            "subject": "orders",
            "measures": [{"concept": "revenue", "operation": "sum"}],
            "filters": [],
            "group_by": [],
            "joins": [{
                "left_source": "orders",
                "left_field": "customer_id",
                "right_source": "customers",
                "right_field": "id",
                "kind": "left"
            }],
            "assumptions": [],
            "unresolved": []
        }))
        .unwrap();
        assert_eq!(contract.joins[0].kind, JoinKind::Left);

        let invalid = AnalysisContract::from_tool_args(&serde_json::json!({
            "interpretation": "assumed",
            "subject": "orders",
            "measures": [{"concept": "revenue", "operation": "sum"}],
            "filters": [],
            "group_by": [],
            "joins": [{
                "left_source": "orders",
                "left_field": "customer_id",
                "right_source": "orders",
                "right_field": "customer_id",
                "kind": "inner"
            }],
            "assumptions": [],
            "unresolved": []
        }));
        assert!(invalid.is_err());
    }
}
