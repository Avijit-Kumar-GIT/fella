//! Fella's Rust analytics engine and its Electron JSON-lines adapter.

pub mod engine;
pub mod stdio;

/// Start the engine-only JSON-lines bridge used by the Electron shell.
pub fn run_engine_stdio(data_dir: &std::path::Path) -> Result<(), String> {
    // Preserve local settings, credentials, and history from the former app
    // identifier before initializing the engine under the Fella name.
    migrate_from_woody(data_dir);
    stdio::run(data_dir)
}

/// One-time: the app shipped as "Woody" with identifier `dev.woody.app`. The
/// first launch under the new identifier moves the old data dir's contents
/// (`auth.json`, the settings db, and saved conversations) into the
/// new location, so the rename doesn't cost anyone their keys or history. Only
/// runs into a fresh install; never overwrites.
fn migrate_from_woody(new_dir: &std::path::Path) {
    let Some(old_dir) = new_dir.parent().map(|p| p.join("dev.woody.app")) else {
        return;
    };
    if old_dir == new_dir || !old_dir.is_dir() {
        return;
    }
    if new_dir.join("auth.json").exists() || new_dir.join("fella.db").exists() {
        return;
    }
    let _ = std::fs::create_dir_all(new_dir);
    let Ok(entries) = std::fs::read_dir(&old_dir) else {
        return;
    };
    let mut moved = 0usize;
    for entry in entries.flatten() {
        let from = entry.path();
        // Rename the settings db as it moves; leave everything else as-is.
        let name = entry
            .file_name()
            .to_string_lossy()
            .replacen("woody.db", "fella.db", 1);
        let to = new_dir.join(name);
        if to.exists() {
            continue;
        }
        if std::fs::rename(&from, &to).is_ok() || copy_tree(&from, &to).is_ok() {
            moved += 1;
        }
    }
    if moved > 0 {
        log::info!("migrated {moved} item(s) from the old dev.woody.app data dir");
    }
}

fn copy_tree(from: &std::path::Path, to: &std::path::Path) -> std::io::Result<()> {
    if from.is_dir() {
        std::fs::create_dir_all(to)?;
        for entry in std::fs::read_dir(from)? {
            let entry = entry?;
            copy_tree(&entry.path(), &to.join(entry.file_name()))?;
        }
        Ok(())
    } else {
        std::fs::copy(from, to).map(|_| ())
    }
}
