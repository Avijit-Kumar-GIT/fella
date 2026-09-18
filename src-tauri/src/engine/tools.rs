//! The deterministic tools the agent may call. This is the ONLY path from the
//! model to the data the model never touches the database directly. Adding a tool
//! is a deliberate code change; there is no plugin mechanism.

use async_trait::async_trait;
use serde_json::{json, Value as Json};
use std::sync::{atomic::AtomicBool, Arc};

use crate::engine::analytics::chart::{self, ChartData, ChartKind};
use crate::engine::analytics::data::DEFAULT_ROW_CAP;
use crate::engine::analytics::verify::truncate as truncate_chars;
use crate::engine::error::{EngineError, EngineResult};
use crate::engine::llm::ToolSchema;
use crate::engine::state::{EngineState, GrepHit, QueryResult};

/// Keep the model's context bounded for large result sets, while allowing it
/// to answer complete-table requests for ordinary small results. A 50-row
/// category table is still a small result; hiding its last 20 rows makes an
/// exact transcription request impossible to answer safely.
const MODEL_TABLE_PREVIEW_ROWS: usize = 30;
const MODEL_TABLE_COMPLETE_ROWS: usize = 100;

/// What a tool produces: a human-facing summary + optional tabular detail for
/// the evidence panel, and a compact text rendering for the model.
pub struct ToolOutput {
    pub summary: String,
    pub llm_text: String,
    pub sql: Option<String>,
    pub columns: Option<Vec<String>>,
    pub rows: Option<Vec<Vec<Json>>>,
    pub row_count: Option<usize>,
    pub output: Option<String>,
    /// Structured chart data from a chart tool (e.g. `make_chart`) -- labels
    /// and numbers only, no markup. Rendered client-side.
    pub chart: Option<ChartData>,
}

impl ToolOutput {
    fn text(summary: impl Into<String>, llm_text: impl Into<String>) -> Self {
        Self {
            summary: summary.into(),
            llm_text: llm_text.into(),
            sql: None,
            columns: None,
            rows: None,
            row_count: None,
            output: None,
            chart: None,
        }
    }
}

#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn parameters(&self) -> Json;
    async fn run(&self, engine: &EngineState, args: &Json) -> EngineResult<ToolOutput>;

    /// Cancellation-aware entry point. Existing deterministic tools inherit
    /// the ordinary implementation; tools that can block add the flag to
    /// their backend call without changing the public tool contract.
    async fn run_with_cancel(
        &self,
        engine: &EngineState,
        args: &Json,
        _cancel: Arc<AtomicBool>,
    ) -> EngineResult<ToolOutput> {
        self.run(engine, args).await
    }
}

pub struct Registry {
    tools: Vec<Box<dyn Tool>>,
    /// Tools contributed at runtime by enabled `mcp` connector packs. Kept
    /// separate because their names/schemas are owned `String`s, not the
    /// `&'static str` the `Tool` trait wants.
    #[cfg(feature = "mcp")]
    mcp: Vec<crate::engine::mcp::McpTool>,
}

impl Registry {
    pub fn standard() -> Self {
        Self::build(true)
    }

    /// Read-only inspection tools for the non-developer path. Python is kept
    /// out of this registry because Inspect is deliberately limited to the
    /// deterministic workspace tools. MCP is attached by the caller only for
    /// ordinary Ask runs.
    pub fn inspect() -> Self {
        Self::build(false)
    }

    fn build(include_python: bool) -> Self {
        let mut tools: Vec<Box<dyn Tool>> = vec![
            Box::new(ListFiles),
            Box::new(InspectTable),
            Box::new(RunSql),
            Box::new(GrepFiles),
            Box::new(ReadFile),
            Box::new(MakeChart),
        ];
        if include_python {
            // Python is available only in ordinary Ask mode; Inspect stays a
            // deterministic read-only surface even though Python is sandboxed.
            tools.insert(5, Box::new(RunPython));
        }
        Self {
            tools,
            #[cfg(feature = "mcp")]
            mcp: Vec::new(),
        }
    }

    #[cfg(feature = "mcp")]
    pub fn set_mcp(&mut self, tools: Vec<crate::engine::mcp::McpTool>) {
        self.mcp = tools;
    }

    /// Whether any runtime (MCP) tools are present.
    pub fn has_mcp(&self) -> bool {
        #[cfg(feature = "mcp")]
        {
            !self.mcp.is_empty()
        }
        #[cfg(not(feature = "mcp"))]
        {
            false
        }
    }

    fn get(&self, name: &str) -> Option<&dyn Tool> {
        self.tools
            .iter()
            .find(|t| t.name() == name)
            .map(|b| b.as_ref())
    }

    /// Run the tool named `name`. `None` = no such tool.
    pub async fn run(
        &self,
        engine: &EngineState,
        name: &str,
        args: &Json,
    ) -> Option<EngineResult<ToolOutput>> {
        self.run_with_cancel(engine, name, args, Arc::new(AtomicBool::new(false)))
            .await
    }

    /// Run a tool while allowing long-running built-ins and remote connector
    /// calls to observe the question's stop flag.
    pub async fn run_with_cancel(
        &self,
        engine: &EngineState,
        name: &str,
        args: &Json,
        cancel: Arc<AtomicBool>,
    ) -> Option<EngineResult<ToolOutput>> {
        if let Some(tool) = self.get(name) {
            return Some(tool.run_with_cancel(engine, args, cancel).await);
        }
        #[cfg(feature = "mcp")]
        if let Some(t) = self.mcp.iter().find(|t| t.namespaced == name) {
            return Some(crate::engine::mcp::run_mcp_tool(t, args, Some(cancel)).await);
        }
        None
    }

    pub fn schemas(&self) -> Vec<ToolSchema> {
        #[cfg_attr(not(feature = "mcp"), allow(unused_mut))]
        let mut out: Vec<ToolSchema> = self
            .tools
            .iter()
            .map(|t| ToolSchema {
                name: t.name().to_string(),
                description: t.description().to_string(),
                parameters: with_note_param(t.parameters()),
            })
            .collect();
        #[cfg(feature = "mcp")]
        out.extend(self.mcp.iter().map(|t| ToolSchema {
            name: t.namespaced.clone(),
            description: t.description.clone(),
            parameters: with_note_param(object_schema(t.input_schema.clone())),
        }));
        out
    }
}

/// Ensure a schema is an object with a `properties` map so `with_note_param`
/// can attach `note` (some MCP servers send a bare `{"type":"object"}`).
#[cfg(feature = "mcp")]
fn object_schema(mut s: Json) -> Json {
    if !s.is_object() {
        s = json!({ "type": "object" });
    }
    let obj = s.as_object_mut().unwrap();
    obj.entry("type").or_insert_with(|| json!("object"));
    obj.entry("properties").or_insert_with(|| json!({}));
    s
}

/// Add a shared optional `note` string to a tool's parameter schema. The model
/// fills it with one plain sentence describing the step, which the evidence
/// panel shows to a non-technical reader in place of the raw SQL / tool name.
fn with_note_param(mut params: Json) -> Json {
    if let Some(props) = params.get_mut("properties").and_then(|p| p.as_object_mut()) {
        props.insert(
            "note".to_string(),
            json!({
                "type": "string",
                "description": "Optional. 4-8 plain words for the activity display, \
            e.g. \"Add up spending by month\"."
            }),
        );
    }
    params
}

fn str_arg<'a>(args: &'a Json, key: &str) -> EngineResult<&'a str> {
    args.get(key)
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| EngineError::msg(format!("missing required argument `{key}`")))
}

/// Render a QueryResult as a compact monospace table for the model.
fn table_text(q: &QueryResult, max_rows: usize) -> String {
    if q.columns.is_empty() {
        return format!("(0 columns, {} rows)", q.row_count);
    }
    let shown = q.rows.iter().take(max_rows);
    let mut widths: Vec<usize> = q.columns.iter().map(|c| c.len()).collect();
    for row in shown.clone() {
        for (i, cell) in row.iter().enumerate() {
            widths[i] = widths[i].max(cell_str(cell).len());
        }
    }
    let mut out = String::new();
    out.push_str(
        &q.columns
            .iter()
            .enumerate()
            .map(|(i, c)| format!("{:<w$}", c, w = widths[i]))
            .collect::<Vec<_>>()
            .join("  "),
    );
    out.push('\n');
    for row in shown {
        out.push_str(
            &row.iter()
                .enumerate()
                .map(|(i, c)| format!("{:<w$}", cell_str(c), w = widths[i]))
                .collect::<Vec<_>>()
                .join("  "),
        );
        out.push('\n');
    }
    let extra = q.row_count.saturating_sub(max_rows.min(q.rows.len()));
    if extra > 0 {
        out.push_str(&format!("… {extra} more rows\n"));
    }
    // A bare aggregate over an empty set comes back as one all-NULL row, which
    // renders as a blank cell; a smaller model reads that as "the query failed"
    // and starts probing whether the category/filter exists. Say plainly that
    // nothing matched so an empty SUM/COUNT is taken as the answer (0 / none).
    let nothing_matched =
        q.row_count == 0 || (q.rows.len() == 1 && q.rows[0].iter().all(|c| c.is_null()));
    if nothing_matched {
        out.push_str("nothing matched this query an empty SUM/COUNT is 0, an empty MIN/MAX/AVG is none; that is the answer\n");
    }
    out.push_str(&format!("({} rows total)", q.row_count));
    out
}

fn model_table_limit(q: &QueryResult) -> usize {
    if !q.truncated && q.row_count <= MODEL_TABLE_COMPLETE_ROWS {
        q.rows.len()
    } else {
        MODEL_TABLE_PREVIEW_ROWS
    }
}

fn cell_str(v: &Json) -> String {
    match v {
        Json::Null => "".into(),
        Json::String(s) => s.clone(),
        other => other.to_string(),
    }
}

// --- list_files ----------------------------------------------------------

pub struct ListFiles;

#[async_trait]
impl Tool for ListFiles {
    fn name(&self) -> &'static str {
        "list_files"
    }
    fn description(&self) -> &'static str {
        "List the files in the workspace and the tables detected in them."
    }
    fn parameters(&self) -> Json {
        json!({ "type": "object", "properties": {}, "additionalProperties": false })
    }
    async fn run(&self, engine: &EngineState, _args: &Json) -> EngineResult<ToolOutput> {
        let catalog = engine.catalog();
        if catalog.workspace.is_none() {
            return Err(EngineError::NoWorkspace);
        }
        let mut lines = Vec::new();
        for s in &catalog.sources {
            match &s.view {
                Some(v) => lines.push(format!(
                    "table {v}  (from {}, {} rows)",
                    s.name,
                    s.row_count
                        .map(|n| n.to_string())
                        .unwrap_or_else(|| "?".into())
                )),
                None => lines.push(format!(
                    "document {}  ({:?}, {} KB)",
                    s.name,
                    s.kind,
                    s.size_bytes / 1024
                )),
            }
        }
        let text = if lines.is_empty() {
            "workspace is empty".to_string()
        } else {
            lines.join("\n")
        };
        Ok(ToolOutput {
            summary: format!("{} files", catalog.sources.len()),
            llm_text: text.clone(),
            sql: None,
            columns: None,
            rows: None,
            row_count: None,
            // Row/table counts named here (e.g. "30 rows") are real figures a
            // summary answer may quote -- stored so verify's number check
            // finds them as backed, not "not found in any result".
            output: Some(text),
            chart: None,
        })
    }
}

// --- inspect_table -----------------------------------------------------
//
// One tool for "look at a table": per-column stats + sample rows. Merged from
// the old `describe_schema` + `sample_rows` (the former already folded in three
// rows, so the pair was mostly one thing with two names).

pub struct InspectTable;

#[async_trait]
impl Tool for InspectTable {
    fn name(&self) -> &'static str {
        "inspect_table"
    }
    fn description(&self) -> &'static str {
        "Per-column stats for a table (type, null fraction, distinct count, min, max) \
and its first few rows. One call to see everything about a table. `rows` sets how \
many sample rows to return (default 5, max 50)."
    }
    fn parameters(&self) -> Json {
        json!({
            "type": "object",
            "properties": {
                "name": { "type": "string" },
                "rows": { "type": "integer", "minimum": 0, "maximum": 50 }
            },
            "required": ["name"],
            "additionalProperties": false
        })
    }
    async fn run(&self, engine: &EngineState, args: &Json) -> EngineResult<ToolOutput> {
        let name = str_arg(args, "name")?;
        let n = args
            .get("rows")
            .and_then(|v| v.as_u64())
            .unwrap_or(5)
            .clamp(0, 50) as usize;

        let info = engine.describe_source(name)?;
        let cols = info.columns.unwrap_or_default();
        let mut lines = vec![format!(
            "{} {} rows, {} columns",
            name,
            info.row_count
                .map(|n| n.to_string())
                .unwrap_or_else(|| "?".into()),
            cols.len()
        )];
        if let Some(note) = &info.note {
            lines.push(format!("  note: {note}"));
        }
        for c in &cols {
            lines.push(format!(
                "  {}  {}  null={}  distinct={}  min={}  max={}{}",
                c.name,
                c.type_,
                c.null_fraction
                    .map(|f| format!("{:.0}%", f * 100.0))
                    .unwrap_or_else(|| "?".into()),
                c.distinct
                    .map(|d| d.to_string())
                    .unwrap_or_else(|| "?".into()),
                c.min.clone().unwrap_or_default(),
                c.max.clone().unwrap_or_default(),
                c.note
                    .as_deref()
                    .map(|n| format!("  [{n}]"))
                    .unwrap_or_default(),
            ));
        }

        let sample = if n > 0 {
            engine.sample(name, n).ok()
        } else {
            None
        };
        if let Some(s) = &sample {
            if !s.rows.is_empty() {
                lines.push(String::new());
                lines.push(format!("first {} row(s):", s.rows.len()));
                lines.push(table_text(s, n));
            }
        }

        let text = lines.join("\n");
        Ok(ToolOutput {
            summary: format!("inspected {name}"),
            llm_text: text.clone(),
            sql: None,
            columns: sample.as_ref().map(|s| s.columns.clone()),
            rows: sample.as_ref().map(|s| s.rows.clone()),
            row_count: sample.as_ref().map(|s| s.row_count),
            // The table's total row count and per-column stats (distinct,
            // null%, min/max) live only in this text -- `rows`/`row_count`
            // above are the *sample*, not the full table. Stored so a
            // summary answer quoting the real total isn't flagged unbacked.
            output: Some(text),
            chart: None,
        })
    }
}

// --- run_sql -----------------------------------------------------------

pub struct RunSql;

#[async_trait]
impl Tool for RunSql {
    fn name(&self) -> &'static str {
        "run_sql"
    }
    fn description(&self) -> &'static str {
        "Run a read-only SQL query (SELECT / WITH only) and return the rows. Complete results up to 100 rows are shown; larger results receive a bounded preview. Text comparisons are case-sensitive; for category or status values whose case may vary, use lower(column) = lower(value)."
    }
    fn parameters(&self) -> Json {
        json!({
            "type": "object",
            "properties": { "sql": { "type": "string", "description": "a single SELECT / WITH statement" } },
            "required": ["sql"],
            "additionalProperties": false
        })
    }
    async fn run(&self, engine: &EngineState, args: &Json) -> EngineResult<ToolOutput> {
        let sql = str_arg(args, "sql")?;
        let q = engine.run_sql(sql)?;
        Ok(sql_output(engine, sql, q))
    }
    async fn run_with_cancel(
        &self,
        engine: &EngineState,
        args: &Json,
        cancel: Arc<AtomicBool>,
    ) -> EngineResult<ToolOutput> {
        let sql = str_arg(args, "sql")?;
        let q = engine.run_sql_cancellable(sql, cancel)?;
        Ok(sql_output(engine, sql, q))
    }
}

fn sql_output(engine: &EngineState, sql: &str, q: QueryResult) -> ToolOutput {
    let table = table_text(&q, model_table_limit(&q));
    let warning = text_agg_warning(engine, sql).or_else(|| case_filter_warning(engine, sql));
    let llm_text = match warning {
        Some(w) => format!("{w}\n{table}"),
        None => table,
    };
    ToolOutput {
        summary: format!(
            "{} row{}{} in {}ms",
            q.row_count,
            if q.row_count == 1 { "" } else { "s" },
            if q.truncated { " (capped)" } else { "" },
            q.ms
        ),
        llm_text,
        sql: Some(sql.to_string()),
        columns: Some(q.columns),
        rows: Some(q.rows),
        row_count: Some(q.row_count),
        output: None,
        chart: None,
    }
}

/// If `sql` sums/averages a column the catalog says is `TEXT`, warn the model:
/// SQLite reads non-numeric text as 0, so the total is silently wrong.
fn text_agg_warning(engine: &EngineState, sql: &str) -> Option<String> {
    // `(lowercased, original-case)` text columns - shared collector with verify.
    let text_cols = crate::engine::analytics::verify::text_columns(engine);
    let lowered: Vec<String> = text_cols.iter().map(|(l, _)| l.clone()).collect();
    let hit = crate::engine::analytics::verify::aggregates_text_column(sql, &lowered)?;
    let name = text_cols
        .iter()
        .find(|(l, _)| l == hit)
        .map(|(_, n)| n.clone())
        .unwrap_or_else(|| hit.to_string());
    Some(format!(
        "NOTE: \"{name}\" is a TEXT column SUM/AVG reads non-numeric text \
(currency signs, commas, \"N/A\") as 0, so the total can be silently wrong. Cast it, e.g. \
SUM(CAST(REPLACE(REPLACE(\"{name}\", '$', ''), ',', '') AS REAL))."
    ))
}

/// If `sql` filters a likely label column by exact case, tell the model to fold
/// case. This also catches a uniformly cased source when the model writes
/// `Leisure` for stored `leisure`.
fn case_filter_warning(engine: &EngineState, sql: &str) -> Option<String> {
    let cols = crate::engine::analytics::verify::filterable_text_columns(engine);
    let lowered: Vec<String> = cols.iter().map(|(l, _)| l.clone()).collect();
    let hit = crate::engine::analytics::verify::case_sensitive_label_filter(sql, &lowered)?;
    let name = cols
        .iter()
        .find(|(l, _)| l == hit)
        .map_or(hit, |(_, n)| n.as_str());
    let mixed_case = crate::engine::analytics::verify::mixed_case_columns(engine)
        .iter()
        .any(|(l, _)| l == hit);
    let detail = if mixed_case {
        format!(
            "NOTE: \"{name}\" has values that differ only in capitalisation (e.g. Rent / rent). \
This filter matches exact case fold it: lower(\"{name}\") = lower('value'), or \
\"{name}\" = 'value' COLLATE NOCASE."
        )
    } else {
        format!(
            "NOTE: text filters on \"{name}\" match exact case. If the source value's \
spelling or case may differ, fold it: lower(\"{name}\") = lower('value')."
        )
    };
    Some(detail)
}

// --- grep_files ------------------------------------------------------

pub struct GrepFiles;

#[async_trait]
impl Tool for GrepFiles {
    fn name(&self) -> &'static str {
        "grep_files"
    }
    fn description(&self) -> &'static str {
        "Search the text of your documents (not tables SQL already covers those) \
for a word or regular expression. Returns matching lines with their file and \
line number good for a targeted lookup (a name, an amount, a specific word)."
    }
    fn parameters(&self) -> Json {
        json!({
            "type": "object",
            "properties": {
                "pattern": { "type": "string", "description": "a word, phrase, or regular expression" },
                "max_hits": { "type": "integer", "minimum": 1, "maximum": 100 }
            },
            "required": ["pattern"],
            "additionalProperties": false
        })
    }
    async fn run(&self, engine: &EngineState, args: &Json) -> EngineResult<ToolOutput> {
        let pattern = str_arg(args, "pattern")?;
        let max_hits = args
            .get("max_hits")
            .and_then(|v| v.as_u64())
            .unwrap_or(30)
            .clamp(1, 100) as usize;
        let hits = engine.grep_files(pattern, max_hits)?;

        if hits.is_empty() {
            return Ok(ToolOutput::text(
                "no matches",
                "No document contains that text. Try list_files to see what's there, or a \
different word.",
            ));
        }

        let llm_text = hits
            .iter()
            .map(|h| format!("{}:{}: {}", h.source, h.line, h.text))
            .collect::<Vec<_>>()
            .join("\n");

        Ok(ToolOutput {
            summary: format!("{} match(es)", hits.len()),
            llm_text,
            sql: None,
            columns: Some(vec!["source".into(), "line".into(), "text".into()]),
            rows: Some(
                hits.iter()
                    .map(|h: &GrepHit| {
                        vec![
                            Json::from(h.source.clone()),
                            Json::from(h.line),
                            Json::from(h.text.clone()),
                        ]
                    })
                    .collect(),
            ),
            row_count: Some(hits.len()),
            output: None,
            chart: None,
        })
    }
}

// --- read_file ---------------------------------------------------------

pub struct ReadFile;

#[async_trait]
impl Tool for ReadFile {
    fn name(&self) -> &'static str {
        "read_file"
    }
    fn description(&self) -> &'static str {
        "Read the full text of one or more documents by name (as listed above). Pass \
`names` (an array) to read several at once in one call. Use this when a broad or \
summarization question needs the documents' actual content."
    }
    fn parameters(&self) -> Json {
        json!({
            "type": "object",
            "properties": {
                "name": { "type": "string", "description": "a single document name" },
                "names": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "several document names to read in one call"
                }
            },
            "additionalProperties": false
        })
    }
    async fn run(&self, engine: &EngineState, args: &Json) -> EngineResult<ToolOutput> {
        let names: Vec<String> = match args.get("names").and_then(|v| v.as_array()) {
            Some(arr) if !arr.is_empty() => arr
                .iter()
                .filter_map(|v| v.as_str())
                .map(str::to_string)
                .collect(),
            _ => vec![str_arg(args, "name")?.to_string()],
        };

        // Read each; header multi-file output so the model knows which is which,
        // and bound the combined text so a big batch can't blow up the prompt.
        const COMBINED_CAP: usize = 16_000;
        let mut parts: Vec<String> = Vec::new();
        let mut labels: Vec<String> = Vec::new();
        let mut used = 0usize;
        let mut any_truncated = false;
        for name in &names {
            let (text, truncated) = engine.read_file(name)?;
            any_truncated |= truncated;
            let remaining = COMBINED_CAP.saturating_sub(used);
            let clipped: String = if text.chars().count() > remaining {
                any_truncated = true;
                text.chars().take(remaining).collect()
            } else {
                text
            };
            used += clipped.chars().count();
            labels.push(format!("{name} ({} chars)", clipped.chars().count()));
            parts.push(if names.len() > 1 {
                format!("=== {name} ===\n{clipped}")
            } else {
                clipped
            });
        }
        let combined = parts.join("\n\n");
        let summary = format!(
            "{}{}",
            labels.join("; "),
            if any_truncated { " (truncated)" } else { "" }
        );
        Ok(ToolOutput {
            summary,
            llm_text: combined.clone(),
            sql: None,
            columns: None,
            rows: None,
            row_count: None,
            output: Some(combined),
            chart: None,
        })
    }
}

// --- run_python ----------------------------------------------------

pub struct RunPython;

#[async_trait]
impl Tool for RunPython {
    fn name(&self) -> &'static str {
        "run_python"
    }
    fn description(&self) -> &'static str {
        "Run a short Python 3 snippet for analysis that SQL can't express: `median(values)`, \
`stdev(values)`, correlation (`pearsonr(x, y)`), or a simple linear regression \
(`linregress(x, y)` -> slope, intercept, r). These helpers are built in. `sql(query)` \
returns a list of dictionaries from the workspace tables. The snippet runs in Fella's \
local WASM + RustPython sandbox: it has no filesystem, network, environment, or subprocess \
access, and can only print or request bounded read-only workspace SQL. Use it for computation \
over the mounted data, not for fetching anything."
    }
    fn parameters(&self) -> Json {
        json!({
            "type": "object",
            "properties": { "code": { "type": "string", "description": "Python 3 source using the core language, Fella's built-in analytics helpers, and sql(query)" } },
            "required": ["code"],
            "additionalProperties": false
        })
    }
    async fn run(&self, engine: &EngineState, args: &Json) -> EngineResult<ToolOutput> {
        let code = str_arg(args, "code")?;
        let r = engine.run_python(code).await?;
        Ok(python_output(r))
    }
    async fn run_with_cancel(
        &self,
        engine: &EngineState,
        args: &Json,
        cancel: Arc<AtomicBool>,
    ) -> EngineResult<ToolOutput> {
        let code = str_arg(args, "code")?;
        let r = engine.run_python_cancellable(code, cancel).await?;
        Ok(python_output(r))
    }
}

fn python_output(r: crate::engine::analytics::pyexec::PyResult) -> ToolOutput {
    let mut combined = String::new();
    if !r.stdout.is_empty() {
        combined.push_str(&r.stdout);
    }
    if !r.stderr.is_empty() {
        if !combined.is_empty() {
            combined.push('\n');
        }
        combined.push_str("stderr:\n");
        combined.push_str(&r.stderr);
    }
    if combined.is_empty() {
        combined.push_str("(no output)");
    }

    let summary = if r.cancelled {
        format!("python stopped by you after {}ms · local sandbox", r.ms)
    } else if r.timed_out {
        format!(
            "python execution budget ended after {}ms · local sandbox",
            r.ms
        )
    } else {
        match r.exit_code {
            Some(0) => format!("python finished in {}ms · local sandbox", r.ms),
            Some(c) => format!("python exited with code {c} in {}ms · local sandbox", r.ms),
            None => format!(
                "python was stopped after {}ms inside the local sandbox",
                r.ms
            ),
        }
    };
    let llm_text = format!("{summary}\n\n{}", truncate_chars(&combined, 6000));

    ToolOutput {
        summary,
        llm_text,
        sql: None,
        columns: None,
        rows: None,
        row_count: None,
        output: Some(combined),
        chart: None,
    }
}

// --- make_chart ------------------------------------------------------
//
// The data types and validation live in `analytics::chart` (plain data,
// independent of the rest of the app); this is just the app-calling glue
// that turns arguments into that data and hands the result back as a
// `ToolOutput`, same as every other tool here.

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ChartArgs {
    kind: ChartKind,
    #[serde(default)]
    title: Option<String>,
    sql: String,
    #[serde(default)]
    unit: Option<String>,
    #[serde(default, rename = "note")]
    _note: Option<String>,
}

pub struct MakeChart;

#[async_trait]
impl Tool for MakeChart {
    fn name(&self) -> &'static str {
        "make_chart"
    }
    fn description(&self) -> &'static str {
        "Draw a chart from a read-only SQL query. Use auto unless the user clearly asks for \
        a bar or line chart. The first query column must be the label or date and the remaining \
        one or two columns must be numeric. Category charts support up to 12 labels; time-series \
        line charts support up to 1000 points. For longer periods, aggregate to a coarser time \
        period or narrow the date range. It renders itself in the answer."
    }
    fn parameters(&self) -> Json {
        json!({
            "type": "object",
            "properties": {
                "kind": {
                    "type": "string",
                    "enum": ["auto", "bar", "line"],
                    "description": "auto chooses a line for time periods and a bar chart for categories"
                },
                "title": { "type": "string", "description": "short chart title, e.g. \"Spending by category\"" },
                "sql": {
                    "type": "string",
                    "description": "single read-only SELECT/WITH query; first column is the label/date and the next one or two columns are numeric"
                },
                "unit": { "type": "string", "description": "optional short suffix/prefix for values, e.g. \"$\" or \"%\"" }
            },
            "required": ["kind", "sql"],
            "additionalProperties": false
        })
    }
    async fn run(&self, engine: &EngineState, args: &Json) -> EngineResult<ToolOutput> {
        let parsed: ChartArgs = serde_json::from_value(args.clone())
            .map_err(|e| EngineError::msg(format!("invalid make_chart arguments: {e}")))?;
        let sql = parsed.sql.trim();
        if sql.is_empty() {
            return Err(EngineError::msg("make_chart needs a non-empty SQL query"));
        }
        let q = engine.run_sql(sql)?;
        if q.truncated {
            return Err(EngineError::msg(format!(
                "the chart query returned {} rows, beyond the {}-row raw result limit; aggregate \
to a coarser time period or narrow the date range first",
                q.row_count, DEFAULT_ROW_CAP
            )));
        }
        let data = chart::from_query(parsed.kind, parsed.title, parsed.unit, &q.columns, &q.rows)
            .map_err(EngineError::msg)?;

        let n_series = data.series.len();
        let n_labels = data.labels.len();
        let kind_word = match data.kind {
            // `from_query` resolves auto before returning. Keep this arm for
            // exhaustiveness if a future caller constructs the type directly.
            ChartKind::Auto => "chart",
            ChartKind::Bar => "bar",
            ChartKind::Line => "line",
        };
        Ok(ToolOutput {
            summary: format!(
                "{kind_word} chart, {n_labels} categor{}, {n_series} series",
                if n_labels == 1 { "y" } else { "ies" }
            ),
            llm_text: format!(
                "Chart drawn from the query result below; it renders as a visual answer block. \
                 Lead with one short sentence explaining the main pattern. Do not list every \
                 value in prose.\n\n{}",
                table_text(&q, chart::MAX_CATEGORIES)
            ),
            sql: Some(sql.to_string()),
            columns: Some(q.columns),
            rows: Some(q.rows),
            row_count: Some(q.row_count),
            output: None,
            chart: Some(data),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn qr(columns: &[&str], rows: Vec<Vec<Json>>) -> QueryResult {
        QueryResult {
            columns: columns.iter().map(|s| s.to_string()).collect(),
            row_count: rows.len(),
            rows,
            ms: 0,
            truncated: false,
        }
    }

    #[test]
    fn table_text_spells_out_an_empty_aggregate() {
        // SUM over no matching rows -> one all-NULL row.
        let t = table_text(&qr(&["SUM(amount)"], vec![vec![Json::Null]]), 30);
        assert!(t.contains("nothing matched"), "{t}");

        // GROUP BY with no matches -> zero rows.
        let t0 = table_text(&qr(&["category", "n"], vec![]), 30);
        assert!(t0.contains("nothing matched"), "{t0}");

        // A real zero (COUNT) is unambiguous already leave it alone.
        let c = table_text(&qr(&["n"], vec![vec![Json::from(0)]]), 30);
        assert!(!c.contains("nothing matched"), "{c}");

        // A normal result is untouched.
        let r = table_text(&qr(&["x"], vec![vec![Json::from(5)]]), 30);
        assert!(!r.contains("nothing matched"), "{r}");
    }

    #[test]
    fn small_sql_results_are_rendered_completely_for_the_model() {
        let rows: Vec<Vec<Json>> = (0..50)
            .map(|i| vec![Json::from(format!("category-{i}")), Json::from(i)])
            .collect();
        let q = qr(&["category", "total"], rows);
        let text = table_text(&q, model_table_limit(&q));

        assert!(text.contains("category-49"), "last row was hidden: {text}");
        assert!(
            !text.contains("more rows"),
            "small result was previewed: {text}"
        );
    }

    #[test]
    fn large_sql_results_keep_a_bounded_model_preview() {
        let rows: Vec<Vec<Json>> = (0..101).map(|i| vec![Json::from(i)]).collect();
        let q = qr(&["value"], rows);
        let text = table_text(&q, model_table_limit(&q));

        assert!(
            !text.lines().any(|line| line.trim() == "100"),
            "large result was rendered in full: {text}"
        );
        assert!(
            text.contains("… 71 more rows"),
            "preview count was missing: {text}"
        );
    }

    #[test]
    fn inspect_registry_excludes_python() {
        let inspect_names: Vec<String> = Registry::inspect()
            .schemas()
            .into_iter()
            .map(|schema| schema.name)
            .collect();
        assert!(!inspect_names.iter().any(|name| name == "run_python"));
        assert!(inspect_names.iter().any(|name| name == "run_sql"));

        let standard_names: Vec<String> = Registry::standard()
            .schemas()
            .into_iter()
            .map(|schema| schema.name)
            .collect();
        assert!(standard_names.iter().any(|name| name == "run_python"));
    }
}
