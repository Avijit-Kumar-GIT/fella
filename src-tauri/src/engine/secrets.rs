//! Where credentials live.
//!
//! Not the SQLite `settings` table: that file is easy to copy into a backup or
//! a sync folder, and a leaked key is worse than a leaked list of table names.
//! Instead an owner-only (`0600`) `auth.json` in the app data dir the same
//! shape `gh`, `aws`, `codex` and `fx` use for their credential files.
//!
//! An OS-keychain backend (macOS Keychain / Windows Credential Manager /
//! libsecret) is a reasonable future addition behind a build feature; it is
//! deliberately not a hard dependency because it fails closed on headless Linux
//! and WSL (no Secret Service) and pulls a large dependency tree.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::engine::error::{EngineError, EngineResult};

/// The on-disk credential store. Cheap to clone the handle; writes are
/// serialized through an internal lock.
pub struct Secrets {
    path: PathBuf,
    lock: Mutex<()>,
}

impl Secrets {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            path: data_dir.join("auth.json"),
            lock: Mutex::new(()),
        }
    }

    /// Parse `auth.json`. An empty map is returned **only** when the file
    /// genuinely isn't there yet; an unreadable or corrupt file is an error, so
    /// a follow-up write can't silently overwrite it and lose the keys it still
    /// holds. (fx draws the same line: "not found" and "unreadable" are
    /// different answers, only one of them safe to write over.)
    fn read(&self) -> EngineResult<BTreeMap<String, String>> {
        match std::fs::read_to_string(&self.path) {
            Ok(s) => serde_json::from_str(&s).map_err(|e| {
                EngineError::msg(format!(
                    "{} is corrupt ({e}) fix or delete it by hand; not overwriting it \
                     so any keys it still has are safe",
                    self.path.display()
                ))
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
            Err(e) => Err(EngineError::io(format!("read {}", self.path.display()), e)),
        }
    }

    /// Write the map by replacing `auth.json` atomically: a crash mid-write
    /// leaves the old file intact rather than a half-written one that `read`
    /// would then reject.
    fn write(&self, map: &BTreeMap<String, String>) -> EngineResult<()> {
        let body = serde_json::to_string_pretty(map).unwrap_or_else(|_| "{}".into());
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, body)
            .map_err(|e| EngineError::io(format!("write {}", tmp.display()), e))?;
        restrict(&tmp);
        std::fs::rename(&tmp, &self.path)
            .map_err(|e| EngineError::io(format!("replace {}", self.path.display()), e))?;
        restrict(&self.path);
        Ok(())
    }

    /// The API key stored for `provider`, if any. A missing or unreadable store
    /// reads as "no key" here the mutating paths are where an unreadable file
    /// must not be papered over.
    pub fn api_key(&self, provider: &str) -> Option<String> {
        self.read()
            .unwrap_or_default()
            .get(&entry(provider))
            .filter(|v| !v.is_empty())
            .cloned()
    }

    pub fn has(&self, provider: &str) -> bool {
        self.api_key(provider).is_some()
    }

    pub fn set_api_key(&self, provider: &str, value: &str) -> EngineResult<()> {
        let _g = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        let mut m = self.read()?;
        m.insert(entry(provider), value.trim().to_string());
        self.write(&m)
    }

    /// Remove the credential for `provider`. Returns whether anything was there.
    pub fn clear(&self, provider: &str) -> EngineResult<bool> {
        let _g = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        let mut m = self.read()?;
        let existed = m.remove(&entry(provider)).is_some();
        self.write(&m)?;
        Ok(existed)
    }
}

fn entry(provider: &str) -> String {
    format!("apikey:{provider}")
}

#[cfg(unix)]
fn restrict(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
}
#[cfg(not(unix))]
fn restrict(_path: &Path) {}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> PathBuf {
        let n = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p = std::env::temp_dir().join(format!("fella-secrets-{n}"));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn round_trip_and_clear() {
        let dir = tmp();
        let s = Secrets::new(&dir);

        assert!(!s.has("openai"));
        s.set_api_key("openai", "  sk-abc  ").unwrap();
        assert_eq!(s.api_key("openai").as_deref(), Some("sk-abc")); // trimmed
        assert!(s.has("openai"));

        // a second provider is independent
        s.set_api_key("xai", "xai-1").unwrap();
        assert_eq!(s.api_key("openai").as_deref(), Some("sk-abc"));

        assert!(s.clear("openai").unwrap());
        assert!(!s.has("openai"));
        assert!(!s.clear("openai").unwrap()); // already gone
        assert_eq!(s.api_key("xai").as_deref(), Some("xai-1"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_corrupt_store_is_never_silently_overwritten() {
        let dir = tmp();
        let s = Secrets::new(&dir);
        s.set_api_key("openai", "sk-keep-me").unwrap();

        // Something mangles auth.json (bad partial write, hand edit, disk gremlin).
        std::fs::write(dir.join("auth.json"), b"{not json").unwrap();

        // Reads degrade to "no key"...
        assert!(!s.has("openai"));
        // ...but a write refuses rather than clobbering the file.
        assert!(s.set_api_key("xai", "xai-1").is_err());
        assert!(s.clear("openai").is_err());
        assert_eq!(
            std::fs::read_to_string(dir.join("auth.json")).unwrap(),
            "{not json"
        );

        // Once the file is valid again, writes resume normally.
        std::fs::write(dir.join("auth.json"), b"{}").unwrap();
        s.set_api_key("xai", "xai-1").unwrap();
        assert_eq!(s.api_key("xai").as_deref(), Some("xai-1"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn file_is_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tmp();
        let s = Secrets::new(&dir);
        s.set_api_key("openai", "sk").unwrap();
        let mode = std::fs::metadata(dir.join("auth.json"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
