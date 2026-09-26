//! Line-delimited JSON bridge used by the Electron shell.
//!
//! The Tauri shell calls the same `EngineState` methods through
//! `#[tauri::command]`. Electron cannot call those commands directly, so the
//! Rust engine exposes the identical command names over stdin/stdout instead.
//! stdout is reserved for JSON responses; diagnostics stay on stderr.

use std::io::{self, BufRead, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::engine::{EngineError, EngineResult, EngineState};

type Output = Arc<Mutex<BufWriter<io::Stdout>>>;

#[derive(Debug, Deserialize)]
struct Request {
    id: u64,
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Debug, Serialize)]
struct Response {
    id: u64,
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<EngineError>,
}

fn emit(output: &Output, value: Value) {
    let Ok(mut output) = output.lock() else {
        return;
    };
    if serde_json::to_writer(&mut *output, &value).is_ok() {
        let _ = output.write_all(b"\n");
        let _ = output.flush();
    }
}

fn respond(output: &Output, id: u64, result: EngineResult<Value>) {
    match result {
        Ok(result) => emit(
            output,
            serde_json::to_value(Response {
                id,
                ok: true,
                result: Some(result),
                error: None,
            })
            .unwrap_or_else(|_| Value::Null),
        ),
        Err(error) => emit(
            output,
            serde_json::to_value(Response {
                id,
                ok: false,
                result: None,
                error: Some(error),
            })
            .unwrap_or_else(|_| Value::Null),
        ),
    }
}

fn event(output: &Output, id: u64, value: impl Serialize) {
    emit(
        output,
        serde_json::json!({
            "id": id,
            "event": value,
        }),
    );
}

fn serialized<T: Serialize>(value: T) -> EngineResult<Value> {
    serde_json::to_value(value).map_err(EngineError::from)
}

fn required<T: DeserializeOwned>(params: &Value, name: &str) -> EngineResult<T> {
    let value = params
        .get(name)
        .cloned()
        .ok_or_else(|| EngineError::msg(format!("missing IPC argument: {name}")))?;
    serde_json::from_value(value).map_err(EngineError::from)
}

fn expand_tilde(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix('~') {
        if rest.is_empty() || rest.starts_with('/') || rest.starts_with('\\') {
            if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))
            {
                return Path::new(&home).join(rest.trim_start_matches(['/', '\\']));
            }
        }
    }
    PathBuf::from(path)
}

fn value_result<T: Serialize>(result: EngineResult<T>) -> EngineResult<Value> {
    result.and_then(serialized)
}

async fn dispatch(
    request: Request,
    engine: Arc<EngineState>,
    output: Output,
    started: Instant,
) -> EngineResult<()> {
    let id = request.id;
    let result = match request.method.as_str() {
        "ping" => Ok(Value::String("pong".into())),
        "app_info" => serialized(serde_json::json!({
            "name": "fella",
            "version": env!("CARGO_PKG_VERSION"),
            "uptime_ms": started.elapsed().as_millis(),
        })),
        "app_ready" => {
            let ms = started.elapsed().as_millis();
            eprintln!("fella: interactive in {ms} ms");
            serialized(ms)
        }
        "open_workspace" => {
            let path: String = required(&request.params, "path")?;
            value_result(engine.open_workspace(&expand_tilde(&path)))
        }
        "get_catalog" => serialized(engine.catalog()),
        "last_workspace_path" => serialized(engine.last_workspace_path()),
        "describe" => {
            let name: String = required(&request.params, "name")?;
            value_result(engine.describe_source(&name))
        }
        "sample_source" => {
            let name: String = required(&request.params, "name")?;
            let rows = request
                .params
                .get("rows")
                .and_then(Value::as_u64)
                .and_then(|rows| usize::try_from(rows).ok())
                .unwrap_or(5)
                .clamp(1, 50);
            value_result(engine.sample(&name, rows))
        }
        "run_sql_direct" => {
            let sql: String = required(&request.params, "sql")?;
            value_result(engine.run_sql(&sql))
        }
        "reindex" => value_result(engine.reindex()),
        "memory_file" => serialized(engine.folder_memory_file()),
        "forget_memory" => value_result(engine.forget_folder_memory()),
        "get_settings" => serialized(engine.settings()),
        "set_settings" => {
            let settings = request
                .params
                .get("settings")
                .and_then(Value::as_object)
                .ok_or_else(|| EngineError::msg("settings must be an object"))?;
            value_result(engine.save_settings(settings))
        }
        "list_providers" => serialized(engine.list_providers()),
        "set_api_key" => {
            let provider: String = required(&request.params, "provider")?;
            let key: String = required(&request.params, "key")?;
            value_result(engine.set_api_key(&provider, &key))
        }
        "logout" => {
            let provider: String = required(&request.params, "provider")?;
            let forget = request
                .params
                .get("forget")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            value_result(engine.logout(&provider, forget))
        }
        "provider_health" => value_result(Ok(engine.provider_health().await)),
        "set_window_appearance" | "unhide_cursor" => Ok(Value::Null),
        "cancel" => {
            let conversation_id: String = required(&request.params, "conversationId")?;
            engine.cancel_run(&conversation_id);
            Ok(Value::Null)
        }
        "forget_conversation" => {
            let conversation_id: String = required(&request.params, "conversationId")?;
            engine.forget_conversation(&conversation_id);
            Ok(Value::Null)
        }
        "context_file" => serialized(engine.context_file()),
        "save_context" => {
            let contents: String = required(&request.params, "contents")?;
            value_result(engine.save_context(&contents))
        }
        "archive_conversation" => {
            let id: String = required(&request.params, "id")?;
            let body: String = required(&request.params, "body")?;
            value_result(engine.archive_conversation(&id, &body))
        }
        "conversations_info" => serialized(engine.conversations_info()),
        "conversations_list" => serialized(engine.conversations_list()),
        "conversation_load" => {
            let id: String = required(&request.params, "id")?;
            value_result(engine.conversation_load(&id))
        }
        "delete_conversation" => {
            let id: String = required(&request.params, "id")?;
            value_result(engine.delete_conversation(&id))
        }
        "rename_conversation" => {
            let id: String = required(&request.params, "id")?;
            let title: String = required(&request.params, "title")?;
            value_result(engine.rename_conversation(&id, &title))
        }
        "update" => value_result(engine.check_update().await),
        "ask" => {
            let conversation_id: String = required(&request.params, "conversationId")?;
            let question: String = required(&request.params, "question")?;
            let model = request
                .params
                .get("model")
                .and_then(Value::as_str)
                .map(str::to_owned);
            let inspect = request.params.get("mode").and_then(Value::as_str) == Some("inspect");
            let events = output.clone();
            let answer = engine
                .ask_with_mode(
                    &conversation_id,
                    &question,
                    model.as_deref(),
                    inspect,
                    move |item| event(&events, id, item),
                )
                .await;
            value_result(answer)
        }
        method => Err(EngineError::msg(format!("unknown IPC command: {method}"))),
    };
    respond(&output, id, result);
    Ok(())
}

/// Run the engine as a long-lived JSON-lines child process for Electron.
pub fn run(data_dir: &Path) -> Result<(), String> {
    let engine = Arc::new(EngineState::new(data_dir).map_err(|error| error.to_string())?);
    let output = Arc::new(Mutex::new(BufWriter::new(io::stdout())));
    let started = Instant::now();

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .worker_threads(2)
        .build()
        .map_err(|error| format!("create engine runtime: {error}"))?;

    runtime.block_on(async move {
        let (requests, mut incoming) = tokio::sync::mpsc::unbounded_channel::<String>();
        std::thread::spawn(move || {
            let stdin = io::stdin();
            for line in stdin.lock().lines() {
                match line {
                    Ok(line) => {
                        if requests.send(line).is_err() {
                            break;
                        }
                    }
                    Err(error) => {
                        eprintln!("fella engine stdin: {error}");
                        break;
                    }
                }
            }
        });

        let mut tasks = Vec::new();
        while let Some(line) = incoming.recv().await {
            let request = match serde_json::from_str::<Request>(&line) {
                Ok(request) => request,
                Err(error) => {
                    eprintln!("fella engine request: {error}");
                    continue;
                }
            };
            tasks.push(tokio::spawn(dispatch(
                request,
                Arc::clone(&engine),
                Arc::clone(&output),
                started,
            )));
        }

        for task in tasks {
            let _ = task.await;
        }
    });

    Ok(())
}
