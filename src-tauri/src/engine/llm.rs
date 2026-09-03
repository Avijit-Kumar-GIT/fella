//! Model-agnostic chat client. One struct, branching on the configured
//! provider Ollama by default, any OpenAI-compatible endpoint as an
//! override. The Ollama path streams tokens as they arrive; OpenAI-compatible
//! endpoints deliver the reply in one delta.

use futures_util::StreamExt;
use serde::Serialize;
use serde_json::{json, Value as Json};
use std::time::Duration;

use crate::engine::error::{EngineError, EngineResult};
use crate::engine::sqlite::Settings;

/// Per-call ceiling for a model request. A healthy chat turn is a few seconds;
/// past this the model (rate-limited, overloaded, or wedged) has stalled and it's
/// better to fail loudly than leave the UI on `thinking…`.
fn request_timeout() -> Duration {
    std::env::var("FELLA_MODEL_TIMEOUT_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .map(Duration::from_secs)
        .unwrap_or(Duration::from_secs(100))
}

/// How many times to retry a transient model failure before giving up.
fn retry_budget() -> u32 {
    std::env::var("FELLA_MODEL_RETRIES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(3)
}

/// How long Ollama should keep the model resident in memory after a call. The
/// default (5 min) drops it between questions, so the next question eats a
/// 10-20 s reload; `"30m"` keeps a conversation warm. `FELLA_OLLAMA_KEEP_ALIVE`
/// overrides (any Ollama duration string, e.g. `"-1"` for "never unload").
fn ollama_keep_alive() -> String {
    std::env::var("FELLA_OLLAMA_KEEP_ALIVE")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| "30m".to_string())
}

/// Ollama context window (`num_ctx`). Ollama's own default is 2-4k tokens
/// smaller than Fella's system prompt + tool schemas + history, so the tail
/// (the schema, or the question) is silently truncated and the model looks
/// dumb. 8192 fits the prompt with room for a few tool-result rounds.
/// `FELLA_OLLAMA_NUM_CTX` overrides (bigger = smarter but slower first token
/// and more RAM).
fn ollama_num_ctx() -> u32 {
    std::env::var("FELLA_OLLAMA_NUM_CTX")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|&n: &u32| n >= 512)
        .unwrap_or(8192)
}

/// Cap on tokens the model may generate in one turn (`num_predict` on Ollama,
/// `max_tokens` on the OpenAI wire). A tool call or a normal answer fits well
/// under 1024; this only bounds a runaway. `FELLA_MODEL_MAX_OUTPUT` overrides.
fn max_output_tokens() -> u32 {
    std::env::var("FELLA_MODEL_MAX_OUTPUT")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|&n: &u32| n >= 64)
        .unwrap_or(1024)
}

/// Whether to let a reasoning model (gpt-oss, deepseek-r1, qwen3, …) emit its
/// chain-of-thought. Fella never shows it, and generating it adds seconds to
/// every turn's first token, so the loop asks for it off. `FELLA_OLLAMA_THINK=1`
/// re-enables it. Ignored by models that don't think.
fn ollama_think() -> bool {
    matches!(
        std::env::var("FELLA_OLLAMA_THINK").ok().as_deref(),
        Some("1") | Some("true") | Some("yes")
    )
}

/// Worth another attempt: rate limiting and transient upstream errors.
fn is_retryable(status: reqwest::StatusCode) -> bool {
    status == reqwest::StatusCode::TOO_MANY_REQUESTS
        || matches!(status.as_u16(), 500 | 502 | 503 | 504)
}

/// Wait before retry `n` (1-based) when the server gave no `Retry-After`.
fn backoff(n: u32) -> Duration {
    Duration::from_secs(match n {
        1 => 2,
        2 => 5,
        _ => 10,
    })
}

/// `Retry-After` as whole seconds, if present and numeric (the delta-seconds form).
fn retry_after_secs(headers: &reqwest::header::HeaderMap) -> Option<u64> {
    headers
        .get(reqwest::header::RETRY_AFTER)?
        .to_str()
        .ok()?
        .trim()
        .parse()
        .ok()
}

/// Notified just before each retry with a ready-to-display line ("the model is
/// busy retrying in 3s…"). The agent forwards it to the transcript; other
/// callers (embeddings) pass a no-op.
pub type RetryNotify<'a> = &'a (dyn Fn(&str) + Send + Sync);

#[derive(Debug, Clone)]
pub enum ChatMessage {
    System(String),
    User(String),
    Assistant {
        content: String,
        tool_calls: Vec<ToolCall>,
    },
    Tool {
        call_id: String,
        name: String,
        content: String,
    },
}

#[derive(Debug, Clone)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Json,
}

#[derive(Debug, Clone, Default)]
pub struct ChatResponse {
    pub content: String,
    pub tool_calls: Vec<ToolCall>,
}

#[derive(Debug, Clone)]
pub struct ToolSchema {
    pub name: String,
    pub description: String,
    pub parameters: Json,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProviderHealth {
    pub reachable: bool,
    /// The probe got a 401/403 the endpoint is up but the key is wrong or
    /// unauthorized. Distinguishes "your key is bad" from "can't reach it" so
    /// `/login` can say which. Always `false` when `reachable`.
    pub rejected: bool,
    pub models: Vec<String>,
}

pub struct LlmClient {
    http: reqwest::Client,
    provider: String,
    base_url: String,
    model: String,
    embed_model: String,
    api_key: Option<String>,
}

impl LlmClient {
    pub fn new(http: reqwest::Client, s: &Settings, api_key: Option<String>) -> Self {
        Self {
            http,
            provider: s.provider.clone(),
            base_url: s.base_url.trim_end_matches('/').to_string(),
            model: s.model.clone(),
            embed_model: s.embed_model.clone(),
            api_key,
        }
    }

    fn is_openai(&self) -> bool {
        use crate::engine::provider::{wire_of, Wire};
        wire_of(&self.provider) == Wire::OpenAi
    }

    pub async fn chat(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolSchema],
        on_retry: RetryNotify<'_>,
        on_delta: RetryNotify<'_>,
    ) -> EngineResult<ChatResponse> {
        if self.is_openai() {
            self.chat_openai(messages, tools, on_retry, on_delta).await
        } else {
            self.chat_ollama(messages, tools, on_retry, on_delta).await
        }
    }

    pub async fn health(&self) -> ProviderHealth {
        let url = if self.is_openai() {
            format!("{}/models", self.base_url)
        } else {
            format!("{}/api/tags", self.base_url)
        };
        let mut req = self.http.get(&url).timeout(Duration::from_secs(3));
        if let Some(k) = &self.api_key {
            req = req.bearer_auth(k);
        }
        match req.send().await {
            Ok(r) if r.status().is_success() => {
                let v: Json = r.json().await.unwrap_or_else(|_| json!({}));
                let arr = v["models"].as_array().or_else(|| v["data"].as_array());
                let models = arr
                    .map(|a| {
                        a.iter()
                            .filter_map(|m| {
                                // The API id is what `chat` sends. Some gateways
                                // also carry a spaced display `name` never use
                                // it. Ollama's `/api/tags` has only `name`.
                                m["id"].as_str().or_else(|| m["name"].as_str()).map(String::from)
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                ProviderHealth { reachable: true, rejected: false, models }
            }
            Ok(r) => {
                let status = r.status();
                log::warn!("health: {url} returned {status}");
                ProviderHealth {
                    reachable: false,
                    rejected: matches!(status.as_u16(), 401 | 403),
                    models: Vec::new(),
                }
            }
            Err(e) => {
                log::warn!("health: {url} unreachable: {e}");
                ProviderHealth { reachable: false, rejected: false, models: Vec::new() }
            }
        }
    }

    /// Ask Ollama to load the model into memory now and hold it (`keep_alive`),
    /// so the first real question doesn't pay the 10-20 s cold-load stall.
    /// Fire-and-forget: unreachable server, a hosted provider, or a model that
    /// isn't pulled yet are all swallowed.
    pub async fn warm(&self) {
        if self.is_openai() {
            return;
        }
        let url = format!("{}/api/chat", self.base_url);
        let body = json!({
            "model": self.model,
            "messages": [],
            "stream": false,
            "keep_alive": ollama_keep_alive(),
        });
        let mut req = self.http.post(&url).timeout(Duration::from_secs(20)).json(&body);
        if let Some(k) = &self.api_key {
            req = req.bearer_auth(k);
        }
        match req.send().await {
            Ok(r) => log::info!("warm: {} \u{2192} {}", self.model, r.status()),
            Err(e) => log::info!("warm: {} skipped ({e})", self.model),
        }
    }

    // --- embeddings ------------------------------------------------------

    pub async fn embed(
        &self,
        inputs: &[String],
        on_retry: RetryNotify<'_>,
    ) -> EngineResult<Vec<Vec<f32>>> {
        if inputs.is_empty() {
            return Ok(Vec::new());
        }
        if self.is_openai() {
            let url = format!("{}/embeddings", self.base_url);
            let v = self
                .send(&url, &json!({ "model": self.embed_model, "input": inputs }), on_retry)
                .await?;
            v["data"]
                .as_array()
                .ok_or_else(|| EngineError::msg("embedding response had no `data`"))?
                .iter()
                .map(|d| parse_vec(&d["embedding"]))
                .collect()
        } else {
            let url = format!("{}/api/embed", self.base_url);
            let v = self
                .send(&url, &json!({ "model": self.embed_model, "input": inputs }), on_retry)
                .await?;
            v["embeddings"]
                .as_array()
                .ok_or_else(|| EngineError::msg("embedding response had no `embeddings`"))?
                .iter()
                .map(parse_vec)
                .collect()
        }
    }

    // --- Ollama /api/chat --------------------------------------------------

    async fn chat_ollama(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolSchema],
        on_retry: RetryNotify<'_>,
        on_delta: RetryNotify<'_>,
    ) -> EngineResult<ChatResponse> {
        let url = format!("{}/api/chat", self.base_url);
        let mut body = json!({
            "model": self.model,
            "messages": messages.iter().map(ollama_message).collect::<Vec<_>>(),
            "stream": true,
            "keep_alive": ollama_keep_alive(),
            "think": ollama_think(),
            "options": {
                "temperature": 0.2,
                "num_ctx": ollama_num_ctx(),
                "num_predict": max_output_tokens(),
            },
        });
        if !tools.is_empty() {
            body["tools"] = Json::Array(tools.iter().map(tool_schema_json).collect());
        }

        let v = self.send_stream(&url, &body, on_retry, on_delta).await?;
        let msg = &v["message"];
        let content = msg["content"].as_str().unwrap_or_default().to_string();
        let tool_calls = parse_tool_calls(msg["tool_calls"].as_array(), false);
        Ok(ChatResponse { content, tool_calls })
    }

    // --- OpenAI-compatible {base_url}/chat/completions -------------------

    async fn chat_openai(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolSchema],
        on_retry: RetryNotify<'_>,
        on_delta: RetryNotify<'_>,
    ) -> EngineResult<ChatResponse> {
        let url = format!("{}/chat/completions", self.base_url);
        let mut body = json!({
            "model": self.model,
            "messages": messages.iter().map(openai_message).collect::<Vec<_>>(),
            "temperature": 0.2,
            "max_tokens": max_output_tokens(),
            "stream": true,
        });
        if !tools.is_empty() {
            body["tools"] = Json::Array(tools.iter().map(tool_schema_json).collect());
        }

        // Stream so the text appears token by token like the Ollama path.
        // `send_stream` reassembles the SSE `delta` fragments (including
        // tool_calls, which arrive split by `index`) into the same
        // `{ "message": { content, tool_calls } }` shape the buffered path
        // returned, then `parse_tool_calls` handles it unchanged.
        let v = self.send_stream(&url, &body, on_retry, on_delta).await?;
        let msg = &v["message"];
        let content = msg["content"].as_str().unwrap_or_default().to_string();
        let tool_calls = parse_tool_calls(msg["tool_calls"].as_array(), true);
        Ok(ChatResponse { content, tool_calls })
    }

    /// POST `body` to `url`, retrying transient failures (429, 5xx, timeouts,
    /// connect errors) up to `retry_budget()` times with backoff honoring a
    /// `Retry-After` header when the server sends one. `on_retry` gets a
    /// display line before each wait.
    async fn send(&self, url: &str, body: &Json, on_retry: RetryNotify<'_>) -> EngineResult<Json> {
        let budget = retry_budget();
        let mut attempt: u32 = 0;
        loop {
            attempt += 1;
            log::info!(
                "model request → {url} (model={}, attempt {attempt})",
                self.model
            );
            let started = std::time::Instant::now();
            let mut req = self.http.post(url).timeout(request_timeout()).json(body);
            if let Some(k) = &self.api_key {
                req = req.bearer_auth(k);
            }

            // Transport-level outcome (never reached the app layer of the server).
            let resp = match req.send().await {
                Ok(r) => r,
                Err(e) => {
                    log::warn!("model request failed after {:?}: {e}", started.elapsed());
                    if (e.is_timeout() || e.is_connect()) && attempt <= budget {
                        let delay = backoff(attempt);
                        on_retry(&format!(
                            "connection problem retrying in {}s… ({attempt}/{budget})",
                            delay.as_secs()
                        ));
                        tokio::time::sleep(delay).await;
                        continue;
                    }
                    return Err(if e.is_timeout() {
                        EngineError::msg(format!(
                            "the model didn't respond within {}s after {attempt} attempt(s) it \
                             may be busy or overloaded. Try again, or pick a different model with \
                             /model (it lists them).",
                            request_timeout().as_secs()
                        ))
                    } else {
                        EngineError::msg(format!("Fella couldn't reach the model at {url}: {e}"))
                    });
                }
            };

            let status = resp.status();
            let retry_after = retry_after_secs(resp.headers());
            let text = resp.text().await.unwrap_or_default();
            log::info!("model response ← {status} in {:?}", started.elapsed());

            if status.is_success() {
                return serde_json::from_str(&text).map_err(|e| {
                    EngineError::msg(format!("could not parse the model response: {e}"))
                });
            }

            if is_retryable(status) && attempt <= budget {
                let delay = retry_after
                    .map(Duration::from_secs)
                    .unwrap_or_else(|| backoff(attempt))
                    .min(Duration::from_secs(20));
                log::warn!("model retry {attempt}/{budget} after {status}, waiting {delay:?}");
                on_retry(&format!(
                    "the model is busy retrying in {}s… ({attempt}/{budget})",
                    delay.as_secs()
                ));
                tokio::time::sleep(delay).await;
                continue;
            }

            let snippet: String = text.chars().take(400).collect();
            return Err(self.http_error(status, &snippet, attempt));
        }
    }

    /// Map a non-2xx model response to a user-facing error. Shared by the
    /// buffered and streaming request paths.
    fn http_error(&self, status: reqwest::StatusCode, snippet: &str, attempt: u32) -> EngineError {
        let provider = crate::engine::provider::get(&self.provider)
            .map(|p| p.display)
            .unwrap_or(self.provider.as_str());
        match status {
            reqwest::StatusCode::TOO_MANY_REQUESTS => EngineError::msg(format!(
                "the model service is limiting how many requests it will take right now, even \
                 after {attempt} attempt(s). Wait a moment and try again, or pick a different \
                 model with /model. ({snippet})"
            )),
            // A refused / missing key. The health probe classifies these as
            // `rejected` and shows a panel, but a question in flight only gets
            // here, so say what to do.
            reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN => {
                EngineError::msg(format!(
                    "{provider} refused the API key. Run /login to paste a fresh one, or /auth to \
                     see what's signed in."
                ))
            }
            // The key is valid but the account can't use this model (a paid tier,
            // or Ollama Cloud's per-model gating).
            reqwest::StatusCode::PAYMENT_REQUIRED => EngineError::msg(format!(
                "your {provider} plan doesn't cover the model \"{}\". Pick another with /model, or \
                 upgrade your {provider} plan.",
                self.model
            )),
            s if s.is_server_error() => EngineError::msg(format!(
                "the model service kept failing ({status}) after {attempt} attempt(s) it's \
                 having trouble. Try again shortly. ({snippet})"
            )),
            // Ollama returns 404 "model '...' not found" when the id isn't
            // pulled (default `llama3.1` vs a pulled `llama3.1:8b`). Say what
            // to do instead of echoing the raw 404.
            _ if crate::engine::provider::normalize_id(&self.provider) == "ollama"
                && snippet.contains("not found") =>
            {
                EngineError::msg(format!(
                    "The model \"{}\" isn't downloaded in Ollama. Run `ollama pull {}` in a \
                     terminal, or pick one you already have with /model.",
                    self.model, self.model
                ))
            }
            _ => EngineError::msg(format!(
                "the model service returned an error ({status}): {snippet}"
            )),
        }
    }

    /// Like [`send`], but consumes Ollama's newline-delimited streaming body:
    /// each `on_delta` fires with a token as it lands, and the assembled
    /// `{ "message": { content, tool_calls } }` is returned so the caller
    /// parses it exactly as the buffered path. Retries cover the connection
    /// attempt only once tokens are flowing a drop is surfaced as an error.
    async fn send_stream(
        &self,
        url: &str,
        body: &Json,
        on_retry: RetryNotify<'_>,
        on_delta: RetryNotify<'_>,
    ) -> EngineResult<Json> {
        let budget = retry_budget();
        let mut attempt: u32 = 0;
        loop {
            attempt += 1;
            log::info!(
                "model request → {url} (model={}, attempt {attempt}, streaming)",
                self.model
            );
            let started = std::time::Instant::now();
            let mut req = self.http.post(url).timeout(request_timeout()).json(body);
            if let Some(k) = &self.api_key {
                req = req.bearer_auth(k);
            }

            let resp = match req.send().await {
                Ok(r) => r,
                Err(e) => {
                    log::warn!("model request failed after {:?}: {e}", started.elapsed());
                    if (e.is_timeout() || e.is_connect()) && attempt <= budget {
                        let delay = backoff(attempt);
                        on_retry(&format!(
                            "connection problem retrying in {}s… ({attempt}/{budget})",
                            delay.as_secs()
                        ));
                        tokio::time::sleep(delay).await;
                        continue;
                    }
                    return Err(if e.is_timeout() {
                        EngineError::msg(format!(
                            "the model didn't respond within {}s after {attempt} attempt(s) it \
                             may be busy or overloaded. Try again, or pick a different model with \
                             /model (it lists them).",
                            request_timeout().as_secs()
                        ))
                    } else {
                        EngineError::msg(format!("Fella couldn't reach the model at {url}: {e}"))
                    });
                }
            };

            let status = resp.status();
            if !status.is_success() {
                let retry_after = retry_after_secs(resp.headers());
                let text = resp.text().await.unwrap_or_default();
                log::info!("model response ← {status} in {:?}", started.elapsed());
                if is_retryable(status) && attempt <= budget {
                    let delay = retry_after
                        .map(Duration::from_secs)
                        .unwrap_or_else(|| backoff(attempt))
                        .min(Duration::from_secs(20));
                    on_retry(&format!(
                        "the model is busy retrying in {}s… ({attempt}/{budget})",
                        delay.as_secs()
                    ));
                    tokio::time::sleep(delay).await;
                    continue;
                }
                let snippet: String = text.chars().take(400).collect();
                return Err(self.http_error(status, &snippet, attempt));
            }

            let openai = self.is_openai();
            let mut stream = resp.bytes_stream();
            let mut buf: Vec<u8> = Vec::new();
            let mut content = String::new();
            let mut tool_calls: Option<Json> = None;
            let mut oai_tools: Vec<OaiToolAccum> = Vec::new();

            while let Some(chunk) = stream.next().await {
                let chunk = chunk.map_err(|e| {
                    EngineError::msg(format!("the model connection dropped mid-reply: {e}"))
                })?;
                buf.extend_from_slice(&chunk);
                while let Some(nl) = buf.iter().position(|b| *b == b'\n') {
                    let raw: Vec<u8> = buf.drain(..=nl).collect();
                    let line = String::from_utf8_lossy(&raw);
                    if openai {
                        absorb_openai_line(&line, on_delta, &mut content, &mut oai_tools);
                    } else {
                        absorb_stream_line(&line, on_delta, &mut content, &mut tool_calls);
                    }
                }
            }
            if !buf.is_empty() {
                // A body with no trailing newline (or a mock that sends one
                // JSON object) still needs the leftover flushed.
                let line = String::from_utf8_lossy(&buf);
                if openai {
                    absorb_openai_line(&line, on_delta, &mut content, &mut oai_tools);
                } else {
                    absorb_stream_line(&line, on_delta, &mut content, &mut tool_calls);
                }
            }
            log::info!(
                "model response ← {status} streamed {} chars in {:?}",
                content.len(),
                started.elapsed()
            );

            if !oai_tools.is_empty() {
                tool_calls = Some(Json::Array(
                    oai_tools
                        .iter()
                        .enumerate()
                        .map(|(i, t)| {
                            let id = if t.id.is_empty() { format!("call_{i}") } else { t.id.clone() };
                            json!({
                                "id": id,
                                "function": { "name": t.name, "arguments": t.arguments },
                            })
                        })
                        .collect(),
                ));
            }
            let mut message = json!({ "role": "assistant", "content": content });
            if let Some(tc) = tool_calls {
                message["tool_calls"] = tc;
            }
            return Ok(json!({ "message": message }));
        }
    }
}

/// Fold one line of an Ollama streaming body into the running `content` /
/// `tool_calls`, emitting each new text fragment through `on_delta`.
fn absorb_stream_line(
    line: &str,
    on_delta: RetryNotify<'_>,
    content: &mut String,
    tool_calls: &mut Option<Json>,
) {
    let line = line.trim();
    if line.is_empty() {
        return;
    }
    let Ok(v) = serde_json::from_str::<Json>(line) else {
        return;
    };
    if let Some(piece) = v["message"]["content"].as_str() {
        if !piece.is_empty() {
            on_delta(piece);
            content.push_str(piece);
        }
    }
    if let Some(arr) = v["message"]["tool_calls"].as_array() {
        if !arr.is_empty() {
            *tool_calls = Some(Json::Array(arr.clone()));
        }
    }
}

/// One tool call being reassembled from OpenAI SSE `delta.tool_calls[]`
/// fragments: `id` and `function.name` arrive once, `function.arguments`
/// arrives as a string in pieces.
#[derive(Default)]
struct OaiToolAccum {
    id: String,
    name: String,
    arguments: String,
}

/// Fold one line of an OpenAI-compatible response body into the running
/// `content` / tool-call accumulators. Handles both streaming SSE
/// (`data: {choices:[{delta:…}]}` / `data: [DONE]`) and a server that ignored
/// `stream:true` and returned one whole `{choices:[{message:…}]}` object.
fn absorb_openai_line(
    line: &str,
    on_delta: RetryNotify<'_>,
    content: &mut String,
    tools: &mut Vec<OaiToolAccum>,
) {
    let line = line.trim();
    if line.is_empty() {
        return;
    }
    // SSE frames are `data: <payload>`; a raw JSON body has no prefix.
    let payload = line.strip_prefix("data:").map(str::trim).unwrap_or(line);
    if payload.is_empty() || payload == "[DONE]" {
        return;
    }
    let Ok(v) = serde_json::from_str::<Json>(payload) else {
        return;
    };
    let choice = &v["choices"][0];
    // `delta` for a streamed chunk, `message` for a whole non-streamed reply.
    let node = if choice.get("message").is_some() {
        &choice["message"]
    } else {
        &choice["delta"]
    };
    if let Some(piece) = node["content"].as_str() {
        if !piece.is_empty() {
            on_delta(piece);
            content.push_str(piece);
        }
    }
    if let Some(arr) = node["tool_calls"].as_array() {
        for (i, tc) in arr.iter().enumerate() {
            let idx = tc["index"].as_u64().map(|n| n as usize).unwrap_or(i);
            if tools.len() <= idx {
                tools.resize_with(idx + 1, OaiToolAccum::default);
            }
            let slot = &mut tools[idx];
            if let Some(id) = tc["id"].as_str().filter(|s| !s.is_empty()) {
                slot.id = id.to_string();
            }
            let f = &tc["function"];
            if let Some(name) = f["name"].as_str().filter(|s| !s.is_empty()) {
                slot.name = name.to_string();
            }
            if let Some(args) = f["arguments"].as_str() {
                slot.arguments.push_str(args);
            }
        }
    }
}

fn parse_vec(v: &Json) -> EngineResult<Vec<f32>> {
    v.as_array()
        .ok_or_else(|| EngineError::msg("an embedding was not an array"))?
        .iter()
        .map(|x| {
            x.as_f64()
                .map(|f| f as f32)
                .ok_or_else(|| EngineError::msg("an embedding value was not a number"))
        })
        .collect()
}

fn tool_schema_json(t: &ToolSchema) -> Json {
    json!({
        "type": "function",
        "function": {
            "name": t.name,
            "description": t.description,
            "parameters": t.parameters,
        }
    })
}

fn parse_tool_calls(calls: Option<&Vec<Json>>, openai_style: bool) -> Vec<ToolCall> {
    let Some(calls) = calls else { return Vec::new() };
    calls
        .iter()
        .enumerate()
        .filter_map(|(i, c)| {
            let f = &c["function"];
            let name = f["name"].as_str()?.to_string();
            if name.is_empty() {
                return None;
            }
            let arguments = match &f["arguments"] {
                Json::String(s) if !s.is_empty() => {
                    serde_json::from_str(s).unwrap_or_else(|_| json!({}))
                }
                Json::String(_) => json!({}),
                other => other.clone(),
            };
            let id = if openai_style {
                c["id"]
                    .as_str()
                    .map(String::from)
                    .unwrap_or_else(|| format!("call_{i}"))
            } else {
                format!("call_{i}")
            };
            Some(ToolCall { id, name, arguments })
        })
        .collect()
}

fn ollama_message(m: &ChatMessage) -> Json {
    match m {
        ChatMessage::System(s) => json!({ "role": "system", "content": s }),
        ChatMessage::User(s) => json!({ "role": "user", "content": s }),
        ChatMessage::Assistant {
            content,
            tool_calls,
        } => {
            let mut o = json!({ "role": "assistant", "content": content });
            if !tool_calls.is_empty() {
                o["tool_calls"] = Json::Array(
                    tool_calls
                        .iter()
                        .map(|tc| {
                            json!({ "function": { "name": tc.name, "arguments": tc.arguments } })
                        })
                        .collect(),
                );
            }
            o
        }
        ChatMessage::Tool { name, content, .. } => {
            json!({ "role": "tool", "tool_name": name, "content": content })
        }
    }
}

fn openai_message(m: &ChatMessage) -> Json {
    match m {
        ChatMessage::System(s) => json!({ "role": "system", "content": s }),
        ChatMessage::User(s) => json!({ "role": "user", "content": s }),
        ChatMessage::Assistant {
            content,
            tool_calls,
        } => {
            let mut o = json!({ "role": "assistant", "content": content });
            if !tool_calls.is_empty() {
                o["tool_calls"] = Json::Array(
                    tool_calls
                        .iter()
                        .map(|tc| {
                            json!({
                                "id": tc.id,
                                "type": "function",
                                "function": {
                                    "name": tc.name,
                                    "arguments": tc.arguments.to_string(),
                                }
                            })
                        })
                        .collect(),
                );
            }
            o
        }
        ChatMessage::Tool {
            call_id, content, ..
        } => json!({ "role": "tool", "tool_call_id": call_id, "content": content }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, Write};
    use std::net::TcpListener;

    fn settings(provider: &str, base_url: &str) -> Settings {
        Settings {
            provider: provider.into(),
            base_url: base_url.into(),
            model: "m".into(),
            embed_model: "e".into(),
            has_credential: false,
        }
    }

    /// `base_url` is the API root (with any `/v1`); the OpenAI wire appends only
    /// `/models`, `/chat/completions`, `/embeddings` never a second `/v1`.
    #[tokio::test]
    async fn openai_wire_does_not_double_the_v1_segment() {
        let _ = rustls::crypto::ring::default_provider().install_default();

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let mut line = String::new();
            std::io::BufReader::new(&s).read_line(&mut line).unwrap();
            let body = r#"{"data":[{"id":"gpt-4o-mini"}]}"#;
            write!(
                s,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .unwrap();
            line
        });

        let base = format!("http://127.0.0.1:{port}/v1");
        let client = LlmClient::new(reqwest::Client::new(), &settings("openai", &base), None);
        let health = client.health().await;

        let request_line = server.join().unwrap();
        assert_eq!(request_line.trim_end(), "GET /v1/models HTTP/1.1");
        assert!(health.reachable);
        assert_eq!(health.models, vec!["gpt-4o-mini".to_string()]);
    }

    /// Some gateways list a model with both `id` (the chat identifier) and a
    /// display `name` containing spaces. `health` must surface the id.
    #[tokio::test]
    async fn model_list_uses_the_id_not_the_display_name() {
        let _ = rustls::crypto::ring::default_provider().install_default();

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let mut line = String::new();
            std::io::BufReader::new(&s).read_line(&mut line).unwrap();
            let body = r#"{"data":[
                {"id":"nvidia/nemotron-nano-9b-v2","name":"NVIDIA: Nemotron Nano 9B V2"},
                {"id":"deepseek/deepseek-chat","name":"DeepSeek: DeepSeek V3"}
            ]}"#;
            write!(
                s,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .unwrap();
            line
        });

        let base = format!("http://127.0.0.1:{port}");
        let client = LlmClient::new(reqwest::Client::new(), &settings("vercel", &base), None);
        let health = client.health().await;
        server.join().unwrap();

        assert_eq!(
            health.models,
            vec![
                "nvidia/nemotron-nano-9b-v2".to_string(),
                "deepseek/deepseek-chat".to_string()
            ]
        );
    }

    /// A 401 means the endpoint answered but refused the key: not reachable,
    /// but `rejected` so `/login` can say "that key is wrong" rather than
    /// "can't reach the provider".
    #[tokio::test]
    async fn a_401_is_reported_as_a_rejected_key() {
        let _ = rustls::crypto::ring::default_provider().install_default();

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let mut line = String::new();
            std::io::BufReader::new(&s).read_line(&mut line).unwrap();
            write!(
                s,
                "HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            )
            .unwrap();
        });

        let base = format!("http://127.0.0.1:{port}/v1");
        let client = LlmClient::new(
            reqwest::Client::new(),
            &settings("openai", &base),
            Some("sk-bad".into()),
        );
        let health = client.health().await;
        server.join().unwrap();

        assert!(!health.reachable);
        assert!(health.rejected);
        assert!(health.models.is_empty());
    }

    /// Nothing listening is unreachable, but not `rejected` there was no
    /// answer to reject the key.
    #[tokio::test]
    async fn an_unreachable_provider_is_not_a_rejected_key() {
        let _ = rustls::crypto::ring::default_provider().install_default();

        // Bind then drop, so the port is almost certainly closed.
        let port = {
            let l = TcpListener::bind("127.0.0.1:0").unwrap();
            l.local_addr().unwrap().port()
        };

        let base = format!("http://127.0.0.1:{port}/v1");
        let client = LlmClient::new(
            reqwest::Client::new(),
            &settings("openai", &base),
            Some("sk-whatever".into()),
        );
        let health = client.health().await;

        assert!(!health.reachable);
        assert!(!health.rejected);
    }

    /// Read (and discard) the request line so the client's write side unblocks.
    fn drain(s: &std::net::TcpStream) {
        let mut line = String::new();
        let _ = std::io::BufReader::new(s).read_line(&mut line);
    }

    /// A 429 with `Retry-After` is retried, and a subsequent 200 succeeds.
    #[tokio::test]
    async fn a_429_is_retried_then_succeeds() {
        let _ = rustls::crypto::ring::default_provider().install_default();

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            drain(&s);
            let _ = write!(
                s,
                "HTTP/1.1 429 Too Many Requests\r\nRetry-After: 0\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{{}}"
            );
            drop(s);
            let (mut s, _) = listener.accept().unwrap();
            drain(&s);
            let ok = r#"{"choices":[{"message":{"role":"assistant","content":"hi"}}]}"#;
            let _ = write!(
                s,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                ok.len(),
                ok
            );
        });

        let base = format!("http://127.0.0.1:{port}");
        let client = LlmClient::new(reqwest::Client::new(), &settings("openai", &base), None);
        let retries = std::sync::atomic::AtomicU32::new(0);
        let notify = |_: &str| {
            retries.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        };
        let resp = client
            .chat(&[ChatMessage::User("q".into())], &[], &notify, &|_: &str| {})
            .await
            .unwrap();
        server.join().unwrap();

        assert_eq!(resp.content, "hi");
        assert_eq!(retries.load(std::sync::atomic::Ordering::Relaxed), 1);
    }

    /// A persistent 429 exhausts the budget and returns an actionable message.
    #[tokio::test]
    async fn a_persistent_429_is_actionable() {
        let _ = rustls::crypto::ring::default_provider().install_default();

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let budget = retry_budget();
        let server = std::thread::spawn(move || {
            for _ in 0..=budget {
                let (mut s, _) = listener.accept().unwrap();
                drain(&s);
                let _ = write!(
                    s,
                    "HTTP/1.1 429 Too Many Requests\r\nRetry-After: 0\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{{}}"
                );
            }
        });

        let base = format!("http://127.0.0.1:{port}");
        let client = LlmClient::new(reqwest::Client::new(), &settings("vercel", &base), None);
        let err = client
            .chat(&[ChatMessage::User("q".into())], &[], &|_: &str| {}, &|_: &str| {})
            .await
            .unwrap_err()
            .to_string();
        server.join().unwrap();

        assert!(err.contains("limiting how many requests"), "{err}");
        assert!(err.contains("/model"), "{err}"); // points at an actionable fix
    }

    /// A single non-retryable auth/plan error is turned into actionable text,
    /// not the raw provider body.
    #[tokio::test]
    async fn auth_and_payment_errors_are_actionable() {
        let _ = rustls::crypto::ring::default_provider().install_default();

        async fn err_for(status_line: &str, provider: &str) -> String {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let port = listener.local_addr().unwrap().port();
            let sl = status_line.to_string();
            let server = std::thread::spawn(move || {
                let (mut s, _) = listener.accept().unwrap();
                drain(&s);
                let _ = write!(
                    s,
                    "HTTP/1.1 {sl}\r\nContent-Length: 15\r\nConnection: close\r\n\r\n{{\"error\":\"x\"}}"
                );
            });
            let base = format!("http://127.0.0.1:{port}");
            let client =
                LlmClient::new(reqwest::Client::new(), &settings(provider, &base), None);
            let e = client
                .chat(&[ChatMessage::User("q".into())], &[], &|_: &str| {}, &|_: &str| {})
                .await
                .unwrap_err()
                .to_string();
            server.join().unwrap();
            e
        }

        let e401 = err_for("401 Unauthorized", "openai").await;
        assert!(e401.contains("refused the API key") && e401.contains("/login"), "{e401}");
        assert!(!e401.contains("\"error\""), "raw body leaked: {e401}");

        let e402 = err_for("402 Payment Required", "vercel").await;
        assert!(e402.contains("plan doesn't cover") && e402.contains("/model"), "{e402}");
    }
}
