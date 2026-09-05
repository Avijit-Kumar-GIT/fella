//! Fella desktop runtime.
//!
//! The UI is a thin presentation layer; everything meaningful happens here in
//! Rust. The analytical engine (catalog, data engine, tools, agent loop) lives under
//! `engine/`.

mod commands;
pub mod engine;

/// Small process-wide bits not owned by the engine.
pub struct AppState {
    pub started: std::time::Instant,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            started: std::time::Instant::now(),
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            use tauri::Manager;

            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }

            let data_dir = app.path().app_data_dir()?;
            migrate_from_woody(&data_dir);
            let engine = engine::EngineState::new(&data_dir)
                .map_err(|e| format!("engine init failed: {e}"))?;
            app.manage(engine);

            Ok(())
        })
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            commands::ping,
            commands::app_info,
            commands::app_ready,
            commands::open_workspace,
            commands::get_catalog,
            commands::describe,
            commands::run_sql_direct,
            commands::reindex,
            commands::get_settings,
            commands::set_settings,
            commands::list_providers,
            commands::set_api_key,
            commands::logout,
            commands::ollama_health,
            commands::probe_ollama,
            commands::ask,
            commands::cancel,
            commands::forget_conversation,
            commands::packs_list,
            commands::packs_add,
            commands::packs_remove,
            commands::packs_set_enabled,
            commands::packs_install,
            commands::packs_theme,
            commands::mcp_set_token,
            commands::mcp_clear_token,
            commands::archive_conversation,
            commands::conversations_info,
            commands::conversations_list,
            commands::conversation_load,
            commands::update,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// One-time: the app shipped as "Woody" with identifier `dev.woody.app`. The
/// first launch under the new identifier moves the old data dir's contents
/// (`auth.json`, the settings db, saved conversations, installed packs) into the
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
        return; // already set up under the new name
    }
    let _ = std::fs::create_dir_all(new_dir);
    let Ok(entries) = std::fs::read_dir(&old_dir) else {
        return;
    };
    let mut moved = 0usize;
    for entry in entries.flatten() {
        let from = entry.path();
        // Rename the settings db as it moves; leave everything else as-is.
        let name = entry.file_name().to_string_lossy().replacen("woody.db", "fella.db", 1);
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
