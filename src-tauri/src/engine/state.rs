//! `EngineState` the shared runtime object every command handler borrows.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use serde::Serialize;

use crate::engine::agent;
use crate::engine::catalog::{self, Catalog, SourceInfo, SourceKind};
use crate::engine::data::{self, DataEngine, PythonBridge, DEFAULT_ROW_CAP};
use crate::engine::error::{EngineError, EngineResult};
use crate::engine::evidence::{Answer, AskEvent};
use crate::engine::extensions::{self, InstalledPack};
use crate::engine::ingest::docs;
use crate::engine::llm::{LlmClient, ProviderHealth};
use crate::engine::provider::{self, AuthKind, PROVIDERS};
use crate::engine::pyexec;
use crate::engine::secrets::Secrets;
use crate::engine::sqlite::{self, Settings};
use crate::engine::tools::Registry;
use crate::engine::update;

pub struct EngineState {
    data: Mutex<Box<dyn DataEngine>>,
    sqlite: Mutex<rusqlite::Connection>,
    inner: Mutex<Inner>,
    http: reqwest::Client,
    /// API keys / tokens, keyed by provider. Kept out of the SQLite file.
    secrets: Secrets,
    /// The app data dir. Home to `fella.db`, `auth.json`, and archived
    /// conversations under `conversations/`.
    data_dir: PathBuf,
    /// One stop-flag per in-flight conversation, so `/stop` in one tab doesn't
    /// cancel another tab's run. Keyed by `conversation_id`; an entry is created
    /// when `ask` starts and removed when it finishes.
    cancel: Mutex<HashMap<String, Arc<AtomicBool>>>,
    /// Extracted PDF text, keyed by path. PDF parsing is slow and `grep_files` /
    /// `read_file` can each hit the same file many times in one run. Entries are
    /// invalidated when the file's mtime changes. Text files aren't cached they
    /// stream.
    doc_cache: Mutex<HashMap<String, (u64, Arc<str>)>>,
}

/// Where past conversations are archived, and how many are there.
#[derive(Debug, Serialize)]
pub struct ConversationsInfo {
    pub path: String,
    pub count: usize,
}

/// One row of `/history`'s list enough to recognize and pick a past
/// conversation without knowing its id.
#[derive(Debug, Serialize)]
pub struct ConversationSummary {
    pub id: String,
    pub saved_at_ms: i64,
    pub workspace: Option<String>,
    pub preview: String,
    pub message_count: usize,
}

/// One row of the provider list shown by `/login` and `/auth`.
#[derive(Debug, Serialize)]
pub struct ProviderInfo {
    pub id: String,
    pub display: String,
    /// `"none"` or `"key"`.
    pub auth: String,
    pub base_url: String,
    pub get_key_url: String,
    pub embeddings: bool,
    /// A credential is present, or the provider needs none.
    pub authed: bool,
    /// This is the currently-selected provider.
    pub current: bool,
}

#[derive(Default)]
struct Inner {
    workspace: Option<PathBuf>,
    sources: Vec<SourceInfo>,
    /// Contents of `fella.md` at the workspace root, if present. User-written
    /// context fed to the system prompt alongside enabled skill packs.
    user_md: Option<String>,
    /// Distilled context from earlier questions, one entry per conversation
    /// (tab). Bounded by `SESSION_CAP`, LRU by `last_used`. Cleared on workspace
    /// (re)open; an entry is dropped when its tab is closed
    /// (`forget_conversation`).
    sessions: HashMap<String, SessionMemory>,
    /// Monotonic counter stamped onto `SessionMemory::last_used` so the least
    /// recently asked conversation can be evicted when `sessions` is full.
    session_tick: u64,
    /// Rendered `schema_block()` for the current sources - it re-samples every
    /// table, so we build it once per workspace and clear it on (re)open or
    /// when `describe_source` refreshes a table's stats.
    schema_cache: Option<String>,
    /// Files the last scan/ingest noticed but couldn't load.
    skipped: Vec<catalog::SkippedFile>,
}

/// Most conversations we keep distilled memory for at once.
const SESSION_CAP: usize = 24;

/// A few earlier turns of one conversation, distilled so a follow-up question
/// doesn't have to rediscover the schema. In-memory only.
#[derive(Default)]
struct SessionMemory {
    turns: Vec<TurnDigest>,
    last_used: u64,
}

struct TurnDigest {
    question: String,
    headline: String,
    queries: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct QueryResult {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<serde_json::Value>>,
    pub row_count: usize,
    pub ms: u64,
    pub truncated: bool,
}

/// One `grep_files` match.
#[derive(Debug, Serialize)]
pub struct GrepHit {
    pub source: String,
    pub line: usize,
    pub text: String,
}

/// `read_file` truncates a document past this many characters so one huge
/// PDF can't blow the context window `grep_files` can find a spot in a
/// bigger file first.
const READ_FILE_CHAR_CAP: usize = 12_000;

impl EngineState {
    pub fn new(data_dir: &Path) -> EngineResult<Self> {
        // reqwest is built with `rustls-no-provider`; install `ring` once
        // (process-global). Doing it here covers both the app and the tests.
        static CRYPTO: std::sync::Once = std::sync::Once::new();
        CRYPTO.call_once(|| {
            let _ = rustls::crypto::ring::default_provider().install_default();
        });

        std::fs::create_dir_all(data_dir)
            .map_err(|e| EngineError::io(format!("create {}", data_dir.display()), e))?;
        let data = data::open_engine(data_dir)?;
        let sqlite = open_or_recover(&data_dir.join("fella.db"));
        let secrets = Secrets::new(data_dir);
        migrate_legacy_key(&sqlite, &secrets);
        reconcile_provider(&sqlite, &secrets);
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(180))
            .connect_timeout(std::time::Duration::from_secs(10))
            // Keep a few idle sockets so a multi-turn question doesn't pay a
            // fresh TLS handshake on every model call. Retire them after 20 s so
            // a hosted LB that silently drops an idle connection isn't hit on
            // reuse (a dropped connection is a retryable `is_connect` error).
            .pool_max_idle_per_host(4)
            .pool_idle_timeout(std::time::Duration::from_secs(20))
            .build()
            // A default client would silently drop the timeouts above and stall
            // the UI on a wedged request; a build failure here is not
            // recoverable, so surface it.
            .expect("build HTTP client");
        Ok(Self {
            data: Mutex::new(data),
            sqlite: Mutex::new(sqlite),
            inner: Mutex::new(Inner::default()),
            http,
            secrets,
            data_dir: data_dir.to_path_buf(),
            cancel: Mutex::new(HashMap::new()),
            doc_cache: Mutex::new(HashMap::new()),
        })
    }

    /// Stop the in-progress `ask()` for one conversation (if any).
    pub fn cancel_run(&self, conversation_id: &str) {
        if let Some(flag) = self
            .cancel
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(conversation_id)
        {
            flag.store(true, Ordering::Relaxed);
        }
    }

    /// Drop a conversation's distilled memory and stop-flag it's a closed tab.
    pub fn forget_conversation(&self, conversation_id: &str) {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .sessions
            .remove(conversation_id);
        self.cancel
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(conversation_id);
    }

    /// Write a finished transcript to `<data_dir>/conversations/`. `body` is the
    /// JSON the UI assembled (`{id, saved_at_ms, workspace, messages}`); it is
    /// re-serialized pretty so the file reads well when opened by hand. Returns
    /// the file path. If an archive for `id` already exists (a double restart,
    /// a retried call) its path is returned without rewriting.
    pub fn archive_conversation(&self, id: &str, body: &str) -> EngineResult<String> {
        let value: serde_json::Value = serde_json::from_str(body)
            .map_err(|e| EngineError::msg(format!("conversation body is not JSON: {e}")))?;

        let slug: String = id
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .take(32)
            .collect();
        let slug = if slug.is_empty() { "unknown".to_string() } else { slug };
        let suffix = format!("_{slug}.json");

        let dir = self.data_dir.join("conversations");
        std::fs::create_dir_all(&dir)
            .map_err(|e| EngineError::io(format!("create {}", dir.display()), e))?;

        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                if entry.file_name().to_string_lossy().ends_with(&suffix) {
                    return Ok(entry.path().display().to_string());
                }
            }
        }

        let ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        let path = dir.join(format!("conv_{ms}{suffix}"));
        let pretty = serde_json::to_string_pretty(&value).unwrap_or_else(|_| body.to_string());

        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, &pretty)
            .map_err(|e| EngineError::io(format!("write {}", tmp.display()), e))?;
        std::fs::rename(&tmp, &path)
            .map_err(|e| EngineError::io(format!("replace {}", path.display()), e))?;

        Ok(path.display().to_string())
    }

    /// The conversations directory and how many `*.json` archives it holds.
    pub fn conversations_info(&self) -> ConversationsInfo {
        let dir = self.data_dir.join("conversations");
        let count = std::fs::read_dir(&dir)
            .map(|entries| {
                entries
                    .flatten()
                    .filter(|e| {
                        e.file_name()
                            .to_string_lossy()
                            .ends_with(".json")
                    })
                    .count()
            })
            .unwrap_or(0);
        ConversationsInfo {
            path: dir.display().to_string(),
            count,
        }
    }

    /// Every archived conversation, newest first, with enough to pick one
    /// from without knowing its id: when, what folder it was about, its
    /// first question, and how long it ran. Best-effort a file that
    /// doesn't parse as the expected shape is skipped, not an error, since
    /// this is a browse list, not a data-integrity check.
    pub fn conversations_list(&self) -> Vec<ConversationSummary> {
        let dir = self.data_dir.join("conversations");
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return Vec::new();
        };
        let mut out: Vec<ConversationSummary> = entries
            .flatten()
            .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("json"))
            .filter_map(|e| std::fs::read_to_string(e.path()).ok())
            .filter_map(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
            .filter_map(|v| {
                let id = v.get("id")?.as_str()?.to_string();
                let saved_at_ms = v.get("saved_at_ms").and_then(|x| x.as_i64()).unwrap_or(0);
                let workspace = v
                    .get("workspace")
                    .and_then(|x| x.as_str())
                    .map(str::to_string);
                let messages = v.get("messages")?.as_array()?;
                let preview = messages
                    .iter()
                    .find(|m| m.get("role").and_then(|r| r.as_str()) == Some("user"))
                    .and_then(|m| m.get("text")?.as_str())
                    .map(|s| cap_chars(s, 80))
                    .unwrap_or_else(|| "(empty conversation)".to_string());
                Some(ConversationSummary {
                    id,
                    saved_at_ms,
                    workspace,
                    preview,
                    message_count: messages.len(),
                })
            })
            .collect();
        out.sort_by_key(|c| std::cmp::Reverse(c.saved_at_ms));
        out
    }

    /// Raw JSON text of one archived conversation `{id, workspace, messages}`,
    /// by id looked up the same slug-suffix way `archive_conversation` writes
    /// it. The frontend parses this itself (same shape it wrote when it was
    /// archived); no need to round-trip every message/evidence field through
    /// a typed Rust struct just to pass it straight back through.
    pub fn conversation_load(&self, id: &str) -> EngineResult<String> {
        let dir = self.data_dir.join("conversations");
        let slug: String = id
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .take(32)
            .collect();
        let suffix = format!("_{slug}.json");
        let entries = std::fs::read_dir(&dir)
            .map_err(|e| EngineError::io(format!("read {}", dir.display()), e))?;
        for entry in entries.flatten() {
            if entry.file_name().to_string_lossy().ends_with(&suffix) {
                return std::fs::read_to_string(entry.path())
                    .map_err(|e| EngineError::io(format!("read {}", entry.path().display()), e));
            }
        }
        Err(EngineError::msg(
            "that conversation couldn't be found it may have been deleted",
        ))
    }

    pub fn catalog(&self) -> Catalog {
        let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        Catalog {
            workspace: inner.workspace.as_ref().map(|p| p.display().to_string()),
            sources: inner.sources.clone(),
            skipped: inner.skipped.clone(),
        }
    }

    /// Compact one-line-per-table schema: `view("col" TYPE, ...)`. Used to echo
    /// the real schema back to the model after a SQL error.
    pub(crate) fn schema_oneline(&self) -> String {
        let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let mut out = String::new();
        for s in inner.sources.iter().filter(|s| s.view.is_some()) {
            let cols = s
                .columns
                .as_ref()
                .map(|cs| {
                    cs.iter()
                        .map(|c| format!("\"{}\" {}", c.name, c.type_))
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .unwrap_or_default();
            out.push_str(&format!("  {}({cols})\n", s.view.as_deref().unwrap_or("")));
        }
        out
    }

    /// The table/document digest embedded in the system prompt. Within a modest
    /// budget it lists every column with its type and any ingest note, and a few
    /// sample rows for a small workspace; a large/messy folder falls back to
    /// names + shape only (keeps the prompt small).
    pub(crate) fn schema_block(&self) -> String {
        // Clone out first: `self.sample` re-locks `inner`.
        let sources = {
            let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(cached) = &inner.schema_cache {
                return cached.clone();
            }
            inner.sources.clone()
        };
        let tables: Vec<&SourceInfo> = sources.iter().filter(|s| s.view.is_some()).collect();
        let total_cols: usize = tables
            .iter()
            .map(|s| s.columns.as_ref().map(|c| c.len()).unwrap_or(0))
            .sum();
        let full = tables.len() <= 12 && total_cols <= 60;
        let with_samples = full && tables.len() <= 4;

        let mut p = String::new();
        if tables.is_empty() {
            p.push_str("No tables were detected.\n");
        } else if full {
            p.push_str("Tables (columns and types shown; use sample_rows for values):\n");
            for s in &tables {
                let view = s.view.as_deref().unwrap_or("");
                let rows = s.row_count.map(|n| n.to_string()).unwrap_or_else(|| "?".into());
                p.push_str(&format!("  {view}  ({rows} rows)\n"));
                if let Some(note) = &s.note {
                    p.push_str(&format!("    note: {note}\n"));
                }
                if let Some(cols) = &s.columns {
                    for c in cols {
                        match &c.note {
                            Some(n) => {
                                p.push_str(&format!("    \"{}\" {}  [{}]\n", c.name, c.type_, n))
                            }
                            None => p.push_str(&format!("    \"{}\" {}\n", c.name, c.type_)),
                        }
                    }
                }
                if with_samples {
                    if let Ok(sample) = self.sample(view, 3) {
                        for line in mini_table(&sample) {
                            p.push_str(&format!("    {line}\n"));
                        }
                    }
                }
            }
        } else {
            p.push_str("Tables (use describe_schema or sample_rows for their columns):\n");
            for s in &tables {
                let ncols = s.columns.as_ref().map(|c| c.len()).unwrap_or(0);
                p.push_str(&format!(
                    "  {}  {} rows, {ncols} columns\n",
                    s.view.as_deref().unwrap_or(""),
                    s.row_count.map(|n| n.to_string()).unwrap_or_else(|| "?".into()),
                ));
            }
        }

        let docs: Vec<&SourceInfo> = sources.iter().filter(|s| s.view.is_none()).collect();
        if !docs.is_empty() {
            p.push_str("Documents (list_files/grep_files/read_file):\n");
            for d in docs {
                match &d.synopsis {
                    Some(syn) => p.push_str(&format!("  {}  {}\n", d.name, syn)),
                    None => p.push_str(&format!("  {}\n", d.name)),
                }
            }
        }
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).schema_cache = Some(p.clone());
        p
    }

    /// The "Earlier in this conversation" block for `conversation_id`, or `None`
    /// on that conversation's first turn.
    pub(crate) fn session_block(&self, conversation_id: &str) -> Option<String> {
        let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let turns = match inner.sessions.get(conversation_id) {
            Some(s) if !s.turns.is_empty() => &s.turns,
            _ => return None,
        };
        let mut p = String::from("Earlier in this conversation (reuse what still applies):\n");
        for t in turns {
            p.push_str(&format!("- Q: \"{}\"  A: \"{}\"\n", t.question, t.headline));
            for q in &t.queries {
                p.push_str(&format!("  used: {q}\n"));
            }
        }
        Some(p)
    }

    /// User-written context for the system prompt: the workspace `fella.md`
    /// followed by the Markdown of every enabled `skill` pack.
    pub fn user_context(&self) -> Vec<String> {
        let mut out = Vec::new();
        if let Some(md) = self.inner.lock().unwrap_or_else(|e| e.into_inner()).user_md.clone() {
            out.push(md);
        }
        out.extend(extensions::enabled_skill_texts(
            &self.data_dir,
            &self.sqlite.lock().unwrap_or_else(|e| e.into_inner()),
        ));
        out
    }

    // --- packs (installed extensions) ------------------------------------

    pub fn packs_list(&self) -> Vec<InstalledPack> {
        let mut list = extensions::list(&self.sqlite.lock().unwrap_or_else(|e| e.into_inner()));
        for p in &mut list {
            if p.kind == "mcp" && !self.mcp_has_token(&p.id) {
                p.needs_token = true;
            }
        }
        list
    }

    pub fn packs_add(&self, src: &Path) -> EngineResult<Vec<InstalledPack>> {
        let conn = self.sqlite.lock().unwrap_or_else(|e| e.into_inner());
        extensions::install_local(&self.data_dir, &conn, src)?;
        Ok(extensions::list(&conn))
    }

    pub fn packs_remove(&self, id: &str) -> EngineResult<Vec<InstalledPack>> {
        let conn = self.sqlite.lock().unwrap_or_else(|e| e.into_inner());
        extensions::remove(&self.data_dir, &conn, id)?;
        Ok(extensions::list(&conn))
    }

    pub fn packs_set_enabled(
        &self,
        id: &str,
        enabled: bool,
    ) -> EngineResult<Vec<InstalledPack>> {
        let conn = self.sqlite.lock().unwrap_or_else(|e| e.into_inner());
        extensions::set_enabled(&conn, id, enabled)?;
        Ok(extensions::list(&conn))
    }

    /// Install a pack from the marketplace by id (files are SHA-256 checked
    /// against the catalog). Network happens before the DB lock is taken.
    pub async fn packs_install(&self, id: &str) -> EngineResult<Vec<InstalledPack>> {
        let downloaded =
            extensions::download_pack(&self.http, &extensions::catalog_url(), id).await?;
        let conn = self.sqlite.lock().unwrap_or_else(|e| e.into_inner());
        extensions::install_downloaded(&self.data_dir, &conn, &downloaded)?;
        Ok(extensions::list(&conn))
    }

    /// Check the latest GitHub release and, if it's newer, download +
    /// checksum-verify the right installer for this OS and hand off to it
    /// (the app exits as part of that handoff see `engine::update`).
    /// Returns without exiting when already up to date, or on any failure
    /// before the handoff.
    pub async fn update(&self, app: tauri::AppHandle) -> EngineResult<update::UpdateStatus> {
        update::apply(&self.http, app).await
    }

    /// CSS token map of the active theme pack, for the frontend to apply.
    pub fn packs_theme(&self) -> Option<std::collections::BTreeMap<String, String>> {
        extensions::active_theme_tokens(&self.data_dir, &self.sqlite.lock().unwrap_or_else(|e| e.into_inner()))
    }

    pub fn settings(&self) -> Settings {
        let mut s = sqlite::load_settings(&self.sqlite.lock().unwrap_or_else(|e| e.into_inner()));
        let id = provider::normalize_id(&s.provider);
        let needs_none = provider::get(id).map(|p| p.auth == AuthKind::None).unwrap_or(false);
        s.has_credential = needs_none || self.secrets.has(id);
        s
    }

    pub fn save_settings(
        &self,
        patch: &serde_json::Map<String, serde_json::Value>,
    ) -> EngineResult<Settings> {
        let mut patch = patch.clone();

        // Changing `provider` on its own (the `/model provider <x>` path) should
        // move `base_url` / `model` / `embed_model` to that provider's defaults
        // for any of them not set in the same patch just like `/login` does.
        // Without this you'd be left pointed at the previous provider's URL.
        if let Some(new_id) = patch.get("provider").and_then(|v| v.as_str()) {
            if let Some(p) = provider::get(new_id) {
                let switching = provider::normalize_id(&self.settings().provider) != p.id;
                if !patch.contains_key("base_url") && !p.base_url.is_empty() {
                    patch.insert("base_url".into(), p.base_url.into());
                }
                if !patch.contains_key("model") {
                    if !p.default_model.is_empty() {
                        patch.insert("model".into(), p.default_model.into());
                    } else if switching {
                        patch.insert("model".into(), "".into());
                    }
                }
                if !patch.contains_key("embed_model") && !p.default_embed_model.is_empty() {
                    patch.insert("embed_model".into(), p.default_embed_model.into());
                }
            }
        }

        sqlite::save_settings(&self.sqlite.lock().unwrap_or_else(|e| e.into_inner()), &patch)?;
        // A model or provider change: pre-load the new local model.
        self.warm_model();
        Ok(self.settings())
    }

    /// The providers Fella has built-in support for, and whether each is
    /// signed in. Drives `/login` and `/auth`.
    pub fn list_providers(&self) -> Vec<ProviderInfo> {
        let current = provider::normalize_id(&self.settings().provider).to_string();
        PROVIDERS
            .iter()
            .map(|p| ProviderInfo {
                id: p.id.to_string(),
                display: p.display.to_string(),
                auth: p.auth.as_str().to_string(),
                base_url: p.base_url.to_string(),
                get_key_url: p.get_key_url.to_string(),
                embeddings: p.embeddings,
                authed: p.auth == AuthKind::None || self.secrets.has(p.id),
                current: p.id == current,
            })
            .collect()
    }

    /// Store an API key for `provider_id` and make it the active provider,
    /// moving `base_url` / `model` / `embed_model` to that provider's defaults.
    pub fn set_api_key(&self, provider_id: &str, key: &str) -> EngineResult<Settings> {
        let p = provider::get(provider_id)
            .ok_or_else(|| EngineError::msg(format!("unknown provider: {provider_id}")))?;
        if p.auth != AuthKind::ApiKey {
            return Err(EngineError::msg(format!("{} does not use an API key", p.display)));
        }
        let key = key.trim();
        if key.is_empty() {
            return Err(EngineError::msg("that key is empty"));
        }
        self.secrets.set_api_key(p.id, key)?;

        let switching = provider::normalize_id(&self.settings().provider) != p.id;

        let mut patch = serde_json::Map::new();
        patch.insert("provider".into(), p.id.into());
        if !p.base_url.is_empty() {
            patch.insert("base_url".into(), p.base_url.into());
        }
        if !p.default_model.is_empty() {
            patch.insert("model".into(), p.default_model.into());
        } else if switching {
            // New provider with no default (e.g. a gateway with drifting ids):
            // clear the model so we don't inherit the previous provider's.
            patch.insert("model".into(), "".into());
        }
        if !p.default_embed_model.is_empty() {
            patch.insert("embed_model".into(), p.default_embed_model.into());
        }
        self.save_settings(&patch)
    }

    /// Forget the stored credential for `provider_id`. If it was the provider
    /// Fella is currently on, also drop `base_url` / `model` / `embed_model`
    /// back to the local default so the app isn't left pointed at a service it
    /// can no longer reach. Logging out of any other provider only forgets its
    /// key.
    pub fn logout(&self, provider_id: &str) -> EngineResult<Settings> {
        let id = provider::normalize_id(provider_id);
        let is_current = provider::normalize_id(&self.settings().provider) == id;
        let had_key = self.secrets.has(id);

        // Only a genuine typo (not registered, not the active provider, no key
        // on file) is an error otherwise there's something real to clear.
        if provider::get(id).is_none() && !is_current && !had_key {
            return Err(EngineError::msg(format!("unknown provider: {provider_id}")));
        }
        self.secrets.clear(id)?;

        if is_current {
            let d = provider::get(provider::DEFAULT_ID)
                .expect("the default provider is always in the registry");
            let mut patch = serde_json::Map::new();
            patch.insert("provider".into(), d.id.into());
            patch.insert("base_url".into(), d.base_url.into());
            patch.insert("model".into(), d.default_model.into());
            patch.insert("embed_model".into(), d.default_embed_model.into());
            return self.save_settings(&patch);
        }

        Ok(self.settings())
    }

    /// Point the engine at a folder: scan it, load every tabular file as a
    /// table, and replace the catalog.
    pub fn open_workspace(&self, path: &Path) -> EngineResult<Catalog> {
        if !path.is_dir() {
            return Err(EngineError::msg(format!(
                "That doesn't look like a folder: {}",
                path.display()
            )));
        }
        let (scanned, mut skipped) = catalog::scan(path)?;

        // Lock order is always inner -> data, and never both at once.
        let old_views: Vec<String> = {
            let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
            inner.sources.iter().filter_map(|s| s.view.clone()).collect()
        };

        let mut sources = Vec::with_capacity(scanned.len());
        {
            // Clear the previous workspace's views in one short critical section.
            let mut data = self.data.lock().unwrap_or_else(|e| e.into_inner());
            for v in &old_views {
                data.drop_source(v);
            }
        }

        {
            let mut used: HashSet<String> = HashSet::new();
            // Each text-doc synopsis is a file open + short read; cap how many we
            // do so a folder with thousands of notes doesn't pay thousands of
            // extra reads on every open. Beyond the cap, docs list without one.
            let mut synopsis_budget: usize = 250;
            for f in scanned {
                let name = f
                    .path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("source")
                    .to_string();
                let stem = f.path.file_stem().and_then(|s| s.to_str()).unwrap_or("source");
                let path_str = f.path.display().to_string();

                let synopsis = if f.kind == SourceKind::Text && synopsis_budget > 0 {
                    synopsis_budget -= 1;
                    first_line_synopsis(&path_str)
                } else {
                    None
                };
                let mut info = SourceInfo {
                    name,
                    path: path_str.clone(),
                    kind: f.kind,
                    view: None,
                    row_count: None,
                    columns: None,
                    size_bytes: f.size_bytes,
                    mtime: f.mtime,
                    synopsis,
                    note: None,
                };

                match f.kind {
                    #[cfg(feature = "xlsx")]
                    catalog::SourceKind::Xlsx => {
                        // Lock the data engine only for this one file's ingest,
                        // so a question asked mid-(re)scan isn't blocked for the
                        // whole folder just for one big file.
                        let mut data = self.data.lock().unwrap_or_else(|e| e.into_inner());
                        match crate::engine::ingest::excel::ingest_workbook(
                            &mut **data,
                            &path_str,
                            stem,
                            &mut used,
                        ) {
                            Ok((sheets, _)) if !sheets.is_empty() => {
                                for sh in sheets {
                                    sources.push(SourceInfo {
                                        name: format!("{} \u{b7} {}", info.name, sh.sheet),
                                        path: path_str.clone(),
                                        kind: f.kind,
                                        view: Some(sh.view),
                                        row_count: Some(sh.row_count),
                                        columns: Some(sh.columns),
                                        size_bytes: f.size_bytes,
                                        mtime: f.mtime,
                                        synopsis: None,
                                        note: sh.note,
                                    });
                                }
                                continue;
                            }
                            Ok((_, reasons)) => {
                                log::warn!("{path_str}: no usable sheets ({reasons:?})");
                                let reason = if reasons.is_empty() {
                                    "no readable sheets".to_string()
                                } else {
                                    format!("no readable sheets ({})", reasons.join("; "))
                                };
                                skipped.push(catalog::SkippedFile {
                                    name: info.name.clone(),
                                    reason,
                                });
                                continue;
                            }
                            Err(e) => {
                                log::warn!("skipping {path_str}: {e}");
                                skipped.push(catalog::SkippedFile {
                                    name: info.name.clone(),
                                    reason: format!("couldn't be opened as a spreadsheet: {e}"),
                                });
                                continue;
                            }
                        }
                    }
                    k if k.is_tabular() && k != SourceKind::Xlsx => {
                        let view = catalog::unique_view_name(stem, &mut used);
                        let mut data = self.data.lock().unwrap_or_else(|e| e.into_inner());
                        match data.add_source(&view, f.kind, &path_str) {
                            Ok(load) => {
                                info.row_count = Some(load.row_count);
                                info.columns = Some(load.columns);
                                info.note = load.note;
                                info.view = Some(view);
                            }
                            Err(e) => {
                                log::warn!("skipping {path_str}: {e}");
                                skipped.push(catalog::SkippedFile {
                                    name: info.name.clone(),
                                    reason: "couldn't be read as a table".into(),
                                });
                                continue;
                            }
                        }
                    }
                    _ => {}
                }
                sources.push(info);
            }
        }

        self.persist_sources(path, &sources);

        // `fella.md` at the root is optional user context, not a data file.
        let user_md = std::fs::read_to_string(path.join("fella.md"))
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        skipped.sort_by(|a, b| a.name.cmp(&b.name));
        skipped.dedup_by(|a, b| a.name == b.name);
        {
            let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
            inner.workspace = Some(path.to_path_buf());
            inner.sources = sources;
            inner.skipped = skipped;
            inner.user_md = user_md;
            // The sources changed, so every conversation's distilled memory
            // (schema hints, prior queries) is now stale.
            inner.sessions.clear();
            inner.schema_cache = None;
        }
        // The user is about to ask something: warm the model now so the first
        // question doesn't wait on a cold load.
        self.warm_model();
        Ok(self.catalog())
    }

    /// Re-open the current workspace (used by `/reindex`).
    pub fn reindex(&self) -> EngineResult<Catalog> {
        let ws = {
            let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
            inner.workspace.clone()
        };
        match ws {
            Some(path) => self.open_workspace(&path),
            None => Err(EngineError::NoWorkspace),
        }
    }

    /// Full per-column stats for one source.
    pub fn describe_source(&self, name: &str) -> EngineResult<SourceInfo> {
        let mut info = {
            let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
            inner
                .sources
                .iter()
                .find(|s| s.name == name || s.view.as_deref() == Some(name))
                .cloned()
                .ok_or_else(|| EngineError::UnknownSource(name.to_string()))?
        };
        let view = info
            .view
            .clone()
            .ok_or_else(|| EngineError::msg(format!("{name} is not a tabular source")))?;

        let columns = {
            let data = self.data.lock().unwrap_or_else(|e| e.into_inner());
            let mut cols = data.describe(&view)?;
            if info.row_count.is_none() {
                info.row_count = data
                    .query(&format!("SELECT count(*) FROM {}", data::quote_ident(&view)), 1)
                    .ok()
                    .and_then(|o| o.rows.first().and_then(|r| r.first()).and_then(|v| v.as_i64()));
            }
            // Carry any ingest-time note (coercion, mixed column) onto the
            // freshly-computed stats, keyed by column name.
            if let Some(prior) = &info.columns {
                for c in &mut cols {
                    if c.note.is_none() {
                        c.note = prior.iter().find(|p| p.name == c.name).and_then(|p| p.note.clone());
                    }
                }
            }
            cols
        };
        info.columns = Some(columns.clone());

        // Cache the enriched schema back so a later turn's describe / the
        // system-prompt digest is instant and carries the stats.
        {
            let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(s) = inner
                .sources
                .iter_mut()
                .find(|s| s.name == name || s.view.as_deref() == Some(name))
            {
                s.columns = Some(columns);
                if s.row_count.is_none() {
                    s.row_count = info.row_count;
                }
            }
            inner.schema_cache = None;
        }
        Ok(info)
    }

    /// Run a read-only SQL statement (used by `/sql` and the agent).
    pub fn run_sql(&self, sql: &str) -> EngineResult<QueryResult> {
        data::ensure_read_only(sql)?;
        let data = self.data.lock().unwrap_or_else(|e| e.into_inner());
        let t = Instant::now();
        let out = data.query(sql, DEFAULT_ROW_CAP)?;
        Ok(QueryResult {
            columns: out.columns,
            rows: out.rows,
            row_count: out.row_count,
            ms: t.elapsed().as_millis() as u64,
            truncated: out.truncated,
        })
    }

    /// First `n` rows of a source (used by the `sample_rows` tool).
    pub fn sample(&self, name: &str, n: usize) -> EngineResult<QueryResult> {
        let view = self.view_for(name)?;
        self.run_sql(&format!(
            "SELECT * FROM {} LIMIT {}",
            data::quote_ident(&view),
            n.clamp(1, 200)
        ))
    }

    /// Run a Python snippet against the workspace data (blocking work is moved
    /// off the async executor).
    pub async fn run_python(&self, code: &str) -> EngineResult<pyexec::PyResult> {
        let bridge: PythonBridge = self.data.lock().unwrap_or_else(|e| e.into_inner()).python_bridge();
        let code = code.to_string();
        tokio::task::spawn_blocking(move || pyexec::run(&code, bridge))
            .await
            .map_err(|e| EngineError::msg(format!("python task panicked: {e}")))?
    }

    fn view_for(&self, name: &str) -> EngineResult<String> {
        let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner
            .sources
            .iter()
            .find(|s| s.name == name || s.view.as_deref() == Some(name))
            .and_then(|s| s.view.clone())
            .ok_or_else(|| EngineError::UnknownSource(name.to_string()))
    }

    /// Run the agent loop for one question, streaming progress through `emit`.
    ///
    /// `conversation_id` scopes the distilled session memory and the stop-flag,
    /// so several conversations (tabs) can run at once without clobbering each
    /// other. Its memory is kept until the tab is closed or the workspace is
    /// (re)opened.
    ///
    /// `model` is this tab's chosen model (of the one signed-in provider);
    /// `None` uses the saved default.
    pub async fn ask(
        &self,
        conversation_id: &str,
        question: &str,
        model: Option<&str>,
        emit: impl Fn(AskEvent) + Send + Sync,
    ) -> EngineResult<Answer> {
        let effective_model = model
            .map(str::trim)
            .filter(|m| !m.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| self.settings().model);
        if effective_model.trim().is_empty() {
            return Err(EngineError::msg(
                "No model chosen yet. Run /model to see the options and pick one.",
            ));
        }
        // Touch this conversation's memory slot (create it, mark it most-recently
        // used) and evict the least-recently-used one if we're over the cap.
        {
            let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
            inner.session_tick += 1;
            let tick = inner.session_tick;
            inner
                .sessions
                .entry(conversation_id.to_string())
                .or_default()
                .last_used = tick;
            if inner.sessions.len() > SESSION_CAP {
                if let Some(victim) = inner
                    .sessions
                    .iter()
                    .filter(|(k, _)| k.as_str() != conversation_id)
                    .min_by_key(|(_, v)| v.last_used)
                    .map(|(k, _)| k.clone())
                {
                    inner.sessions.remove(&victim);
                }
            }
        }

        // A fresh stop-flag for this run, registered under the conversation id so
        // `cancel_run(id)` can find it.
        let cancel = Arc::new(AtomicBool::new(false));
        self.cancel
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(conversation_id.to_string(), cancel.clone());

        let llm = self.llm_with_model(&effective_model);
        #[allow(unused_mut)]
        let mut registry = Registry::standard();
        #[cfg(feature = "mcp")]
        self.attach_mcp_tools(&mut registry, &emit).await;
        let answer =
            agent::run(self, &llm, &registry, conversation_id, question, &cancel, &emit).await;
        // (`&cancel` derefs `Arc<AtomicBool>` -> `&AtomicBool` for `run`.)

        // Drop this run's stop-flag (unless a newer run for the same id already
        // replaced it).
        {
            let mut flags = self.cancel.lock().unwrap_or_else(|e| e.into_inner());
            if flags.get(conversation_id).is_some_and(|f| Arc::ptr_eq(f, &cancel)) {
                flags.remove(conversation_id);
            }
        }
        let answer = answer?;

        // Distil this turn for the next question in the conversation - but not a
        // cancelled or content-free run, which would only evict a useful earlier
        // turn from the 3-slot memory.
        if !cancel.load(Ordering::Relaxed) {
            let headline: String = answer
                .text
                .lines()
                .map(str::trim)
                .find(|l| !l.is_empty())
                .unwrap_or("")
                .chars()
                .take(200)
                .collect();
            let queries: Vec<String> = answer
                .evidence
                .iter()
                .filter(|e| e.tool == "run_sql" && e.error.is_none())
                .filter_map(|e| e.sql.clone())
                .map(|s| s.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(160).collect())
                .take(3)
                .collect();
            let uninformative = queries.is_empty()
                && (headline.is_empty()
                    || headline == "Stopped."
                    || headline.starts_with("I ran out of analysis steps"));
            if !uninformative {
                let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
                let entry = inner.sessions.entry(conversation_id.to_string()).or_default();
                entry.turns.push(TurnDigest {
                    question: question.chars().take(200).collect(),
                    headline,
                    queries,
                });
                let n = entry.turns.len();
                if n > 3 {
                    entry.turns.drain(0..n - 3);
                }
            }
        }
        Ok(answer)
    }

    /// Connect every enabled `mcp` connector and register its tools. Failures
    /// and withheld (non-read-only) tools are reported as notices, never fatal.
    #[cfg(feature = "mcp")]
    async fn attach_mcp_tools(
        &self,
        registry: &mut Registry,
        emit: &(impl Fn(AskEvent) + Send + Sync),
    ) {
        let connectors = {
            let conn = self.sqlite.lock().unwrap_or_else(|e| e.into_inner());
            extensions::enabled_mcp_connectors(&self.data_dir, &conn)
        };
        let mut tools = Vec::new();
        for (id, cfg) in connectors {
            let token = cfg
                .auth
                .secret_name()
                .and_then(|v| self.secrets.api_key(&format!("mcp:{id}:{v}")));
            if cfg.auth.secret_name().is_some() && token.is_none() {
                emit(AskEvent::Notice {
                    text: format!("connector '{id}' has no token yet run /connect {id}"),
                });
                continue;
            }
            match crate::engine::mcp::connect(&self.http, &id, &cfg, token.as_deref()).await {
                Ok((mut offered, withheld)) => {
                    if !withheld.is_empty() {
                        emit(AskEvent::Notice {
                            text: format!(
                                "connector '{id}': skipped {} tool(s) that can modify the service ({})",
                                withheld.len(),
                                withheld.join(", ")
                            ),
                        });
                    }
                    tools.append(&mut offered);
                }
                Err(e) => emit(AskEvent::Notice {
                    text: format!("couldn't use connector '{id}': {e}"),
                }),
            }
        }
        registry.set_mcp(tools);
    }

    /// Store the token for an `mcp` connector pack (its `connector.json` names
    /// which credential it reads).
    pub fn mcp_set_token(&self, id: &str, token: &str) -> EngineResult<()> {
        let cfg = extensions::connector_config(&self.data_dir, id)?;
        let var = cfg
            .auth
            .secret_name()
            .ok_or_else(|| EngineError::msg(format!("connector '{id}' needs no token")))?;
        self.secrets.set_api_key(&format!("mcp:{id}:{var}"), token)
    }

    /// Forget an `mcp` connector's token.
    pub fn mcp_clear_token(&self, id: &str) -> EngineResult<bool> {
        let cfg = extensions::connector_config(&self.data_dir, id)?;
        match cfg.auth.secret_name() {
            Some(var) => self.secrets.clear(&format!("mcp:{id}:{var}")),
            None => Ok(false),
        }
    }

    /// Whether an `mcp` connector pack has its token stored (or needs none).
    pub fn mcp_has_token(&self, id: &str) -> bool {
        match extensions::connector_config(&self.data_dir, id) {
            Ok(cfg) => match cfg.auth.secret_name() {
                Some(var) => self.secrets.has(&format!("mcp:{id}:{var}")),
                None => true,
            },
            Err(_) => false,
        }
    }

    /// Is the configured model provider reachable?
    pub async fn provider_health(&self) -> ProviderHealth {
        self.llm().health().await
    }

    /// Probe a local Ollama regardless of which provider is configured, so the
    /// UI can offer "use Ollama" when someone installs it after signing in
    /// somewhere else. Returns `reachable: false` when nothing is listening.
    pub async fn probe_ollama(&self) -> ProviderHealth {
        let Some(p) = provider::get("ollama") else {
            return ProviderHealth { reachable: false, rejected: false, models: Vec::new() };
        };
        let s = Settings {
            provider: p.id.to_string(),
            base_url: p.base_url.to_string(),
            model: String::new(),
            embed_model: String::new(),
            has_credential: true,
        };
        LlmClient::new(self.http.clone(), &s, None).health().await
    }

    fn llm(&self) -> LlmClient {
        self.llm_with_model("")
    }

    /// Build an LLM client for the signed-in provider, answering with `model`
    /// (empty string = the saved default). Lets each tab pick its own model.
    fn llm_with_model(&self, model: &str) -> LlmClient {
        let mut settings = self.settings();
        if !model.trim().is_empty() {
            settings.model = model.to_string();
        }
        let key = self
            .secrets
            .api_key(provider::normalize_id(&settings.provider));
        LlmClient::new(self.http.clone(), &settings, key)
    }

    /// Fire-and-forget: ask a local Ollama to load the configured model now so
    /// the next question doesn't stall on a cold load. No-op off a Tokio
    /// runtime, for a hosted provider, or when `FELLA_SKIP_MODEL_WARMUP` is set
    /// (tests point at a fixed-count mock server).
    fn warm_model(&self) {
        if std::env::var_os("FELLA_SKIP_MODEL_WARMUP").is_some() {
            return;
        }
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            let llm = self.llm();
            handle.spawn(async move { llm.warm().await });
        }
    }

    /// Catalogued documents (text/PDF, not tables SQL already covers those),
    /// as (name, path, kind), for the `grep_files`/`read_file` tools.
    fn documents(&self) -> Vec<(String, String, SourceKind)> {
        let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner
            .sources
            .iter()
            .filter(|s| s.view.is_none())
            .map(|s| (s.name.clone(), s.path.clone(), s.kind))
            .collect()
    }

    /// Regex (case-insensitive) search over the extracted text of every
    /// catalogued document. No index to build or keep in sync just reads
    /// the files that are there right now.
    pub fn grep_files(&self, pattern: &str, max_hits: usize) -> EngineResult<Vec<GrepHit>> {
        let re = regex::RegexBuilder::new(pattern)
            .case_insensitive(true)
            .build()
            .map_err(|e| EngineError::msg(format!("bad pattern: {e}")))?;
        let mut hits = Vec::new();
        for (name, path, kind) in self.documents() {
            if kind == SourceKind::Pdf {
                // PDF text isn't streamable parse (cached) and scan in memory.
                let Ok(text) = self.pdf_text(&path) else { continue };
                for (i, line) in text.lines().enumerate() {
                    if re.is_match(line) {
                        hits.push(GrepHit {
                            source: name.clone(),
                            line: i + 1,
                            text: line.trim().to_string(),
                        });
                        if hits.len() >= max_hits {
                            return Ok(hits);
                        }
                    }
                }
            } else {
                // Text files stream a multi-GB `.log` never lands in memory.
                let mut stop = false;
                let _ = docs::grep_lines(&path, |i, line| {
                    if re.is_match(line) {
                        hits.push(GrepHit {
                            source: name.clone(),
                            line: i,
                            text: line.trim().to_string(),
                        });
                        stop = hits.len() >= max_hits;
                    }
                    !stop
                });
                if stop {
                    return Ok(hits);
                }
            }
        }
        Ok(hits)
    }

    /// Full extracted text of one catalogued document, by the name shown by
    /// `list_files`/`grep_files` (not a filesystem path). Capped so one huge
    /// document can't blow the context window `grep_files` can find a spot
    /// in a bigger file first.
    pub fn read_file(&self, name: &str) -> EngineResult<(String, bool)> {
        let (path, kind) = self
            .documents()
            .into_iter()
            .find(|(n, _, _)| n == name)
            .map(|(_, p, k)| (p, k))
            .ok_or_else(|| EngineError::UnknownSource(name.to_string()))?;
        let text = if kind == SourceKind::Pdf {
            let t = self.pdf_text(&path)?;
            if docs::looks_like_no_text_layer(&t) {
                return Ok((
                    "(this PDF has no selectable text it looks like a scan or photos of pages)"
                        .to_string(),
                    false,
                ));
            }
            t.to_string()
        } else {
            // Read only what we might return, not the whole file. The extra
            // margin covers multi-byte characters so the char cap below still
            // has enough to decide "truncated".
            docs::read_text_head(&path, READ_FILE_CHAR_CAP * 4 + 64)?
        };
        Ok(match text.char_indices().nth(READ_FILE_CHAR_CAP) {
            Some((i, _)) => (text[..i].to_string(), true),
            None => (text, false),
        })
    }

    /// Extracted text of a PDF, cached by path and invalidated on mtime change.
    fn pdf_text(&self, path: &str) -> EngineResult<Arc<str>> {
        let mtime = std::fs::metadata(path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);
        if let Some((ts, text)) = self.doc_cache.lock().unwrap_or_else(|e| e.into_inner()).get(path) {
            if *ts == mtime {
                return Ok(text.clone());
            }
        }
        let text: Arc<str> = Arc::from(docs::extract_pdf(path)?);
        self.doc_cache
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(path.to_string(), (mtime, text.clone()));
        Ok(text)
    }

    fn persist_sources(&self, workspace: &Path, sources: &[SourceInfo]) {
        let conn = self.sqlite.lock().unwrap_or_else(|e| e.into_inner());
        let ws = workspace.display().to_string();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let _ = conn.execute("DELETE FROM sources WHERE workspace = ?1", [&ws]);
        for s in sources {
            let _ = conn.execute(
                "INSERT OR REPLACE INTO sources (workspace, name, path, kind, view, row_count)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                rusqlite::params![
                    ws,
                    s.name,
                    s.path,
                    format!("{:?}", s.kind).to_lowercase(),
                    s.view,
                    s.row_count,
                ],
            );
        }
        let _ = conn.execute(
            "INSERT OR REPLACE INTO recent_workspaces (path, opened_at) VALUES (?1, ?2)",
            rusqlite::params![ws, now],
        );
    }
}

/// First non-empty line of a text document, trimmed and capped, for the
/// system-prompt document listing. Reads at most a few KB.
fn first_line_synopsis(path: &str) -> Option<String> {
    use std::io::Read;
    let mut f = std::fs::File::open(path).ok()?;
    let mut buf = [0u8; 4096];
    let n = f.read(&mut buf).ok()?;
    let text = String::from_utf8_lossy(&buf[..n]);
    let line = text.lines().map(str::trim).find(|l| !l.is_empty())?;
    let s: String = line.chars().take(120).collect();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

/// Truncate to at most `cap` characters (not bytes), for a short preview
/// line no ellipsis added; the caller decides whether one reads better.
fn cap_chars(s: &str, cap: usize) -> String {
    match s.char_indices().nth(cap) {
        Some((i, _)) => s[..i].to_string(),
        None => s.to_string(),
    }
}

/// Render a small `QueryResult` as compact `col=val` sample lines for the
/// system-prompt schema digest.
fn mini_table(q: &QueryResult) -> Vec<String> {
    q.rows
        .iter()
        .take(3)
        .map(|row| {
            q.columns
                .iter()
                .zip(row)
                .map(|(c, v)| {
                    let s = match v {
                        serde_json::Value::Null => "".to_string(),
                        serde_json::Value::String(s) => s.clone(),
                        other => other.to_string(),
                    };
                    let s: String = s.chars().take(24).collect();
                    format!("{c}={s}")
                })
                .collect::<Vec<_>>()
                .join(", ")
        })
        .collect()
}

/// Open `fella.db`, or, if it's corrupt (a truncated write, a bad disk),
/// move it aside as `fella.db.corrupt-<unix>` and start fresh. Losing it costs
/// the user their settings and pack list, not their API keys (`auth.json` is
/// separate) or their archived conversations. Better than a non-starting app
/// with a stderr-only message.
fn open_or_recover(path: &Path) -> rusqlite::Connection {
    // The app was "Woody" until the rename: carry a `woody.db` (and its WAL
    // sidecars) forward the first time we open at the new name.
    if !path.exists() {
        for suffix in ["", "-wal", "-shm"] {
            let old = PathBuf::from(format!("{}{suffix}", path.with_file_name("woody.db").display()));
            let new = PathBuf::from(format!("{}{suffix}", path.display()));
            if old.exists() {
                let _ = std::fs::rename(&old, &new);
            }
        }
    }
    match sqlite::open(path) {
        Ok(c) => c,
        Err(e) => {
            log::warn!("fella.db unusable ({e}); moving it aside and starting fresh");
            let ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            for suffix in ["", "-wal", "-shm"] {
                let from = PathBuf::from(format!("{}{suffix}", path.display()));
                let to = PathBuf::from(format!("{}.corrupt-{ts}{suffix}", path.display()));
                let _ = std::fs::rename(&from, &to);
            }
            sqlite::open(path).expect("recreate fella.db after moving the corrupt one aside")
        }
    }
}

/// One-shot: move a plaintext `api_key` left in the settings table by a
/// pre-keychain build into the credential store, then drop the row.
fn migrate_legacy_key(conn: &rusqlite::Connection, secrets: &Secrets) {
    let Some(key) = sqlite::take_legacy_api_key(conn) else {
        return;
    };
    let stored = sqlite::load_settings(conn).provider;
    let id = provider::normalize_id(&stored);
    // A pre-registry key almost always belonged to the "openai-compatible"
    // (now `custom`) path; if the provider still reads as the local default,
    // park it there rather than on `ollama` (which needs no key).
    let target: &str = if id == provider::DEFAULT_ID { "custom" } else { id };
    if secrets.set_api_key(target, &key).is_ok() {
        let _ = sqlite::clear_legacy_api_key(conn);
        log::info!("migrated a stored API key out of the settings table into auth.json");
    }
}

/// On startup: if the stored provider needs an API key but none is saved (a
/// past `/logout`, a deleted `auth.json`), fall back to the local default so
/// the app doesn't come up pointed at a service it can't reach with a stale
/// model still showing in the status bar.
fn reconcile_provider(conn: &rusqlite::Connection, secrets: &Secrets) {
    let stored = sqlite::load_settings(conn).provider;
    match provider::get(&stored) {
        // Registered provider that needs no key, or has one saved nothing to do.
        Some(p) if p.auth == AuthKind::None || secrets.has(p.id) => return,
        // Registered but keyless: fall through and reset.
        Some(_) => {}
        // An id no build knows (a stray `/model provider x`, a removed registry
        // row). Its key, if any, is unreachable through the UI reset too.
        None => {}
    }
    let d = provider::get(provider::DEFAULT_ID).expect("the default provider is in the registry");
    let mut patch = serde_json::Map::new();
    patch.insert("provider".into(), d.id.into());
    patch.insert("base_url".into(), d.base_url.into());
    patch.insert("model".into(), d.default_model.into());
    patch.insert("embed_model".into(), d.default_embed_model.into());
    if sqlite::save_settings(conn, &patch).is_ok() {
        log::info!("no usable credential for provider {stored:?}; reset to {}", d.id);
    }
}
