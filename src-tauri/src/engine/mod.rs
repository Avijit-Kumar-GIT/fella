//! The Fella analytical engine: workspace catalog, the data engine (SQLite by
//! default, DuckDB behind a feature), the tool registry, and the agent loop.

pub mod agent;
pub mod analytics;
pub mod augment;
pub mod catalog;
mod env;
pub mod error;
pub mod evidence;
pub mod extensions;
mod friction;
pub mod ingest;
pub mod llm;
#[cfg(feature = "mcp")]
pub mod mcp;
pub mod memory;
pub mod provider;
pub mod secrets;
pub mod sqlite;
pub mod state;
/// Deterministic fixtures + golden answers for `examples/agent_eval`.
/// Dev-only: compiled under test or `--features eval`, never in the app.
#[cfg(any(test, feature = "eval"))]
pub mod testkit;
pub mod tools;
pub mod update;

pub use catalog::{Catalog, SourceInfo};
pub use error::{EngineError, EngineResult};
pub use extensions::InstalledPack;
pub use evidence::{Answer, AskEvent};
pub use llm::ProviderHealth;
pub use provider::{AuthKind, Provider, PROVIDERS};
pub use sqlite::Settings;
pub use state::{ConversationSummary, ConversationsInfo, EngineState, ProviderInfo, QueryResult};
pub use update::UpdateStatus;
