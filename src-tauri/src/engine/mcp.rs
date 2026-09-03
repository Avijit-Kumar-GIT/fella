//! MCP connector packs (`mcp` kind). Fella connects to a remote MCP server
//! over Streamable HTTP and exposes its tools to the agent, namespaced.
//!
//! `rmcp` owns the protocol; this module owns a thin HTTP adapter over Fella's
//! existing `reqwest 0.13` client (so we don't pull rmcp's `reqwest 0.12` +
//! quinn transport), plus the mapping between rmcp's tool types and Fella's.
//!
//! Compiled only with `--features mcp`.

use std::collections::HashMap;
use std::sync::Arc;

use futures::stream::{BoxStream, StreamExt};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use rmcp::model::{
    CallToolRequestParams, CallToolResult, ClientJsonRpcMessage, ServerJsonRpcMessage,
};
use rmcp::transport::common::http_header::{
    EVENT_STREAM_MIME_TYPE, HEADER_LAST_EVENT_ID, HEADER_SESSION_ID, JSON_MIME_TYPE,
};
use rmcp::transport::streamable_http_client::{
    StreamableHttpClient, StreamableHttpClientTransport, StreamableHttpClientTransportConfig,
    StreamableHttpError, StreamableHttpPostResponse,
};
use rmcp::ServiceExt;
use sse_stream::{Error as SseError, Sse, SseStream};

use crate::engine::error::{EngineError, EngineResult};
use crate::engine::extensions::{ConnectorAuth, ConnectorConfig};

/// An HTTP backend for `rmcp`'s Streamable-HTTP transport, over Fella's shared
/// `reqwest::Client`. Auth headers are baked in at construction.
#[derive(Clone)]
pub struct FellaHttp {
    http: reqwest::Client,
    headers: HeaderMap,
}

impl FellaHttp {
    fn new(http: reqwest::Client, auth: &ConnectorAuth, token: Option<&str>) -> EngineResult<Self> {
        let mut headers = HeaderMap::new();
        match auth {
            ConnectorAuth::None => {}
            ConnectorAuth::Bearer { .. } => {
                let t = token.ok_or_else(|| EngineError::msg("this connector needs a token"))?;
                let v = HeaderValue::from_str(&format!("Bearer {t}"))
                    .map_err(|_| EngineError::msg("invalid token for a bearer header"))?;
                headers.insert(reqwest::header::AUTHORIZATION, v);
            }
            ConnectorAuth::Header { header, .. } => {
                let t = token.ok_or_else(|| EngineError::msg("this connector needs a token"))?;
                let name = HeaderName::from_bytes(header.as_bytes())
                    .map_err(|_| EngineError::msg(format!("invalid auth header name '{header}'")))?;
                let value = HeaderValue::from_str(t)
                    .map_err(|_| EngineError::msg("invalid token for a custom header"))?;
                headers.insert(name, value);
            }
        }
        Ok(Self { http, headers })
    }

    fn accept() -> HeaderValue {
        HeaderValue::from_static("text/event-stream, application/json")
    }
}

fn sse_box(
    stream: impl futures::Stream<Item = reqwest::Result<bytes::Bytes>> + Send + 'static,
) -> BoxStream<'static, Result<Sse, SseError>> {
    SseStream::from_bytes_stream(stream).boxed()
}

impl StreamableHttpClient for FellaHttp {
    type Error = reqwest::Error;

    async fn post_message(
        &self,
        uri: Arc<str>,
        message: ClientJsonRpcMessage,
        session_id: Option<Arc<str>>,
        _auth_header: Option<String>,
        _custom_headers: HashMap<HeaderName, HeaderValue>,
    ) -> Result<StreamableHttpPostResponse, StreamableHttpError<Self::Error>> {
        let mut req = self
            .http
            .post(uri.as_ref())
            .header(reqwest::header::ACCEPT, FellaHttp::accept())
            .headers(self.headers.clone());
        if let Some(sid) = &session_id {
            req = req.header(HEADER_SESSION_ID, sid.as_ref());
        }
        let resp = req.json(&message).send().await.map_err(StreamableHttpError::Client)?;
        let status = resp.status();
        if matches!(
            status,
            reqwest::StatusCode::ACCEPTED | reqwest::StatusCode::NO_CONTENT
        ) {
            return Ok(StreamableHttpPostResponse::Accepted);
        }
        if status == reqwest::StatusCode::NOT_FOUND && session_id.is_some() {
            return Err(StreamableHttpError::SessionExpired);
        }
        let new_session = resp
            .headers()
            .get(HEADER_SESSION_ID)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        let content_type = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .map(|v| String::from_utf8_lossy(v.as_bytes()).into_owned());
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(StreamableHttpError::UnexpectedServerResponse(
                format!("HTTP {status}: {body}").into(),
            ));
        }
        match content_type.as_deref() {
            Some(ct) if ct.starts_with(EVENT_STREAM_MIME_TYPE) => Ok(
                StreamableHttpPostResponse::Sse(sse_box(resp.bytes_stream()), new_session),
            ),
            Some(ct) if ct.starts_with(JSON_MIME_TYPE) => {
                match resp.json::<ServerJsonRpcMessage>().await {
                    Ok(msg) => Ok(StreamableHttpPostResponse::Json(msg, new_session)),
                    Err(_) => Ok(StreamableHttpPostResponse::Accepted),
                }
            }
            other => Err(StreamableHttpError::UnexpectedContentType(
                other.map(str::to_owned),
            )),
        }
    }

    async fn get_stream(
        &self,
        uri: Arc<str>,
        session_id: Option<Arc<str>>,
        last_event_id: Option<String>,
        _auth_header: Option<String>,
        _custom_headers: HashMap<HeaderName, HeaderValue>,
    ) -> Result<BoxStream<'static, Result<Sse, SseError>>, StreamableHttpError<Self::Error>> {
        let mut req = self
            .http
            .get(uri.as_ref())
            .header(
                reqwest::header::ACCEPT,
                HeaderValue::from_static(EVENT_STREAM_MIME_TYPE),
            )
            .headers(self.headers.clone());
        if let Some(sid) = &session_id {
            req = req.header(HEADER_SESSION_ID, sid.as_ref());
        }
        if let Some(eid) = last_event_id {
            req = req.header(HEADER_LAST_EVENT_ID, eid);
        }
        let resp = req.send().await.map_err(StreamableHttpError::Client)?;
        if resp.status() == reqwest::StatusCode::METHOD_NOT_ALLOWED {
            return Err(StreamableHttpError::ServerDoesNotSupportSse);
        }
        let resp = resp.error_for_status().map_err(StreamableHttpError::Client)?;
        Ok(sse_box(resp.bytes_stream()))
    }

    async fn delete_session(
        &self,
        uri: Arc<str>,
        session_id: Arc<str>,
        _auth_header: Option<String>,
        _custom_headers: HashMap<HeaderName, HeaderValue>,
    ) -> Result<(), StreamableHttpError<Self::Error>> {
        let resp = self
            .http
            .delete(uri.as_ref())
            .headers(self.headers.clone())
            .header(HEADER_SESSION_ID, session_id.as_ref())
            .send()
            .await
            .map_err(StreamableHttpError::Client)?;
        if resp.status() == reqwest::StatusCode::METHOD_NOT_ALLOWED {
            return Ok(());
        }
        resp.error_for_status().map_err(StreamableHttpError::Client)?;
        Ok(())
    }
}

/// A live connection to one MCP server. Dropping it closes the connection.
pub struct McpConn {
    running: rmcp::service::RunningService<rmcp::RoleClient, ()>,
}

impl McpConn {
    pub async fn call(
        &self,
        tool: &str,
        arguments: serde_json::Map<String, serde_json::Value>,
    ) -> EngineResult<CallToolResult> {
        self.running
            .call_tool(CallToolRequestParams::new(tool.to_string()).with_arguments(arguments))
            .await
            .map_err(|e| EngineError::msg(format!("connector tool '{tool}' failed: {e}")))
    }
}

/// A tool discovered on a connected MCP server, ready to register in the agent
/// registry.
pub struct McpTool {
    /// `<connector-id>__<tool>`, sanitised for OpenAI-compatible name rules.
    pub namespaced: String,
    /// The name the server knows it by.
    pub server_name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
    /// The server did not declare `readOnlyHint: true`; flag its effects.
    pub effects_undeclared: bool,
    pub conn: Arc<McpConn>,
}

/// Connect to `cfg`, run the `initialize` handshake, and list the tools we will
/// offer. Non-read-only tools (`readOnlyHint: false` / `destructiveHint: true`)
/// are dropped and returned separately so the caller can tell the user.
pub async fn connect(
    http: &reqwest::Client,
    id: &str,
    cfg: &ConnectorConfig,
    token: Option<&str>,
) -> EngineResult<(Vec<McpTool>, Vec<String>)> {
    let backend = FellaHttp::new(http.clone(), &cfg.auth, token)?;
    let transport = StreamableHttpClientTransport::with_client(
        backend,
        StreamableHttpClientTransportConfig::with_uri(cfg.url.clone()),
    );
    let running = ()
        .serve(transport)
        .await
        .map_err(|e| EngineError::msg(format!("couldn't connect to the '{id}' connector: {e}")))?;

    let tools = running
        .list_all_tools()
        .await
        .map_err(|e| EngineError::msg(format!("'{id}' connector: listing tools failed: {e}")))?;

    let conn = Arc::new(McpConn { running });
    let mut offered = Vec::new();
    let mut withheld = Vec::new();

    for t in tools {
        let (read_only, destructive) = t
            .annotations
            .as_ref()
            .map(|a| (a.read_only_hint, a.destructive_hint))
            .unwrap_or((None, None));
        if read_only == Some(false) || destructive == Some(true) {
            withheld.push(t.name.to_string());
            continue;
        }
        offered.push(McpTool {
            namespaced: namespaced_name(id, &t.name),
            server_name: t.name.to_string(),
            description: t.description.map(|c| c.to_string()).unwrap_or_default(),
            input_schema: serde_json::Value::Object((*t.input_schema).clone()),
            effects_undeclared: read_only != Some(true),
            conn: conn.clone(),
        });
    }
    Ok((offered, withheld))
}

/// `<id>__<tool>`, lowercased connector id, non-`[A-Za-z0-9_-]` → `_`, capped
/// at 64 (OpenAI-compatible tool-name rule).
fn namespaced_name(id: &str, tool: &str) -> String {
    let mut s = format!("{id}__{tool}");
    s = s
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' })
        .collect();
    if s.len() > 64 {
        s.truncate(64);
    }
    s
}

/// Call an MCP tool and shape the result like any built-in tool's output.
pub async fn run_mcp_tool(
    tool: &McpTool,
    args: &serde_json::Value,
) -> EngineResult<crate::engine::tools::ToolOutput> {
    let mut map = args
        .as_object()
        .cloned()
        .unwrap_or_default();
    map.remove("note");

    let result = tool.conn.call(&tool.server_name, map).await?;
    let text = result_text(&result);

    if result.is_error == Some(true) {
        return Err(EngineError::msg(if text.is_empty() {
            format!("the '{}' connector reported an error", tool.namespaced)
        } else {
            text
        }));
    }

    let flag = if tool.effects_undeclared {
        " (this connector tool's effects are not declared)"
    } else {
        ""
    };
    Ok(crate::engine::tools::ToolOutput {
        summary: format!("{}{flag}", tool.namespaced),
        llm_text: if text.is_empty() { "(no content)".into() } else { text.clone() },
        sql: None,
        columns: None,
        rows: None,
        row_count: None,
        output: (!text.is_empty()).then_some(text),
    })
}

/// Flatten a `CallToolResult` into the text Fella feeds back to the model.
pub fn result_text(r: &CallToolResult) -> String {
    let mut parts: Vec<String> = r
        .content
        .iter()
        .filter_map(|c| c.as_text().map(|t| t.text.clone()))
        .collect();
    if parts.is_empty() {
        if let Some(sc) = &r.structured_content {
            parts.push(sc.to_string());
        }
    }
    parts.join("\n")
}
