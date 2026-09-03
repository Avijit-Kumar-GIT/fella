//! Types that make an answer auditable. These serialize to match
//! `src/lib/types.ts` (EvidenceItem / VerificationCheck / Answer / AskEvent).

use serde::Serialize;
use serde_json::Value as Json;

#[derive(Debug, Clone, Serialize)]
pub struct EvidenceItem {
    pub tool: String,
    pub args: Json,
    /// One plain sentence the model wrote describing what this step does, for a
    /// non-technical reader (e.g. "Add up spending by month"). Absent if the
    /// model omitted it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sql: Option<String>,
    pub result_summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub columns: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rows: Option<Vec<Vec<Json>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub row_count: Option<usize>,
    /// Free-form text output (e.g. Python stdout/stderr).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    pub ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct VerificationCheck {
    pub label: String,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Answer {
    pub text: String,
    pub evidence: Vec<EvidenceItem>,
    pub verification: Vec<VerificationCheck>,
}

/// Streamed to the UI over a Tauri channel during `ask`.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AskEvent {
    AssistantDelta { text: String },
    ToolStart { tool: String, args: Json },
    ToolEnd { item: EvidenceItem },
    /// A transient status line for the UI (e.g. "rate limited retrying in 3s…").
    Notice { text: String },
    AnswerDone { answer: Answer },
}
