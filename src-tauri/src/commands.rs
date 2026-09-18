//! The `#[tauri::command]` surface callable from the UI via `invoke`.
//!
//! Keep this file a thin adapter: parse/validate arguments, call into
//! `engine::*`, map errors to strings. No business logic here.

use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{State, Window};

use crate::engine::{
    Answer, AskEvent, Catalog, ConversationSummary, ConversationsInfo, EngineError, EngineResult,
    EngineState, ProviderHealth, ProviderInfo, QueryResult, Settings, SourceInfo, UpdateStatus,
};
use crate::AppState;

/// Expand a leading `~` (or `~/…`, `~\…`) to the user's home directory. Typed
/// paths come from the composer, a text field, not a shell, so nothing else
/// expands this the way a terminal would. Used by every command that takes a
/// user-typed filesystem path (`open_workspace`).
fn expand_tilde(path: &str) -> std::path::PathBuf {
    if let Some(rest) = path.strip_prefix('~') {
        if rest.is_empty() || rest.starts_with('/') || rest.starts_with('\\') {
            if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))
            {
                return std::path::Path::new(&home).join(rest.trim_start_matches(['/', '\\']));
            }
        }
    }
    std::path::PathBuf::from(path)
}

/// Cheap liveness check used by the UI on startup.
#[tauri::command]
pub fn ping() -> &'static str {
    "pong"
}

#[derive(Serialize)]
pub struct AppInfo {
    pub name: &'static str,
    pub version: &'static str,
    pub uptime_ms: u128,
}

#[tauri::command]
pub fn app_info(state: State<'_, AppState>) -> AppInfo {
    AppInfo {
        name: "fella",
        version: env!("CARGO_PKG_VERSION"),
        uptime_ms: state.started.elapsed().as_millis(),
    }
}

/// Called once from the UI's `onMount`: the moment the app is interactive.
/// Prints to stderr (works in release, where the logger is disabled) so
/// `scripts/measure.sh` can read the cold-start time.
#[tauri::command]
pub fn app_ready(state: State<'_, AppState>) -> u128 {
    let ms = state.started.elapsed().as_millis();
    eprintln!("fella: interactive in {ms} ms");
    ms
}

// --- workspace / data --------------------------------------------------------

#[tauri::command]
pub async fn open_workspace(
    path: String,
    engine: State<'_, EngineState>,
) -> Result<Catalog, EngineError> {
    engine.open_workspace(&expand_tilde(&path))
}

#[tauri::command]
pub fn get_catalog(engine: State<'_, EngineState>) -> Catalog {
    engine.catalog()
}

/// Path of the folder from the last session, if it still exists. The welcome
/// screen uses it for a one-click "reopen" button; Fella no longer opens it
/// automatically on launch. `null` if there's no history or it's gone.
#[tauri::command]
pub fn last_workspace_path(engine: State<'_, EngineState>) -> Option<String> {
    engine.last_workspace_path()
}

#[tauri::command]
pub fn describe(name: String, engine: State<'_, EngineState>) -> EngineResult<SourceInfo> {
    engine.describe_source(&name)
}

#[tauri::command]
pub fn sample_source(
    name: String,
    rows: Option<usize>,
    engine: State<'_, EngineState>,
) -> EngineResult<QueryResult> {
    engine.sample(&name, rows.unwrap_or(5).clamp(1, 50))
}

#[tauri::command]
pub fn run_sql_direct(sql: String, engine: State<'_, EngineState>) -> EngineResult<QueryResult> {
    engine.run_sql(&sql)
}

#[tauri::command]
pub async fn reindex(engine: State<'_, EngineState>) -> Result<Catalog, EngineError> {
    engine.reindex()
}

/// `[path, contents_or_null]` for the current folder's `memory.md` (`/memory`).
/// `null` when no folder is open.
#[tauri::command]
pub fn memory_file(engine: State<'_, EngineState>) -> Option<(String, Option<String>)> {
    engine.folder_memory_file()
}

/// Delete the current folder's learned notes. `true` if a file was removed.
#[tauri::command]
pub fn forget_memory(engine: State<'_, EngineState>) -> EngineResult<bool> {
    engine.forget_folder_memory()
}

// --- settings --------------------------------------------------------------

#[tauri::command]
pub fn get_settings(engine: State<'_, EngineState>) -> Settings {
    engine.settings()
}

#[tauri::command]
pub fn set_settings(
    settings: serde_json::Value,
    engine: State<'_, EngineState>,
) -> EngineResult<Settings> {
    let obj = settings
        .as_object()
        .ok_or_else(|| EngineError::msg("settings must be an object"))?;
    engine.save_settings(obj)
}

// --- providers / auth ----------------------------------------------------

/// The built-in providers and whether each is signed in.
#[tauri::command]
pub fn list_providers(engine: State<'_, EngineState>) -> Vec<ProviderInfo> {
    engine.list_providers()
}

/// Store an API key for `provider` and switch to it.
#[tauri::command]
pub fn set_api_key(
    provider: String,
    key: String,
    engine: State<'_, EngineState>,
) -> Result<Settings, EngineError> {
    engine.set_api_key(&provider, &key)
}

/// Stop using `provider`; `forget` also deletes its stored key.
#[tauri::command]
pub fn logout(
    provider: String,
    forget: bool,
    engine: State<'_, EngineState>,
) -> Result<Settings, EngineError> {
    engine.logout(&provider, forget)
}

// --- update --------------------------------------------------------------

/// Check for a newer release and, if one exists, download + verify +
/// install it and exit. Only ever called by the user typing `/update`.
#[tauri::command]
pub async fn update(
    app: tauri::AppHandle,
    engine: State<'_, EngineState>,
) -> Result<UpdateStatus, EngineError> {
    engine.update(app).await
}

// --- user context -------------------------------------------------------

/// `[path, contents_or_null]` for the current workspace's `fella.md`.
#[tauri::command]
pub fn context_file(engine: State<'_, EngineState>) -> Option<(String, Option<String>)> {
    engine.context_file()
}

/// Save the explicitly user-authored `fella.md` context file.
#[tauri::command]
pub fn save_context(contents: String, engine: State<'_, EngineState>) -> Result<(), EngineError> {
    engine.save_context(&contents)
}

// --- ask (the agent loop) -------------------------------------------------

/// `model` is the calling tab's chosen model (of the one signed-in provider);
/// `None` uses the saved default.
#[tauri::command]
pub async fn ask(
    conversation_id: String,
    question: String,
    model: Option<String>,
    mode: Option<String>,
    channel: Channel<AskEvent>,
    engine: State<'_, EngineState>,
) -> Result<Answer, EngineError> {
    let inspect = mode.as_deref() == Some("inspect");
    engine
        .ask_with_mode(
            &conversation_id,
            &question,
            model.as_deref(),
            inspect,
            move |ev| {
                let _ = channel.send(ev);
            },
        )
        .await
}

#[tauri::command]
pub async fn provider_health(
    engine: State<'_, EngineState>,
) -> Result<ProviderHealth, EngineError> {
    Ok(engine.provider_health().await)
}

/// Keep the native window surface in step with the webview when the user
/// chooses an explicit appearance. This matters on frameless Windows windows
/// and during a theme switch, where a strip outside the document can otherwise
/// flash the old color.
#[tauri::command]
pub fn set_window_appearance(window: Window, dark: bool) -> Result<(), String> {
    let color = if dark {
        tauri::webview::Color(14, 14, 16, 255)
    } else {
        tauri::webview::Color(252, 252, 251, 255)
    };
    window
        .set_background_color(Some(color))
        .map_err(|error| error.to_string())
}

// --- conversation archive ----------------------------------------------------

/// Write a finished transcript to `<app data dir>/conversations/`. The UI calls
/// this when a conversation ends (restart, `/clear`); returns the file path.
#[tauri::command]
pub fn archive_conversation(
    id: String,
    body: String,
    engine: State<'_, EngineState>,
) -> Result<String, EngineError> {
    engine.archive_conversation(&id, &body)
}

/// Where archived conversations live, and how many there are (for `/history`).
#[tauri::command]
pub fn conversations_info(engine: State<'_, EngineState>) -> ConversationsInfo {
    engine.conversations_info()
}

/// Every archived conversation, newest first, enough to pick one from
/// without knowing its id.
#[tauri::command]
pub fn conversations_list(engine: State<'_, EngineState>) -> Vec<ConversationSummary> {
    engine.conversations_list()
}

/// Raw JSON of one archived conversation, by id (see `conversations_list`).
#[tauri::command]
pub fn conversation_load(
    id: String,
    engine: State<'_, EngineState>,
) -> Result<String, EngineError> {
    engine.conversation_load(&id)
}

/// Remove one archived conversation. Used by the sidebar's per-row delete.
#[tauri::command]
pub fn delete_conversation(id: String, engine: State<'_, EngineState>) -> Result<(), EngineError> {
    engine.delete_conversation(&id)
}

/// Set (empty string clears) a custom title for one archived conversation.
/// Used by the sidebar's per-row rename, for a conversation not necessarily
/// open in a live tab right now.
#[tauri::command]
pub fn rename_conversation(
    id: String,
    title: String,
    engine: State<'_, EngineState>,
) -> Result<(), EngineError> {
    engine.rename_conversation(&id, &title)
}

/// Stop the in-progress `ask` for one conversation (tab).
#[tauri::command]
pub fn cancel(conversation_id: String, engine: State<'_, EngineState>) {
    engine.cancel_run(&conversation_id);
}

/// Drop a closed tab's distilled memory and stop-flag from the engine.
#[tauri::command]
pub fn forget_conversation(conversation_id: String, engine: State<'_, EngineState>) {
    engine.forget_conversation(&conversation_id);
}

/// Make sure the OS mouse cursor is visible before the UI opens a modal native
/// dialog (the folder picker). On Windows, "hide pointer while typing" lowers an
/// internal per-thread counter and normally raises it again on the next
/// mouse-move but a modal dialog opened straight from a keystroke (`/open` +
/// Enter) grabs the message loop first, so the pointer stays invisible for the
/// whole time the picker is up. Raise the counter back to non-negative here.
/// Every non-Windows target compiles this to an empty body.
#[tauri::command]
pub fn unhide_cursor() {
    #[cfg(windows)]
    // SAFETY: ShowCursor is a thread-safe Win32 call with no preconditions.
    unsafe {
        use windows::Win32::UI::WindowsAndMessaging::ShowCursor;
        // ShowCursor(TRUE) increments the counter and returns the new value;
        // the cursor is shown once it's >= 0. Stop as soon as it is, and bound
        // the loop so a pathological starting value can't spin.
        for _ in 0..16 {
            if ShowCursor(true.into()) >= 0 {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tilde_tests {
    use super::expand_tilde;

    #[test]
    fn expands_home_relative_paths_only() {
        std::env::set_var("HOME", "/home/somebody");
        assert_eq!(
            expand_tilde("~"),
            std::path::PathBuf::from("/home/somebody")
        );
        assert_eq!(
            expand_tilde("~/Downloads/pack"),
            std::path::PathBuf::from("/home/somebody/Downloads/pack")
        );
        // A bare relative or absolute path passes through untouched.
        assert_eq!(expand_tilde("./pack"), std::path::PathBuf::from("./pack"));
        assert_eq!(
            expand_tilde("/tmp/pack"),
            std::path::PathBuf::from("/tmp/pack")
        );
        // "~foo" (another user's home) is left alone, same as a shell with no
        // matching user would leave it we don't try to resolve /etc/passwd.
        assert_eq!(
            expand_tilde("~foo/pack"),
            std::path::PathBuf::from("~foo/pack")
        );
    }
}
