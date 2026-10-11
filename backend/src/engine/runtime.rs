//! The typed runtime spine for an analytical turn.
//!
//! These types make the model-directed lifecycle explicit: an optional semantic
//! hypothesis, model-selected tool calls, deterministic plans when useful, and
//! the same trace and verification objects for every route.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value as Json;

use crate::engine::context::ContextAssemblyAudit;
use crate::engine::evidence::{AnswerProvenance, Usage, VerificationCheck, VerificationStatus};
use crate::engine::investigation::InvestigationContextSnapshot;

/// A stable identifier for one analytical question, distinct from the
/// conversation/tab that contains it.
pub type TurnId = String;

/// A stable identifier for the execution trace produced by one turn.
pub type TraceId = String;

/// Internal function name used when the model wants to state a semantic
/// hypothesis. It never reaches the data engine and is omitted from user-facing
/// evidence.
pub const CONTRACT_TOOL_NAME: &str = "__analysis_contract";

/// Internal control-flow action used when the model concludes that a user,
/// rather than another heuristic, must resolve a material interpretation.
pub const CLARIFICATION_TOOL_NAME: &str = "__request_clarification";

/// A user-selected workspace starting point. References are hints for
/// interpretation, never evidence or permission grants; the engine still
/// resolves them against the current catalog and verifies all results.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextReference {
    pub kind: String,
    pub key: String,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// Explicit lifecycle states for the analytical runtime. A model-directed turn
/// may move through several states in one model response, but it still reports
/// the same protocol for every execution route.
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

/// A user-facing decision request emitted when the available evidence supports
/// more than one materially different interpretation. This is deliberately
/// separate from `unresolved`: unresolved describes what the runtime still
/// needs to investigate, while a clarification is the model's conclusion that
/// the user is the right authority to choose.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClarificationRequest {
    pub question: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Transport metadata for a response (choice or free text) to one persisted
/// clarification. The backend resolves and validates the referenced turn.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClarificationReply {
    pub turn_id: TurnId,
    pub response: String,
}

/// Validated clarification context passed to the model for a resumed analysis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedClarification {
    pub turn_id: TurnId,
    pub original_question: String,
    pub request: ClarificationRequest,
    pub response: String,
    pub source_revision_changed: bool,
    /// The parent's structured working interpretation. It helps the resumed
    /// model preserve unaffected dimensions (such as grouping and time scope),
    /// but is not execution evidence and must not be trusted as a result.
    pub parent_contract: Option<AnalysisContract>,
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
    /// `false` means the observed values are included; `true` means they are
    /// excluded. Keeping this in the contract prevents the planner from
    /// losing the difference between “category is Income” and “everything
    /// except category Income”.
    #[serde(default, skip_serializing_if = "is_false")]
    pub exclude: bool,
    #[serde(default)]
    pub candidate_values: Vec<String>,
    #[serde(default)]
    pub resolved_values: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolution: Option<String>,
}

fn is_false(value: &bool) -> bool {
    !*value
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
    Difference,
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

/// The first typed comparison primitive. Explicit windows keep date alignment
/// in the contract instead of asking the executor or answer prose to infer it
/// from a label such as "year over year".
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComparisonKind {
    #[default]
    PeriodOverPeriod,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ContractComparison {
    pub kind: ComparisonKind,
    pub current_range: String,
    pub previous_range: String,
}

/// The compact intermediate representation between the user's language and
/// physical execution.  It is intentionally not chain-of-thought.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnalysisContract {
    #[serde(default)]
    pub interpretation: InterpretationStatus,
    /// Physical workspace source selected for this analysis. This remains
    /// separate from `subject`, which describes the user's semantic topic.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
    /// Human-language population selected by the model before physical filters
    /// are resolved. Keeping this beside `filters` lets verification distinguish
    /// "the rows that mean spending" from merely "a valid WHERE clause".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub population: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grain: Option<String>,
    /// Domain meaning for a measure whose sign or label is not self-evident,
    /// for example a positive expense export or a category-based income flow.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_semantics: Option<String>,
    /// Explicit missing-value policy for averages, rates, and counts.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub missing_policy: Option<String>,
    /// What the denominator means for a ratio or percentage.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub denominator: Option<String>,
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
    /// Structured comparison semantics. The legacy freeform `comparison`
    /// field remains readable for archived contracts but is not executable by
    /// the deterministic planner.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comparison_spec: Option<ContractComparison>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comparison: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presentation: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assumptions: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unresolved: Vec<String>,
    /// A blocking semantic choice for the user. The model may propose this
    /// after grounding reveals two plausible populations, fields, or scopes;
    /// the runtime never silently turns it into a data query.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clarification: Option<ClarificationRequest>,
}

impl AnalysisContract {
    /// Parse and normalize a model-proposed contract. The model cannot grant
    /// itself `grounded` status: only later data probes and verification may
    /// promote an interpretation beyond `assumed`. The result remains advisory
    /// and does not grant or remove tool access.
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
            // Older providers may still echo the archived free-form field
            // alongside the typed comparison. The typed IR is authoritative;
            // rejecting the entire contract would turn a harmless migration
            // artifact into a failed analytical turn.
            if contract.comparison_spec.is_some() {
                contract.comparison = None;
            }
            if let Some(comparison) = &contract.comparison_spec {
                if comparison.current_range.trim().is_empty()
                    || comparison.previous_range.trim().is_empty()
                {
                    // Optional model metadata should not invalidate the rest of a
                    // usable contract. An explicit comparison with two equal
                    // windows is still rejected below because it changes the
                    // requested computation rather than merely being incomplete.
                    contract.comparison_spec = None;
                } else if comparison.current_range.trim() == comparison.previous_range.trim() {
                    return Err("a comparison needs two different ranges".into());
                }
            }
            if !contract.unresolved.is_empty() {
                contract.interpretation = InterpretationStatus::Ambiguous;
            }
            if let Some(clarification) = contract.clarification.take() {
                contract.clarification = Some(normalize_clarification(clarification)?);
                // Asking the user is a semantic decision, not a weakly stated
                // assumption. Keep the state explicit even if a provider
                // accidentally sent `assumed` beside the request.
                contract.interpretation = InterpretationStatus::Ambiguous;
            }
            if contract.subject.is_none()
                && contract.measures.is_empty()
                && contract.filters.is_empty()
                && contract.group_by.is_empty()
                && contract.clarification.is_none()
            {
                return Err("the contract did not identify an analytical subject".into());
            }
        }
        Ok(contract)
    }
}

/// Parse and validate the model's explicit user-interaction action. Keeping
/// this separate from the analytical contract makes a clarification a clear
/// control-flow decision rather than optional metadata buried in a larger IR.
pub fn clarification_from_tool_args(value: &Json) -> Result<ClarificationRequest, String> {
    let request: ClarificationRequest = serde_json::from_value(value.clone())
        .map_err(|error| format!("invalid clarification request: {error}"))?;
    normalize_clarification(request)
}

fn normalize_clarification(
    mut clarification: ClarificationRequest,
) -> Result<ClarificationRequest, String> {
    clarification.question = clarification.question.trim().to_string();
    if clarification.question.is_empty() {
        return Err("a clarification needs a user-facing question".into());
    }
    if clarification.question.chars().count() > 800 {
        return Err("a clarification question is too long".into());
    }
    clarification.options = clarification
        .options
        .iter()
        .map(|option| option.trim().to_string())
        .filter(|option| !option.is_empty())
        .collect();
    if clarification.options.len() > 6 {
        return Err("a clarification may offer at most six options".into());
    }
    if clarification
        .options
        .iter()
        .any(|option| option.chars().count() > 240)
    {
        return Err("a clarification option is too long".into());
    }
    if clarification
        .reason
        .as_ref()
        .is_some_and(|reason| reason.trim().is_empty() || reason.chars().count() > 500)
    {
        return Err("a clarification reason must be short and non-empty".into());
    }
    if !clarification.options.is_empty() {
        let mut unique = clarification.options.clone();
        unique.sort_by_key(|option| option.to_ascii_lowercase());
        unique.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
        clarification.options = unique;
    }
    if let Some(reason) = clarification.reason.as_mut() {
        *reason = reason.trim().to_string();
    }
    Ok(clarification)
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
    /// Capability path for this turn. None on older persisted traces.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<InteractionMode>,
    /// Configured model identifier; prompt contents and credentials are never
    /// stored in the trace.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Provider round trips, separate from local tool steps.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub model_calls: Vec<ModelCallTrace>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub elapsed_ms: Option<u64>,
    #[serde(default)]
    pub steps: Vec<TraceStep>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InteractionMode {
    ModelOnly,
    WorkspaceAsk,
    WorkspaceInspect,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelCallTrace {
    pub model: String,
    pub duration_ms: u64,
    pub success: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_tokens: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completion_tokens: Option<u32>,
}

/// A content-minimized, local projection of a completed run for the Settings
/// activity log. Deliberately excludes prompts, answers, tool arguments/results,
/// source names, and workspace paths.
#[derive(Debug, Clone, Serialize)]
pub struct RunLogEntry {
    pub id: String,
    pub at_ms: u64,
    pub kind: RunLogKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<InteractionMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub elapsed_ms: Option<u64>,
    #[serde(default)]
    pub model_calls: Vec<RunLogModelCall>,
    #[serde(default)]
    pub operations: Vec<RunLogOperation>,
    pub prior_analysis_count: usize,
    pub context_reference_count: usize,
    pub clarification_continuation: bool,
    pub rerun: bool,
    pub outcome: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger_steps: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger_errors: Option<usize>,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunLogKind {
    Turn,
    Trigger,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunLogModelCall {
    pub model: String,
    pub duration_ms: u64,
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completion_tokens: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunLogOperation {
    pub operation: String,
    pub duration_ms: u64,
    pub success: bool,
}

/// Compact catalog metadata captured with a canonical turn. It is deliberately
/// smaller than `WorkspaceModel`: replay needs to explain freshness changes,
/// not persist every inferred statistic or sample value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceColumnSnapshot {
    pub name: String,
    #[serde(rename = "type")]
    pub type_: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceSourceSnapshot {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub view: Option<String>,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub row_count: Option<i64>,
    #[serde(default)]
    pub columns: Vec<WorkspaceColumnSnapshot>,
    pub size_bytes: u64,
    pub mtime: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceRevisionSnapshot {
    pub path: String,
    pub revision: String,
    #[serde(default)]
    pub sources: Vec<WorkspaceSourceSnapshot>,
    #[serde(default)]
    pub skipped: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceChangeKind {
    Added,
    Removed,
    Changed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceSourceChange {
    pub name: String,
    pub kind: WorkspaceChangeKind,
    #[serde(default)]
    pub details: Vec<String>,
}

impl WorkspaceRevisionSnapshot {
    /// Compare the compact catalog metadata from two revisions. The revision
    /// hash remains the authoritative freshness check; these details make a
    /// replay explainable to a person and to evaluation tooling.
    pub fn changes_to(&self, current: &Self) -> Vec<WorkspaceSourceChange> {
        let mut previous = BTreeMap::new();
        for source in &self.sources {
            previous.insert(source_key(source), source);
        }
        let mut next = BTreeMap::new();
        for source in &current.sources {
            next.insert(source_key(source), source);
        }

        let mut changes = Vec::new();
        for key in previous.keys() {
            if !next.contains_key(key) {
                changes.push(WorkspaceSourceChange {
                    name: (*key).clone(),
                    kind: WorkspaceChangeKind::Removed,
                    details: vec!["source removed".into()],
                });
            }
        }
        for (key, source) in &next {
            let Some(previous_source) = previous.get(key) else {
                changes.push(WorkspaceSourceChange {
                    name: (*key).clone(),
                    kind: WorkspaceChangeKind::Added,
                    details: vec!["source added".into()],
                });
                continue;
            };
            let mut details = Vec::new();
            if previous_source.kind != source.kind {
                details.push(format!(
                    "kind changed from {} to {}",
                    previous_source.kind, source.kind
                ));
            }
            if previous_source.row_count != source.row_count {
                details.push(format!(
                    "row count changed from {} to {}",
                    display_optional(previous_source.row_count),
                    display_optional(source.row_count)
                ));
            }
            if previous_source.columns != source.columns {
                details.push("columns changed".into());
            }
            if previous_source.size_bytes != source.size_bytes
                || previous_source.mtime != source.mtime
            {
                details.push("file metadata changed".into());
            }
            if !details.is_empty() {
                changes.push(WorkspaceSourceChange {
                    name: (*key).clone(),
                    kind: WorkspaceChangeKind::Changed,
                    details,
                });
            }
        }
        if self.skipped != current.skipped {
            changes.push(WorkspaceSourceChange {
                name: "workspace scan".into(),
                kind: WorkspaceChangeKind::Changed,
                details: vec!["skipped-file set changed".into()],
            });
        }
        changes
    }
}

fn source_key(source: &WorkspaceSourceSnapshot) -> String {
    source.view.clone().unwrap_or_else(|| source.name.clone())
}

fn display_optional(value: Option<i64>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "unknown".into())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnalysisTurnReplayStatus {
    pub turn_id: TurnId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_revision: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_revision: Option<String>,
    pub same_workspace: bool,
    pub revision_changed: bool,
    pub snapshot_available: bool,
    pub can_rerun: bool,
    #[serde(default)]
    pub source_changes: Vec<WorkspaceSourceChange>,
    /// Immutable scope summary captured when this turn was persisted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_scope: Option<AnalysisSourceScope>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnalysisSourceReference {
    /// Workspace-relative file or document identity.
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub table: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence_ids: Vec<String>,
}

/// The source references observed in a turn's evidence, plus files the
/// workspace catalog explicitly skipped at the same revision. This is a
/// historical snapshot; later catalog or scope changes cannot rewrite it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnalysisSourceScope {
    #[serde(default)]
    pub used_sources: Vec<AnalysisSourceReference>,
    #[serde(default)]
    pub skipped_sources: Vec<String>,
}

impl AnalysisSourceScope {
    pub fn from_evidence(
        evidence: &[crate::engine::evidence::EvidenceItem],
        skipped_sources: &[String],
    ) -> Self {
        let mut used_sources = Vec::new();
        for item in evidence {
            if item.error.is_some() {
                continue;
            }
            for source in &item.sources {
                remember_source(
                    &mut used_sources,
                    &source.source,
                    Some(&source.table),
                    &item.id,
                );
            }

            // Document tools expose file names in their structured arguments
            // or result rows rather than `EvidenceItem.sources`.
            match item.tool.as_str() {
                "read_file" => {
                    let args = &item.args;
                    if let Some(names) = args.get("names").and_then(serde_json::Value::as_array) {
                        for name in names.iter().filter_map(serde_json::Value::as_str) {
                            remember_source(&mut used_sources, name, None, &item.id);
                        }
                    } else if let Some(name) = args.get("name").and_then(serde_json::Value::as_str)
                    {
                        remember_source(&mut used_sources, name, None, &item.id);
                    }
                }
                "grep_files" => {
                    let source_column = item
                        .columns
                        .as_ref()
                        .and_then(|columns| columns.iter().position(|column| column == "source"));
                    if let (Some(column), Some(rows)) = (source_column, item.rows.as_ref()) {
                        for name in rows
                            .iter()
                            .filter_map(|row| row.get(column))
                            .filter_map(serde_json::Value::as_str)
                        {
                            remember_source(&mut used_sources, name, None, &item.id);
                        }
                    }
                }
                _ => {}
            }
        }

        Self {
            used_sources,
            skipped_sources: skipped_sources.to_vec(),
        }
    }
}

fn remember_source(
    sources: &mut Vec<AnalysisSourceReference>,
    source: &str,
    table: Option<&str>,
    evidence_id: &str,
) {
    if source.trim().is_empty() {
        return;
    }
    let table = table.filter(|table| !table.is_empty());
    if let Some(existing) = sources
        .iter_mut()
        .find(|existing| existing.source == source && existing.table.as_deref() == table)
    {
        if !evidence_id.is_empty() && !existing.evidence_ids.iter().any(|id| id == evidence_id) {
            existing.evidence_ids.push(evidence_id.to_owned());
        }
        return;
    }
    sources.push(AnalysisSourceReference {
        source: source.to_owned(),
        table: table.map(str::to_owned),
        evidence_ids: if evidence_id.is_empty() {
            Vec::new()
        } else {
            vec![evidence_id.to_owned()]
        },
    });
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
    /// Provider-reported token use summed across this model-driven turn.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
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
    /// Structured context-picker references used as starting points for this
    /// turn. Optional in serialized records so older turns remain readable.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub context_refs: Vec<ContextReference>,
    /// Earlier workspace analyses explicitly retrieved by this turn. These
    /// links are validated against conversation and source revision before
    /// their evidence is made available to the model.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub prior_turn_refs: Vec<TurnId>,
    /// Parent turn when this analysis continues after a clarification.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clarification_of: Option<TurnId>,
    /// The user's resolution of the parent's pending semantic choice.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clarification_response: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_revision: Option<String>,
    /// Compact source metadata for explaining what changed before a rerun.
    /// Optional so records written before revision-aware replay remain valid.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_snapshot: Option<WorkspaceRevisionSnapshot>,
    /// Source identities actually referenced by this run's evidence and the
    /// catalog-skipped inputs at the same workspace revision.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_scope: Option<AnalysisSourceScope>,
    /// Exact Investigation definitions and assumptions supplied to this run.
    /// The snapshot survives later edits/removals and remains optional for old
    /// turns and unscoped conversations.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub investigation_context: Option<InvestigationContextSnapshot>,
    /// Length-only audit of bounded model context; text is never copied here.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_audit: Option<ContextAssemblyAudit>,
    /// Explicit links from this turn to evidence, included context, and any
    /// clarification that resolved an interpretation. Optional for older
    /// persisted turns.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance: Option<AnswerProvenance>,
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
            source: Some("transactions".into()),
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
        assert_eq!(value["source"], "transactions");
        assert_eq!(value["measures"][0]["operation"], "sum");
        assert!(value.get("unresolved").is_none());
    }

    #[test]
    fn legacy_contracts_without_a_source_remain_readable() {
        let contract: AnalysisContract = serde_json::from_value(serde_json::json!({
            "interpretation": "assumed",
            "measures": [],
            "filters": [],
            "group_by": [],
            "assumptions": [],
            "unresolved": []
        }))
        .unwrap();

        assert_eq!(contract.source, None);
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
    fn clarification_requests_are_typed_and_force_ambiguous_state() {
        let contract = AnalysisContract::from_tool_args(&serde_json::json!({
            "interpretation": "assumed",
            "measures": [{ "concept": "amount", "operation": "sum" }],
            "clarification": {
                "question": "Should income rows be included in this total? ",
                "options": ["Include income", "Expenses only", "include income"]
            },
            "assumptions": [],
            "unresolved": []
        }))
        .unwrap();
        assert_eq!(contract.interpretation, InterpretationStatus::Ambiguous);
        let clarification = contract.clarification.unwrap();
        assert_eq!(
            clarification.question,
            "Should income rows be included in this total?"
        );
        assert_eq!(
            clarification.options,
            vec!["Expenses only", "Include income"]
        );
    }

    #[test]
    fn clarification_without_a_question_is_rejected() {
        let result = AnalysisContract::from_tool_args(&serde_json::json!({
            "interpretation": "ambiguous",
            "clarification": { "question": "  " },
            "assumptions": [],
            "unresolved": []
        }));
        assert!(result.is_err());
    }

    #[test]
    fn standalone_clarification_action_uses_the_same_validation() {
        let request = clarification_from_tool_args(&serde_json::json!({
            "question": "  Include utilities? ",
            "options": [" Yes ", "No", "yes"],
            "reason": "  This changes the total.  "
        }))
        .unwrap();
        assert_eq!(request.question, "Include utilities?");
        assert_eq!(request.options, ["No", "Yes"]);
        assert_eq!(request.reason.as_deref(), Some("This changes the total."));
        assert!(clarification_from_tool_args(&serde_json::json!({
            "question": "  "
        }))
        .is_err());
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

    #[test]
    fn typed_period_comparisons_validate_explicit_windows() {
        let contract = AnalysisContract::from_tool_args(&serde_json::json!({
            "interpretation": "assumed",
            "subject": "sales",
            "measures": [{"concept": "revenue", "operation": "sum"}],
            "filters": [],
            "time": {"field": "month"},
            "group_by": [],
            "comparison_spec": {
                "kind": "period_over_period",
                "current_range": "2024",
                "previous_range": "2023"
            },
            "assumptions": [],
            "unresolved": []
        }))
        .unwrap();
        assert_eq!(
            contract.comparison_spec.unwrap().kind,
            ComparisonKind::PeriodOverPeriod
        );

        let invalid = AnalysisContract::from_tool_args(&serde_json::json!({
            "interpretation": "assumed",
            "subject": "sales",
            "measures": [{"concept": "revenue", "operation": "sum"}],
            "filters": [],
            "time": {"field": "month"},
            "group_by": [],
            "comparison_spec": {
                "kind": "period_over_period",
                "current_range": "2024",
                "previous_range": "2024"
            },
            "assumptions": [],
            "unresolved": []
        }));
        assert!(invalid.is_err());

        let incomplete = AnalysisContract::from_tool_args(&serde_json::json!({
            "interpretation": "assumed",
            "subject": "books",
            "measures": [{"concept": "rating", "operation": "average"}],
            "filters": [],
            "group_by": [],
            "comparison_spec": {},
            "assumptions": [],
            "unresolved": []
        }))
        .unwrap();
        assert!(incomplete.comparison_spec.is_none());

        let migrated = AnalysisContract::from_tool_args(&serde_json::json!({
            "interpretation": "assumed",
            "subject": "sales",
            "measures": [{"concept": "revenue", "operation": "sum"}],
            "filters": [],
            "group_by": [],
            "comparison": "year over year",
            "comparison_spec": {
                "kind": "period_over_period",
                "current_range": "2024",
                "previous_range": "2023"
            },
            "assumptions": [],
            "unresolved": []
        }))
        .unwrap();
        assert!(migrated.comparison.is_none());
        assert!(migrated.comparison_spec.is_some());
    }

    #[test]
    fn revision_snapshots_explain_source_changes() {
        let before = WorkspaceRevisionSnapshot {
            path: "/tmp/workspace".into(),
            revision: "rev-a".into(),
            sources: vec![WorkspaceSourceSnapshot {
                name: "sales.csv".into(),
                view: Some("sales".into()),
                kind: "csv".into(),
                row_count: Some(2),
                columns: vec![WorkspaceColumnSnapshot {
                    name: "amount".into(),
                    type_: "DOUBLE".into(),
                }],
                size_bytes: 20,
                mtime: 1,
            }],
            skipped: vec![],
        };
        let after = WorkspaceRevisionSnapshot {
            path: before.path.clone(),
            revision: "rev-b".into(),
            sources: vec![WorkspaceSourceSnapshot {
                name: "sales.csv".into(),
                view: Some("sales".into()),
                kind: "csv".into(),
                row_count: Some(3),
                columns: vec![WorkspaceColumnSnapshot {
                    name: "amount".into(),
                    type_: "DOUBLE".into(),
                }],
                size_bytes: 30,
                mtime: 2,
            }],
            skipped: vec![],
        };

        let changes = before.changes_to(&after);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].name, "sales");
        assert_eq!(changes[0].kind, WorkspaceChangeKind::Changed);
        assert!(changes[0]
            .details
            .iter()
            .any(|detail| detail.contains("row count")));
        assert!(changes[0]
            .details
            .iter()
            .any(|detail| detail == "file metadata changed"));
    }

    #[test]
    fn source_scope_collects_tabular_and_document_evidence_without_listing_inventory_as_used() {
        use crate::engine::evidence::{EvidenceItem, EvidenceSource};
        use serde_json::json;

        let item = |id: &str,
                    tool: &str,
                    args: serde_json::Value,
                    sources: Vec<EvidenceSource>,
                    columns: Option<Vec<String>>,
                    rows: Option<Vec<Vec<serde_json::Value>>>| EvidenceItem {
            id: id.into(),
            tool: tool.into(),
            sources,
            args,
            note: None,
            sql: None,
            result_summary: String::new(),
            columns,
            rows,
            row_count: None,
            output: None,
            chart: None,
            result_table: None,
            python_input_trace: None,
            python_queries: None,
            python_queries_complete: None,
            ms: 0,
            error: None,
            verifier_disposition: None,
        };
        let evidence = vec![
            item(
                "evidence-sql",
                "run_sql",
                json!({}),
                vec![EvidenceSource {
                    source: "sales.csv".into(),
                    table: "sales".into(),
                    note: None,
                }],
                None,
                None,
            ),
            item(
                "evidence-read",
                "read_file",
                json!({"names": ["notes/brief.md", "notes/method.md"]}),
                Vec::new(),
                None,
                None,
            ),
            item(
                "evidence-search",
                "grep_files",
                json!({"pattern": "retention"}),
                Vec::new(),
                Some(vec!["rank".into(), "source".into()]),
                Some(vec![vec![json!(1), json!("notes/brief.md")]]),
            ),
            item(
                "evidence-inventory",
                "list_files",
                json!({}),
                Vec::new(),
                Some(vec!["source".into()]),
                Some(vec![vec![json!("unused.csv")]]),
            ),
        ];

        let scope =
            AnalysisSourceScope::from_evidence(&evidence, &["unsupported.xlsx: encrypted".into()]);
        assert_eq!(
            scope.used_sources,
            vec![
                AnalysisSourceReference {
                    source: "sales.csv".into(),
                    table: Some("sales".into()),
                    evidence_ids: vec!["evidence-sql".into()],
                },
                AnalysisSourceReference {
                    source: "notes/brief.md".into(),
                    table: None,
                    evidence_ids: vec!["evidence-read".into(), "evidence-search".into()],
                },
                AnalysisSourceReference {
                    source: "notes/method.md".into(),
                    table: None,
                    evidence_ids: vec!["evidence-read".into()],
                },
            ]
        );
        assert_eq!(scope.skipped_sources, vec!["unsupported.xlsx: encrypted"]);
    }
}
