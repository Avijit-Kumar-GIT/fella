//! The default data engine: a file-backed SQLite database. Files are sniffed
//! for column types and imported as real tables. Read-only queries run on a
//! fresh `SQLITE_OPEN_READ_ONLY` connection.

use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use rusqlite::functions::FunctionFlags;
use rusqlite::types::ValueRef;
use rusqlite::{Connection, OpenFlags};
use serde_json::Value as Json;

use crate::engine::analytics::data::{
    infer_numeric_date_order, is_blankish, parse_date_value, parse_date_value_with_order,
    parse_numeric, quote_ident, Cell, ColType, DataEngine, NumericDateOrder, PythonBridge,
    QueryOutcome, SourceLoad,
};
use crate::engine::catalog::{ColumnInfo, SourceKind};
use crate::engine::error::{EngineError, EngineResult};

#[cfg(test)]
use crate::engine::analytics::data::parse_named_month_date;

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
        let conn = Connection::open_with_flags(
            &self.path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
        )
        .map_err(|e| EngineError::msg(format!("open read-only: {e}")))?;
        register_parse_num(&conn)?;
        Ok(conn)
    }

    /// Profile a delimited source with constant row memory, then stream it into
    /// SQLite in a second pass. The two-pass design keeps full-file type/date
    /// inference (including late outliers) without retaining the whole file.
    fn add_delimited_source(
        &mut self,
        name: &str,
        path: &str,
        default_delim: u8,
    ) -> EngineResult<SourceLoad> {
        let stamp = file_stamp(path)?;
        let profile = inspect_delimited(path, default_delim)?;
        if file_stamp(path)? != stamp {
            return Err(EngineError::msg(format!(
                "{path}: the file changed while Fella was inspecting it; retry the mount"
            )));
        }

        if profile.headers.is_empty() {
            return Err(EngineError::msg(format!("{path}: no tabular rows found")));
        }
        let columns: Vec<(String, ColType)> = profile
            .headers
            .iter()
            .cloned()
            .zip(profile.types.iter().copied())
            .collect();
        let ident = quote_ident(name);
        let cols_sql = columns
            .iter()
            .map(|(column, ty)| format!("{} {}", quote_ident(column), ty.sqlite()))
            .collect::<Vec<_>>()
            .join(", ");
        let placeholders = vec!["?"; columns.len()].join(", ");
        let insert_sql = format!("INSERT INTO {ident} VALUES ({placeholders})");
        let tx = self.conn.transaction()?;
        tx.execute_batch(&format!(
            "DROP TABLE IF EXISTS {ident}; CREATE TABLE {ident} ({cols_sql});"
        ))?;
        let mut loaded = 0usize;
        let mut valid_records = 0usize;
        let mut dropped_records = 0usize;
        let mut case_collisions: Vec<CaseCollisionAccumulator> = (0..columns.len())
            .map(|_| CaseCollisionAccumulator::default())
            .collect();
        {
            let mut statement = tx.prepare(&insert_sql)?;
            let mut reader = delimited_reader(path, profile.delimiter)?;
            for result in reader.records() {
                let mut record = match result {
                    Ok(record) => record,
                    Err(_) => {
                        dropped_records += 1;
                        continue;
                    }
                };
                if valid_records == 0 {
                    strip_leading_bom(&mut record);
                }
                let row_index = valid_records;
                valid_records += 1;
                if row_index < profile.data_start
                    || (profile.trailing_total && row_index + 1 == profile.total_records)
                {
                    continue;
                }

                let mut values = Vec::with_capacity(columns.len());
                for (index, (_, ty)) in columns.iter().enumerate() {
                    let raw = record.get(index).unwrap_or("");
                    case_collisions[index].observe(raw);
                    values.push(string_cell_with_order(raw, *ty, profile.date_orders[index]));
                }
                let sqlite_values: Vec<rusqlite::types::Value> = values
                    .iter()
                    .map(|value| cell_to_sqlite(value, ColType::Text))
                    .collect();
                statement.execute(rusqlite::params_from_iter(sqlite_values.iter()))?;
                loaded += 1;
            }
        }

        if valid_records != profile.total_records || dropped_records != profile.dropped_records {
            return Err(EngineError::msg(format!(
                "{path}: the file changed while Fella was loading it; retry the mount"
            )));
        }
        if file_stamp(path)? != stamp {
            return Err(EngineError::msg(format!(
                "{path}: the file changed while Fella was loading it; retry the mount"
            )));
        }
        tx.commit()?;

        let mut notes = profile.notes;
        for (index, ((_, ty), collision)) in columns.iter().zip(case_collisions).enumerate() {
            if *ty == ColType::Text {
                if let Some(collision_note) = collision.finish() {
                    notes[index] = merge_note(notes[index].take(), collision_note);
                }
            }
        }

        Ok(SourceLoad {
            row_count: loaded as i64,
            note: profile.note,
            columns: columns
                .iter()
                .zip(notes)
                .map(|((column, ty), note)| {
                    let mut info = ColumnInfo::bare(column.clone(), ty.sqlite());
                    info.note = note;
                    info
                })
                .collect(),
        })
    }
}

/// `parse_num(x)`: read-only SQL escape hatch for a column ingest left as
/// TEXT because it's a mix of numbers and something else (see the
/// "looks numeric but is mixed" column note). A bare `CAST(x AS REAL)`
/// silently truncates at the first non-numeric character `CAST('1,200'
/// AS REAL)` is `1.0`, not an error, which is how a messy column turns into
/// a confidently wrong total. `parse_num` instead reuses the same tolerant
/// parser the ingest side already trusts (currency signs, thousands
/// separators, `%`, accounting negatives — see `parse_numeric`'s own doc),
/// and returns NULL for anything it can't make sense of, so `SUM`/`AVG`
/// quietly skip it instead of the query lying. Not specific to currency or
/// to any one ingest path deliberately general, so the model has one real
/// tool for "this column is mixed" instead of a bigger pile of heuristics.
fn register_parse_num(conn: &Connection) -> EngineResult<()> {
    conn.create_scalar_function(
        "parse_num",
        1,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        |ctx| {
            let parsed = match ctx.get_raw(0) {
                ValueRef::Null => None,
                ValueRef::Integer(i) => Some(i as f64),
                ValueRef::Real(r) => Some(r),
                ValueRef::Text(t) => std::str::from_utf8(t).ok().and_then(parse_numeric),
                ValueRef::Blob(_) => None,
            };
            Ok(parsed)
        },
    )?;
    Ok(())
}

impl DataEngine for SqliteEngine {
    fn add_source(&mut self, name: &str, kind: SourceKind, path: &str) -> EngineResult<SourceLoad> {
        match kind {
            SourceKind::Csv => return self.add_delimited_source(name, path, b','),
            SourceKind::Tsv => return self.add_delimited_source(name, path, b'\t'),
            _ => {}
        }
        let parsed =
            match kind {
                SourceKind::Json => read_json(path, false)?,
                SourceKind::Ndjson => read_json(path, true)?,
                SourceKind::Parquet => return Err(EngineError::msg(
                    "Parquet needs the DuckDB build rebuild with `cargo build --features duckdb`",
                )),
                _ => return Err(EngineError::msg("not a path-readable tabular source")),
            };
        let Parsed {
            headers,
            types,
            notes,
            rows,
            note,
        } = parsed;
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
        // Every source this engine loads is a TABLE, never a VIEW (nothing
        // here ever does CREATE VIEW). `DROP VIEW IF EXISTS` on a name that
        // exists as a table is not the no-op it looks like `IF EXISTS` only
        // suppresses "no such view", not a type mismatch, so SQLite raises
        // "use DROP TABLE to delete table X" the moment a second ingest of
        // the same source name (e.g. /reindex) runs this. Plain
        // `DROP TABLE IF EXISTS` is already correct and idempotent on its
        // own for the only kind of object this engine ever creates.
        self.conn.execute_batch(&format!(
            "DROP TABLE IF EXISTS {ident};
             CREATE TABLE {ident} ({cols_sql});"
        ))?;

        let placeholders = vec!["?"; columns.len()].join(", ");
        let tx = self.conn.transaction()?;
        {
            let mut stmt = tx.prepare(&format!("INSERT INTO {ident} VALUES ({placeholders})"))?;
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
        // See add_rows: every source is a TABLE, and `DROP VIEW IF EXISTS`
        // on a table name errors rather than no-opping, which used to make
        // this whole call fail (silently the Result is discarded) and skip
        // the DROP TABLE that follows it leaving the old table in place
        // for the next ingest's own DROP VIEW IF EXISTS to fail on for real.
        let ident = quote_ident(name);
        let _ = self
            .conn
            .execute_batch(&format!("DROP TABLE IF EXISTS {ident};"));
    }

    fn describe(&self, name: &str) -> EngineResult<Vec<ColumnInfo>> {
        let ro = self.ro()?;
        let mut cols: Vec<(String, String)> = Vec::new();
        {
            let mut stmt = ro.prepare(&format!("PRAGMA table_info({})", quote_ident(name)))?;
            let rows =
                stmt.query_map([], |r| Ok((r.get::<_, String>(1)?, r.get::<_, String>(2)?)))?;
            for r in rows {
                cols.push(r?);
            }
        }
        if cols.is_empty() {
            return Err(EngineError::UnknownSource(name.to_string()));
        }

        let total: i64 = ro
            .query_row(
                &format!("SELECT count(*) FROM {}", quote_ident(name)),
                [],
                |r| r.get(0),
            )
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
            let common_value_counts = common_value_counts(&ro, &t, &q, distinct);
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
                common_values: common_value_counts
                    .as_ref()
                    .map(|values| values.iter().map(|entry| entry.value.clone()).collect()),
                common_value_counts,
                note: None,
            });
        }
        Ok(out)
    }

    fn query(&self, sql: &str, max_rows: usize) -> EngineResult<QueryOutcome> {
        let ro = self.ro()?;
        query_connection(ro, sql, max_rows)
    }

    fn query_with_cancel(
        &self,
        sql: &str,
        max_rows: usize,
        cancel: Arc<AtomicBool>,
    ) -> EngineResult<QueryOutcome> {
        let ro = self.ro()?;
        query_connection_cancellable(ro, sql, max_rows, Some(cancel))
    }

    fn python_bridge(&self) -> PythonBridge {
        PythonBridge::SqliteFile(self.path.clone())
    }
}

/// Return a compact, bounded list of frequent values for low-cardinality text
/// columns. The query is only useful as a human/model hint, so high-cardinality
/// columns skip the extra work and long values are clipped before serialization.
fn common_value_counts(
    ro: &Connection,
    table: &str,
    column: &str,
    distinct: i64,
) -> Option<Vec<crate::engine::catalog::ValueFrequency>> {
    const DISTINCT_CAP: i64 = 32;
    const VALUE_CAP: usize = 80;
    if !(1..=DISTINCT_CAP).contains(&distinct) {
        return None;
    }
    let mut stmt = ro
        .prepare(&format!(
            "SELECT CAST({column} AS TEXT), count(*) FROM {table} \
             WHERE {column} IS NOT NULL GROUP BY {column} \
             ORDER BY count(*) DESC, CAST({column} AS TEXT) LIMIT 8"
        ))
        .ok()?;
    let values = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })
        .ok()?
        .filter_map(Result::ok)
        .map(|(value, count)| crate::engine::catalog::ValueFrequency {
            value: value.chars().take(VALUE_CAP).collect(),
            count,
        })
        .filter(|entry| !entry.value.is_empty())
        .collect::<Vec<_>>();
    (!values.is_empty()).then_some(values)
}

/// Query the workspace file from a separate read-only connection. The
/// embedded Python guest uses this instead of receiving the SQLite path, so
/// generated code never gets a filesystem capability of its own.
pub(crate) fn query_read_only(
    path: &Path,
    sql: &str,
    max_rows: usize,
) -> EngineResult<QueryOutcome> {
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
    )
    .map_err(|e| EngineError::msg(format!("open read-only: {e}")))?;
    register_parse_num(&conn)?;
    query_connection(conn, sql, max_rows)
}

pub(crate) fn query_read_only_cancellable(
    path: &Path,
    sql: &str,
    max_rows: usize,
    cancel: Arc<AtomicBool>,
) -> EngineResult<QueryOutcome> {
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
    )
    .map_err(|e| EngineError::msg(format!("open read-only: {e}")))?;
    register_parse_num(&conn)?;
    query_connection_cancellable(conn, sql, max_rows, Some(cancel))
}

fn query_connection(ro: Connection, sql: &str, max_rows: usize) -> EngineResult<QueryOutcome> {
    query_connection_cancellable(ro, sql, max_rows, None)
}

fn query_connection_cancellable(
    ro: Connection,
    sql: &str,
    max_rows: usize,
    cancel: Option<Arc<AtomicBool>>,
) -> EngineResult<QueryOutcome> {
    // Watchdog: interrupt the statement after QUERY_TIMEOUT_SECS. The handle
    // is Send + Sync; `interrupt()` makes an in-flight step return SQLITE_INTERRUPT.
    let handle = ro.get_interrupt_handle();
    let done = Arc::new(AtomicBool::new(false));
    let timed_out = Arc::new(AtomicBool::new(false));
    let cancelled = Arc::new(AtomicBool::new(false));
    let watchdog = {
        let done = done.clone();
        let timed_out = timed_out.clone();
        let cancelled = cancelled.clone();
        let cancel = cancel.clone();
        std::thread::spawn(move || {
            let secs = crate::engine::analytics::data::query_timeout_secs();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(secs);
            loop {
                if done.load(Ordering::Relaxed) {
                    return;
                }
                if cancel
                    .as_ref()
                    .is_some_and(|flag| flag.load(Ordering::Relaxed))
                {
                    cancelled.store(true, Ordering::Relaxed);
                    handle.interrupt();
                    return;
                }
                if std::time::Instant::now() >= deadline {
                    if !done.load(Ordering::Relaxed) {
                        timed_out.store(true, Ordering::Relaxed);
                        handle.interrupt();
                    }
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        })
    };

    let result = (|| -> EngineResult<QueryOutcome> {
        let mut stmt = ro.prepare(sql)?;
        let columns: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
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
        Ok(QueryOutcome {
            columns,
            rows: rows_out,
            row_count: total,
            truncated,
        })
    })();

    done.store(true, Ordering::Relaxed);
    let _ = watchdog.join();

    if cancelled.load(Ordering::Relaxed) {
        return Err(EngineError::msg("query stopped by user"));
    }
    if timed_out.load(Ordering::Relaxed) {
        return Err(EngineError::msg(format!(
            "query stopped after {} s try narrowing it (add a WHERE or LIMIT)",
            crate::engine::analytics::data::query_timeout_secs()
        )));
    }
    result
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct FileStamp {
    size: u64,
    modified: Option<std::time::SystemTime>,
}

fn file_stamp(path: &str) -> EngineResult<FileStamp> {
    let metadata = std::fs::metadata(path)
        .map_err(|error| EngineError::io(format!("inspect {path}"), error))?;
    Ok(FileStamp {
        size: metadata.len(),
        modified: metadata.modified().ok(),
    })
}

struct DelimitedProfile {
    delimiter: u8,
    headers: Vec<String>,
    types: Vec<ColType>,
    date_orders: Vec<Option<NumericDateOrder>>,
    notes: Vec<Option<String>>,
    note: Option<String>,
    data_start: usize,
    total_records: usize,
    dropped_records: usize,
    trailing_total: bool,
}

/// Reversible full-file type statistics. A small prefix and the last record
/// are removed after the full pass once header and trailing-total detection
/// are known, keeping classification identical to a complete in-memory scan.
#[derive(Default)]
struct StringColumnProfile {
    nonblank: usize,
    integers: usize,
    floats: usize,
    booleans: usize,
    loose_ok: usize,
    loose_used: usize,
    numeric: usize,
    dates_without_order: usize,
    dates_month_first: usize,
    dates_day_first: usize,
    month_first_hints: usize,
    day_first_hints: usize,
    date_examples: Vec<(usize, String)>,
}

impl StringColumnProfile {
    fn observe(&mut self, value: &str, row_index: usize) {
        self.update(value, true, row_index);
    }

    fn unobserve(&mut self, value: &str) {
        self.update(value, false, 0);
    }

    fn update(&mut self, value: &str, adding: bool, row_index: usize) {
        let value = value.trim();
        if is_blankish(value) {
            return;
        }

        let is_integer = value.parse::<i64>().is_ok();
        let is_float = value.parse::<f64>().is_ok();
        let parsed_number = parse_numeric(value);
        let is_boolean = matches!(value.to_ascii_lowercase().as_str(), "true" | "false");
        bump(&mut self.nonblank, adding);
        if is_integer {
            bump(&mut self.integers, adding);
        }
        if is_float {
            bump(&mut self.floats, adding);
        }
        if is_boolean {
            bump(&mut self.booleans, adding);
        }
        if is_float || parsed_number.is_some() {
            bump(&mut self.loose_ok, adding);
        }
        if !is_float && parsed_number.is_some() {
            bump(&mut self.loose_used, adding);
        }
        if is_float || parsed_number.is_some() {
            bump(&mut self.numeric, adding);
        }

        let direct_date = parse_date_value(value);
        let month_first_date = direct_date.clone().or_else(|| {
            value
                .contains('/')
                .then(|| parse_date_value_with_order(value, Some(NumericDateOrder::MonthFirst)))?
        });
        let day_first_date = direct_date.clone().or_else(|| {
            value
                .contains('/')
                .then(|| parse_date_value_with_order(value, Some(NumericDateOrder::DayFirst)))?
        });
        if direct_date.is_some() {
            bump(&mut self.dates_without_order, adding);
        }
        if month_first_date.is_some() {
            bump(&mut self.dates_month_first, adding);
        }
        if day_first_date.is_some() {
            bump(&mut self.dates_day_first, adding);
        }
        match infer_numeric_date_order(std::iter::once(value)) {
            Some(NumericDateOrder::MonthFirst) => bump(&mut self.month_first_hints, adding),
            Some(NumericDateOrder::DayFirst) => bump(&mut self.day_first_hints, adding),
            None => {}
        }

        if adding
            && (direct_date.is_some() || month_first_date.is_some() || day_first_date.is_some())
            && self.date_examples.len() < 32
        {
            self.date_examples
                .push((row_index, value.chars().take(128).collect::<String>()));
        }
    }

    fn infer_date_order(&self) -> Option<NumericDateOrder> {
        match (self.month_first_hints > 0, self.day_first_hints > 0) {
            (true, false) => Some(NumericDateOrder::MonthFirst),
            (false, true) => Some(NumericDateOrder::DayFirst),
            _ => None,
        }
    }

    fn finish(
        &self,
        data_start: usize,
        data_end: usize,
    ) -> (ColType, Option<String>, Option<NumericDateOrder>) {
        let date_order = self.infer_date_order();
        let dates = match date_order {
            Some(NumericDateOrder::MonthFirst) => self.dates_month_first,
            Some(NumericDateOrder::DayFirst) => self.dates_day_first,
            None => self.dates_without_order,
        };
        let date_example = self.date_examples.iter().find_map(|(row, raw)| {
            (*row >= data_start && *row < data_end)
                .then(|| parse_date_value_with_order(raw, date_order))
                .flatten()
                .map(|normalized| (raw.clone(), normalized))
        });

        if self.nonblank == 0 {
            return (ColType::Text, None, date_order);
        }
        if self.booleans == self.nonblank {
            return (ColType::Bool, None, date_order);
        }
        if self.integers == self.nonblank {
            return (ColType::Int, None, date_order);
        }
        if self.floats == self.nonblank {
            return (ColType::Float, None, date_order);
        }
        if self.loose_ok == self.nonblank && self.loose_used > 0 {
            return (
                ColType::Float,
                Some(
                    "amounts were stored as text (currency, commas, percent) and read as numbers"
                        .into(),
                ),
                date_order,
            );
        }
        if dates == self.nonblank && dates > 0 {
            let (raw, normalized) = date_example.unwrap_or_default();
            let order_note = numeric_date_order_note(date_order);
            return (
                ColType::Date,
                Some(format!(
                    "dates were stored as text (e.g. \"{raw}\") and normalized to ISO-8601 \
                     (e.g. {normalized}) for querying with strftime()/date(){order_note}"
                )),
                date_order,
            );
        }
        if self.nonblank >= 3
            && self.numeric >= 3
            && (self.numeric as u128) * 100 > (self.nonblank as u128) * 80
        {
            return (
                ColType::Float,
                Some(format!(
                    "{} of {} values were normalized as numbers; unparseable values were left NULL",
                    self.numeric, self.nonblank
                )),
                date_order,
            );
        }
        if self.nonblank >= 3 && dates >= 3 && (dates as u128) * 100 >= (self.nonblank as u128) * 80
        {
            let (raw, normalized) = date_example.unwrap_or_default();
            let order_note = numeric_date_order_note(date_order);
            return (
                ColType::Date,
                Some(format!(
                    "{dates} of {} values were normalized as dates (e.g. \"{raw}\" -> {normalized}); \
                     unparseable values were left NULL{order_note}",
                    self.nonblank
                )),
                date_order,
            );
        }
        if self.nonblank >= 3
            && self.numeric >= 3
            && (self.numeric as u128) * 100 >= (self.nonblank as u128) * 60
        {
            return (
                ColType::Text,
                Some(format!(
                    "{} of {} values parse as numbers; kept as text. Use \
                     parse_num(col) for a reliable total (CAST silently truncates text like \
                     \"1,200\" instead of erroring); COUNT(*) - COUNT(parse_num(col)) shows how \
                     many rows didn't parse",
                    self.numeric, self.nonblank
                )),
                date_order,
            );
        }
        (ColType::Text, None, date_order)
    }
}

fn bump(counter: &mut usize, adding: bool) {
    if adding {
        *counter += 1;
    } else {
        debug_assert!(*counter > 0, "profile counter underflow");
        *counter = counter.saturating_sub(1);
    }
}

fn delimited_reader(path: &str, delimiter: u8) -> EngineResult<csv::Reader<std::fs::File>> {
    csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .flexible(true)
        .has_headers(false)
        .from_path(path)
        .map_err(|error| EngineError::msg(format!("read {path}: {error}")))
}

fn strip_leading_bom(record: &mut csv::StringRecord) {
    if record
        .get(0)
        .is_some_and(|cell| cell.starts_with('\u{feff}'))
    {
        let mut fixed: Vec<String> = record.iter().map(str::to_string).collect();
        fixed[0] = fixed[0].trim_start_matches('\u{feff}').to_string();
        *record = csv::StringRecord::from(fixed);
    }
}

/// Pick the delimiter from the first line: the candidate that appears most,
/// counting only outside double quotes. Falls back to `default_delim`.
fn sniff_delimiter(path: &str, default_delim: u8) -> u8 {
    use std::io::BufRead;
    let Ok(file) = std::fs::File::open(path) else {
        return default_delim;
    };
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

fn inspect_delimited(path: &str, default_delim: u8) -> EngineResult<DelimitedProfile> {
    let mut note: Option<String> = None;
    let delimiter = sniff_delimiter(path, default_delim);
    if delimiter != default_delim {
        note = Some(format!(
            "columns are separated by '{}', not ','",
            if delimiter == b'\t' {
                "tab".to_string()
            } else {
                (delimiter as char).to_string()
            }
        ));
    }

    let mut reader = delimited_reader(path, delimiter)?;
    let mut prefix: Vec<csv::StringRecord> = Vec::with_capacity(16);
    let mut last: Option<csv::StringRecord> = None;
    let mut profiles: Vec<StringColumnProfile> = Vec::new();
    let mut width = 0usize;
    let mut total_records = 0usize;
    let mut dropped_records = 0usize;
    for result in reader.records() {
        match result {
            Ok(mut record) => {
                if total_records == 0 {
                    strip_leading_bom(&mut record);
                }
                if prefix.len() < 16 {
                    prefix.push(record.clone());
                }
                width = width.max(record.len());
                profiles.resize_with(width, StringColumnProfile::default);
                for (column, value) in record.iter().enumerate() {
                    profiles[column].observe(value, total_records);
                }
                last = Some(record);
                total_records += 1;
            }
            Err(_) => dropped_records += 1,
        }
    }

    if dropped_records > 0 {
        note = merge_note(
            note,
            format!(
                "{dropped_records} row(s) had characters Fella couldn't read (not UTF-8) and were skipped"
            ),
        );
    }
    if total_records == 0 {
        return Ok(DelimitedProfile {
            delimiter,
            headers: vec![],
            types: vec![],
            date_orders: vec![],
            notes: vec![],
            note,
            data_start: 0,
            total_records,
            dropped_records,
            trailing_total: false,
        });
    }

    // The old detector searched the first 15 records using the full-file width.
    // Sixteen retained records preserve the look-ahead required at index 14.
    let (headers, data_start) = find_header(&prefix, width);
    if data_start > 1 {
        note = merge_note(
            note,
            format!("{} row(s) above the header were skipped", data_start - 1),
        );
    } else if data_start == 0 {
        note = merge_note(
            note,
            "no header row was found; columns are named col1, col2, …".to_string(),
        );
    }

    let trailing_total = last.as_ref().is_some_and(|record| {
        total_records.saturating_sub(1) >= data_start
            && looks_like_total_row(&record.iter().collect::<Vec<_>>(), width)
    });
    if trailing_total {
        note = merge_note(
            note,
            "a trailing total row was left out of the table".to_string(),
        );
    }

    // Exclude header/preamble and an optional trailing total from the full-file
    // statistics without retaining the rest of the records.
    for (row_index, record) in prefix.iter().enumerate().take(data_start) {
        for (column, profile) in profiles.iter_mut().enumerate() {
            profile.unobserve(record.get(column).unwrap_or(""));
        }
        debug_assert!(row_index < data_start);
    }
    if trailing_total {
        if let Some(record) = &last {
            for (column, profile) in profiles.iter_mut().enumerate() {
                profile.unobserve(record.get(column).unwrap_or(""));
            }
        }
    }

    let data_end = total_records - usize::from(trailing_total);
    let finalized: Vec<(ColType, Option<String>, Option<NumericDateOrder>)> = profiles
        .iter()
        .map(|profile| profile.finish(data_start, data_end))
        .collect();
    let mut types = Vec::with_capacity(width);
    let mut notes = Vec::with_capacity(width);
    let mut date_orders = Vec::with_capacity(width);
    for (ty, column_note, date_order) in finalized {
        types.push(ty);
        notes.push(column_note);
        date_orders.push(date_order);
    }

    Ok(DelimitedProfile {
        delimiter,
        headers,
        types,
        date_orders,
        notes,
        note,
        data_start,
        total_records,
        dropped_records,
        trailing_total,
    })
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
            .map(|j| {
                cells
                    .get(j)
                    .map(|s| s.trim().to_string())
                    .unwrap_or_default()
            })
            .collect();
        return (dedupe_headers(&raw), i + 1);
    }
    ((0..width).map(|i| format!("col{}", i + 1)).collect(), 0)
}

/// A trailing summary line ("Total", "Subtotal", "Grand total 2024") rather
/// than a data row: carries a total-ish label and is mostly empty.
fn looks_like_total_row(row: &[&str], width: usize) -> bool {
    let filled = row.iter().filter(|c| !c.trim().is_empty()).count();
    let has_label = row
        .iter()
        .any(|c| crate::engine::analytics::data::is_total_label(c));
    has_label && filled * 3 <= width * 2 + 2
}

fn read_json(path: &str, ndjson: bool) -> EngineResult<Parsed> {
    let byte_cap = crate::engine::analytics::data::ingest_byte_cap();
    if std::fs::metadata(path)
        .map(|metadata| metadata.len() > byte_cap as u64)
        .unwrap_or(false)
    {
        return Err(EngineError::msg(format!(
            "{path}: JSON input is larger than the {byte_cap} byte ingest limit"
        )));
    }
    let text =
        std::fs::read_to_string(path).map_err(|e| EngineError::io(format!("read {path}"), e))?;

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
            Json::Array(a) => a
                .into_iter()
                .filter_map(|v| v.as_object().cloned())
                .collect(),
            Json::Object(o) => vec![o],
            _ => {
                return Err(EngineError::msg(format!(
                    "{path}: expected a JSON array of objects"
                )))
            }
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
    let mut date_orders = Vec::with_capacity(headers.len());
    for h in &headers {
        let values: Vec<&Json> = objs
            .iter()
            .map(|o| o.get(h).unwrap_or(&Json::Null))
            .collect();
        let date_order = infer_numeric_date_order(values.iter().filter_map(|v| v.as_str()));
        let (ty, note) = sniff_json_with_order(values.iter().copied(), date_order);
        types.push(ty);
        notes.push(note);
        date_orders.push(date_order);
    }

    let rows: Vec<Vec<Cell>> = objs
        .iter()
        .map(|o| {
            headers
                .iter()
                .zip(types.iter().zip(&date_orders))
                .map(|(h, (t, order))| {
                    json_cell_with_order(o.get(h).unwrap_or(&Json::Null), *t, *order)
                })
                .collect()
        })
        .collect();

    Ok(Parsed {
        headers,
        types,
        notes,
        rows,
        note: None,
    })
}

fn dedupe_headers(raw: &[String]) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    raw.iter()
        .enumerate()
        .map(|(i, h)| {
            let base = if h.is_empty() {
                format!("col{}", i + 1)
            } else {
                h.clone()
            };
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
/// `-`) are ignored. Mostly parseable numeric/date columns are normalized while
/// the small number of malformed cells become NULL and are called out in the
/// column note. This keeps one bad export cell from making an entire measure or
/// time series unreadable without pretending the bad cell was valid.
#[cfg(test)]
fn sniff_strings<'a>(cells: impl Iterator<Item = &'a str>) -> (ColType, Option<String>) {
    let cells: Vec<&str> = cells.collect();
    let date_order = infer_numeric_date_order(cells.iter().copied());
    sniff_strings_with_order(cells.into_iter(), date_order)
}

#[cfg(test)]
fn sniff_strings_with_order<'a>(
    cells: impl Iterator<Item = &'a str>,
    date_order: Option<NumericDateOrder>,
) -> (ColType, Option<String>) {
    use crate::engine::analytics::data::{is_blankish, parse_numeric};

    let (mut any, mut int, mut float, mut boolean) = (false, true, true, true);
    let (mut loose_ok, mut loose_used) = (true, false);
    let (mut n_nonblank, mut n_numeric) = (0usize, 0usize);
    let (mut date_ok, mut date_used, mut date_example, mut n_dates) = (true, false, None, 0usize);

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
        match parse_date_value_with_order(c, date_order) {
            Some(iso) => {
                date_used = true;
                n_dates += 1;
                date_example.get_or_insert_with(|| (c.to_string(), iso));
            }
            None => date_ok = false,
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
            Some(
                "amounts were stored as text (currency, commas, percent) and read as numbers"
                    .into(),
            ),
        );
    }
    // Only when every non-blank cell parses. Ambiguous numeric dates reach
    // here only when same-column evidence established a consistent order.
    if date_ok && date_used {
        let (raw, iso) = date_example.unwrap_or_default();
        let order_note = numeric_date_order_note(date_order);
        return (
            ColType::Date,
            Some(format!(
                "dates were stored as text (e.g. \"{raw}\") and normalized to ISO-8601 \
                 (e.g. {iso}) for querying with strftime()/date(){order_note}"
            )),
        );
    }
    if n_nonblank >= 3 && n_numeric >= 3 && n_numeric * 100 > n_nonblank * 80 {
        return (
            ColType::Float,
            Some(format!(
                "{n_numeric} of {n_nonblank} values were normalized as numbers; unparseable values were left NULL"
            )),
        );
    }
    if n_nonblank >= 3 && n_dates >= 3 && n_dates * 100 >= n_nonblank * 80 {
        let (raw, iso) = date_example.unwrap_or_default();
        let order_note = numeric_date_order_note(date_order);
        return (
            ColType::Date,
            Some(format!(
                "{n_dates} of {n_nonblank} values were normalized as dates (e.g. \"{raw}\" -> {iso}); \
                 unparseable values were left NULL{order_note}"
            )),
        );
    }
    if n_nonblank >= 3 && n_numeric * 100 >= n_nonblank * 60 {
        return (
            ColType::Text,
            Some(format!(
                "{n_numeric} of {n_nonblank} values parse as numbers; kept as text. Use \
                 parse_num(col) for a reliable total (CAST silently truncates text like \
                 \"1,200\" instead of erroring); COUNT(*) - COUNT(parse_num(col)) shows how \
                 many rows didn't parse"
            )),
        );
    }
    (ColType::Text, None)
}

/// If a text column's distinct values collapse under case-folding (`Rent` and
/// `rent` both present), return a note. Bounded: gives up once a column has too
/// many distinct values to be a label (free text, ids), and stops at 5000 rows.
#[cfg(test)]
fn case_collision<'a>(cells: impl Iterator<Item = &'a str>) -> Option<String> {
    let mut profile = CaseCollisionAccumulator::default();
    for cell in cells {
        profile.observe(cell);
    }
    profile.finish()
}

#[derive(Default)]
struct CaseCollisionAccumulator {
    records_seen: usize,
    raw: std::collections::BTreeSet<String>,
    folded: std::collections::BTreeSet<String>,
    example: Option<(String, String)>,
    too_many_values: bool,
}

impl CaseCollisionAccumulator {
    fn observe(&mut self, cell: &str) {
        if self.records_seen >= 5000 || self.too_many_values {
            return;
        }
        self.records_seen += 1;
        let cell = cell.trim();
        if is_blankish(cell) {
            return;
        }
        if self.raw.len() > 60 {
            self.too_many_values = true;
            return;
        }
        let folded = cell.to_lowercase();
        if !self.raw.contains(cell) && self.folded.contains(&folded) && self.example.is_none() {
            if let Some(previous) = self.raw.iter().find(|value| value.to_lowercase() == folded) {
                self.example = Some((previous.clone(), cell.to_string()));
            }
        }
        self.raw.insert(cell.to_string());
        self.folded.insert(folded);
    }

    fn finish(self) -> Option<String> {
        if self.too_many_values || self.raw.len() <= self.folded.len() {
            return None;
        }
        let example = self
            .example
            .map(|(a, b)| format!(" (e.g. {a} / {b})"))
            .unwrap_or_default();
        Some(format!(
            "values differ only in capitalisation{example}; when filtering by a value, fold case \
             (lower(col) = lower('value'), or col = 'value' COLLATE NOCASE)"
        ))
    }
}

#[cfg(test)]
fn sniff_json<'a>(vals: impl Iterator<Item = &'a Json>) -> (ColType, Option<String>) {
    let vals: Vec<&Json> = vals.collect();
    let date_order = infer_numeric_date_order(vals.iter().filter_map(|v| v.as_str()));
    sniff_json_with_order(vals.into_iter(), date_order)
}

fn sniff_json_with_order<'a>(
    vals: impl Iterator<Item = &'a Json>,
    date_order: Option<NumericDateOrder>,
) -> (ColType, Option<String>) {
    use crate::engine::analytics::data::{is_blankish, parse_numeric};

    let (mut any, mut int, mut float, mut boolean) = (false, true, true, true);
    let (mut saw_str, mut all_str_numeric) = (false, true);
    let (mut all_str_date, mut date_example) = (true, None);
    let (mut nonblank, mut numeric, mut dates) = (0usize, 0usize, 0usize);
    for v in vals {
        match v {
            Json::Null => {}
            Json::String(s) if is_blankish(s) => {}
            Json::Bool(_) => {
                any = true;
                nonblank += 1;
                int = false;
                float = false;
                all_str_date = false;
            }
            Json::Number(n) => {
                any = true;
                nonblank += 1;
                numeric += 1;
                boolean = false;
                if !n.is_i64() && !n.is_u64() {
                    int = false;
                }
                all_str_date = false;
            }
            Json::String(s) => {
                any = true;
                nonblank += 1;
                int = false;
                float = false;
                boolean = false;
                saw_str = true;
                if parse_numeric(s).is_some() {
                    numeric += 1;
                } else {
                    all_str_numeric = false;
                }
                match parse_date_value_with_order(s, date_order) {
                    Some(iso) => {
                        dates += 1;
                        date_example.get_or_insert_with(|| (s.clone(), iso));
                    }
                    None => all_str_date = false,
                }
            }
            _ => {
                any = true;
                int = false;
                float = false;
                boolean = false;
                all_str_numeric = false;
                all_str_date = false;
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
    // Same reasoning as sniff_strings: only when every string value parses.
    if saw_str && all_str_date {
        let (raw, iso) = date_example.unwrap_or_default();
        let order_note = numeric_date_order_note(date_order);
        return (
            ColType::Date,
            Some(format!(
                "dates were stored as text (e.g. \"{raw}\") and normalized to ISO-8601 \
                 (e.g. {iso}) for querying with strftime()/date(){order_note}"
            )),
        );
    }
    if nonblank >= 3 && numeric >= 3 && numeric * 100 >= nonblank * 80 {
        return (
            ColType::Float,
            Some(format!(
                "{numeric} of {nonblank} values were normalized as numbers; unparseable values were left NULL"
            )),
        );
    }
    if nonblank >= 3 && dates >= 3 && dates * 100 >= nonblank * 80 {
        let (raw, iso) = date_example.unwrap_or_default();
        let order_note = numeric_date_order_note(date_order);
        return (
            ColType::Date,
            Some(format!(
                "{dates} of {nonblank} values were normalized as dates (e.g. \"{raw}\" -> {iso}); \
                 unparseable values were left NULL{order_note}"
            )),
        );
    }
    (ColType::Text, None)
}

#[cfg(test)]
fn string_cell(s: &str, ty: ColType) -> Cell {
    string_cell_with_order(s, ty, None)
}

fn string_cell_with_order(s: &str, ty: ColType, date_order: Option<NumericDateOrder>) -> Cell {
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
        _ if crate::engine::analytics::data::is_blankish(s) => Json::Null,
        ColType::Int => s.parse::<i64>().map(Json::from).unwrap_or(Json::Null),
        // A `Float` column may have been chosen by loose coercion, so fall back
        // to `parse_numeric` for cells the strict parse rejects ("$1,200.00").
        ColType::Float => s
            .parse::<f64>()
            .ok()
            .or_else(|| crate::engine::analytics::data::parse_numeric(s))
            .and_then(serde_json::Number::from_f64)
            .map(Json::Number)
            .unwrap_or(Json::Null),
        ColType::Bool => match s.to_ascii_lowercase().as_str() {
            "true" => Json::from(1),
            "false" => Json::from(0),
            _ => Json::Null,
        },
        ColType::Date => parse_date_value_with_order(s, date_order)
            .map(Json::from)
            .unwrap_or(Json::Null),
    }
}

#[cfg(test)]
fn json_cell(v: &Json, ty: ColType) -> Cell {
    json_cell_with_order(v, ty, None)
}

fn json_cell_with_order(v: &Json, ty: ColType, date_order: Option<NumericDateOrder>) -> Cell {
    use crate::engine::analytics::data::{is_blankish, parse_numeric};
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
        (Json::String(s), ColType::Int) => parse_numeric(s)
            .map(|f| Json::from(f as i64))
            .unwrap_or(Json::Null),
        (Json::String(s), ColType::Date) => parse_date_value_with_order(s, date_order)
            .map(Json::from)
            .unwrap_or(Json::Null),
        (Json::String(s), _) => Json::from(s.clone()),
        (other, ColType::Text) => Json::from(other.to_string()),
        _ => Json::Null,
    }
}

fn numeric_date_order_note(order: Option<NumericDateOrder>) -> String {
    match order {
        Some(NumericDateOrder::MonthFirst) => {
            "; ambiguous numeric dates were interpreted month-first from unambiguous dates in this column".into()
        }
        Some(NumericDateOrder::DayFirst) => {
            "; ambiguous numeric dates were interpreted day-first from unambiguous dates in this column".into()
        }
        None => String::new(),
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
        R::Real(f) => serde_json::Number::from_f64(f)
            .map(Json::Number)
            .unwrap_or(Json::Null),
        R::Text(t) => Json::from(String::from_utf8_lossy(t).into_owned()),
        R::Blob(b) => Json::from(format!("<{} bytes>", b.len())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sniffs_types() {
        assert_eq!(
            sniff_strings(["1", "2", "", "3"].into_iter()).0,
            ColType::Int
        );
        assert_eq!(
            sniff_strings(["1", "2.5", "3"].into_iter()).0,
            ColType::Float
        );
        assert_eq!(
            sniff_strings(["true", "false"].into_iter()).0,
            ColType::Bool
        );
        assert_eq!(sniff_strings(["a", "1", "b"].into_iter()).0, ColType::Text);
        assert_eq!(sniff_strings(["", ""].into_iter()).0, ColType::Text);
    }

    #[test]
    fn streaming_column_profile_matches_full_column_type_inference() {
        let cases: &[&[&str]] = &[
            &["1", "2", "", "3"],
            &["1", "2.5", "3"],
            &["true", "false", "TRUE"],
            &["$1,200.00", "1,150", "1200", "N/A"],
            &["$1,200", "1,300", "bad", "800", "725"],
            &["2024-01-02", "Feb 3, 2024", "2024/03/04"],
            &["03/04/2024", "4 Mar 2024", "07/25/2024", "06/03/2024"],
            &["03/04/2024", "13/04/2024", "07/25/2024"],
            &["inf", "$2.00"],
            &["Rent", "rent", "Food", "N/A"],
            &["", "N/A", "-"],
        ];

        for values in cases {
            let date_order = infer_numeric_date_order(values.iter().copied());
            let expected = sniff_strings_with_order(values.iter().copied(), date_order);
            let mut profile = StringColumnProfile::default();
            for (index, value) in values.iter().enumerate() {
                profile.observe(value, index);
            }
            let actual = profile.finish(0, values.len());
            assert_eq!(actual.0, expected.0, "column values: {values:?}");
            assert_eq!(actual.1, expected.1, "column values: {values:?}");
            assert_eq!(actual.2, date_order, "column values: {values:?}");
        }
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
    fn mixed_column_note_points_at_parse_num_not_cast() {
        // A column that's mostly-but-not-cleanly numeric (60-100%) stays TEXT;
        // the note must steer toward parse_num(), not the bare CAST that
        // silently truncates comma-formatted text instead of erroring.
        let (ty, note) = sniff_strings(
            [
                "$1,200.00",
                "$1,300.00",
                "$1,300.00 (fee $1.95)",
                "500",
                "600",
            ]
            .into_iter(),
        );
        assert_eq!(ty, ColType::Text);
        let note = note.expect("a mixed column should be noted");
        assert!(
            note.contains("parse_num"),
            "note should mention parse_num: {note:?}"
        );
        assert!(
            !note.contains("CAST it for a total"),
            "note should not tell the model to CAST: {note:?}"
        );
    }

    #[test]
    fn parses_named_month_dates() {
        // The reported bug: "Aug 1, 2026" made strftime() return NULL.
        assert_eq!(
            parse_named_month_date("Aug 1, 2026").as_deref(),
            Some("2026-08-01")
        );
        assert_eq!(
            parse_named_month_date("August 1, 2026").as_deref(),
            Some("2026-08-01")
        );
        assert_eq!(
            parse_named_month_date("Aug 15 2026").as_deref(),
            Some("2026-08-15")
        );
        // day-first, with an ordinal suffix
        assert_eq!(
            parse_named_month_date("1st Aug 2026").as_deref(),
            Some("2026-08-01")
        );
        assert_eq!(
            parse_named_month_date("21st August 2026").as_deref(),
            Some("2026-08-21")
        );
        // case-insensitive
        assert_eq!(
            parse_named_month_date("aug 1, 2026").as_deref(),
            Some("2026-08-01")
        );
        // not a date at all
        assert_eq!(parse_named_month_date("Groceries"), None);
        assert_eq!(parse_named_month_date("$1,200.00"), None);
        assert_eq!(parse_named_month_date("2026-08-01"), None); // already ISO, not this format
                                                                // out-of-range day/year rejected rather than silently wrapped
        assert_eq!(parse_named_month_date("Aug 45, 2026"), None);
        assert_eq!(parse_named_month_date("Aug 1, 26"), None);
        // deliberately NOT attempted: ambiguous numeric formats
        assert_eq!(parse_named_month_date("08/01/2026"), None);
    }

    #[test]
    fn parses_year_first_and_iso_dates_into_one_sortable_shape() {
        assert_eq!(
            parse_date_value("2024-01-02").as_deref(),
            Some("2024-01-02")
        );
        assert_eq!(
            parse_date_value("2024/01/02").as_deref(),
            Some("2024-01-02")
        );
        assert_eq!(
            parse_date_value("2024-01-02T12:30:00Z").as_deref(),
            Some("2024-01-02")
        );
        assert_eq!(parse_date_value("2024-01").as_deref(), Some("2024-01"));
        assert_eq!(parse_date_value("01/02/2024"), None);
    }

    #[test]
    fn sniffs_named_month_dates_and_normalizes_to_iso() {
        let (ty, note) =
            sniff_strings(["Aug 1, 2026", "Sep 15, 2026", "Oct 1, 2026", "N/A"].into_iter());
        assert_eq!(ty, ColType::Date);
        let note = note.expect("a coerced date column should be noted");
        assert!(note.contains("ISO-8601"), "{note}");
        assert_eq!(
            string_cell("Aug 1, 2026", ColType::Date),
            Json::from("2026-08-01")
        );

        // one genuinely unparseable value -> the whole column stays text,
        // rather than silently mixing ISO and non-ISO dates.
        let (ty, _) = sniff_strings(["Aug 1, 2026", "sometime in September"].into_iter());
        assert_eq!(ty, ColType::Text);

        // a plain text column is unaffected
        assert_eq!(
            sniff_strings(["Groceries", "Rent", "Transport"].into_iter()).0,
            ColType::Text
        );
    }

    #[test]
    fn normalizes_a_majority_date_column_and_nulls_bad_cells() {
        let (ty, note) = sniff_strings(
            [
                "not a date",
                "31-May-2024",
                "Jun 1, 2024",
                "2024-07-01",
                "Aug 1, 2024",
            ]
            .into_iter(),
        );
        assert_eq!(ty, ColType::Date);
        assert!(note.unwrap().contains("4 of 5"));
        assert_eq!(string_cell("31-May-2024", ty), Json::from("2024-05-31"));
        assert_eq!(string_cell("not a date", ty), Json::Null);
    }

    #[test]
    fn normalizes_a_majority_numeric_column_and_nulls_bad_cells() {
        let (ty, note) = sniff_strings(["$1,200", "1,300", "950", "bad", "800", "725"].into_iter());
        assert_eq!(ty, ColType::Float);
        assert!(note.unwrap().contains("5 of 6"));
        assert_eq!(string_cell("$1,200", ty), Json::from(1200.0));
        assert_eq!(string_cell("bad", ty), Json::Null);
    }

    #[test]
    fn sniffs_named_month_dates_from_json() {
        let vals = [
            Json::from("Aug 1, 2026"),
            Json::from("Sep 15, 2026"),
            Json::Null,
        ];
        let (ty, note) = sniff_json(vals.iter());
        assert_eq!(ty, ColType::Date);
        assert!(note.unwrap().contains("ISO-8601"));
        assert_eq!(
            json_cell(&Json::from("Aug 1, 2026"), ColType::Date),
            Json::from("2026-08-01")
        );
    }

    #[test]
    fn infers_numeric_date_order_for_json_columns_too() {
        let vals = [
            Json::from("03/04/2024"),
            Json::from("4 Mar 2024"),
            Json::from("07/25/2024"),
            Json::from("06/03/2024"),
        ];
        let (ty, note) = sniff_json(vals.iter());
        assert_eq!(ty, ColType::Date);
        assert!(note.unwrap().contains("month-first"));
        assert_eq!(
            json_cell_with_order(
                &Json::from("06/03/2024"),
                ColType::Date,
                Some(NumericDateOrder::MonthFirst)
            ),
            Json::from("2024-06-03")
        );
    }

    #[test]
    fn case_collision_flags_a_label_that_folds() {
        let c = case_collision(["Rent", "rent", "food", "RENT", "food"].into_iter());
        let c = c.expect("Rent/rent/RENT collide under case-fold");
        assert!(c.contains("capitalisation"), "{c}");
        assert!(
            c.contains("lower(") || c.contains("NOCASE"),
            "steers to case-fold: {c}"
        );

        // consistent casing -> no note
        assert!(case_collision(["rent", "food", "transport", "rent"].into_iter()).is_none());
        // blanks ignored, single value -> no note
        assert!(case_collision(["rent", "", "N/A", "rent"].into_iter()).is_none());
        // a high-cardinality column (free text / ids) -> not a label, no note
        let many: Vec<String> = (0..80).map(|i| format!("Item{i}")).collect();
        assert!(case_collision(many.iter().map(String::as_str)).is_none());
    }

    #[test]
    fn parse_num_sql_function_matches_parse_numeric() {
        let conn = Connection::open_in_memory().unwrap();
        register_parse_num(&conn).unwrap();
        let get = |sql: &str| -> Option<f64> { conn.query_row(sql, [], |r| r.get(0)).unwrap() };
        assert_eq!(get("SELECT parse_num('$1,200.00')"), Some(1200.0));
        assert_eq!(get("SELECT parse_num('1,300.00 (paid)')"), Some(1300.0));
        assert_eq!(get("SELECT parse_num('1,300.00 (fee $1.95)')"), None);
        assert_eq!(get("SELECT parse_num(42)"), Some(42.0));
        assert_eq!(get("SELECT parse_num(42.5)"), Some(42.5));
        assert_eq!(get("SELECT parse_num(NULL)"), None);
        // A mixed column: SUM skips the NULLs from unparseable rows rather
        // than erroring or silently truncating them like CAST would.
        conn.execute_batch(
            "CREATE TABLE t (amount TEXT);
             INSERT INTO t VALUES ('$1,200.00'), ('$1,300.00 (fee $1.95)'), ('500');",
        )
        .unwrap();
        let total: f64 = conn
            .query_row("SELECT SUM(parse_num(amount)) FROM t", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            total, 1700.0,
            "the unparseable row should be skipped, not truncated to 1.0"
        );
        let parsed_count: i64 = conn
            .query_row("SELECT COUNT(parse_num(amount)) FROM t", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            parsed_count, 2,
            "exactly the two clean rows should have parsed"
        );
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
