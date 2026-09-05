//! The `#[tauri::command]` surface callable from the UI via `invoke`.
//!
//! Keep this file a thin adapter: parse/validate arguments, call into
//! `engine::*`, map errors to strings. No business logic here.

use serde::Serialize;
use tauri::ipc::Channel;
use tauri::State;

use crate::engine::{
    Answer, AskEvent, Catalog, ConversationsInfo, EngineError, EngineResult, EngineState,
    InstalledPack, ProviderHealth, ProviderInfo, QueryResult, Settings, SourceInfo, UpdateStatus,
};
use crate::AppState;

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
    engine.open_workspace(std::path::Path::new(&path))
}

#[tauri::command]
pub fn get_catalog(engine: State<'_, EngineState>) -> Catalog {
    engine.catalog()
}

#[tauri::command]
pub fn describe(name: String, engine: State<'_, EngineState>) -> EngineResult<SourceInfo> {
    engine.describe_source(&name)
}

#[tauri::command]
pub fn run_sql_direct(sql: String, engine: State<'_, EngineState>) -> EngineResult<QueryResult> {
    engine.run_sql(&sql)
}

#[tauri::command]
pub async fn reindex(engine: State<'_, EngineState>) -> Result<Catalog, EngineError> {
    engine.reindex()
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

/// Forget the stored credential for `provider`.
#[tauri::command]
pub fn logout(provider: String, engine: State<'_, EngineState>) -> Result<Settings, EngineError> {
    engine.logout(&provider)
}

// --- packs (installed extensions) --------------------------------------

#[tauri::command]
pub fn packs_list(engine: State<'_, EngineState>) -> Vec<InstalledPack> {
    engine.packs_list()
}

#[tauri::command]
pub fn packs_add(
    path: String,
    engine: State<'_, EngineState>,
) -> Result<Vec<InstalledPack>, EngineError> {
    engine.packs_add(std::path::Path::new(&path))
}

#[tauri::command]
pub fn packs_remove(
    id: String,
    engine: State<'_, EngineState>,
) -> Result<Vec<InstalledPack>, EngineError> {
    engine.packs_remove(&id)
}

#[tauri::command]
pub fn packs_set_enabled(
    id: String,
    enabled: bool,
    engine: State<'_, EngineState>,
) -> Result<Vec<InstalledPack>, EngineError> {
    engine.packs_set_enabled(&id, enabled)
}

/// Install a pack from the marketplace by id.
#[tauri::command]
pub async fn packs_install(
    id: String,
    engine: State<'_, EngineState>,
) -> Result<Vec<InstalledPack>, EngineError> {
    engine.packs_install(&id).await
}

/// Check for a newer release and, if one exists, download + verify +
/// install it and exit. Only ever called by the user typing `/update` no
/// background/startup check.
#[tauri::command]
pub async fn update(
    app: tauri::AppHandle,
    engine: State<'_, EngineState>,
) -> Result<UpdateStatus, EngineError> {
    engine.update(app).await
}

/// Store the token an `mcp` connector pack needs.
#[tauri::command]
pub fn mcp_set_token(
    id: String,
    token: String,
    engine: State<'_, EngineState>,
) -> Result<(), EngineError> {
    engine.mcp_set_token(&id, &token)
}

/// Forget an `mcp` connector pack's token.
#[tauri::command]
pub fn mcp_clear_token(id: String, engine: State<'_, EngineState>) -> Result<bool, EngineError> {
    engine.mcp_clear_token(&id)
}

/// CSS token map of the active theme pack, or null. The UI applies it to
/// `document.documentElement`.
#[tauri::command]
pub fn packs_theme(
    engine: State<'_, EngineState>,
) -> Option<std::collections::BTreeMap<String, String>> {
    engine.packs_theme()
}

// --- ask (the agent loop) -------------------------------------------------

/// `model` is the calling tab's chosen model (of the one signed-in provider);
/// `None` uses the saved default.
#[tauri::command]
pub async fn ask(
    conversation_id: String,
    question: String,
    model: Option<String>,
    channel: Channel<AskEvent>,
    engine: State<'_, EngineState>,
) -> Result<Answer, EngineError> {
    engine
        .ask(&conversation_id, &question, model.as_deref(), move |ev| {
            let _ = channel.send(ev);
        })
        .await
}

#[tauri::command]
pub async fn ollama_health(engine: State<'_, EngineState>) -> Result<ProviderHealth, EngineError> {
    Ok(engine.provider_health().await)
}

/// Is a local Ollama running, whatever the configured provider is?
#[tauri::command]
pub async fn probe_ollama(engine: State<'_, EngineState>) -> Result<ProviderHealth, EngineError> {
    Ok(engine.probe_ollama().await)
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
