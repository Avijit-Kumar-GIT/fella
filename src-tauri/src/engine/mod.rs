//! The Fella analytical engine: workspace catalog, the data engine (SQLite by
//! default, DuckDB behind a feature), the tool registry, and the agent loop.

pub mod agent;
pub mod catalog;
pub mod data;
pub mod error;
pub mod evidence;
pub mod extensions;
pub mod ingest;
pub mod llm;
#[cfg(feature = "mcp")]
pub mod mcp;
pub mod provider;
pub mod pyexec;
pub mod secrets;
pub mod sqlite;
pub mod state;
pub mod tools;
pub mod update;
pub mod verify;

pub use catalog::{Catalog, SourceInfo};
pub use error::{EngineError, EngineResult};
pub use extensions::InstalledPack;
pub use evidence::{Answer, AskEvent};
pub use llm::ProviderHealth;
pub use provider::{AuthKind, Provider, PROVIDERS};
pub use sqlite::Settings;
pub use state::{ConversationSummary, ConversationsInfo, EngineState, ProviderInfo, QueryResult};
pub use update::UpdateStatus;
