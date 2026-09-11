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
    /// Sanitized-safe inline SVG from a chart tool (e.g. `make_chart`).
    /// Rust-generated, not model-authored; the frontend still runs it
    /// through an allow-list before `{@html}` (see `src/lib/svg.ts`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chart: Option<String>,
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

/// Token accounting for one `ask`, summed across every model turn. Populated
/// only when the provider reports it (Ollama always; an OpenAI-compatible
/// endpoint when it honours `stream_options.include_usage`). `None` otherwise.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
}

impl Usage {
    /// Add two optional readings; `Some` wins over `None` so a single turn that
    /// reported usage isn't lost because another turn didn't.
    pub fn merge(a: Option<Usage>, b: Option<Usage>) -> Option<Usage> {
        match (a, b) {
            (Some(x), Some(y)) => Some(Usage {
                prompt_tokens: x.prompt_tokens + y.prompt_tokens,
                completion_tokens: x.completion_tokens + y.completion_tokens,
            }),
            (x, y) => x.or(y),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Answer {
    pub text: String,
    pub evidence: Vec<EvidenceItem>,
    pub verification: Vec<VerificationCheck>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usage_merge_sums_and_tolerates_missing() {
        let a = Usage { prompt_tokens: 100, completion_tokens: 10 };
        let b = Usage { prompt_tokens: 40, completion_tokens: 5 };
        assert_eq!(
            Usage::merge(Some(a), Some(b)),
            Some(Usage { prompt_tokens: 140, completion_tokens: 15 })
        );
        assert_eq!(Usage::merge(None, Some(b)), Some(b));
        assert_eq!(Usage::merge(Some(a), None), Some(a));
        assert_eq!(Usage::merge(None, None), None);
    }
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
