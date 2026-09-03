//! The deterministic tools the agent may call. This is the ONLY path from the
//! model to the data the model never touches the database directly. Adding a tool
//! is a deliberate code change; there is no plugin mechanism.

use async_trait::async_trait;
use serde_json::{json, Value as Json};

use crate::engine::error::{EngineError, EngineResult};
use crate::engine::llm::ToolSchema;
use crate::engine::state::{EngineState, GrepHit, QueryResult};

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
        }
    }
}

#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn parameters(&self) -> Json;
    async fn run(&self, engine: &EngineState, args: &Json) -> EngineResult<ToolOutput>;
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
        Self {
            tools: vec![
                Box::new(ListFiles),
                Box::new(DescribeSchema),
                Box::new(SampleRows),
                Box::new(RunSql),
                Box::new(GrepFiles),
                Box::new(ReadFile),
                Box::new(RunPython),
            ],
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
        self.tools.iter().find(|t| t.name() == name).map(|b| b.as_ref())
    }

    /// Run the tool named `name`. `None` = no such tool.
    pub async fn run(
        &self,
        engine: &EngineState,
        name: &str,
        args: &Json,
    ) -> Option<EngineResult<ToolOutput>> {
        if let Some(tool) = self.get(name) {
            return Some(tool.run(engine, args).await);
        }
        #[cfg(feature = "mcp")]
        if let Some(t) = self.mcp.iter().find(|t| t.namespaced == name) {
            return Some(crate::engine::mcp::run_mcp_tool(t, args).await);
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

/// Render a QueryResult as a small monospace table for the model.
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
    out.push_str(&format!("({} rows total)", q.row_count));
    out
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
                    s.row_count.map(|n| n.to_string()).unwrap_or_else(|| "?".into())
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
        Ok(ToolOutput::text(
            format!("{} files", catalog.sources.len()),
            text,
        ))
    }
}

// --- describe_schema -------------------------------------------------

pub struct DescribeSchema;

#[async_trait]
impl Tool for DescribeSchema {
    fn name(&self) -> &'static str {
        "describe_schema"
    }
    fn description(&self) -> &'static str {
        "Per-column stats for a table (type, null fraction, distinct count, min, max) \
plus a few sample rows. One call to see everything about a table."
    }
    fn parameters(&self) -> Json {
        json!({
            "type": "object",
            "properties": { "name": { "type": "string" } },
            "required": ["name"],
            "additionalProperties": false
        })
    }
    async fn run(&self, engine: &EngineState, args: &Json) -> EngineResult<ToolOutput> {
        let name = str_arg(args, "name")?;
        let info = engine.describe_source(name)?;
        let cols = info.columns.unwrap_or_default();
        let mut lines = vec![format!(
            "{} {} rows, {} columns",
            name,
            info.row_count.map(|n| n.to_string()).unwrap_or_else(|| "?".into()),
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
                c.distinct.map(|d| d.to_string()).unwrap_or_else(|| "?".into()),
                c.min.clone().unwrap_or_default(),
                c.max.clone().unwrap_or_default(),
                c.note.as_deref().map(|n| format!("  [{n}]")).unwrap_or_default(),
            ));
        }
        // Fold in a few sample rows so the model doesn't need a follow-up
        // sample_rows call to see what the data actually looks like.
        if let Ok(sample) = engine.sample(name, 3) {
            if !sample.rows.is_empty() {
                lines.push(String::new());
                lines.push("sample rows:".to_string());
                lines.push(table_text(&sample, 3));
            }
        }
        Ok(ToolOutput::text(
            format!("schema of {name}"),
            lines.join("\n"),
        ))
    }
}

// --- sample_rows ---------------------------------------------------------

pub struct SampleRows;

#[async_trait]
impl Tool for SampleRows {
    fn name(&self) -> &'static str {
        "sample_rows"
    }
    fn description(&self) -> &'static str {
        "Return the first N rows of a table (default 10, max 50)."
    }
    fn parameters(&self) -> Json {
        json!({
            "type": "object",
            "properties": {
                "name": { "type": "string" },
                "n": { "type": "integer", "minimum": 1, "maximum": 50 }
            },
            "required": ["name"],
            "additionalProperties": false
        })
    }
    async fn run(&self, engine: &EngineState, args: &Json) -> EngineResult<ToolOutput> {
        let name = str_arg(args, "name")?;
        let n = args
            .get("n")
            .and_then(|v| v.as_u64())
            .unwrap_or(10)
            .clamp(1, 50) as usize;
        let q = engine.sample(name, n)?;
        Ok(ToolOutput {
            summary: format!("{} sample rows from {name}", q.rows.len()),
            llm_text: table_text(&q, n),
            sql: None,
            columns: Some(q.columns),
            rows: Some(q.rows),
            row_count: Some(q.row_count),
            output: None,
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
        "Run a read-only SQL query (SELECT / WITH only) and return the rows."
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
        let table = table_text(&q, 30);
        let llm_text = match text_agg_warning(engine, sql) {
            Some(w) => format!("{w}\n{table}"),
            None => table,
        };
        Ok(ToolOutput {
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
        })
    }
}

/// If `sql` sums/averages a column the catalog says is `TEXT`, warn the model:
/// SQLite reads non-numeric text as 0, so the total is silently wrong.
fn text_agg_warning(engine: &EngineState, sql: &str) -> Option<String> {
    // `(lowercased, original-case)` text columns - shared collector with verify.
    let text_cols = crate::engine::verify::text_columns(engine);
    let lowered: Vec<String> = text_cols.iter().map(|(l, _)| l.clone()).collect();
    let hit = crate::engine::verify::aggregates_text_column(sql, &lowered)?;
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
                        vec![Json::from(h.source.clone()), Json::from(h.line), Json::from(h.text.clone())]
                    })
                    .collect(),
            ),
            row_count: Some(hits.len()),
            output: None,
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
            Some(arr) if !arr.is_empty() => {
                arr.iter().filter_map(|v| v.as_str()).map(str::to_string).collect()
            }
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
        "Run a short Python 3 snippet for analysis that SQL can't express (e.g. \
scipy stats, regressions). A helper `sql(query)` returns a pandas DataFrame of \
the workspace tables. Print results to stdout. About 20s and 1 GB; use it only \
to compute over the workspace data, not to fetch anything."
    }
    fn parameters(&self) -> Json {
        json!({
            "type": "object",
            "properties": { "code": { "type": "string", "description": "Python 3 source" } },
            "required": ["code"],
            "additionalProperties": false
        })
    }
    async fn run(&self, engine: &EngineState, args: &Json) -> EngineResult<ToolOutput> {
        let code = str_arg(args, "code")?;
        let r = engine.run_python(code).await?;

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

        let summary = if r.timed_out {
            format!("python timed out after {}ms", r.ms)
        } else {
            match r.exit_code {
                Some(0) => format!("python finished in {}ms", r.ms),
                Some(c) => format!("python exited with code {c} in {}ms", r.ms),
                // No exit code = the process was killed by a signal, usually the
                // memory or CPU rlimit.
                None => format!("python was stopped after {}ms (it ran out of memory or time)", r.ms),
            }
        };
        let llm_text = format!("{summary}\n\n{}", truncate_chars(&combined, 6000));

        Ok(ToolOutput {
            summary,
            llm_text,
            sql: None,
            columns: None,
            rows: None,
            row_count: None,
            output: Some(combined),
        })
    }
}

fn truncate_chars(s: &str, n: usize) -> String {
    match s.char_indices().nth(n) {
        Some((i, _)) => format!("{}…", &s[..i]),
        None => s.to_string(),
    }
}
