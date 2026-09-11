//! Augment capabilities: the small first-party surfaces an `augment` pack can
//! switch on (`docs/EXTENSIBILITY.md`). Each writes exactly one user-authored
//! file into the open workspace, and only when the user types in the view.
//!
//! The agent's tool set is unchanged there is still no write tool. This
//! module is the *only* path from a pack to a workspace write, and every write
//! goes through `resolve_in_workspace`, which refuses anything outside the open
//! folder or with a disallowed extension.

use std::path::{Component, Path, PathBuf};

use crate::engine::error::{EngineError, EngineResult};
use crate::engine::extensions::AUGMENT_FILE_EXTS;

/// Capabilities this build ships. An `augment.json` naming anything else still
/// loads, but is reported unsupported the compatibility contract in
/// `docs/EXTENSIBILITY.md`.
///
/// `buffer` is a plain-text tab; `grid` is a small editable table. Both persist
/// through `write_buffer` the frontend serialises the grid to CSV text before
/// it gets here so the capability only picks which view renders.
pub const CAPABILITIES: &[&str] = &["buffer", "grid"];

/// Resolve `rel` to an absolute path inside `workspace`, or refuse. Guards
/// against `..`, absolute paths, a disallowed extension, and a symlinked parent
/// directory that would land the write outside the folder.
fn resolve_in_workspace(workspace: &Path, rel: &str) -> EngineResult<PathBuf> {
    let p = Path::new(rel);
    let shape_ok = !rel.is_empty()
        && !p.is_absolute()
        && p.components().all(|c| matches!(c, Component::Normal(_)));
    let ext_ok = rel
        .rsplit('.')
        .next()
        .map(|e| AUGMENT_FILE_EXTS.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false);
    if !shape_ok || !ext_ok {
        return Err(EngineError::msg(
            "that file path isn't allowed for an augment it must be inside the folder \
             and end in .md, .txt, .csv or .tsv",
        ));
    }

    let root = std::fs::canonicalize(workspace)
        .map_err(|e| EngineError::io(format!("resolve {}", workspace.display()), e))?;
    let target = root.join(p);

    // The file may not exist yet, but its parent directory must and its
    // real path must sit under the workspace root (defends against a symlinked
    // subdirectory pointing elsewhere).
    let parent = target.parent().unwrap_or(&root);
    let parent_canon = std::fs::canonicalize(parent)
        .map_err(|e| EngineError::io(format!("the folder for '{rel}' doesn't exist yet"), e))?;
    if !parent_canon.starts_with(&root) {
        return Err(EngineError::msg("that path would write outside the open folder"));
    }

    let name = target
        .file_name()
        .ok_or_else(|| EngineError::msg("that augment file has no name"))?;
    Ok(parent_canon.join(name))
}

/// Write `contents` to `rel` inside `workspace`, atomically (hidden tmp file +
/// rename, so a crash mid-write leaves the previous version intact). Returns the
/// absolute path written. The one path from a pack to a workspace write.
pub fn write_buffer(workspace: &Path, rel: &str, contents: &str) -> EngineResult<PathBuf> {
    let path = resolve_in_workspace(workspace, rel)?;
    let fname = path.file_name().and_then(|n| n.to_str()).unwrap_or("buf");
    let tmp = path.with_file_name(format!(".{fname}.fella-tmp"));

    std::fs::write(&tmp, contents)
        .map_err(|e| EngineError::io(format!("write {}", tmp.display()), e))?;
    std::fs::rename(&tmp, &path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        EngineError::io(format!("replace {}", path.display()), e)
    })?;
    Ok(path)
}

/// Read `rel` back from `workspace` for the editor. `Ok(None)` if the file
/// doesn't exist yet (a fresh augment).
pub fn read_buffer(workspace: &Path, rel: &str) -> EngineResult<Option<String>> {
    let path = resolve_in_workspace(workspace, rel)?;
    match std::fs::read_to_string(&path) {
        Ok(s) => Ok(Some(s)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(EngineError::io(format!("read {}", path.display()), e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_ws(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("fella-aug-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::canonicalize(&d).unwrap()
    }

    #[test]
    fn rejects_escape_and_bad_ext() {
        let ws = tmp_ws("reject");
        assert!(resolve_in_workspace(&ws, "../escape.md").is_err());
        assert!(resolve_in_workspace(&ws, "/etc/passwd.md").is_err());
        assert!(resolve_in_workspace(&ws, "notes.exe").is_err());
        assert!(resolve_in_workspace(&ws, "").is_err());
        assert!(resolve_in_workspace(&ws, "notes.md").is_ok());
        std::fs::create_dir_all(ws.join("sub")).unwrap();
        assert!(resolve_in_workspace(&ws, "sub/notes.md").is_ok());
        assert!(resolve_in_workspace(&ws, "missing/notes.md").is_err());
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn atomic_write_and_read_roundtrip() {
        let ws = tmp_ws("roundtrip");
        let p = write_buffer(&ws, "notes.md", "hello").unwrap();
        assert!(p.starts_with(&ws));
        assert_eq!(read_buffer(&ws, "notes.md").unwrap().as_deref(), Some("hello"));
        write_buffer(&ws, "notes.md", "world").unwrap();
        assert_eq!(read_buffer(&ws, "notes.md").unwrap().as_deref(), Some("world"));
        // no stray tmp file left behind
        let leftovers: Vec<_> = std::fs::read_dir(&ws)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains("fella-tmp"))
            .collect();
        assert!(leftovers.is_empty());
        assert_eq!(read_buffer(&ws, "absent.md").unwrap(), None);
        let _ = std::fs::remove_dir_all(&ws);
    }
}
