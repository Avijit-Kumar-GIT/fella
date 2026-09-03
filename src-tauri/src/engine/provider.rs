//! The set of model providers Fella knows how to talk to.
//!
//! Adding a provider is a single row in [`PROVIDERS`] that is the whole
//! "bring your own provider" surface. A row only needs a new [`Wire`] or
//! [`AuthKind`] variant if it speaks a protocol we don't already handle.

/// How a provider is authenticated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthKind {
    /// No credential at all (a local server).
    None,
    /// A bearer API key the user pastes in.
    ApiKey,
}

impl AuthKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            AuthKind::None => "none",
            AuthKind::ApiKey => "key",
        }
    }
}

/// Which HTTP shape the provider speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wire {
    /// `POST {base_url}/api/chat`, `POST {base_url}/api/embed` (Ollama).
    Ollama,
    /// `POST {base_url}/chat/completions`, `/embeddings` so `base_url` is the
    /// API root *including* any `/v1` (e.g. `https://api.openai.com/v1`).
    OpenAi,
}

#[derive(Debug, Clone)]
pub struct Provider {
    pub id: &'static str,
    pub display: &'static str,
    pub auth: AuthKind,
    pub base_url: &'static str,
    /// Chat model set on `/login`. Empty = keep whatever is configured.
    pub default_model: &'static str,
    /// Embedding model set on `/login`. Empty = provider has no embeddings.
    pub default_embed_model: &'static str,
    pub wire: Wire,
    /// Whether the provider exposes an embeddings endpoint. Currently unused
    /// document search reads files directly (`grep_files`/`read_file`), not
    /// embeddings kept for a future feature that wants it.
    pub embeddings: bool,
    /// Page where a user gets an API key (shown by `/login`). Empty when N/A.
    pub get_key_url: &'static str,
}

/// The baseline providers. `custom` preserves the pre-registry
/// "`/model base_url` + `/model key`" path for anything not listed here.
///
/// Hosted model names drift; if a `default_model` 404s, `/model <name>` fixes
/// it and `docs/DEV_SETUP.md` carries the current-good values.
pub const PROVIDERS: &[Provider] = &[
    Provider {
        id: "ollama",
        display: "Ollama (local)",
        auth: AuthKind::None,
        base_url: "http://localhost:11434",
        default_model: "llama3.1",
        default_embed_model: "nomic-embed-text",
        wire: Wire::Ollama,
        embeddings: true,
        get_key_url: "",
    },
    Provider {
        id: "openai",
        display: "OpenAI",
        auth: AuthKind::ApiKey,
        base_url: "https://api.openai.com/v1",
        default_model: "gpt-4o-mini",
        default_embed_model: "text-embedding-3-small",
        wire: Wire::OpenAi,
        embeddings: true,
        get_key_url: "https://platform.openai.com/api-keys",
    },
    Provider {
        id: "vercel",
        display: "Vercel AI Gateway",
        auth: AuthKind::ApiKey,
        base_url: "https://ai-gateway.vercel.sh/v1",
        // Chat ids drift and are provider-namespaced (`creator/model`); leave it
        // unset so `/login` prompts the user to pick one from `/models`.
        default_model: "",
        default_embed_model: "openai/text-embedding-3-small",
        wire: Wire::OpenAi,
        embeddings: true,
        get_key_url: "https://vercel.com/dashboard/ai-gateway/api-keys",
    },
    Provider {
        id: "xai",
        display: "xAI (Grok)",
        auth: AuthKind::ApiKey,
        base_url: "https://api.x.ai/v1",
        default_model: "grok-2-latest",
        default_embed_model: "",
        wire: Wire::OpenAi,
        embeddings: false,
        get_key_url: "https://console.x.ai",
    },
    Provider {
        id: "ollama-cloud",
        display: "Ollama Cloud",
        auth: AuthKind::ApiKey,
        // Same wire as local Ollama, just hosted and behind a key. `/api/tags`
        // with the bearer lists the models your account can run; browse the
        // catalogue at ollama.com/search?c=cloud. No `default_model` pick
        // one with `/model` after signing in.
        base_url: "https://ollama.com",
        default_model: "",
        default_embed_model: "",
        wire: Wire::Ollama,
        embeddings: false,
        get_key_url: "https://ollama.com/settings/keys",
    },
    Provider {
        id: "openrouter",
        display: "OpenRouter",
        auth: AuthKind::ApiKey,
        base_url: "https://openrouter.ai/api/v1",
        // One key, a large catalogue of `creator/model` ids that drift leave it
        // unset so `/login` prompts a `/model` pick, same as the Vercel row.
        default_model: "",
        default_embed_model: "",
        wire: Wire::OpenAi,
        embeddings: false,
        get_key_url: "https://openrouter.ai/keys",
    },
    Provider {
        id: "custom",
        display: "Custom (OpenAI-compatible)",
        auth: AuthKind::ApiKey,
        base_url: "",
        default_model: "",
        default_embed_model: "",
        wire: Wire::OpenAi,
        embeddings: false,
        get_key_url: "",
    },
];

/// The provider Fella starts on before anything is configured.
pub const DEFAULT_ID: &str = "ollama";

/// Look a provider up by id, after normalizing legacy names.
pub fn get(id: &str) -> Option<&'static Provider> {
    let id = normalize_id(id);
    PROVIDERS.iter().find(|p| p.id == id)
}

/// Map values stored by older builds onto the registry.
pub fn normalize_id(stored: &str) -> &str {
    match stored {
        "" => DEFAULT_ID,
        "openai-compatible" => "custom",
        other => other,
    }
}

/// The wire format for a stored provider id (unknown ⇒ assume OpenAI-compatible).
pub fn wire_of(stored: &str) -> Wire {
    get(stored).map(|p| p.wire).unwrap_or(Wire::OpenAi)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_names_normalize() {
        assert_eq!(normalize_id("openai-compatible"), "custom");
        assert_eq!(normalize_id(""), "ollama");
        assert_eq!(normalize_id("openai"), "openai");
    }

    #[test]
    fn registry_lookups() {
        assert_eq!(get("ollama").unwrap().wire, Wire::Ollama);
        assert_eq!(get("vercel").unwrap().wire, Wire::OpenAi);
        assert!(get("vercel").unwrap().embeddings);
        assert!(!get("xai").unwrap().embeddings);
        assert_eq!(get("openrouter").unwrap().wire, Wire::OpenAi);
        assert!(!get("openrouter").unwrap().embeddings);
        assert_eq!(get("openrouter").unwrap().auth, AuthKind::ApiKey);
        // Ollama Cloud speaks the Ollama wire but needs a key.
        assert_eq!(get("ollama-cloud").unwrap().wire, Wire::Ollama);
        assert_eq!(get("ollama-cloud").unwrap().auth, AuthKind::ApiKey);
        assert!(get("openai").unwrap().embeddings);
        // legacy id resolves through the registry
        assert_eq!(get("openai-compatible").unwrap().id, "custom");
        assert_eq!(wire_of("something-unknown"), Wire::OpenAi);
    }

    #[test]
    fn every_apikey_provider_has_a_key_url_except_custom() {
        for p in PROVIDERS {
            if p.auth == AuthKind::ApiKey && p.id != "custom" {
                assert!(!p.get_key_url.is_empty(), "{} lacks get_key_url", p.id);
            }
        }
    }
}
