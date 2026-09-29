//! The data engine: the thing that turns files into queryable tables and runs
//! read-only SQL against them.
//!
//! Default backend is `sqlite` (bundled, tiny). The `duckdb` Cargo feature
//! swaps in DuckDB (Parquet, faster on big files). Everything above this
//! module the agent loop, the tools, verification is backend-agnostic.

use serde::Serialize;
use serde_json::Value as Json;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use crate::engine::catalog::{ColumnInfo, SourceKind};
use crate::engine::error::{EngineError, EngineResult};

#[cfg(feature = "duckdb")]
pub mod duck;
pub mod sqlite;

/// Rows returned to callers are capped at this many by default.
pub const DEFAULT_ROW_CAP: usize = 1000;

/// A single `run_sql` is interrupted after this long. Personal-analytics
/// queries on MB-scale files finish in milliseconds; this catches runaways.
pub const QUERY_TIMEOUT_SECS: u64 = 15;

/// Effective per-query timeout. `FELLA_QUERY_TIMEOUT_SECS` overrides the
/// default a power-user escape hatch, and how the tests exercise the path
/// without waiting 15 s.
pub fn query_timeout_secs() -> u64 {
    crate::engine::env::positive("FELLA_QUERY_TIMEOUT_SECS", QUERY_TIMEOUT_SECS)
}

/// Most rows we'll pull into memory from one delimited file at ingest. Generous
/// enough for any real personal export; a guard so a multi-GB log or a runaway
/// dump can't spike RAM into the GBs and freeze every query. The file still
/// loads its first `n` rows are usable, with a note that it was truncated.
/// `FELLA_INGEST_ROW_CAP` overrides it (and lets tests hit the path cheaply).
pub const INGEST_ROW_CAP: usize = 2_000_000;

/// Approximate amount of delimited input we retain while ingesting one source.
/// Row count alone is not a useful memory guard when a single export contains
/// very wide text fields.
pub const INGEST_BYTE_CAP: usize = 256 * 1024 * 1024;

pub fn ingest_byte_cap() -> usize {
    crate::engine::env::positive("FELLA_INGEST_BYTE_CAP", INGEST_BYTE_CAP)
}

pub fn ingest_row_cap() -> usize {
    crate::engine::env::positive("FELLA_INGEST_ROW_CAP", INGEST_ROW_CAP)
}

/// A neutral cell value used for bulk inserts and query results.
pub type Cell = Json;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ColType {
    Int,
    Float,
    Bool,
    Text,
    /// A text column whose values were confidently parsed as a
    /// spelled-out-month date ("Aug 1, 2026") and normalized to ISO-8601
    /// (`YYYY-MM-DD`) at ingest, so `strftime()`/`date()` just work. Stored
    /// as TEXT like `ColType::Text`; the distinction only matters at
    /// ingest time (see `sqlite::string_cell`/`json_cell`).
    Date,
}

impl ColType {
    pub fn sqlite(self) -> &'static str {
        match self {
            ColType::Int | ColType::Bool => "INTEGER",
            ColType::Float => "REAL",
            ColType::Text | ColType::Date => "TEXT",
        }
    }
    #[cfg(feature = "duckdb")]
    pub fn duckdb(self) -> &'static str {
        match self {
            ColType::Int => "BIGINT",
            ColType::Float => "DOUBLE",
            ColType::Bool => "BOOLEAN",
            ColType::Text | ColType::Date => "VARCHAR",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
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
    /// The host opens this SQLite file read-only on behalf of the guest.
    SqliteFile(std::path::PathBuf),
    /// The host recreates these `(table, FROM-expression)` views in its own
    /// DuckDB connection; the guest never receives the paths or expressions.
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

    /// Run a read-only query while observing a question's stop flag. Backends
    /// that can interrupt an in-flight statement should override this. The
    /// default keeps the API safe for optional backends and checks the flag at
    /// the boundary.
    fn query_with_cancel(
        &self,
        sql: &str,
        max_rows: usize,
        cancel: Arc<AtomicBool>,
    ) -> EngineResult<QueryOutcome> {
        if cancel.load(Ordering::Relaxed) {
            return Err(EngineError::msg("query stopped by user"));
        }
        let result = self.query(sql, max_rows)?;
        if cancel.load(Ordering::Relaxed) {
            return Err(EngineError::msg("query stopped by user"));
        }
        Ok(result)
    }

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
    let scan = scan_sql(sql);
    let lower = scan.code.to_lowercase();
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
        "select",
        "with",
        "table",
        "from",
        "values",
        "describe",
        "summarize",
        "explain",
        "pragma",
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

    // `replace` (the mutating `REPLACE INTO ...` statement) is deliberately not
    // listed here: any statement starting with that keyword is already rejected
    // above by the "must start with SELECT/WITH" check, so banning the bare
    // token here would only catch the harmless read-only `REPLACE(str, from,
    // to)` *function* inside a SELECT the standard way to strip currency
    // formatting (commas, `$`) before CAST/SUM.
    const BANNED: &[&str] = &[
        "attach",
        "detach",
        "copy",
        "install",
        "load",
        "export",
        "import",
        "vacuum",
        "reindex",
        "analyze",
        "call",
        "create",
        "drop",
        "alter",
        "insert",
        "update",
        "delete",
        "truncate",
        "begin",
        "commit",
        "rollback",
        "savepoint",
        "read_text",
        "read_blob",
        "glob",
        // DuckDB file/DB-reading table functions: the catalog builds the views
        // Fella needs; the model never calls these directly, and left open they
        // let a query read any path on disk (`read_csv_auto('/etc/passwd')`),
        // which would breach the workspace boundary the docs call the safety
        // story. Harmless no-ops on the SQLite backend.
        "read_csv",
        "read_csv_auto",
        "read_parquet",
        "parquet_scan",
        "parquet_metadata",
        "parquet_schema",
        "read_json",
        "read_json_auto",
        "read_json_objects",
        "read_ndjson",
        "read_ndjson_auto",
        "read_ndjson_objects",
        "postgres_scan",
        "postgres_query",
        "sqlite_scan",
        "sqlite_query",
        "mysql_scan",
        "mysql_query",
        "iceberg_scan",
        "delta_scan",
    ];
    let banned_hit = scan.words.iter().find(|tok| BANNED.contains(&tok.as_str()));
    if let Some(tok) = banned_hit {
        return Err(EngineError::Forbidden(format!("`{tok}` is not allowed")));
    }
    Ok(())
}

struct SqlScan {
    code: String,
    words: Vec<String>,
}

/// Keep SQL guard checks out of quoted data. A raw substring scan turns an
/// ordinary label such as "current export" into a false security failure, and
/// also mistakes semicolons inside a string for a second statement.
fn scan_sql(sql: &str) -> SqlScan {
    let mut code = String::with_capacity(sql.len());
    let mut words = Vec::new();
    let mut word = String::new();
    let mut chars = sql.chars().peekable();

    let flush = |word: &mut String, words: &mut Vec<String>| {
        if !word.is_empty() {
            words.push(std::mem::take(word));
        }
    };

    while let Some(c) = chars.next() {
        if c == '-' && chars.peek() == Some(&'-') {
            chars.next();
            flush(&mut word, &mut words);
            for comment in chars.by_ref() {
                if comment == '\n' {
                    code.push('\n');
                    break;
                }
            }
            continue;
        }
        if c == '/' && chars.peek() == Some(&'*') {
            chars.next();
            flush(&mut word, &mut words);
            let mut previous = '\0';
            for comment in chars.by_ref() {
                if previous == '*' && comment == '/' {
                    break;
                }
                previous = comment;
            }
            code.push(' ');
            continue;
        }

        let quote_end = match c {
            '\'' | '"' | '`' => Some(c),
            '[' => Some(']'),
            _ => None,
        };
        if let Some(end) = quote_end {
            flush(&mut word, &mut words);
            code.push(' ');
            while let Some(quoted) = chars.next() {
                if quoted == end {
                    if chars.peek() == Some(&end) {
                        chars.next();
                        continue;
                    }
                    break;
                }
            }
            continue;
        }

        if c == ';' {
            flush(&mut word, &mut words);
            code.push(';');
        } else if c.is_ascii_alphanumeric() || c == '_' {
            word.push(c.to_ascii_lowercase());
            code.push(c);
        } else {
            flush(&mut word, &mut words);
            code.push(c);
        }
    }
    flush(&mut word, &mut words);

    SqlScan { code, words }
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
        "" | "n/a"
            | "na"
            | "#n/a"
            | "-"
            | "--"
            | "\u{2014}"
            | "."
            | "null"
            | "nil"
            | "none"
            | "tbd"
    )
}

/// Parse a number a human wrote into a cell: tolerates a leading currency sign
/// (`$`, `\u{00A3}`, `\u{20AC}`, `\u{00A5}`), thousands separators (`,` or
/// spaces), a trailing `%` (scaled by 1/100), accounting-style negatives
/// `(1,234.50)`, and a trailing parenthetical note with no digits (for example
/// `$210.00 (cash)`). Returns `None` if what remains isn't a plain number.
/// An annotation containing another number (for example
/// `$1,300.00 (fee $1.95)`) remains unresolved because its meaning is
/// ambiguous. Used by the CSV and Excel ingests to rescue an amount column
/// stored as text without silently choosing between multiple figures.
///
/// Assumes **US/UK** number grammar: `,` (or space) groups thousands and `.` is
/// the decimal point. EU-formatted text (`1.234,56`) is not recognised and would
/// be misread the ingest can only coerce one convention and this is the one the
/// `$`/`\u{00A3}` fast path already implies.
pub fn parse_numeric(s: &str) -> Option<f64> {
    let mut t = s.trim();
    if t.is_empty() {
        return None;
    }

    // People commonly append a non-numeric note to a value in an export. It is
    // safe to remove only a final parenthetical suffix that contains no digit;
    // a suffix such as "(fee $1.95)" carries a second candidate amount and
    // must remain unresolved instead of being guessed at.
    if t.ends_with(')') {
        if let Some(open) = t.rfind(" (") {
            let annotation = &t[open + 2..t.len() - 1];
            if !annotation.is_empty() && !annotation.bytes().any(|b| b.is_ascii_digit()) {
                t = t[..open].trim_end();
            }
        }
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

    // Exports also commonly spell the currency as an ISO-style code
    // (`USD 1,200.00`) instead of using a symbol. Strip only known currency
    // codes so arbitrary prose such as `approx 1200` remains unresolved.
    const CURRENCY_CODES: [&str; 20] = [
        "AUD", "CAD", "CHF", "CNY", "DKK", "EUR", "GBP", "HKD", "INR", "JPY", "KRW", "MXN", "NOK",
        "NZD", "PLN", "SEK", "SGD", "THB", "USD", "ZAR",
    ];
    if body.len() >= 4
        && body.as_bytes()[3].is_ascii_whitespace()
        && body[..3].chars().all(|c| c.is_ascii_alphabetic())
        && CURRENCY_CODES
            .iter()
            .any(|code| body[..3].eq_ignore_ascii_case(code))
    {
        body = body[4..].trim_start();
    }

    for sym in ['$', '\u{00A3}', '\u{20AC}', '\u{00A5}', '\u{20B9}'] {
        if let Some(rest) = body.strip_prefix(sym) {
            body = rest.trim_start();
            break;
        }
    }
    // Some exports put the sign after the currency marker (`USD -42` or
    // `$-42`). Handle it here as well as before the marker.
    if let Some(rest) = body.strip_prefix('-') {
        negative = !negative;
        body = rest.trim_start();
    } else if let Some(rest) = body.strip_prefix('+') {
        body = rest.trim_start();
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
    let groups: Vec<&str> = int_part
        .split([',', ' '])
        .filter(|g| !g.is_empty())
        .collect();
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

/// Parses unambiguous month-name date formats into ISO-8601 (`YYYY-MM-DD`):
/// "Aug 1, 2026", "1 Aug 2026", "31-May-2024", and year-first variants.
/// An optional ordinal suffix is accepted. Deliberately does not attempt pure
/// numeric formats (`08/01/2026`): this parser has no column context, so it
/// cannot distinguish MM/DD from DD/MM. The ingestion layer can separately
/// resolve those values through `parse_date_value_with_order` when unambiguous
/// dates in the same column establish one consistent convention.
pub fn parse_named_month_date(s: &str) -> Option<String> {
    const MONTHS: &[(&str, u32)] = &[
        ("jan", 1),
        ("january", 1),
        ("feb", 2),
        ("february", 2),
        ("mar", 3),
        ("march", 3),
        ("apr", 4),
        ("april", 4),
        ("may", 5),
        ("jun", 6),
        ("june", 6),
        ("jul", 7),
        ("july", 7),
        ("aug", 8),
        ("august", 8),
        ("sep", 9),
        ("sept", 9),
        ("september", 9),
        ("oct", 10),
        ("october", 10),
        ("nov", 11),
        ("november", 11),
        ("dec", 12),
        ("december", 12),
    ];
    let cleaned: String = s
        .chars()
        .map(|c| match c {
            ',' | '-' | '/' => ' ',
            other => other,
        })
        .collect();
    let parts: Vec<&str> = cleaned.split_whitespace().collect();
    let [a, b, c] = parts[..] else {
        return None;
    };
    // At least one of the first two components must be a month name. This
    // keeps numeric locale-specific dates unresolved at this parser layer.
    let (year_str, month_str, day_str) = if a.len() == 4
        && a.bytes().all(|byte| byte.is_ascii_digit())
        && b.chars().next().is_some_and(|ch| ch.is_ascii_alphabetic())
    {
        (a, b, c)
    } else if a.chars().next().is_some_and(|ch| ch.is_ascii_alphabetic()) {
        (c, a, b)
    } else if b.chars().next().is_some_and(|ch| ch.is_ascii_alphabetic()) {
        (c, b, a)
    } else {
        return None;
    };
    let month = MONTHS
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(month_str))
        .map(|(_, m)| *m)?;
    let day: u32 = day_str
        .trim_end_matches(|c: char| c.is_ascii_alphabetic())
        .parse()
        .ok()?;
    let year: i32 = year_str.parse().ok()?;
    if !(1900..=2100).contains(&year) || !valid_day(year, month, day) {
        return None;
    }
    Some(format!("{year:04}-{month:02}-{day:02}"))
}

/// Locale order for numeric dates whose month and day are both at most 12.
/// This is inferred from unambiguous values in the same column, never from the
/// machine locale or a fixed regional default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumericDateOrder {
    MonthFirst,
    DayFirst,
}

/// Infer one consistent order from unambiguous year-last numeric dates. A
/// conflict deliberately returns `None`, leaving ambiguous values unresolved.
pub fn infer_numeric_date_order<'a>(
    values: impl IntoIterator<Item = &'a str>,
) -> Option<NumericDateOrder> {
    let mut inferred = None;
    for value in values {
        let Some(order) = numeric_date_order_hint(value) else {
            continue;
        };
        if inferred.is_some_and(|existing| existing != order) {
            return None;
        }
        inferred = Some(order);
    }
    inferred
}

/// Normalize a date using safe global formats first, then a same-column
/// numeric order when the input itself is ambiguous. Unambiguous slash dates
/// (for example, `07/25/2024`) are parsed without a column hint.
pub fn parse_date_value_with_order(
    s: &str,
    column_order: Option<NumericDateOrder>,
) -> Option<String> {
    parse_date_value(s).or_else(|| {
        let date_text = date_part(s.trim());
        let order = numeric_date_order_hint(date_text).or(column_order)?;
        parse_numeric_date(date_text, order)
    })
}

/// Normalize year-first ISO/slash forms, named-month forms, and numeric dates
/// whose order is unambiguous from the date itself. Ambiguous `MM/DD/YYYY` /
/// `DD/MM/YYYY` values remain unresolved here; ingestion can call
/// `parse_date_value_with_order` after examining the whole column.
pub fn parse_date_value(s: &str) -> Option<String> {
    let trimmed = s.trim();
    let date_text = date_part(trimmed);
    let parts: Vec<&str> = date_text.split(['-', '/']).collect();
    if parts.len() == 2 && parts[0].len() == 4 && parts[0].bytes().all(|b| b.is_ascii_digit()) {
        let year = parts[0].parse::<i32>().ok()?;
        let month = parts[1].parse::<u32>().ok()?;
        if (1900..=2100).contains(&year) && (1..=12).contains(&month) {
            return Some(format!("{year:04}-{month:02}"));
        }
    }
    if parts.len() == 3 && parts[0].len() == 4 && parts[0].bytes().all(|b| b.is_ascii_digit()) {
        let year = parts[0].parse::<i32>().ok()?;
        let month = parts[1].parse::<u32>().ok()?;
        let day = parts[2].parse::<u32>().ok()?;
        if (1900..=2100).contains(&year) && valid_day(year, month, day) {
            return Some(format!("{year:04}-{month:02}-{day:02}"));
        }
    }
    parse_named_month_date(date_text).or_else(|| {
        let order = numeric_date_order_hint(date_text)?;
        parse_numeric_date(date_text, order)
    })
}

fn date_part(value: &str) -> &str {
    let bytes = value.as_bytes();
    if bytes.len() > 10
        && bytes.get(..10).is_some_and(|part| {
            part.iter()
                .all(|b| b.is_ascii_digit() || *b == b'/' || *b == b'-')
        })
        && bytes
            .get(10)
            .is_some_and(|b| *b == b'T' || *b == b't' || *b == b' ')
    {
        &value[..10]
    } else {
        value
    }
}

fn numeric_date_parts(value: &str) -> Option<(u32, u32, i32)> {
    let parts: Vec<&str> = date_part(value.trim()).split('/').collect();
    let [first, second, year] = parts.as_slice() else {
        return None;
    };
    if year.len() != 4 || !year.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let first = first.parse::<u32>().ok()?;
    let second = second.parse::<u32>().ok()?;
    let year = year.parse::<i32>().ok()?;
    (1900..=2100)
        .contains(&year)
        .then_some((first, second, year))
}

fn numeric_date_order_hint(value: &str) -> Option<NumericDateOrder> {
    let (first, second, year) = numeric_date_parts(value)?;
    if first <= 12 && second > 12 {
        valid_day(year, first, second).then_some(NumericDateOrder::MonthFirst)
    } else if first > 12 && second <= 12 {
        valid_day(year, second, first).then_some(NumericDateOrder::DayFirst)
    } else {
        None
    }
}

fn parse_numeric_date(value: &str, order: NumericDateOrder) -> Option<String> {
    let (first, second, year) = numeric_date_parts(value)?;
    let (month, day) = match order {
        NumericDateOrder::MonthFirst => (first, second),
        NumericDateOrder::DayFirst => (second, first),
    };
    if !valid_day(year, month, day) {
        return None;
    }
    Some(format!("{year:04}-{month:02}-{day:02}"))
}

fn valid_day(year: i32, month: u32, day: u32) -> bool {
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if year % 400 == 0 || (year % 4 == 0 && year % 100 != 0) => 29,
        2 => 28,
        _ => 0,
    };
    days > 0 && (1..=days).contains(&day)
}

/// Whether a trimmed, case-folded cell reads as a totals/summary-row label
/// ("Total", "Grand Total:", "Subtotal Q1", ...). Shared by the CSV and Excel
/// ingest's "drop the trailing summary row" check so the label vocabulary
/// can't drift apart between the two.
pub fn is_total_label(s: &str) -> bool {
    let l = s.trim().to_ascii_lowercase();
    let l = l.trim_end_matches([':', '.']).trim();
    matches!(
        l,
        "total" | "totals" | "sum" | "grand total" | "subtotal" | "sub total"
    ) || l.starts_with("total ")
        || l.starts_with("grand total ")
        || l.starts_with("subtotal ")
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
        assert_eq!(parse_numeric("USD 1,200.00"), Some(1200.0));
        assert_eq!(parse_numeric("USD -129.18"), Some(-129.18));
        assert_eq!(parse_numeric("$-1,531.97"), Some(-1531.97));
        assert_eq!(parse_numeric("eur 42.50"), Some(42.5));
        assert_eq!(parse_numeric("  1,150 "), Some(1150.0));
        assert_eq!(parse_numeric("\u{00A3}2,000"), Some(2000.0));
        assert_eq!(parse_numeric("(45)"), Some(-45.0));
        assert_eq!(parse_numeric("($1,000.00)"), Some(-1000.0));
        assert_eq!(parse_numeric("-1,000"), Some(-1000.0));
        assert_eq!(parse_numeric("12%"), Some(0.12));
        assert_eq!(parse_numeric("1 200"), Some(1200.0));
        assert_eq!(parse_numeric("12,345"), Some(12345.0));
        assert_eq!(parse_numeric("$210.00 (cash)"), Some(210.0));
        assert_eq!(parse_numeric("1,300.00 (paid)"), Some(1300.0));
        assert_eq!(parse_numeric("1,300.00 (fee $1.95)"), None);
        assert_eq!(parse_numeric("N/A"), None);
        assert_eq!(parse_numeric("pending"), None);
        assert_eq!(parse_numeric(""), None);
        assert_eq!(parse_numeric("-"), None);
        assert_eq!(parse_numeric("1,2,3 apples"), None);
        assert_eq!(parse_numeric("approx 1200"), None);
        // Non-finite and malformed grouping must not sneak through as numbers.
        assert_eq!(parse_numeric("inf"), None);
        assert_eq!(parse_numeric("nan"), None);
        assert_eq!(parse_numeric("1 2 3"), None);
        assert_eq!(parse_numeric("1,23,456"), None);
        assert_eq!(parse_numeric("."), None);
    }

    #[test]
    fn parses_hyphenated_named_month_dates_without_guessing_numeric_dates() {
        assert_eq!(
            parse_named_month_date("31-May-2024").as_deref(),
            Some("2024-05-31")
        );
        assert_eq!(
            parse_named_month_date("2024-May-31").as_deref(),
            Some("2024-05-31")
        );
        assert_eq!(parse_named_month_date("08/01/2026"), None);
    }

    #[test]
    fn infers_ambiguous_numeric_date_order_from_unambiguous_column_values() {
        let values = ["03/04/2024", "4 Mar 2024", "07/25/2024", "06/03/2024"];
        let order = infer_numeric_date_order(values).expect("07/25 establishes month-first");
        assert_eq!(order, NumericDateOrder::MonthFirst);
        assert_eq!(
            parse_date_value_with_order("03/04/2024", Some(order)).as_deref(),
            Some("2024-03-04")
        );
        assert_eq!(
            parse_date_value_with_order("06/03/2024", Some(order)).as_deref(),
            Some("2024-06-03")
        );
        assert_eq!(
            parse_date_value("07/25/2024").as_deref(),
            Some("2024-07-25"),
            "an unambiguous slash date needs no inferred convention"
        );
    }

    #[test]
    fn conflicting_numeric_date_orders_leave_ambiguous_values_unresolved() {
        let values = ["13/04/2024", "04/13/2024", "03/04/2024"];
        assert_eq!(infer_numeric_date_order(values), None);
        assert_eq!(
            parse_date_value_with_order("13/04/2024", None).as_deref(),
            Some("2024-04-13"),
            "individually unambiguous values remain usable"
        );
        assert_eq!(parse_date_value_with_order("03/04/2024", None), None);
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

    #[test]
    fn read_only_guard_allows_replace_function_but_rejects_replace_into() {
        // REPLACE(str, from, to) is a read-only string function the standard
        // way to strip currency formatting (commas, `$`) before CAST/SUM and
        // must not be caught by the mutating-statement ban.
        assert!(
            ensure_read_only("SELECT SUM(CAST(REPLACE(amount, ',', '') AS REAL)) FROM t").is_ok()
        );
        // The mutating `REPLACE INTO ...` statement is still rejected because
        // it fails the "must start with SELECT/WITH" check, not the token ban.
        assert!(ensure_read_only("REPLACE INTO t VALUES (1)").is_err());
    }

    #[test]
    fn read_only_guard_treats_quoted_human_labels_as_data() {
        assert!(ensure_read_only(
            "SELECT SUM(amount) FROM transactions WHERE description LIKE '%current export%'"
        )
        .is_ok());
        assert!(ensure_read_only(
            "SELECT * FROM transactions WHERE note = 'drop table later; keep this row'"
        )
        .is_ok());
    }
}
