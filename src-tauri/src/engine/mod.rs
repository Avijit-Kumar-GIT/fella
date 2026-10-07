//! The Fella analytical engine: workspace catalog, the data engine (SQLite by
//! default, DuckDB behind a feature), the tool registry, and the agent loop.

pub mod agent;
pub mod analysis_store;
pub mod analytics;
pub mod capabilities;
pub mod catalog;
pub mod context;
mod env;
pub mod error;
pub mod evidence;
mod friction;
pub mod grounding;
pub mod ingest;
pub mod llm;
pub mod memory;
pub mod planner;
pub mod provider;
pub mod risk;
pub mod runtime;
pub mod secrets;
pub mod semantic_memory;
pub mod sqlite;
pub mod state;
/// Deterministic fixtures + golden answers for `examples/agent_eval`.
/// Dev-only: compiled under test or `--features eval`, never in the app.
#[cfg(any(test, feature = "eval"))]
pub mod testkit;
pub mod tools;
pub mod workspace_model;

pub use capabilities::AnalysisCapabilities;
pub use catalog::{Catalog, SourceInfo, WorkspaceProgress};
pub use error::{EngineError, EngineResult};
pub use evidence::{Answer, AskEvent};
pub use llm::ProviderHealth;
pub use provider::{AuthKind, Provider, PROVIDERS};
pub use runtime::{
    AnalysisContract, AnalysisTurn, AnalysisTurnReplayStatus, ClarificationReply, ComparisonKind,
    ContextReference, ContractComparison, ContractDerivedMetric, ContractFilter, ContractJoin,
    ContractMeasure, ContractOrder, DerivedMetricKind, InterpretationStatus, JoinKind, RunLogEntry,
    RunLogKind, RunLogModelCall, RunLogOperation, SortDirection, TimeBucket,
};
pub use sqlite::Settings;
pub use state::{
    AskOptions, ConversationSummary, ConversationsInfo, EngineState, ProviderInfo, QueryResult,
};
pub use workspace_model::{
    FieldProfile, FieldRole, RelationshipCandidate, SourceModel, WorkspaceModel,
};
