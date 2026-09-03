//! The data engine: the thing that turns files into queryable tables and runs
//! read-only SQL against them.
//!
//! Default backend is `sqlite` (bundled, tiny). The `duckdb` Cargo feature
//! swaps in DuckDB (Parquet, faster on big files). Everything above this
//! module the agent loop, the tools, verification is backend-agnostic.

use serde::Serialize;
use serde_json::Value as Json;

use crate::engine::catalog::{ColumnInfo, SourceKind};
use crate::engine::error::{EngineError, EngineResult};

pub mod sqlite;
#[cfg(feature = "duckdb")]
pub mod duck;

/// Rows returned to callers are capped at this many by default.
pub const DEFAULT_ROW_CAP: usize = 1000;

/// A single `run_sql` is interrupted after this long. Personal-analytics
/// queries on MB-scale files finish in milliseconds; this catches runaways.
pub const QUERY_TIMEOUT_SECS: u64 = 15;

/// Effective per-query timeout. `FELLA_QUERY_TIMEOUT_SECS` overrides the
/// default a power-user escape hatch, and how the tests exercise the path
/// without waiting 15 s.
pub fn query_timeout_secs() -> u64 {
    std::env::var("FELLA_QUERY_TIMEOUT_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|&n| n > 0)
        .unwrap_or(QUERY_TIMEOUT_SECS)
}

/// Most rows we'll pull into memory from one delimited file at ingest. Generous
/// enough for any real personal export; a guard so a multi-GB log or a runaway
/// dump can't spike RAM into the GBs and freeze every query. The file still
/// loads its first `n` rows are usable, with a note that it was truncated.
/// `FELLA_INGEST_ROW_CAP` overrides it (and lets tests hit the path cheaply).
pub const INGEST_ROW_CAP: usize = 2_000_000;

pub fn ingest_row_cap() -> usize {
    std::env::var("FELLA_INGEST_ROW_CAP")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|&n| n > 0)
        .unwrap_or(INGEST_ROW_CAP)
}

/// A neutral cell value used for bulk inserts and query results.
pub type Cell = Json;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ColType {
    Int,
    Float,
    Bool,
    Text,
}

impl ColType {
    pub fn sqlite(self) -> &'static str {
        match self {
            ColType::Int | ColType::Bool => "INTEGER",
            ColType::Float => "REAL",
            ColType::Text => "TEXT",
        }
    }
    #[cfg(feature = "duckdb")]
    pub fn duckdb(self) -> &'static str {
        match self {
            ColType::Int => "BIGINT",
            ColType::Float => "DOUBLE",
            ColType::Bool => "BOOLEAN",
            ColType::Text => "VARCHAR",
        }
    }
}

#[derive(Debug, Serialize)]
pub struct QueryOutcome {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Json>>,
    /// Total rows produced (may exceed `rows.len()` if capped).
    pub row_count: usize,
    pub truncated: bool,
}

pub struct SourceLoad {
    pub row_count: i64,
    pub columns: Vec<ColumnInfo>,
    /// A whole-file ingest caveat (delimiter guessed, rows dropped for a decode
    /// error). `None` when the file read cleanly.
    pub note: Option<String>,
}

/// How `run_python`'s `sql()` helper reaches the workspace data.
pub enum PythonBridge {
    /// Point the Python subprocess at a read-only SQLite file.
    SqliteFile(std::path::PathBuf),
    /// Hand the subprocess `(table, FROM-expression)` pairs to re-read files
    /// through its own DuckDB (needs `pip install duckdb`).
    #[cfg(feature = "duckdb")]
    DuckReaders(Vec<(String, String)>),
}

pub trait DataEngine: Send {
    /// Register a path-readable tabular file (CSV/TSV/JSON/NDJSON, and Parquet
    /// on the DuckDB backend) as a table called `name`.
    fn add_source(&mut self, name: &str, kind: SourceKind, path: &str) -> EngineResult<SourceLoad>;

    /// Build a table from already-parsed rows (used by the Excel ingest).
    fn add_rows(
        &mut self,
        name: &str,
        columns: &[(String, ColType)],
        rows: &[Vec<Cell>],
    ) -> EngineResult<i64>;

    fn drop_source(&mut self, name: &str);

    /// Per-column stats for `name`: type, null fraction, distinct count, min, max.
    fn describe(&self, name: &str) -> EngineResult<Vec<ColumnInfo>>;

    /// Run a read-only query; materialise up to `max_rows` rows.
    fn query(&self, sql: &str, max_rows: usize) -> EngineResult<QueryOutcome>;

    fn python_bridge(&self) -> PythonBridge;
}

/// Build the configured backend.
pub fn open_engine(data_dir: &std::path::Path) -> EngineResult<Box<dyn DataEngine>> {
    #[cfg(feature = "duckdb")]
    {
        let _ = data_dir;
        Ok(Box::new(duck::DuckEngine::open()?))
    }
    #[cfg(not(feature = "duckdb"))]
    {
        Ok(Box::new(sqlite::SqliteEngine::open(data_dir)?))
    }
}

// --- read-only guard ---------------------------------------------------------

/// Reject anything that could mutate state, touch the filesystem, or smuggle in
/// a second statement. A guard rail, not a hard security boundary (the SQLite
/// backend also opens its query connection read-only for real enforcement).
pub fn ensure_read_only(sql: &str) -> EngineResult<()> {
    let cleaned = strip_comments(sql);
    let lower = cleaned.to_lowercase();
    let trimmed = lower.trim().trim_start_matches('(').trim();

    let body = trimmed.trim_end_matches(';').trim();
    if body.contains(';') {
        return Err(EngineError::Forbidden(
            "only a single statement is allowed".into(),
        ));
    }

    let first = body
        .split(|c: char| c.is_whitespace())
        .next()
        .unwrap_or_default();
    const ALLOWED_START: &[&str] = &[
        "select", "with", "table", "from", "values", "describe", "summarize", "explain", "pragma",
    ];
    if !ALLOWED_START.contains(&first) {
        return Err(EngineError::Forbidden(format!(
            "statements must start with SELECT / WITH (got `{first}`)"
        )));
    }
    // `pragma` is allowed to start (table_info introspection) but only the
    // read-only shape `pragma name(arg)` / `pragma name`.
    if first == "pragma" && (body.contains('=') || body.contains("writable")) {
        return Err(EngineError::Forbidden("that PRAGMA is not allowed".into()));
    }

    const BANNED: &[&str] = &[
        "attach", "detach", "copy", "install", "load", "export", "import", "vacuum", "reindex",
        "analyze", "call", "create", "drop", "alter", "insert", "update", "delete", "replace",
        "truncate", "begin", "commit", "rollback", "savepoint", "read_text", "read_blob", "glob",
        // DuckDB file/DB-reading table functions: the catalog builds the views
        // Fella needs; the model never calls these directly, and left open they
        // let a query read any path on disk (`read_csv_auto('/etc/passwd')`),
        // which would breach the workspace boundary the docs call the safety
        // story. Harmless no-ops on the SQLite backend.
        "read_csv", "read_csv_auto", "read_parquet", "parquet_scan", "parquet_metadata",
        "parquet_schema", "read_json", "read_json_auto", "read_json_objects", "read_ndjson",
        "read_ndjson_auto", "read_ndjson_objects", "postgres_scan", "postgres_query",
        "sqlite_scan", "sqlite_query", "mysql_scan", "mysql_query", "iceberg_scan", "delta_scan",
    ];
    let banned_hit = body
        .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .find(|tok| BANNED.contains(tok));
    if let Some(tok) = banned_hit {
        return Err(EngineError::Forbidden(format!("`{tok}` is not allowed")));
    }
    Ok(())
}

/// Blank out `-- line` and `/* block */` comments so the guard sees only code.
pub fn strip_comments(sql: &str) -> String {
    let mut out = String::with_capacity(sql.len());
    let mut chars = sql.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '-' if chars.peek() == Some(&'-') => {
                for c in chars.by_ref() {
                    if c == '\n' {
                        out.push('\n');
                        break;
                    }
                }
            }
            '/' if chars.peek() == Some(&'*') => {
                chars.next();
                let mut prev = '\0';
                for c in chars.by_ref() {
                    if prev == '*' && c == '/' {
                        break;
                    }
                    prev = c;
                }
                out.push(' ');
            }
            _ => out.push(c),
        }
    }
    out
}

// --- shared helpers --------------------------------------------------------

/// Null-ish placeholder tokens people type into spreadsheet cells. Treated as
/// empty by the ingest type sniffers so a lone `N/A` can't drag a whole amount
/// column down to `TEXT`.
pub fn is_blankish(s: &str) -> bool {
    matches!(
        s.trim().to_ascii_lowercase().as_str(),
        "" | "n/a" | "na" | "#n/a" | "-" | "--" | "\u{2014}" | "." | "null" | "nil" | "none" | "tbd"
    )
}

/// Parse a number a human wrote into a cell: tolerates a leading currency sign
/// (`$`, `\u{00A3}`, `\u{20AC}`, `\u{00A5}`), thousands separators (`,` or
/// spaces), a trailing `%` (scaled by 1/100), and accounting-style negatives
/// `(1,234.50)`. Returns `None` if what remains isn't a plain number. Used by
/// the CSV and Excel ingests to rescue an amount column stored as text.
///
/// Assumes **US/UK** number grammar: `,` (or space) groups thousands and `.` is
/// the decimal point. EU-formatted text (`1.234,56`) is not recognised and would
/// be misread the ingest can only coerce one convention and this is the one the
/// `$`/`\u{00A3}` fast path already implies.
pub fn parse_numeric(s: &str) -> Option<f64> {
    let t = s.trim();
    if t.is_empty() {
        return None;
    }
    // Fast path: already a clean number. Reject non-finite ("inf", "nan") so a
    // column of those doesn't sniff as numeric and then store as NULL.
    if let Ok(v) = t.parse::<f64>() {
        return v.is_finite().then_some(v);
    }

    let mut body = t;
    let mut negative = false;

    if body.starts_with('(') && body.ends_with(')') {
        negative = true;
        body = body[1..body.len() - 1].trim();
    }
    if let Some(rest) = body.strip_prefix('-') {
        negative = !negative;
        body = rest.trim_start();
    } else if let Some(rest) = body.strip_prefix('+') {
        body = rest.trim_start();
    }
    for sym in ['$', '\u{00A3}', '\u{20AC}', '\u{00A5}', '\u{20B9}'] {
        if let Some(rest) = body.strip_prefix(sym) {
            body = rest.trim_start();
            break;
        }
    }
    let mut percent = false;
    if let Some(rest) = body.strip_suffix('%') {
        percent = true;
        body = rest.trim_end();
    }

    // What's left must be digits, grouping separators, and at most a decimal point.
    if body.is_empty()
        || !body
            .bytes()
            .all(|b| b.is_ascii_digit() || b == b',' || b == b' ' || b == b'.')
    {
        return None;
    }
    // Split into an integer part and at most one decimal part.
    let (int_part, frac_part) = match body.split_once('.') {
        Some((i, f)) => {
            if f.contains('.') {
                return None;
            }
            (i, Some(f))
        }
        None => (body, None),
    };
    // The integer part's `,`/space groups must look like real thousands
    // grouping: any group after the first is exactly three digits. This
    // rejects "1 2 3" and "1,23,456" instead of silently reading them as 123 /
    // 123456.
    let groups: Vec<&str> = int_part.split([',', ' ']).filter(|g| !g.is_empty()).collect();
    if groups.is_empty() && frac_part.is_none_or(|f| f.is_empty()) {
        return None;
    }
    if int_part.contains([',', ' ']) {
        for (i, g) in groups.iter().enumerate() {
            let ok_len = if i == 0 { g.len() <= 3 } else { g.len() == 3 };
            if !ok_len || !g.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
        }
    }
    let cleaned: String = body.chars().filter(|c| *c != ',' && *c != ' ').collect();
    if !cleaned.bytes().any(|b| b.is_ascii_digit()) {
        return None;
    }
    let mut v: f64 = cleaned.parse().ok()?;
    if !v.is_finite() {
        return None;
    }
    if percent {
        v /= 100.0;
    }
    if negative {
        v = -v;
    }
    Some(v)
}

/// Quote an identifier for interpolation into SQL: `"a""b"`.
pub fn quote_ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

/// Quote a string literal: `'a''b'`.
pub fn quote_str(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_numeric_rescues_written_numbers() {
        assert_eq!(parse_numeric("1200"), Some(1200.0));
        assert_eq!(parse_numeric("1200.50"), Some(1200.5));
        assert_eq!(parse_numeric("$1,200.00"), Some(1200.0));
        assert_eq!(parse_numeric("  1,150 "), Some(1150.0));
        assert_eq!(parse_numeric("\u{00A3}2,000"), Some(2000.0));
        assert_eq!(parse_numeric("(45)"), Some(-45.0));
        assert_eq!(parse_numeric("($1,000.00)"), Some(-1000.0));
        assert_eq!(parse_numeric("-1,000"), Some(-1000.0));
        assert_eq!(parse_numeric("12%"), Some(0.12));
        assert_eq!(parse_numeric("1 200"), Some(1200.0));
        assert_eq!(parse_numeric("12,345"), Some(12345.0));
        assert_eq!(parse_numeric("N/A"), None);
        assert_eq!(parse_numeric("pending"), None);
        assert_eq!(parse_numeric(""), None);
        assert_eq!(parse_numeric("-"), None);
        assert_eq!(parse_numeric("1,2,3 apples"), None);
        // Non-finite and malformed grouping must not sneak through as numbers.
        assert_eq!(parse_numeric("inf"), None);
        assert_eq!(parse_numeric("nan"), None);
        assert_eq!(parse_numeric("1 2 3"), None);
        assert_eq!(parse_numeric("1,23,456"), None);
        assert_eq!(parse_numeric("."), None);
    }

    #[test]
    fn blankish_tokens() {
        assert!(is_blankish(""));
        assert!(is_blankish("  N/A "));
        assert!(is_blankish("null"));
        assert!(is_blankish("\u{2014}"));
        assert!(!is_blankish("0"));
        assert!(!is_blankish("paid"));
    }

    #[test]
    fn read_only_guard_allows_selects() {
        assert!(ensure_read_only("SELECT 1").is_ok());
        assert!(ensure_read_only("  with x as (select 1) select * from x ").is_ok());
        assert!(ensure_read_only("-- a comment\nSELECT count(*) FROM t").is_ok());
        assert!(ensure_read_only("FROM sales SELECT *").is_ok());
    }

    #[test]
    fn read_only_guard_rejects_mutations() {
        for bad in [
            "DROP TABLE sales",
            "INSERT INTO t VALUES (1)",
            "UPDATE t SET x = 1",
            "DELETE FROM t",
            "ATTACH 'x.db'",
            "SELECT 1; DROP TABLE t",
            "select read_text('/etc/passwd')",
            "CREATE TABLE t (a int)",
            "VACUUM",
            "PRAGMA writable_schema = 1",
            // DuckDB file-reading table functions must not escape the workspace.
            "SELECT * FROM read_csv_auto('/etc/passwd')",
            "select * from read_csv('/etc/hosts')",
            "SELECT * FROM read_parquet('/home/user/.aws/credentials')",
            "with x as (select * from read_json_auto('/secret')) select * from x",
            "select * from parquet_scan('/anywhere.parquet')",
        ] {
            assert!(ensure_read_only(bad).is_err(), "should reject: {bad}");
        }
    }
}
