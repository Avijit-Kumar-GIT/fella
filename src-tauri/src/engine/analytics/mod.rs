//! The deterministic compute-and-check layer: SQL execution, the Python
//! stats sandbox, chart-data validation, and answer verification. Everything
//! in this module is independently readable and testable on its own —
//! no LLM calls, no Tauri/IPC, no secrets, no conversation state. The only
//! seam back into the rest of the app is [`AnalyticsSource`], which the
//! agent loop and tools reach through instead of a full [`crate::engine::state::EngineState`].
//!
//! [`data`], [`pyexec`], and [`chart`] were already decoupled from
//! `EngineState` by accident; [`verify`] wasn't, until [`AnalyticsSource`]
//! narrowed what it's allowed to touch. This module exists so that's
//! deliberate and documented everywhere now, not true by accident in three
//! places and by a real dependency in the fourth.

pub mod chart;
pub mod data;
pub mod provenance;
pub mod pyexec;
pub mod verify;

use crate::engine::catalog::Catalog;
use crate::engine::error::EngineResult;
use crate::engine::state::QueryResult;

/// The one capability this module needs from the app's state: read the
/// current catalog, and run read-only SQL against it. [`crate::engine::state::EngineState`]
/// implements this directly from methods it already has (`catalog()`,
/// `run_sql()`); nothing here duplicates that logic, it just narrows what
/// [`verify::run`] is allowed to see. Returns [`QueryResult`] (not
/// [`data::QueryOutcome`](data::QueryOutcome)) because that's the shape
/// `EngineState::run_sql` already produces -- this trait borrows the type,
/// it doesn't own it.
pub trait AnalyticsSource {
    fn catalog(&self) -> Catalog;
    fn run_sql(&self, sql: &str) -> EngineResult<QueryResult>;
}
