//! Lightweight application state in SQLite: settings and a workspace-scoped
//! source cache. Analytical data never lives here that's DuckDB's job.

use std::path::Path;

use rusqlite::Connection;
use serde::Serialize;

use crate::engine::error::EngineResult;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS sources (
    workspace TEXT NOT NULL,
    name      TEXT NOT NULL,
    path      TEXT NOT NULL,
    kind      TEXT NOT NULL,
    view      TEXT,
    row_count INTEGER,
    PRIMARY KEY (workspace, name)
);
CREATE TABLE IF NOT EXISTS recent_workspaces (
    path      TEXT PRIMARY KEY,
    opened_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS extensions (
    id           TEXT PRIMARY KEY,
    kind         TEXT NOT NULL,
    name         TEXT NOT NULL,
    version      TEXT NOT NULL,
    description  TEXT NOT NULL,
    source       TEXT NOT NULL,
    sha256       TEXT,
    enabled      INTEGER NOT NULL DEFAULT 0,
    installed_at INTEGER NOT NULL
);
";

pub fn open(path: &Path) -> EngineResult<Connection> {
    let conn = Connection::open(path)?;
    conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON;")?;
    conn.execute_batch(SCHEMA)?;
    Ok(conn)
}

#[derive(Debug, Clone, Serialize)]
pub struct Settings {
    pub provider: String,
    pub base_url: String,
    pub model: String,
    pub embed_model: String,
    /// Whether a usable credential exists for `provider`. Filled in by
    /// `EngineState` (it owns the credential store); `load_settings` leaves it
    /// `false`.
    pub has_credential: bool,
}

fn get(conn: &Connection, key: &str) -> Option<String> {
    conn.query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| {
        r.get::<_, String>(0)
    })
    .ok()
}

fn put(conn: &Connection, key: &str, value: &str) -> EngineResult<()> {
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        (key, value),
    )?;
    Ok(())
}

/// A legacy plaintext API key, if a pre-keychain build left one here. Used only
/// by the one-shot migration into the credential store; not part of the normal
/// read path.
pub fn take_legacy_api_key(conn: &Connection) -> Option<String> {
    get(conn, "api_key").filter(|k| !k.is_empty())
}

/// Delete the legacy `api_key` row after it has been migrated out.
pub fn clear_legacy_api_key(conn: &Connection) -> EngineResult<()> {
    conn.execute("DELETE FROM settings WHERE key = 'api_key'", [])?;
    Ok(())
}

pub fn load_settings(conn: &Connection) -> Settings {
    use crate::engine::provider;
    let stored = get(conn, "provider").unwrap_or_else(|| provider::DEFAULT_ID.into());
    let p = provider::get(&stored);
    Settings {
        base_url: get(conn, "base_url")
            .or_else(|| p.map(|p| p.base_url.to_string()).filter(|s| !s.is_empty()))
            .unwrap_or_else(|| "http://localhost:11434".into()),
        model: get(conn, "model").unwrap_or_else(|| {
            p.map(|p| p.default_model).unwrap_or("llama3.1").into()
        }),
        embed_model: get(conn, "embed_model").unwrap_or_else(|| {
            p.map(|p| p.default_embed_model)
                .filter(|s| !s.is_empty())
                .unwrap_or("nomic-embed-text")
                .into()
        }),
        provider: stored,
        has_credential: false,
    }
}

/// Apply a partial settings update. Recognised keys: `provider`, `base_url`,
/// `model`, `embed_model`. Credentials go through the credential store, not here.
pub fn save_settings(
    conn: &Connection,
    patch: &serde_json::Map<String, serde_json::Value>,
) -> EngineResult<Settings> {
    for key in ["provider", "base_url", "model", "embed_model"] {
        if let Some(v) = patch.get(key).and_then(|v| v.as_str()) {
            put(conn, key, v)?;
        }
    }
    Ok(load_settings(conn))
}

// --- installed extensions (packs) -----------------------------------------

/// One row of the `extensions` table. `source` is `"local"` (side-loaded) or
/// `"marketplace"`; `sha256` is set only for marketplace installs.
#[derive(Debug, Clone)]
pub struct ExtRow {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub source: String,
    pub sha256: Option<String>,
    pub enabled: bool,
    pub installed_at: i64,
}

pub fn list_extensions(conn: &Connection) -> Vec<ExtRow> {
    let mut stmt = match conn.prepare(
        "SELECT id, kind, name, version, description, source, sha256, enabled, installed_at
         FROM extensions ORDER BY kind, name",
    ) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let rows = stmt.query_map([], |r| {
        Ok(ExtRow {
            id: r.get(0)?,
            kind: r.get(1)?,
            name: r.get(2)?,
            version: r.get(3)?,
            description: r.get(4)?,
            source: r.get(5)?,
            sha256: r.get(6)?,
            enabled: r.get::<_, i64>(7)? != 0,
            installed_at: r.get(8)?,
        })
    });
    match rows {
        Ok(it) => it.filter_map(Result::ok).collect(),
        Err(_) => Vec::new(),
    }
}

pub fn upsert_extension(conn: &Connection, row: &ExtRow) -> EngineResult<()> {
    conn.execute(
        "INSERT INTO extensions
           (id, kind, name, version, description, source, sha256, enabled, installed_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
         ON CONFLICT(id) DO UPDATE SET
           kind = excluded.kind, name = excluded.name, version = excluded.version,
           description = excluded.description, source = excluded.source,
           sha256 = excluded.sha256, installed_at = excluded.installed_at",
        rusqlite::params![
            row.id,
            row.kind,
            row.name,
            row.version,
            row.description,
            row.source,
            row.sha256,
            row.enabled as i64,
            row.installed_at,
        ],
    )?;
    Ok(())
}

pub fn delete_extension(conn: &Connection, id: &str) -> EngineResult<()> {
    conn.execute("DELETE FROM extensions WHERE id = ?1", [id])?;
    Ok(())
}

pub fn set_extension_enabled(conn: &Connection, id: &str, enabled: bool) -> EngineResult<()> {
    conn.execute(
        "UPDATE extensions SET enabled = ?2 WHERE id = ?1",
        rusqlite::params![id, enabled as i64],
    )?;
    Ok(())
}

/// Turn off every `theme` row used before enabling one so a single theme is
/// active at a time.
pub fn disable_all_themes(conn: &Connection) -> EngineResult<()> {
    conn.execute("UPDATE extensions SET enabled = 0 WHERE kind = 'theme'", [])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_roundtrip() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(SCHEMA).unwrap();

        let s = load_settings(&conn);
        assert_eq!(s.provider, "ollama");
        assert!(!s.has_credential);

        let mut patch = serde_json::Map::new();
        patch.insert("model".into(), "qwen3".into());
        // credentials are not accepted here anymore
        patch.insert("api_key".into(), "secret".into());
        let s = save_settings(&conn, &patch).unwrap();
        assert_eq!(s.model, "qwen3");
        let json = serde_json::to_string(&s).unwrap();
        assert!(!json.contains("secret"));
        assert!(!json.contains("has_api_key"));
    }

    #[test]
    fn switching_provider_moves_the_defaults() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(SCHEMA).unwrap();

        let mut patch = serde_json::Map::new();
        patch.insert("provider".into(), "openai".into());
        patch.insert("base_url".into(), "https://api.openai.com/v1".into());
        let s = save_settings(&conn, &patch).unwrap();
        assert_eq!(s.provider, "openai");
        assert_eq!(s.base_url, "https://api.openai.com/v1");
        // model falls back to the provider default when unset
        assert_eq!(s.model, "gpt-4o-mini");
    }

    #[test]
    fn legacy_api_key_migration_helpers() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(SCHEMA).unwrap();
        put(&conn, "api_key", "sk-old").unwrap();
        assert_eq!(take_legacy_api_key(&conn).as_deref(), Some("sk-old"));
        clear_legacy_api_key(&conn).unwrap();
        assert!(take_legacy_api_key(&conn).is_none());
    }
}
