//! The default data engine: a file-backed SQLite database. Files are sniffed
//! for column types and imported as real tables. Read-only queries run on a
//! fresh `SQLITE_OPEN_READ_ONLY` connection.

use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags};
use serde_json::Value as Json;

use crate::engine::catalog::{ColumnInfo, SourceKind};
use crate::engine::data::{quote_ident, Cell, ColType, DataEngine, PythonBridge, QueryOutcome, SourceLoad};
use crate::engine::error::{EngineError, EngineResult};

pub struct SqliteEngine {
    conn: Connection,
    path: PathBuf,
}

impl SqliteEngine {
    pub fn open(data_dir: &Path) -> EngineResult<Self> {
        let path = data_dir.join("analysis.db");
        // Fresh each app start this DB is a scratch cache, not app state.
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("db-wal"));
        let _ = std::fs::remove_file(path.with_extension("db-shm"));
        let conn = Connection::open(&path)
            .map_err(|e| EngineError::msg(format!("open analysis.db: {e}")))?;
        conn.execute_batch("PRAGMA journal_mode = WAL;")?;
        Ok(Self { conn, path })
    }

    fn ro(&self) -> EngineResult<Connection> {
        Connection::open_with_flags(
            &self.path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
        )
        .map_err(|e| EngineError::msg(format!("open read-only: {e}")))
    }
}

impl DataEngine for SqliteEngine {
    fn add_source(&mut self, name: &str, kind: SourceKind, path: &str) -> EngineResult<SourceLoad> {
        let parsed = match kind {
            SourceKind::Csv => read_delimited(path, b',')?,
            SourceKind::Tsv => read_delimited(path, b'\t')?,
            SourceKind::Json => read_json(path, false)?,
            SourceKind::Ndjson => read_json(path, true)?,
            SourceKind::Parquet => {
                return Err(EngineError::msg(
                    "Parquet needs the DuckDB build rebuild with `cargo build --features duckdb`",
                ))
            }
            _ => return Err(EngineError::msg("not a path-readable tabular source")),
        };
        let Parsed { headers, types, notes, rows, note } = parsed;
        let cols: Vec<(String, ColType)> = headers.into_iter().zip(types).collect();
        let n = self.add_rows(name, &cols, &rows)?;
        Ok(SourceLoad {
            row_count: n,
            note,
            columns: cols
                .iter()
                .zip(notes)
                .map(|((nm, t), cnote)| {
                    let mut c = ColumnInfo::bare(nm.clone(), t.sqlite());
                    c.note = cnote;
                    c
                })
                .collect(),
        })
    }

    fn add_rows(
        &mut self,
        name: &str,
        columns: &[(String, ColType)],
        rows: &[Vec<Cell>],
    ) -> EngineResult<i64> {
        let ident = quote_ident(name);
        let cols_sql = columns
            .iter()
            .map(|(c, t)| format!("{} {}", quote_ident(c), t.sqlite()))
            .collect::<Vec<_>>()
            .join(", ");
        self.conn.execute_batch(&format!(
            "DROP VIEW IF EXISTS {ident}; DROP TABLE IF EXISTS {ident};
             CREATE TABLE {ident} ({cols_sql});"
        ))?;

        let placeholders = vec!["?"; columns.len()].join(", ");
        let tx = self.conn.transaction()?;
        {
            let mut stmt =
                tx.prepare(&format!("INSERT INTO {ident} VALUES ({placeholders})"))?;
            for row in rows {
                let vals: Vec<rusqlite::types::Value> = (0..columns.len())
                    .map(|i| cell_to_sqlite(row.get(i).unwrap_or(&Json::Null), columns[i].1))
                    .collect();
                stmt.execute(rusqlite::params_from_iter(vals.iter()))?;
            }
        }
        tx.commit()?;
        Ok(rows.len() as i64)
    }

    fn drop_source(&mut self, name: &str) {
        let ident = quote_ident(name);
        let _ = self
            .conn
            .execute_batch(&format!("DROP VIEW IF EXISTS {ident}; DROP TABLE IF EXISTS {ident};"));
    }

    fn describe(&self, name: &str) -> EngineResult<Vec<ColumnInfo>> {
        let ro = self.ro()?;
        let mut cols: Vec<(String, String)> = Vec::new();
        {
            let mut stmt = ro.prepare(&format!("PRAGMA table_info({})", quote_ident(name)))?;
            let rows = stmt.query_map([], |r| {
                Ok((r.get::<_, String>(1)?, r.get::<_, String>(2)?))
            })?;
            for r in rows {
                cols.push(r?);
            }
        }
        if cols.is_empty() {
            return Err(EngineError::UnknownSource(name.to_string()));
        }

        let total: i64 = ro
            .query_row(&format!("SELECT count(*) FROM {}", quote_ident(name)), [], |r| {
                r.get(0)
            })
            .unwrap_or(0);

        let mut out = Vec::new();
        for (col, ty) in cols {
            let q = quote_ident(&col);
            let t = quote_ident(name);
            let (non_null, distinct, min, max): (i64, i64, Option<String>, Option<String>) = ro
                .query_row(
                    &format!(
                        "SELECT count({q}), count(DISTINCT {q}),
                                CAST(min({q}) AS TEXT), CAST(max({q}) AS TEXT) FROM {t}"
                    ),
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
                )
                .unwrap_or((0, 0, None, None));
            out.push(ColumnInfo {
                name: col,
                type_: if ty.is_empty() { "TEXT".into() } else { ty },
                null_fraction: if total > 0 {
                    Some(((total - non_null) as f64 / total as f64 * 1e6).round() / 1e6)
                } else {
                    None
                },
                distinct: Some(distinct),
                min,
                max,
                example: None,
                note: None,
            });
        }
        Ok(out)
    }

    fn query(&self, sql: &str, max_rows: usize) -> EngineResult<QueryOutcome> {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;

        let ro = self.ro()?;

        // Watchdog: interrupt the statement after QUERY_TIMEOUT_SECS. The handle
        // is Send + Sync; `interrupt()` makes an in-flight step return SQLITE_INTERRUPT.
        let handle = ro.get_interrupt_handle();
        let done = Arc::new(AtomicBool::new(false));
        let timed_out = Arc::new(AtomicBool::new(false));
        let watchdog = {
            let done = done.clone();
            let timed_out = timed_out.clone();
            std::thread::spawn(move || {
                let secs = crate::engine::data::query_timeout_secs();
                let deadline =
                    std::time::Instant::now() + std::time::Duration::from_secs(secs);
                while std::time::Instant::now() < deadline {
                    if done.load(Ordering::Relaxed) {
                        return;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
                if !done.load(Ordering::Relaxed) {
                    timed_out.store(true, Ordering::Relaxed);
                    handle.interrupt();
                }
            })
        };

        let result = (|| -> EngineResult<QueryOutcome> {
            let mut stmt = ro.prepare(sql)?;
            let columns: Vec<String> =
                stmt.column_names().iter().map(|s| s.to_string()).collect();
            let ncol = columns.len();

            let mut rows_out: Vec<Vec<Json>> = Vec::new();
            let mut total = 0usize;
            let mut truncated = false;

            let mut rows = stmt.query([])?;
            while let Some(row) = rows.next()? {
                total += 1;
                if rows_out.len() >= max_rows {
                    truncated = true;
                    continue;
                }
                let mut cells = Vec::with_capacity(ncol);
                for i in 0..ncol {
                    cells.push(valueref_to_json(row.get_ref(i)?));
                }
                rows_out.push(cells);
            }
            Ok(QueryOutcome { columns, rows: rows_out, row_count: total, truncated })
        })();

        done.store(true, Ordering::Relaxed);
        let _ = watchdog.join();

        if timed_out.load(Ordering::Relaxed) {
            return Err(EngineError::msg(format!(
                "query stopped after {} s try narrowing it (add a WHERE or LIMIT)",
                crate::engine::data::query_timeout_secs()
            )));
        }
        result
    }

    fn python_bridge(&self) -> PythonBridge {
        PythonBridge::SqliteFile(self.path.clone())
    }
}

// --- file readers ---------------------------------------------------------

/// A parsed tabular file: column headers, sniffed types, an optional per-column
/// ingest note (e.g. "amounts stored as text and read as numbers"), and rows.
struct Parsed {
    headers: Vec<String>,
    types: Vec<ColType>,
    notes: Vec<Option<String>>,
    rows: Vec<Vec<Cell>>,
    /// Whole-file caveat (delimiter guessed, rows dropped for a decode error).
    note: Option<String>,
}

/// Pick the delimiter from the first line: the candidate that appears most,
/// counting only outside double quotes. Falls back to `default_delim`.
fn sniff_delimiter(path: &str, default_delim: u8) -> u8 {
    use std::io::BufRead;
    let Ok(file) = std::fs::File::open(path) else { return default_delim };
    let mut first = String::new();
    if std::io::BufReader::new(file).read_line(&mut first).is_err() {
        return default_delim;
    }
    let first = first.trim_start_matches('\u{feff}');
    let mut counts = [0usize; 4]; // , ; \t |
    let mut in_quotes = false;
    for b in first.bytes() {
        match b {
            b'"' => in_quotes = !in_quotes,
            _ if in_quotes => {}
            b',' => counts[0] += 1,
            b';' => counts[1] += 1,
            b'\t' => counts[2] += 1,
            b'|' => counts[3] += 1,
            _ => {}
        }
    }
    let cands = *b",;\t|";
    match counts.iter().enumerate().max_by_key(|(_, &n)| n) {
        Some((i, &n)) if n > 0 => cands[i],
        _ => default_delim,
    }
}

fn read_delimited(path: &str, default_delim: u8) -> EngineResult<Parsed> {
    let mut note: Option<String> = None;
    let delim = sniff_delimiter(path, default_delim);
    if delim != default_delim {
        note = Some(format!(
            "columns are separated by '{}', not ','",
            if delim == b'\t' { "tab".to_string() } else { (delim as char).to_string() }
        ));
    }

    // Read every row (row 0 included) so we can decide whether it's really a
    // header or just the first data row, and skip any report preamble above it.
    let mut rdr = csv::ReaderBuilder::new()
        .delimiter(delim)
        .flexible(true)
        .has_headers(false)
        .from_path(path)
        .map_err(|e| EngineError::msg(format!("read {path}: {e}")))?;

    let cap = crate::engine::data::ingest_row_cap();
    let mut dropped = 0usize;
    let mut truncated = false;
    let mut records: Vec<csv::StringRecord> = Vec::new();
    for r in rdr.records() {
        match r {
            Ok(rec) => records.push(rec),
            Err(_) => dropped += 1,
        }
        if records.len() >= cap {
            truncated = true;
            break;
        }
    }
    if dropped > 0 {
        note = merge_note(
            note,
            format!("{dropped} row(s) had characters Fella couldn't read (not UTF-8) and were skipped"),
        );
    }
    if truncated {
        note = merge_note(note, format!("only the first {cap} rows were loaded (the file is larger)"));
    }

    // Strip a leading BOM from the very first cell (Excel "CSV UTF-8").
    if let Some(first) = records.first_mut() {
        if first.get(0).is_some_and(|c| c.starts_with('\u{feff}')) {
            let mut fixed: Vec<String> = first.iter().map(|s| s.to_string()).collect();
            fixed[0] = fixed[0].trim_start_matches('\u{feff}').to_string();
            *first = csv::StringRecord::from(fixed);
        }
    }

    if records.is_empty() {
        return Ok(Parsed { headers: vec![], types: vec![], notes: vec![], rows: vec![], note });
    }

    let width = records.iter().map(|r| r.len()).max().unwrap_or(0);

    // Header detection: skip any preamble, then take the first header-shaped row.
    let (headers, data_start) = find_header(&records, width);
    if data_start > 1 {
        note = merge_note(note, format!("{} row(s) above the header were skipped", data_start - 1));
    } else if data_start == 0 {
        note = merge_note(note, "no header row was found; columns are named col1, col2, …".to_string());
    }

    let mut data: Vec<&csv::StringRecord> = records[data_start..].iter().collect();

    // Drop a trailing "Total" / "Subtotal" / "Grand total" summary line it
    // isn't data and would double-count in a SUM.
    if let Some(last) = data.last() {
        let cells: Vec<&str> = last.iter().collect();
        if looks_like_total_row(&cells, width) {
            data.pop();
            note = merge_note(note, "a trailing total row was left out of the table".to_string());
        }
    }

    // sniff types per column over the data rows only
    let mut types = vec![ColType::Text; width];
    let mut notes = vec![None; width];
    for i in 0..width {
        let (ty, cnote) = sniff_strings(data.iter().map(|r| r.get(i).unwrap_or("")));
        types[i] = ty;
        notes[i] = cnote;
    }

    let rows: Vec<Vec<Cell>> = data
        .iter()
        .map(|r| (0..width).map(|i| string_cell(r.get(i).unwrap_or(""), types[i])).collect())
        .collect();

    Ok(Parsed { headers, types, notes, rows, note })
}

/// Append `add` to an optional running note, semicolon-separated.
fn merge_note(cur: Option<String>, add: String) -> Option<String> {
    Some(match cur {
        Some(n) => format!("{n}; {add}"),
        None => add,
    })
}

/// Is this row shaped like a header: every cell non-empty after trimming, and
/// none of them a bare number (`amount`, not `1200`).
fn is_header_shaped(row: &[&str]) -> bool {
    let mut any = false;
    for c in row {
        let t = c.trim();
        if t.is_empty() {
            return false;
        }
        any = true;
        if t.parse::<f64>().is_ok() {
            return false;
        }
    }
    any
}

/// Skip a report preamble (title line, blank spacer) and return the header
/// names plus the index of the first data row. `data_start == 0` means no
/// header row was found and names are synthesised (`col1`, `col2`, …).
fn find_header(records: &[csv::StringRecord], width: usize) -> (Vec<String>, usize) {
    let scan = records.len().min(15);
    for i in 0..scan {
        if i + 1 >= records.len() {
            break;
        }
        let cells: Vec<&str> = records[i].iter().collect();
        let filled = cells.iter().filter(|c| !c.trim().is_empty()).count();
        if filled == 0 || filled * 2 < width || !is_header_shaped(&cells) {
            continue;
        }
        let raw: Vec<String> = (0..width)
            .map(|j| cells.get(j).map(|s| s.trim().to_string()).unwrap_or_default())
            .collect();
        return (dedupe_headers(&raw), i + 1);
    }
    ((0..width).map(|i| format!("col{}", i + 1)).collect(), 0)
}

/// A trailing summary line ("Total", "Subtotal", "Grand total 2024") rather
/// than a data row: carries a total-ish label and is mostly empty.
fn looks_like_total_row(row: &[&str], width: usize) -> bool {
    let filled = row.iter().filter(|c| !c.trim().is_empty()).count();
    let has_label = row.iter().any(|c| {
        let t = c.trim().to_ascii_lowercase();
        let t = t.trim_end_matches([':', '.']).trim();
        matches!(t, "total" | "totals" | "sum" | "grand total" | "subtotal" | "sub total")
            || t.starts_with("total ")
            || t.starts_with("grand total ")
            || t.starts_with("subtotal ")
    });
    has_label && filled * 3 <= width * 2 + 2
}

fn read_json(path: &str, ndjson: bool) -> EngineResult<Parsed> {
    let text = std::fs::read_to_string(path).map_err(|e| EngineError::io(format!("read {path}"), e))?;

    let objs: Vec<serde_json::Map<String, Json>> = if ndjson {
        text.lines()
            .filter(|l| !l.trim().is_empty())
            .filter_map(|l| serde_json::from_str::<Json>(l).ok())
            .filter_map(|v| v.as_object().cloned())
            .collect()
    } else {
        match serde_json::from_str::<Json>(&text)
            .map_err(|e| EngineError::msg(format!("{path}: {e}")))?
        {
            Json::Array(a) => a.into_iter().filter_map(|v| v.as_object().cloned()).collect(),
            Json::Object(o) => vec![o],
            _ => return Err(EngineError::msg(format!("{path}: expected a JSON array of objects"))),
        }
    };
    if objs.is_empty() {
        return Err(EngineError::msg(format!("{path}: no JSON objects found")));
    }

    // column order = first-seen across all objects
    let mut headers: Vec<String> = Vec::new();
    for o in &objs {
        for k in o.keys() {
            if !headers.contains(k) {
                headers.push(k.clone());
            }
        }
    }
    let headers = dedupe_headers(&headers);

    let mut types = Vec::with_capacity(headers.len());
    let mut notes = Vec::with_capacity(headers.len());
    for h in &headers {
        let (ty, note) = sniff_json(objs.iter().map(|o| o.get(h).unwrap_or(&Json::Null)));
        types.push(ty);
        notes.push(note);
    }

    let rows: Vec<Vec<Cell>> = objs
        .iter()
        .map(|o| {
            headers
                .iter()
                .zip(&types)
                .map(|(h, t)| json_cell(o.get(h).unwrap_or(&Json::Null), *t))
                .collect()
        })
        .collect();

    Ok(Parsed { headers, types, notes, rows, note: None })
}

fn dedupe_headers(raw: &[String]) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    raw.iter()
        .enumerate()
        .map(|(i, h)| {
            let base = if h.is_empty() { format!("col{}", i + 1) } else { h.clone() };
            let mut name = base.clone();
            let mut n = 2;
            while !seen.insert(name.clone()) {
                name = format!("{base}_{n}");
                n += 1;
            }
            name
        })
        .collect()
}

// --- type sniffing ------------------------------------------------------

/// Sniff a column's type from its string cells. Blank-ish tokens (`""`, `N/A`,
/// `-`) are ignored. If a column isn't cleanly numeric but every remaining cell
/// can still be read as a written number (`$1,200`, `1,150`, `12%`), it becomes
/// `Float` with a note so the caller can surface the coercion.
fn sniff_strings<'a>(cells: impl Iterator<Item = &'a str>) -> (ColType, Option<String>) {
    use crate::engine::data::{is_blankish, parse_numeric};

    let (mut any, mut int, mut float, mut boolean) = (false, true, true, true);
    let (mut loose_ok, mut loose_used) = (true, false);
    let (mut n_nonblank, mut n_numeric) = (0usize, 0usize);

    for c in cells {
        let c = c.trim();
        if is_blankish(c) {
            continue;
        }
        any = true;
        n_nonblank += 1;
        let is_int = c.parse::<i64>().is_ok();
        let is_float = c.parse::<f64>().is_ok();
        if !is_int {
            int = false;
        }
        if !is_float {
            float = false;
        }
        let lc = c.to_ascii_lowercase();
        if lc != "true" && lc != "false" {
            boolean = false;
        }
        if is_float {
            n_numeric += 1;
        } else if parse_numeric(c).is_some() {
            n_numeric += 1;
            loose_used = true;
        } else {
            loose_ok = false;
        }
    }

    if !any {
        return (ColType::Text, None);
    }
    if boolean {
        return (ColType::Bool, None);
    }
    if int {
        return (ColType::Int, None);
    }
    if float {
        return (ColType::Float, None);
    }
    if loose_ok && loose_used {
        return (
            ColType::Float,
            Some("amounts were stored as text (currency, commas, percent) and read as numbers".into()),
        );
    }
    if n_nonblank >= 3 && n_numeric * 100 >= n_nonblank * 60 {
        return (
            ColType::Text,
            Some("looks numeric but has non-number values; kept as text - CAST it for a total".into()),
        );
    }
    (ColType::Text, None)
}

fn sniff_json<'a>(vals: impl Iterator<Item = &'a Json>) -> (ColType, Option<String>) {
    use crate::engine::data::{is_blankish, parse_numeric};

    let (mut any, mut int, mut float, mut boolean) = (false, true, true, true);
    let (mut saw_str, mut all_str_numeric) = (false, true);
    for v in vals {
        match v {
            Json::Null => {}
            Json::String(s) if is_blankish(s) => {}
            Json::Bool(_) => {
                any = true;
                int = false;
                float = false;
            }
            Json::Number(n) => {
                any = true;
                boolean = false;
                if !n.is_i64() && !n.is_u64() {
                    int = false;
                }
            }
            Json::String(s) => {
                any = true;
                int = false;
                float = false;
                boolean = false;
                saw_str = true;
                if parse_numeric(s).is_none() {
                    all_str_numeric = false;
                }
            }
            _ => {
                any = true;
                int = false;
                float = false;
                boolean = false;
                all_str_numeric = false;
            }
        }
    }
    if !any {
        return (ColType::Text, None);
    }
    if boolean {
        return (ColType::Bool, None);
    }
    if int {
        return (ColType::Int, None);
    }
    if float {
        return (ColType::Float, None);
    }
    if saw_str && all_str_numeric {
        return (
            ColType::Float,
            Some("amounts were stored as text and read as numbers".into()),
        );
    }
    (ColType::Text, None)
}

fn string_cell(s: &str, ty: ColType) -> Cell {
    let s = s.trim();
    match ty {
        // In a genuine text column a placeholder like "none" / "-" / "N/A" is a
        // real value the user can see and group by; only a truly empty cell is
        // NULL. The numeric/bool arms below still treat blank-ish tokens as
        // missing data.
        ColType::Text => {
            if s.is_empty() {
                Json::Null
            } else {
                Json::from(s.to_string())
            }
        }
        _ if crate::engine::data::is_blankish(s) => Json::Null,
        ColType::Int => s.parse::<i64>().map(Json::from).unwrap_or(Json::Null),
        // A `Float` column may have been chosen by loose coercion, so fall back
        // to `parse_numeric` for cells the strict parse rejects ("$1,200.00").
        ColType::Float => s
            .parse::<f64>()
            .ok()
            .or_else(|| crate::engine::data::parse_numeric(s))
            .and_then(serde_json::Number::from_f64)
            .map(Json::Number)
            .unwrap_or(Json::Null),
        ColType::Bool => match s.to_ascii_lowercase().as_str() {
            "true" => Json::from(1),
            "false" => Json::from(0),
            _ => Json::Null,
        },
    }
}

fn json_cell(v: &Json, ty: ColType) -> Cell {
    use crate::engine::data::{is_blankish, parse_numeric};
    match (v, ty) {
        (Json::Null, _) => Json::Null,
        // Text column: keep a "none" / "-" placeholder verbatim, NULL only a
        // truly empty string. Numeric/bool columns still treat blank-ish as
        // missing (see the guard on the next arm).
        (Json::String(s), ColType::Text) => {
            if s.trim().is_empty() {
                Json::Null
            } else {
                Json::from(s.clone())
            }
        }
        (Json::String(s), _) if is_blankish(s) => Json::Null,
        (Json::Bool(b), ColType::Bool) => Json::from(*b as i64),
        (Json::Number(_), ColType::Int | ColType::Float) => v.clone(),
        (Json::String(s), ColType::Float) => parse_numeric(s)
            .and_then(serde_json::Number::from_f64)
            .map(Json::Number)
            .unwrap_or(Json::Null),
        (Json::String(s), ColType::Int) => {
            parse_numeric(s).map(|f| Json::from(f as i64)).unwrap_or(Json::Null)
        }
        (Json::String(s), _) => Json::from(s.clone()),
        (other, ColType::Text) => Json::from(other.to_string()),
        _ => Json::Null,
    }
}

fn cell_to_sqlite(v: &Json, _ty: ColType) -> rusqlite::types::Value {
    use rusqlite::types::Value as V;
    match v {
        Json::Null => V::Null,
        Json::Bool(b) => V::Integer(*b as i64),
        Json::Number(n) => {
            if let Some(i) = n.as_i64() {
                V::Integer(i)
            } else {
                V::Real(n.as_f64().unwrap_or(0.0))
            }
        }
        Json::String(s) => V::Text(s.clone()),
        other => V::Text(other.to_string()),
    }
}

fn valueref_to_json(v: rusqlite::types::ValueRef<'_>) -> Json {
    use rusqlite::types::ValueRef as R;
    match v {
        R::Null => Json::Null,
        R::Integer(i) => Json::from(i),
        R::Real(f) => serde_json::Number::from_f64(f).map(Json::Number).unwrap_or(Json::Null),
        R::Text(t) => Json::from(String::from_utf8_lossy(t).into_owned()),
        R::Blob(b) => Json::from(format!("<{} bytes>", b.len())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sniffs_types() {
        assert_eq!(sniff_strings(["1", "2", "", "3"].into_iter()).0, ColType::Int);
        assert_eq!(sniff_strings(["1", "2.5", "3"].into_iter()).0, ColType::Float);
        assert_eq!(sniff_strings(["true", "false"].into_iter()).0, ColType::Bool);
        assert_eq!(sniff_strings(["a", "1", "b"].into_iter()).0, ColType::Text);
        assert_eq!(sniff_strings(["", ""].into_iter()).0, ColType::Text);
    }

    #[test]
    fn sniffs_currency_text_as_number() {
        let (ty, note) = sniff_strings(["$1,200.00", "1,150", "1200", "N/A"].into_iter());
        assert_eq!(ty, ColType::Float);
        assert!(note.is_some(), "coercion should be noted");
        // A genuine category column with a few stray numbers stays text.
        let (ty, _) = sniff_strings(["rent", "deposit", "rent", "rent", "500"].into_iter());
        assert_eq!(ty, ColType::Text);
    }

    #[test]
    fn string_cell_coerces_for_float_column() {
        assert_eq!(string_cell("$1,200.00", ColType::Float), Json::from(1200.0));
        assert_eq!(string_cell("N/A", ColType::Float), Json::Null);
        assert_eq!(string_cell("1200", ColType::Float), Json::from(1200.0));
    }

    #[test]
    fn text_column_keeps_placeholder_values() {
        // "none" / "-" are real, groupable values in a text column - not NULL.
        assert_eq!(string_cell("none", ColType::Text), Json::from("none"));
        assert_eq!(string_cell("-", ColType::Text), Json::from("-"));
        assert_eq!(string_cell("  ", ColType::Text), Json::Null);
        assert_eq!(
            json_cell(&Json::from("N/A"), ColType::Text),
            Json::from("N/A")
        );
        assert_eq!(json_cell(&Json::from(""), ColType::Text), Json::Null);
        // ...but they are still missing data in a numeric column.
        assert_eq!(json_cell(&Json::from("none"), ColType::Float), Json::Null);
    }
}
