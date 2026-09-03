//! Packs: user-installed themes, skills, and MCP connectors. A pack is data the
//! app reads, never code it runs. See `docs/EXTENSIBILITY.md`.
//!
//! This module owns the on-disk layout (`<data_dir>/extensions/<id>/`) and the
//! `extensions` table (via `super::sqlite`). Marketplace download lives in a
//! later phase; today packs are added from a local directory.

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::engine::error::{EngineError, EngineResult};
use crate::engine::sqlite::{self, ExtRow};

/// Skill Markdown longer than this (per pack) is truncated before it reaches
/// the system prompt, so one pack can't crowd out everything else.
const SKILL_CHAR_CAP: usize = 16_000;

/// The colour and scalar CSS custom properties a `theme` pack may set. Anything
/// else in a `theme.json` is ignored.
const THEME_TOKEN_KEYS: &[&str] = &[
    "--bg",
    "--bg-raised",
    "--bg-inset",
    "--border",
    "--border-strong",
    "--text",
    "--text-dim",
    "--text-faint",
    "--accent",
    "--link",
    "--ok",
    "--warn",
    "--err",
    "--radius",
    "--pad",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PackKind {
    Theme,
    Skill,
    Mcp,
}

impl PackKind {
    fn as_str(self) -> &'static str {
        match self {
            PackKind::Theme => "theme",
            PackKind::Skill => "skill",
            PackKind::Mcp => "mcp",
        }
    }
}

/// A `fella-pack.json` manifest.
#[derive(Debug, Clone, Deserialize)]
pub struct Manifest {
    pub schema: u32,
    pub id: String,
    pub kind: PackKind,
    pub name: String,
    pub version: String,
    pub description: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub homepage: String,
    #[serde(default)]
    pub license: String,
    pub payload: String,
}

impl Manifest {
    fn parse(text: &str) -> EngineResult<Self> {
        let m: Manifest = serde_json::from_str(text)
            .map_err(|e| EngineError::msg(format!("fella-pack.json is not valid: {e}")))?;
        m.validate()?;
        Ok(m)
    }

    fn validate(&self) -> EngineResult<()> {
        if self.schema != 1 {
            return Err(EngineError::msg(format!(
                "unsupported pack schema {} (this Fella understands schema 1)",
                self.schema
            )));
        }
        let id_ok = !self.id.is_empty()
            && self.id.len() <= 64
            && self
                .id
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
            && self.id.as_bytes()[0] != b'-';
        if !id_ok {
            return Err(EngineError::msg(
                "pack id must be lowercase letters, digits and dashes, not starting with a dash",
            ));
        }
        if self.name.trim().is_empty() || self.description.trim().is_empty() {
            return Err(EngineError::msg("pack name and description are required"));
        }
        // payload must be a single relative path inside the pack directory.
        let p = Path::new(&self.payload);
        let safe = !self.payload.is_empty()
            && !p.is_absolute()
            && p.components().all(|c| matches!(c, Component::Normal(_)));
        if !safe {
            return Err(EngineError::msg(
                "pack payload must be a relative path inside the pack (no '..', no leading '/')",
            ));
        }
        let ext_ok = match self.kind {
            PackKind::Skill => self.payload.ends_with(".md"),
            PackKind::Theme | PackKind::Mcp => self.payload.ends_with(".json"),
        };
        if !ext_ok {
            return Err(EngineError::msg(format!(
                "a {} pack's payload should be a {} file",
                self.kind.as_str(),
                if matches!(self.kind, PackKind::Skill) { ".md" } else { ".json" }
            )));
        }
        Ok(())
    }
}

/// One installed pack, as shown by `/packs`.
#[derive(Debug, Clone, Serialize)]
pub struct InstalledPack {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub version: String,
    pub description: String,
    /// `"local"` or `"marketplace"`.
    pub source: String,
    /// Installed from the reviewed marketplace (vs. side-loaded).
    pub verified: bool,
    pub enabled: bool,
    /// `mcp` packs only: needs a token that isn't stored yet. Set by
    /// `EngineState::packs_list`; `false` everywhere else.
    #[serde(default)]
    pub needs_token: bool,
}

impl From<ExtRow> for InstalledPack {
    fn from(r: ExtRow) -> Self {
        InstalledPack {
            verified: r.source == "marketplace",
            id: r.id,
            kind: r.kind,
            name: r.name,
            version: r.version,
            description: r.description,
            source: r.source,
            enabled: r.enabled,
            needs_token: false,
        }
    }
}

fn extensions_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("extensions")
}

fn pack_dir(data_dir: &Path, id: &str) -> PathBuf {
    extensions_dir(data_dir).join(id)
}

pub fn list(conn: &rusqlite::Connection) -> Vec<InstalledPack> {
    sqlite::list_extensions(conn)
        .into_iter()
        .map(InstalledPack::from)
        .collect()
}

/// Validate a manifest and write the pack into `<data_dir>/extensions/<id>/`,
/// then upsert its row. Shared by local and marketplace installs. `enabled` is
/// preserved across a reinstall.
fn write_pack(
    data_dir: &Path,
    conn: &rusqlite::Connection,
    manifest_text: &str,
    payload_bytes: &[u8],
    source: &str,
    sha256: Option<String>,
) -> EngineResult<()> {
    let m = Manifest::parse(manifest_text)?;

    let dest = pack_dir(data_dir, &m.id);
    if dest.exists() {
        let _ = std::fs::remove_dir_all(&dest);
    }
    std::fs::create_dir_all(&dest)
        .map_err(|e| EngineError::io(format!("create {}", dest.display()), e))?;
    std::fs::write(dest.join("fella-pack.json"), manifest_text)
        .map_err(|e| EngineError::io("write fella-pack.json", e))?;
    // payload path is validated to have no `..`; join stays inside `dest`.
    if let Some(parent) = dest.join(&m.payload).parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(dest.join(&m.payload), payload_bytes)
        .map_err(|e| EngineError::io(format!("write {}", m.payload), e))?;

    let keep_enabled = sqlite::list_extensions(conn)
        .iter()
        .find(|r| r.id == m.id)
        .map(|r| r.enabled)
        .unwrap_or(false);

    sqlite::upsert_extension(
        conn,
        &ExtRow {
            id: m.id.clone(),
            kind: m.kind.as_str().to_string(),
            name: m.name.clone(),
            version: m.version.clone(),
            description: m.description.clone(),
            source: source.to_string(),
            sha256,
            enabled: keep_enabled,
            installed_at: now_secs(),
        },
    )?;
    Ok(())
}

/// Add a pack from a local directory (or a path to its `fella-pack.json`).
/// Side-loaded: recorded as `source = "local"` and left disabled.
pub fn install_local(
    data_dir: &Path,
    conn: &rusqlite::Connection,
    src: &Path,
) -> EngineResult<()> {
    let src_dir = if src.file_name() == Some(std::ffi::OsStr::new("fella-pack.json")) {
        src.parent().unwrap_or(src).to_path_buf()
    } else {
        src.to_path_buf()
    };
    if !src_dir.is_dir() {
        return Err(EngineError::msg(format!("not a folder: {}", src_dir.display())));
    }

    let manifest_text = std::fs::read_to_string(src_dir.join("fella-pack.json"))
        .map_err(|e| EngineError::io(format!("read {}/fella-pack.json", src_dir.display()), e))?;
    let m = Manifest::parse(&manifest_text)?;

    let payload_src = src_dir.join(&m.payload);
    let payload_bytes = std::fs::read(&payload_src).map_err(|e| {
        EngineError::io(
            format!("read pack payload '{}' from {}", m.payload, src_dir.display()),
            e,
        )
    })?;

    write_pack(data_dir, conn, &manifest_text, &payload_bytes, "local", None)
}

// --- marketplace install -------------------------------------------------

/// Where the app fetches the marketplace index. `FELLA_CATALOG_URL` overrides.
const DEFAULT_CATALOG_URL: &str =
    "https://raw.githubusercontent.com/Avijit-Kumar-GIT/fella-extensions/main/catalog.json";

pub fn catalog_url() -> String {
    std::env::var("FELLA_CATALOG_URL").unwrap_or_else(|_| DEFAULT_CATALOG_URL.to_string())
}

#[derive(Debug, Deserialize)]
struct MarketplaceCatalog {
    packs: Vec<CatalogEntry>,
}

#[derive(Debug, Deserialize)]
struct CatalogEntry {
    id: String,
    files: Vec<CatalogFile>,
}

#[derive(Debug, Deserialize)]
struct CatalogFile {
    path: String,
    url: String,
    sha256: String,
}

/// A verified pack fetched from the marketplace, ready to write to disk.
pub struct DownloadedPack {
    manifest_text: String,
    payload_bytes: Vec<u8>,
    manifest_sha: String,
}

fn sha256_hex(bytes: &[u8]) -> String {
    ring::digest::digest(&ring::digest::SHA256, bytes)
        .as_ref()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

async fn fetch_bytes(http: &reqwest::Client, url: &str) -> EngineResult<Vec<u8>> {
    let resp = http
        .get(url)
        .timeout(std::time::Duration::from_secs(20))
        .send()
        .await
        .map_err(|e| EngineError::msg(format!("could not reach {url}: {e}")))?;
    if !resp.status().is_success() {
        return Err(EngineError::msg(format!("{url}: HTTP {}", resp.status())));
    }
    Ok(resp
        .bytes()
        .await
        .map_err(|e| EngineError::msg(format!("reading {url}: {e}")))?
        .to_vec())
}

/// Fetch the catalog, find `id`, download each listed file and check its
/// SHA-256. No disk writes here so the caller can do them under a lock.
pub async fn download_pack(
    http: &reqwest::Client,
    catalog_url: &str,
    id: &str,
) -> EngineResult<DownloadedPack> {
    let catalog: MarketplaceCatalog = serde_json::from_slice(&fetch_bytes(http, catalog_url).await?)
        .map_err(|e| EngineError::msg(format!("the marketplace catalog is not valid: {e}")))?;
    let entry = catalog
        .packs
        .into_iter()
        .find(|p| p.id == id)
        .ok_or_else(|| EngineError::msg(format!("no pack '{id}' in the marketplace")))?;

    let mut files: std::collections::HashMap<String, Vec<u8>> = std::collections::HashMap::new();
    for f in &entry.files {
        let bytes = fetch_bytes(http, &f.url).await?;
        let got = sha256_hex(&bytes);
        if got != f.sha256.trim().to_lowercase() {
            return Err(EngineError::msg(format!(
                "{}: content does not match the catalog checksum (nothing was installed)",
                f.path
            )));
        }
        files.insert(f.path.clone(), bytes);
    }

    let manifest_bytes = files
        .get("fella-pack.json")
        .ok_or_else(|| EngineError::msg("catalog entry is missing fella-pack.json"))?;
    let manifest_text = std::str::from_utf8(manifest_bytes)
        .map_err(|_| EngineError::msg("fella-pack.json is not UTF-8"))?
        .to_string();
    let m = Manifest::parse(&manifest_text)?;
    if m.id != id {
        return Err(EngineError::msg(format!(
            "catalog id '{id}' does not match the pack's own id '{}'",
            m.id
        )));
    }
    let payload_bytes = files
        .get(&m.payload)
        .ok_or_else(|| {
            EngineError::msg(format!("catalog entry is missing the payload file '{}'", m.payload))
        })?
        .clone();

    Ok(DownloadedPack {
        manifest_sha: sha256_hex(manifest_bytes),
        manifest_text,
        payload_bytes,
    })
}

/// Write a `download_pack` result to disk as a `marketplace` install.
pub fn install_downloaded(
    data_dir: &Path,
    conn: &rusqlite::Connection,
    d: &DownloadedPack,
) -> EngineResult<()> {
    write_pack(
        data_dir,
        conn,
        &d.manifest_text,
        &d.payload_bytes,
        "marketplace",
        Some(d.manifest_sha.clone()),
    )
}

pub fn remove(data_dir: &Path, conn: &rusqlite::Connection, id: &str) -> EngineResult<()> {
    sqlite::delete_extension(conn, id)?;
    let dir = pack_dir(data_dir, id);
    if dir.exists() {
        std::fs::remove_dir_all(&dir)
            .map_err(|e| EngineError::io(format!("remove {}", dir.display()), e))?;
    }
    Ok(())
}

/// Enable or disable a pack. Enabling a `theme` disables any other theme so
/// exactly one is ever active.
pub fn set_enabled(conn: &rusqlite::Connection, id: &str, enabled: bool) -> EngineResult<()> {
    let row = sqlite::list_extensions(conn)
        .into_iter()
        .find(|r| r.id == id)
        .ok_or_else(|| EngineError::msg(format!("no pack '{id}' is installed")))?;
    if enabled && row.kind == "theme" {
        sqlite::disable_all_themes(conn)?;
    }
    sqlite::set_extension_enabled(conn, id, enabled)?;
    Ok(())
}

/// The Markdown of every enabled `skill` pack, each capped, for the system
/// prompt.
pub fn enabled_skill_texts(data_dir: &Path, conn: &rusqlite::Connection) -> Vec<String> {
    sqlite::list_extensions(conn)
        .into_iter()
        .filter(|r| r.enabled && r.kind == "skill")
        .filter_map(|r| read_payload(data_dir, &r.id))
        .map(|t| cap_chars(&t, SKILL_CHAR_CAP))
        .filter(|t| !t.trim().is_empty())
        .collect()
}

/// The CSS token map of the enabled `theme` pack, if any. Keys are filtered to
/// `THEME_TOKEN_KEYS`; a passthrough `"appearance"` string is kept if present.
pub fn active_theme_tokens(
    data_dir: &Path,
    conn: &rusqlite::Connection,
) -> Option<BTreeMap<String, String>> {
    let row = sqlite::list_extensions(conn)
        .into_iter()
        .find(|r| r.enabled && r.kind == "theme")?;
    let raw = read_payload(data_dir, &row.id)?;
    let v: serde_json::Value = serde_json::from_str(&raw).ok()?;
    let obj = v.as_object()?;
    let mut out = BTreeMap::new();
    for (k, val) in obj {
        let keep = k == "appearance" || THEME_TOKEN_KEYS.contains(&k.as_str());
        if let (true, Some(s)) = (keep, val.as_str()) {
            out.insert(k.clone(), s.to_string());
        }
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

fn read_payload(data_dir: &Path, id: &str) -> Option<String> {
    let dir = pack_dir(data_dir, id);
    let manifest_text = std::fs::read_to_string(dir.join("fella-pack.json")).ok()?;
    let m: Manifest = serde_json::from_str(&manifest_text).ok()?;
    std::fs::read_to_string(dir.join(&m.payload)).ok()
}

// --- mcp connectors ----------------------------------------------------

/// How a connector attaches its credential to requests.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ConnectorAuth {
    /// No credential.
    #[default]
    None,
    /// `Authorization: Bearer <token>`. `secret` names the token in `Secrets`.
    Bearer { secret: String },
    /// A custom header. `secret` names the token in `Secrets`.
    Header { header: String, secret: String },
}

impl ConnectorAuth {
    /// The `Secrets` var name this auth reads, if any.
    pub fn secret_name(&self) -> Option<&str> {
        match self {
            ConnectorAuth::None => None,
            ConnectorAuth::Bearer { secret } | ConnectorAuth::Header { secret, .. } => Some(secret),
        }
    }
}

/// A `connector.json` payload (the `mcp` pack kind). HTTP transport only.
#[derive(Debug, Clone, Deserialize)]
pub struct ConnectorConfig {
    pub transport: String,
    pub url: String,
    #[serde(default)]
    pub auth: ConnectorAuth,
    #[serde(default)]
    pub setup: String,
}

impl ConnectorConfig {
    pub fn parse(text: &str) -> EngineResult<Self> {
        let c: ConnectorConfig = serde_json::from_str(text)
            .map_err(|e| EngineError::msg(format!("connector.json is not valid: {e}")))?;
        if c.transport != "http" {
            return Err(EngineError::msg(format!(
                "connector transport '{}' is not supported (only \"http\" for now)",
                c.transport
            )));
        }
        if !(c.url.starts_with("https://") || c.url.starts_with("http://")) {
            return Err(EngineError::msg("connector url must be an http(s) URL"));
        }
        Ok(c)
    }
}

/// `(pack id, config)` for every enabled `mcp` pack with a parseable
/// `connector.json`. A bad payload is skipped, not fatal.
pub fn enabled_mcp_connectors(
    data_dir: &Path,
    conn: &rusqlite::Connection,
) -> Vec<(String, ConnectorConfig)> {
    sqlite::list_extensions(conn)
        .into_iter()
        .filter(|r| r.enabled && r.kind == "mcp")
        .filter_map(|r| {
            let raw = read_payload(data_dir, &r.id)?;
            ConnectorConfig::parse(&raw).ok().map(|c| (r.id, c))
        })
        .collect()
}

/// The `connector.json` of one installed `mcp` pack (enabled or not).
pub fn connector_config(data_dir: &Path, id: &str) -> EngineResult<ConnectorConfig> {
    let raw = read_payload(data_dir, id)
        .ok_or_else(|| EngineError::msg(format!("no connector pack '{id}' is installed")))?;
    ConnectorConfig::parse(&raw)
}

fn cap_chars(s: &str, cap: usize) -> String {
    match s.char_indices().nth(cap) {
        Some((i, _)) => s[..i].to_string(),
        None => s.to_string(),
    }
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_bad_manifests() {
        let bad = [
            r#"{"schema":2,"id":"x","kind":"skill","name":"X","version":"1.0.0","description":"d","payload":"s.md"}"#,
            r#"{"schema":1,"id":"Bad_Id","kind":"skill","name":"X","version":"1.0.0","description":"d","payload":"s.md"}"#,
            r#"{"schema":1,"id":"x","kind":"nope","name":"X","version":"1.0.0","description":"d","payload":"s.md"}"#,
            r#"{"schema":1,"id":"x","kind":"skill","name":"X","version":"1.0.0","description":"d","payload":"../escape.md"}"#,
            r#"{"schema":1,"id":"x","kind":"theme","name":"X","version":"1.0.0","description":"d","payload":"t.md"}"#,
        ];
        for b in bad {
            assert!(Manifest::parse(b).is_err(), "should reject: {b}");
        }
    }

    #[test]
    fn accepts_a_good_manifest() {
        let ok = r#"{"schema":1,"id":"nord-theme","kind":"theme","name":"Nord",
            "version":"1.0.0","description":"cool","payload":"theme.json"}"#;
        let m = Manifest::parse(ok).unwrap();
        assert_eq!(m.id, "nord-theme");
        assert_eq!(m.kind, PackKind::Theme);
    }
}
