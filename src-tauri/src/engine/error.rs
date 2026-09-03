//! Engine error type. Commands return `EngineResult<T>`; the `Err` arm
//! serializes to `{ kind, message }` so the UI can offer the right next step
//! (retry a transient failure, re-enter a refused key, pick another model).

use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};

#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("{0}")]
    Msg(String),

    #[error("No folder is open yet choose one with /open.")]
    NoWorkspace,

    #[error("unknown source: {0}")]
    UnknownSource(String),

    #[error("query rejected: {0}")]
    Forbidden(String),

    #[cfg(feature = "duckdb")]
    #[error(transparent)]
    Duck(#[from] duckdb::Error),

    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),

    #[error("{context}: {source}")]
    Io {
        context: String,
        #[source]
        source: std::io::Error,
    },

    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

impl EngineError {
    pub fn msg(s: impl Into<String>) -> Self {
        Self::Msg(s.into())
    }

    pub fn io(context: impl Into<String>, source: std::io::Error) -> Self {
        Self::Io {
            context: context.into(),
            source,
        }
    }

    /// A coarse category for the UI: `transient` (offer Retry), `auth` /
    /// `payment` (offer /login or /model), `no_model`, `no_workspace`,
    /// `bad_input`, `query`, `internal`.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::NoWorkspace => "no_workspace",
            Self::Forbidden(_) | Self::UnknownSource(_) | Self::Json(_) => "bad_input",
            Self::Io { .. } => "internal",
            Self::Sqlite(_) => "query",
            #[cfg(feature = "duckdb")]
            Self::Duck(_) => "query",
            Self::Msg(m) => {
                let l = m.to_lowercase();
                if l.contains("refused the api key") || l.contains("unauthorized") {
                    "auth"
                } else if l.contains("plan doesn't cover") || l.contains("payment required") {
                    "payment"
                } else if l.contains("no model chosen") || l.contains("isn't downloaded in ollama") {
                    "no_model"
                } else if l.contains("retrying")
                    || l.contains("limiting how many requests")
                    || l.contains("didn't respond within")
                    || l.contains("kept failing")
                    || l.contains("connection dropped")
                    || l.contains("couldn't reach the model")
                    || l.contains("connection problem")
                {
                    "transient"
                } else {
                    "internal"
                }
            }
        }
    }
}

impl Serialize for EngineError {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut st = s.serialize_struct("EngineError", 2)?;
        st.serialize_field("kind", self.kind())?;
        st.serialize_field("message", &self.to_string())?;
        st.end()
    }
}

pub type EngineResult<T> = Result<T, EngineError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_maps_known_messages() {
        assert_eq!(EngineError::NoWorkspace.kind(), "no_workspace");
        assert_eq!(EngineError::Forbidden("x".into()).kind(), "bad_input");
        assert_eq!(
            EngineError::msg("OpenAI refused the API key. Run /login").kind(),
            "auth"
        );
        assert_eq!(
            EngineError::msg("your Vercel plan doesn't cover the model \"x\"").kind(),
            "payment"
        );
        assert_eq!(
            EngineError::msg("No model chosen yet. Run /model").kind(),
            "no_model"
        );
        assert_eq!(
            EngineError::msg("the model is busy retrying in 3s").kind(),
            "transient"
        );
        assert_eq!(EngineError::msg("something odd happened").kind(), "internal");
    }

    #[test]
    fn serializes_as_kind_and_message() {
        let j = serde_json::to_value(EngineError::NoWorkspace).unwrap();
        assert_eq!(j["kind"], "no_workspace");
        assert!(j["message"].as_str().unwrap().contains("/open"));
    }
}
