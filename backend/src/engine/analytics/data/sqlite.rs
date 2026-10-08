//! The default data engine: a file-backed SQLite database. Files are sniffed
//! for column types and imported as real tables. Read-only queries run on a
//! fresh `SQLITE_OPEN_READ_ONLY` connection.

use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, OnceLock,
};

use rusqlite::functions::FunctionFlags;
use rusqlite::types::ValueRef;
use rusqlite::{Connection, OpenFlags};
use serde::de::{Deserializer as _, MapAccess, SeqAccess, Visitor};
use serde_json::Value as Json;

use crate::engine::analytics::data::{
    infer_numeric_date_order, is_blankish, parse_date_value, parse_date_value_with_order,
    parse_numeric, quote_ident, Cell, ColType, DataEngine, NumericDateOrder, PythonBridge,
    QueryOutcome, SourceIngestProgress, SourceLoad,
};
use crate::engine::catalog::{ColumnInfo, SourceKind};
use crate::engine::error::{EngineError, EngineResult};

#[cfg(test)]
use crate::engine::analytics::data::parse_named_month_date;

pub struct SqliteEngine {
    conn: Connection,
    path: PathBuf,
}

fn ingest_timing_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("FELLA_INGEST_TIMING").is_some())
}

const INGEST_PROGRESS_MIN_BYTES: u64 = 64 * 1024 * 1024;
const INGEST_PROGRESS_INTERVAL_BYTES: u64 = 64 * 1024 * 1024;
const INGEST_PROGRESS_ROW_CHECK_INTERVAL: usize = 65_536;

struct JsonSourceOptions {
    ndjson: bool,
    allow_progress: bool,
    progress_min_bytes: u64,
    progress_interval_bytes: u64,
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
        // The workspace database is built in isolation and only read after
        // publication; its ingestion path has no concurrent readers. Rollback
        // journaling avoids WAL checkpointing the completed bulk load before
        // the catalog can be published, without disabling transaction safety.
        conn.execute_batch("PRAGMA journal_mode = DELETE;")?;
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
        allow_progress: bool,
        on_progress: &mut dyn FnMut(SourceIngestProgress),
    ) -> EngineResult<SourceLoad> {
        let stamp = file_stamp(path)?;
        let report_progress = allow_progress && stamp.size >= INGEST_PROGRESS_MIN_BYTES;
        let timing = ingest_timing_enabled();
        let profile_started = timing.then(std::time::Instant::now);
        let mut profile_progress = |bytes_read| {
            on_progress(SourceIngestProgress {
                stage: "profiling",
                bytes_read,
            });
        };
        let profile = inspect_delimited_with_progress(
            path,
            default_delim,
            INGEST_PROGRESS_INTERVAL_BYTES,
            report_progress,
            &mut profile_progress,
        )?;
        let profile_elapsed = profile_started.map(|started| started.elapsed());
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
        let load_started = timing.then(std::time::Instant::now);
        tx.execute_batch(&format!(
            "DROP TABLE IF EXISTS {ident}; CREATE TABLE {ident} ({cols_sql});"
        ))?;
        let mut loaded = 0usize;
        let mut valid_records = 0usize;
        let mut dropped_records = 0usize;
        let mut case_collisions: Vec<CaseCollisionAccumulator> = (0..columns.len())
            .map(|_| CaseCollisionAccumulator::default())
            .collect();
        let mut rows_since_progress_check = 0usize;
        let mut last_progress_bytes = 0u64;
        let mut next_progress_bytes = INGEST_PROGRESS_INTERVAL_BYTES;
        {
            let mut statement = tx.prepare(&insert_sql)?;
            let mut reader = delimited_reader(path, profile.delimiter)?;
            let mut sqlite_values = Vec::with_capacity(columns.len());
            let mut record = csv::StringRecord::new();
            if report_progress {
                on_progress(SourceIngestProgress {
                    stage: "loading",
                    bytes_read: 0,
                });
            }
            loop {
                let read_result = reader.read_record(&mut record);
                if report_progress {
                    rows_since_progress_check += 1;
                    if rows_since_progress_check >= INGEST_PROGRESS_ROW_CHECK_INTERVAL {
                        rows_since_progress_check = 0;
                        let bytes_read = reader.position().byte().min(stamp.size);
                        if bytes_read >= next_progress_bytes {
                            on_progress(SourceIngestProgress {
                                stage: "loading",
                                bytes_read,
                            });
                            last_progress_bytes = bytes_read;
                            next_progress_bytes =
                                bytes_read.saturating_add(INGEST_PROGRESS_INTERVAL_BYTES);
                        }
                    }
                }
                match read_result {
                    Ok(true) => {}
                    Ok(false) => break,
                    Err(_) => {
                        dropped_records += 1;
                        record.clear();
                        continue;
                    }
                }
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

                sqlite_values.clear();
                for (index, (_, ty)) in columns.iter().enumerate() {
                    let raw = record.get(index).unwrap_or("");
                    case_collisions[index].observe(raw);
                    sqlite_values.push(string_cell_to_sqlite(raw, *ty, profile.date_orders[index]));
                }
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
        let load_elapsed = load_started.map(|started| started.elapsed());
        if report_progress && last_progress_bytes < stamp.size {
            on_progress(SourceIngestProgress {
                stage: "loading",
                bytes_read: stamp.size,
            });
        }

        if let (Some(profile_elapsed), Some(load_elapsed)) = (profile_elapsed, load_elapsed) {
            eprintln!(
                "Fella ingest timing: format=delimited path={path} bytes={} rows={} profile_ms={} load_ms={}",
                stamp.size,
                loaded,
                profile_elapsed.as_millis(),
                load_elapsed.as_millis()
            );
        }

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

    /// Profile JSON records without retaining the file, then stream a second
    /// pass into SQLite. A single JSON object is one row; arrays and NDJSON
    /// contribute one row per object record.
    fn add_json_source(
        &mut self,
        name: &str,
        path: &str,
        options: JsonSourceOptions,
        on_progress: &mut dyn FnMut(SourceIngestProgress),
    ) -> EngineResult<SourceLoad> {
        let stamp = file_stamp(path)?;
        let report_progress = options.allow_progress && stamp.size >= options.progress_min_bytes;
        let mut profile = JsonSourceProfile::default();
        if report_progress {
            on_progress(SourceIngestProgress {
                stage: "profiling",
                bytes_read: 0,
            });
        }
        let mut profile_bytes = 0;
        let first_pass = {
            let mut report_profile = |bytes_read| {
                profile_bytes = bytes_read;
                on_progress(SourceIngestProgress {
                    stage: "profiling",
                    bytes_read,
                });
            };
            let profile_callback =
                report_progress.then_some(&mut report_profile as &mut dyn FnMut(u64));
            visit_json_objects(
                path,
                options.ndjson,
                stamp.size,
                options.progress_interval_bytes,
                profile_callback,
                |object| {
                    profile.observe(&object);
                    Ok(())
                },
            )?
        };
        if report_progress && profile_bytes < stamp.size {
            on_progress(SourceIngestProgress {
                stage: "profiling",
                bytes_read: stamp.size,
            });
        }
        if file_stamp(path)? != stamp {
            return Err(EngineError::msg(format!(
                "{path}: the file changed while Fella was inspecting it; retry the mount"
            )));
        }
        if profile.rows == 0 {
            return Err(EngineError::msg(format!("{path}: no JSON objects found")));
        }

        let profiled_rows = profile.rows;
        let (headers, types, date_orders, notes) = profile.finish();
        let columns: Vec<(String, ColType)> =
            headers.iter().cloned().zip(types.iter().copied()).collect();
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
        {
            let mut statement = tx.prepare(&insert_sql)?;
            let mut sqlite_values = Vec::with_capacity(headers.len());
            if report_progress {
                on_progress(SourceIngestProgress {
                    stage: "loading",
                    bytes_read: 0,
                });
            }
            let mut load_bytes = 0;
            let second_pass = {
                let mut report_load = |bytes_read| {
                    load_bytes = bytes_read;
                    on_progress(SourceIngestProgress {
                        stage: "loading",
                        bytes_read,
                    });
                };
                let load_callback =
                    report_progress.then_some(&mut report_load as &mut dyn FnMut(u64));
                visit_json_objects(
                    path,
                    options.ndjson,
                    stamp.size,
                    options.progress_interval_bytes,
                    load_callback,
                    |object| {
                        sqlite_values.clear();
                        sqlite_values.extend(
                            headers.iter().zip(types.iter().zip(&date_orders)).map(
                                |(header, (ty, order))| {
                                    json_cell_to_sqlite(
                                        object.get(header).unwrap_or(&Json::Null),
                                        *ty,
                                        *order,
                                    )
                                },
                            ),
                        );
                        statement
                            .execute(rusqlite::params_from_iter(sqlite_values.iter()))
                            .map_err(|error| EngineError::msg(error.to_string()))?;
                        loaded += 1;
                        Ok(())
                    },
                )?
            };
            if report_progress && load_bytes < stamp.size {
                on_progress(SourceIngestProgress {
                    stage: "loading",
                    bytes_read: stamp.size,
                });
            }
            if second_pass != first_pass {
                return Err(EngineError::msg(format!(
                    "{path}: the file changed while Fella was loading it; retry the mount"
                )));
            }
        }
        if loaded != profiled_rows || file_stamp(path)? != stamp {
            return Err(EngineError::msg(format!(
                "{path}: the file changed while Fella was loading it; retry the mount"
            )));
        }
        tx.commit()?;

        let note = json_read_note(&first_pass);
        Ok(SourceLoad {
            row_count: loaded as i64,
            note,
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
            SourceKind::Csv | SourceKind::Tsv => {
                let mut ignore_progress = |_| {};
                let delimiter = if kind == SourceKind::Csv { b',' } else { b'\t' };
                return self.add_delimited_source(
                    name,
                    path,
                    delimiter,
                    false,
                    &mut ignore_progress,
                );
            }
            _ => {}
        }
        match kind {
            SourceKind::Json | SourceKind::Ndjson => {
                let mut ignore_progress = |_| {};
                self.add_json_source(
                    name,
                    path,
                    JsonSourceOptions {
                        ndjson: kind == SourceKind::Ndjson,
                        allow_progress: false,
                        progress_min_bytes: INGEST_PROGRESS_MIN_BYTES,
                        progress_interval_bytes: INGEST_PROGRESS_INTERVAL_BYTES,
                    },
                    &mut ignore_progress,
                )
            }
            SourceKind::Parquet => Err(EngineError::msg(
                "Parquet needs the DuckDB build rebuild with `cargo build --features duckdb`",
            )),
            _ => Err(EngineError::msg("not a path-readable tabular source")),
        }
    }

    fn add_source_with_progress(
        &mut self,
        name: &str,
        kind: SourceKind,
        path: &str,
        on_progress: &mut dyn FnMut(SourceIngestProgress),
    ) -> EngineResult<SourceLoad> {
        match kind {
            SourceKind::Csv => self.add_delimited_source(name, path, b',', true, on_progress),
            SourceKind::Tsv => self.add_delimited_source(name, path, b'\t', true, on_progress),
            SourceKind::Json | SourceKind::Ndjson => self.add_json_source(
                name,
                path,
                JsonSourceOptions {
                    ndjson: kind == SourceKind::Ndjson,
                    allow_progress: true,
                    progress_min_bytes: INGEST_PROGRESS_MIN_BYTES,
                    progress_interval_bytes: INGEST_PROGRESS_INTERVAL_BYTES,
                },
                on_progress,
            ),
            _ => self.add_source(name, kind, path),
        }
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

#[derive(Debug, Default, PartialEq, Eq)]
struct JsonReadSummary {
    objects: usize,
    non_object_records: usize,
    malformed_records: usize,
}

fn json_read_note(summary: &JsonReadSummary) -> Option<String> {
    let mut notes = Vec::new();
    if summary.non_object_records > 0 {
        notes.push(format!(
            "{} non-object JSON record(s) were skipped",
            summary.non_object_records
        ));
    }
    if summary.malformed_records > 0 {
        notes.push(format!(
            "{} malformed JSON line(s) were skipped",
            summary.malformed_records
        ));
    }
    (!notes.is_empty()).then(|| notes.join("; "))
}

/// Visit object records using bounded memory: at most one JSON record (or one
/// NDJSON line) is materialized at a time. Invalid NDJSON lines and non-object
/// records retain the previous skip behavior but are now reported explicitly.
fn visit_json_objects<F>(
    path: &str,
    ndjson: bool,
    total_bytes: u64,
    progress_interval_bytes: u64,
    on_progress: Option<&mut dyn FnMut(u64)>,
    mut visit: F,
) -> EngineResult<JsonReadSummary>
where
    F: FnMut(serde_json::Map<String, Json>) -> EngineResult<()>,
{
    use std::io::BufRead;

    let mut summary = JsonReadSummary::default();
    if ndjson {
        let file = std::fs::File::open(path)
            .map_err(|error| EngineError::io(format!("read {path}"), error))?;
        let reader = ProgressReader::new(file, total_bytes, progress_interval_bytes, on_progress);
        let mut reader = std::io::BufReader::new(reader);
        let mut line = Vec::new();
        loop {
            line.clear();
            if reader
                .read_until(b'\n', &mut line)
                .map_err(|error| EngineError::io(format!("read {path}"), error))?
                == 0
            {
                break;
            }
            if line.iter().all(|byte| byte.is_ascii_whitespace()) {
                continue;
            }
            match serde_json::from_slice::<Json>(&line) {
                Ok(Json::Object(object)) => {
                    visit(object)?;
                    summary.objects += 1;
                }
                Ok(_) => summary.non_object_records += 1,
                Err(_) => summary.malformed_records += 1,
            }
        }
        return Ok(summary);
    }

    let file = std::fs::File::open(path)
        .map_err(|error| EngineError::io(format!("read {path}"), error))?;
    let reader = ProgressReader::new(file, total_bytes, progress_interval_bytes, on_progress);
    let mut deserializer = serde_json::Deserializer::from_reader(std::io::BufReader::new(reader));
    let visitor = JsonObjectRowsVisitor {
        visit: &mut visit,
        summary: &mut summary,
    };
    deserializer
        .deserialize_any(visitor)
        .map_err(|error| EngineError::msg(format!("{path}: {error}")))?;
    deserializer
        .end()
        .map_err(|error| EngineError::msg(format!("{path}: {error}")))?;
    Ok(summary)
}

/// Count bytes read by serde/NDJSON while emitting bounded progress. The
/// reader is wrapped below `BufReader` so large individual JSON records also
/// advance progress while they are being parsed, rather than only per record.
struct ProgressReader<'a> {
    file: std::fs::File,
    bytes_read: u64,
    total_bytes: u64,
    interval_bytes: u64,
    next_report_bytes: u64,
    on_progress: Option<&'a mut dyn FnMut(u64)>,
}

impl<'a> ProgressReader<'a> {
    fn new(
        file: std::fs::File,
        total_bytes: u64,
        interval_bytes: u64,
        on_progress: Option<&'a mut dyn FnMut(u64)>,
    ) -> Self {
        let interval_bytes = interval_bytes.max(1);
        Self {
            file,
            bytes_read: 0,
            total_bytes,
            interval_bytes,
            next_report_bytes: interval_bytes,
            on_progress,
        }
    }
}

impl std::io::Read for ProgressReader<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let bytes = self.file.read(buffer)?;
        self.bytes_read = self.bytes_read.saturating_add(bytes as u64);
        if bytes > 0 && self.bytes_read >= self.next_report_bytes {
            if let Some(on_progress) = self.on_progress.as_deref_mut() {
                on_progress(self.bytes_read.min(self.total_bytes));
            }
            self.next_report_bytes = self.bytes_read.saturating_add(self.interval_bytes);
        }
        Ok(bytes)
    }
}

struct JsonObjectRowsVisitor<'a, F> {
    visit: &'a mut F,
    summary: &'a mut JsonReadSummary,
}

impl<'de, F> Visitor<'de> for JsonObjectRowsVisitor<'_, F>
where
    F: FnMut(serde_json::Map<String, Json>) -> EngineResult<()>,
{
    type Value = ();

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("a JSON object or an array of JSON records")
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        while let Some(value) = sequence.next_element::<Json>()? {
            match value {
                Json::Object(object) => {
                    (self.visit)(object).map_err(serde::de::Error::custom)?;
                    self.summary.objects += 1;
                }
                _ => self.summary.non_object_records += 1,
            }
        }
        Ok(())
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut object = serde_json::Map::new();
        while let Some((key, value)) = map.next_entry::<String, Json>()? {
            object.insert(key, value);
        }
        (self.visit)(object).map_err(serde::de::Error::custom)?;
        self.summary.objects += 1;
        Ok(())
    }
}

#[derive(Default)]
struct JsonSourceProfile {
    headers: Vec<String>,
    positions: std::collections::HashMap<String, usize>,
    columns: Vec<JsonColumnProfile>,
    rows: usize,
}

type JsonSourceProfileResult = (
    Vec<String>,
    Vec<ColType>,
    Vec<Option<NumericDateOrder>>,
    Vec<Option<String>>,
);

impl JsonSourceProfile {
    fn observe(&mut self, object: &serde_json::Map<String, Json>) {
        for (name, value) in object {
            let index = match self.positions.get(name) {
                Some(index) => *index,
                None => {
                    let index = self.headers.len();
                    self.headers.push(name.clone());
                    self.positions.insert(name.clone(), index);
                    self.columns.push(JsonColumnProfile::default());
                    index
                }
            };
            self.columns[index].observe(value);
        }
        self.rows += 1;
    }

    fn finish(self) -> JsonSourceProfileResult {
        let mut types = Vec::with_capacity(self.columns.len());
        let mut date_orders = Vec::with_capacity(self.columns.len());
        let mut notes = Vec::with_capacity(self.columns.len());
        for column in self.columns {
            let (ty, note, date_order) = column.finish();
            types.push(ty);
            notes.push(note);
            date_orders.push(date_order);
        }
        (self.headers, types, date_orders, notes)
    }
}

#[derive(Default)]
struct JsonColumnProfile {
    nonblank: usize,
    numbers: usize,
    integers: usize,
    booleans: usize,
    numeric: usize,
    dates_without_order: usize,
    dates_month_first: usize,
    dates_day_first: usize,
    date_order_hint: Option<NumericDateOrder>,
    date_order_conflict: bool,
    saw_string: bool,
    non_numeric_string: bool,
    direct_date_example: Option<String>,
    month_first_date_example: Option<String>,
    day_first_date_example: Option<String>,
}

impl JsonColumnProfile {
    fn observe(&mut self, value: &Json) {
        match value {
            Json::Null => return,
            Json::String(value) if is_blankish(value) => return,
            _ => self.nonblank += 1,
        }

        match value {
            Json::Bool(_) => self.booleans += 1,
            Json::Number(number) => {
                self.numbers += 1;
                self.numeric += 1;
                if number.is_i64() || number.is_u64() {
                    self.integers += 1;
                }
            }
            Json::String(value) => {
                self.saw_string = true;
                let is_clean_number = value.parse::<f64>().ok().is_some_and(f64::is_finite);
                let is_loose_number = !is_clean_number
                    && value.bytes().any(|byte| byte.is_ascii_digit())
                    && parse_numeric(value).is_some();
                if is_clean_number || is_loose_number {
                    self.numeric += 1;
                } else {
                    self.non_numeric_string = true;
                }

                match infer_numeric_date_order(std::iter::once(value.as_str())) {
                    Some(order) if self.date_order_hint.is_some_and(|hint| hint != order) => {
                        self.date_order_conflict = true;
                    }
                    Some(order) => self.date_order_hint = Some(order),
                    None => {}
                }

                let direct = parse_date_value(value);
                let month_first = direct.clone().or_else(|| {
                    value.contains('/').then(|| {
                        parse_date_value_with_order(value, Some(NumericDateOrder::MonthFirst))
                    })?
                });
                let day_first = direct.clone().or_else(|| {
                    value.contains('/').then(|| {
                        parse_date_value_with_order(value, Some(NumericDateOrder::DayFirst))
                    })?
                });
                if direct.is_some() {
                    self.dates_without_order += 1;
                    self.direct_date_example
                        .get_or_insert_with(|| value.clone());
                }
                if month_first.is_some() {
                    self.dates_month_first += 1;
                    self.month_first_date_example
                        .get_or_insert_with(|| value.clone());
                }
                if day_first.is_some() {
                    self.dates_day_first += 1;
                    self.day_first_date_example
                        .get_or_insert_with(|| value.clone());
                }
            }
            _ => self.non_numeric_string = true,
        }
    }

    fn finish(self) -> (ColType, Option<String>, Option<NumericDateOrder>) {
        let date_order = (!self.date_order_conflict)
            .then_some(self.date_order_hint)
            .flatten();
        let dates = match date_order {
            Some(NumericDateOrder::MonthFirst) => self.dates_month_first,
            Some(NumericDateOrder::DayFirst) => self.dates_day_first,
            None => self.dates_without_order,
        };
        let date_example = match date_order {
            Some(NumericDateOrder::MonthFirst) => self
                .direct_date_example
                .as_ref()
                .or(self.month_first_date_example.as_ref()),
            Some(NumericDateOrder::DayFirst) => self
                .direct_date_example
                .as_ref()
                .or(self.day_first_date_example.as_ref()),
            None => self.direct_date_example.as_ref(),
        }
        .and_then(|raw| {
            parse_date_value_with_order(raw, date_order)
                .map(|normalized| (raw.as_str(), normalized))
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
        if self.numbers == self.nonblank {
            return (ColType::Float, None, date_order);
        }
        if self.saw_string && !self.non_numeric_string {
            return (
                ColType::Float,
                Some("amounts were stored as text and read as numbers".into()),
                date_order,
            );
        }
        if dates == self.nonblank && dates > 0 {
            let (raw, normalized) = date_example.unwrap_or(("", String::new()));
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
            && (self.numeric as u128) * 100 >= (self.nonblank as u128) * 80
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
            let (raw, normalized) = date_example.unwrap_or(("", String::new()));
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
        (ColType::Text, None, date_order)
    }
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
        let is_float = is_integer || value.parse::<f64>().is_ok();
        let parsed_number = (!is_float && value.bytes().any(|byte| byte.is_ascii_digit()))
            .then(|| parse_numeric(value))
            .flatten();
        let is_boolean = value.eq_ignore_ascii_case("true") || value.eq_ignore_ascii_case("false");
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

        // Date parsing is intentionally avoided for ordinary numeric and label
        // cells. The supported date forms all contain a separator or a
        // whitespace-delimited month name plus digits.
        let date_candidate = may_be_date(value);
        let direct_date = date_candidate.then(|| parse_date_value(value)).flatten();
        let month_first_date = if direct_date.is_some() {
            direct_date.clone()
        } else if date_candidate && value.contains('/') {
            parse_date_value_with_order(value, Some(NumericDateOrder::MonthFirst))
        } else {
            None
        };
        let day_first_date = if direct_date.is_some() {
            direct_date.clone()
        } else if date_candidate && value.contains('/') {
            parse_date_value_with_order(value, Some(NumericDateOrder::DayFirst))
        } else {
            None
        };
        if direct_date.is_some() {
            bump(&mut self.dates_without_order, adding);
        }
        if month_first_date.is_some() {
            bump(&mut self.dates_month_first, adding);
        }
        if day_first_date.is_some() {
            bump(&mut self.dates_day_first, adding);
        }
        let order_hint = value
            .contains('/')
            .then(|| infer_numeric_date_order(std::iter::once(value)))
            .flatten();
        match order_hint {
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

fn may_be_date(value: &str) -> bool {
    let has_digit = value.bytes().any(|byte| byte.is_ascii_digit());
    if !has_digit {
        return false;
    }
    value.bytes().any(|byte| matches!(byte, b'-' | b'/'))
        || (value.chars().any(char::is_whitespace) && value.chars().any(char::is_alphabetic))
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

fn inspect_delimited_with_progress(
    path: &str,
    default_delim: u8,
    progress_interval_bytes: u64,
    report_progress: bool,
    on_progress: &mut dyn FnMut(u64),
) -> EngineResult<DelimitedProfile> {
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
    let mut record = csv::StringRecord::new();
    let mut last = csv::StringRecord::new();
    let mut profiles: Vec<StringColumnProfile> = Vec::new();
    let mut width = 0usize;
    let mut total_records = 0usize;
    let mut dropped_records = 0usize;
    let total_bytes = std::fs::metadata(path)
        .map(|metadata| metadata.len())
        .unwrap_or_default();
    let mut rows_since_progress_check = 0usize;
    let mut last_progress_bytes = 0u64;
    let mut next_progress_bytes = progress_interval_bytes.max(1);
    if report_progress {
        on_progress(0);
    }
    loop {
        let read_result = reader.read_record(&mut record);
        if report_progress {
            rows_since_progress_check += 1;
            if rows_since_progress_check >= INGEST_PROGRESS_ROW_CHECK_INTERVAL {
                rows_since_progress_check = 0;
                let bytes_read = reader.position().byte().min(total_bytes);
                if bytes_read >= next_progress_bytes {
                    on_progress(bytes_read);
                    last_progress_bytes = bytes_read;
                    next_progress_bytes = bytes_read.saturating_add(progress_interval_bytes.max(1));
                }
            }
        }
        match read_result {
            Ok(true) => {
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
                std::mem::swap(&mut record, &mut last);
                total_records += 1;
            }
            Ok(false) => break,
            Err(_) => {
                dropped_records += 1;
                record.clear();
            }
        }
    }
    if report_progress && last_progress_bytes < total_bytes {
        on_progress(total_bytes);
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

    let trailing_total = total_records.saturating_sub(1) >= data_start
        && looks_like_total_row(&last.iter().collect::<Vec<_>>(), width);
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
        for (column, profile) in profiles.iter_mut().enumerate() {
            profile.unobserve(last.get(column).unwrap_or(""));
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

#[cfg(test)]
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

#[cfg(test)]
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

fn string_cell_to_sqlite(
    s: &str,
    ty: ColType,
    date_order: Option<NumericDateOrder>,
) -> rusqlite::types::Value {
    use rusqlite::types::Value as SqlValue;
    let s = s.trim();
    match ty {
        ColType::Text => {
            if s.is_empty() {
                SqlValue::Null
            } else {
                SqlValue::Text(s.to_string())
            }
        }
        _ if crate::engine::analytics::data::is_blankish(s) => SqlValue::Null,
        ColType::Int => s
            .parse::<i64>()
            .map(SqlValue::Integer)
            .unwrap_or(SqlValue::Null),
        ColType::Float => s
            .parse::<f64>()
            .ok()
            .or_else(|| crate::engine::analytics::data::parse_numeric(s))
            .and_then(serde_json::Number::from_f64)
            .map(|number| {
                number
                    .as_i64()
                    .map(SqlValue::Integer)
                    .unwrap_or_else(|| SqlValue::Real(number.as_f64().unwrap_or(0.0)))
            })
            .unwrap_or(SqlValue::Null),
        ColType::Bool => {
            if s.eq_ignore_ascii_case("true") {
                SqlValue::Integer(1)
            } else if s.eq_ignore_ascii_case("false") {
                SqlValue::Integer(0)
            } else {
                SqlValue::Null
            }
        }
        ColType::Date => parse_date_value_with_order(s, date_order)
            .map(SqlValue::Text)
            .unwrap_or(SqlValue::Null),
    }
}

#[cfg(test)]
fn json_cell(v: &Json, ty: ColType) -> Cell {
    json_cell_with_order(v, ty, None)
}

#[cfg(test)]
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

/// Convert parsed JSON directly to SQLite storage so each cell doesn't need a
/// temporary normalized `Json` value before binding. Keep behavior in lockstep
/// with `json_cell_with_order` plus `cell_to_sqlite`.
fn json_cell_to_sqlite(
    value: &Json,
    ty: ColType,
    date_order: Option<NumericDateOrder>,
) -> rusqlite::types::Value {
    use crate::engine::analytics::data::{is_blankish, parse_numeric};
    use rusqlite::types::Value as SqlValue;

    match (value, ty) {
        (Json::Null, _) => SqlValue::Null,
        (Json::String(s), ColType::Text) => {
            if s.trim().is_empty() {
                SqlValue::Null
            } else {
                SqlValue::Text(s.clone())
            }
        }
        (Json::String(s), _) if is_blankish(s) => SqlValue::Null,
        (Json::Bool(value), ColType::Bool) => SqlValue::Integer(*value as i64),
        (Json::Number(_), ColType::Int | ColType::Float) => cell_to_sqlite(value, ty),
        (Json::String(s), ColType::Float) => parse_numeric(s)
            .and_then(serde_json::Number::from_f64)
            .map(|number| {
                number
                    .as_i64()
                    .map(SqlValue::Integer)
                    .unwrap_or_else(|| SqlValue::Real(number.as_f64().unwrap_or(0.0)))
            })
            .unwrap_or(SqlValue::Null),
        (Json::String(s), ColType::Int) => parse_numeric(s)
            .map(|number| SqlValue::Integer(number as i64))
            .unwrap_or(SqlValue::Null),
        (Json::String(s), ColType::Date) => parse_date_value_with_order(s, date_order)
            .map(SqlValue::Text)
            .unwrap_or(SqlValue::Null),
        (Json::String(s), _) => SqlValue::Text(s.clone()),
        (value, ColType::Text) => SqlValue::Text(value.to_string()),
        _ => SqlValue::Null,
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

    struct ScratchDir(PathBuf);

    impl ScratchDir {
        fn new() -> Self {
            let nonce = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!("fella-sqlite-test-{nonce}"));
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for ScratchDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn rebuildable_workspace_database_uses_rollback_journal_mode() {
        let dir = ScratchDir::new();
        let engine = SqliteEngine::open(&dir.0).unwrap();
        let journal_mode: String = engine
            .conn
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .unwrap();

        assert_eq!(journal_mode, "delete");
    }

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
    fn direct_delimited_sqlite_cells_match_the_reference_conversion() {
        let cases = [
            ("", ColType::Text),
            ("N/A", ColType::Text),
            ("  label  ", ColType::Text),
            ("12", ColType::Int),
            ("bad", ColType::Int),
            ("$1,200.00", ColType::Float),
            ("NaN", ColType::Float),
            ("TRUE", ColType::Bool),
            ("unknown", ColType::Bool),
            ("Feb 3, 2024", ColType::Date),
            ("13/04/2024", ColType::Date),
            ("03/04/2024", ColType::Date),
            ("not a date", ColType::Date),
        ];
        for (value, kind) in cases {
            let date_order = (kind == ColType::Date).then_some(NumericDateOrder::DayFirst);
            let reference = cell_to_sqlite(
                &string_cell_with_order(value, kind, date_order),
                ColType::Text,
            );
            let direct = string_cell_to_sqlite(value, kind, date_order);
            assert_eq!(direct, reference, "cell: {value:?}, type: {kind:?}");
        }
    }

    #[test]
    fn direct_json_sqlite_cells_match_the_reference_conversion() {
        let cases = [
            (Json::Null, ColType::Text),
            (serde_json::json!(""), ColType::Text),
            (serde_json::json!("none"), ColType::Text),
            (serde_json::json!("  label  "), ColType::Text),
            (serde_json::json!("$1,200.00"), ColType::Float),
            (serde_json::json!("N/A"), ColType::Float),
            (serde_json::json!("1,200"), ColType::Int),
            (serde_json::json!("bad"), ColType::Int),
            (serde_json::json!("13/04/2024"), ColType::Date),
            (serde_json::json!("03/04/2024"), ColType::Date),
            (serde_json::json!(true), ColType::Bool),
            (serde_json::json!(false), ColType::Text),
            (serde_json::json!(true), ColType::Float),
            (serde_json::json!(123), ColType::Int),
            (serde_json::json!(12.5), ColType::Float),
            (serde_json::json!(12), ColType::Text),
            (serde_json::json!({"nested": 1}), ColType::Text),
            (serde_json::json!([1, 2]), ColType::Text),
        ];

        for (value, kind) in cases {
            let date_order = (kind == ColType::Date).then_some(NumericDateOrder::DayFirst);
            let reference = cell_to_sqlite(&json_cell_with_order(&value, kind, date_order), kind);
            let direct = json_cell_to_sqlite(&value, kind, date_order);
            assert_eq!(direct, reference, "cell: {value:?}, type: {kind:?}");
        }
    }

    #[test]
    fn streaming_json_profile_matches_full_column_type_inference() {
        let cases: &[&[Json]] = &[
            &[
                serde_json::json!(1),
                serde_json::json!(2),
                serde_json::json!(3),
            ],
            &[
                serde_json::json!(1),
                serde_json::json!(2.5),
                serde_json::json!(3),
            ],
            &[serde_json::json!(true), serde_json::json!(false)],
            &[
                serde_json::json!("$1,200.00"),
                serde_json::json!("1,150"),
                serde_json::json!("N/A"),
            ],
            &[
                serde_json::json!("03/04/2024"),
                serde_json::json!("13/04/2024"),
                serde_json::json!("07/25/2024"),
            ],
            &[
                serde_json::json!("$1,200"),
                serde_json::json!("bad"),
                serde_json::json!(800),
                serde_json::json!(725),
            ],
            &[
                serde_json::json!("2024-01-02"),
                serde_json::json!("Feb 3, 2024"),
                serde_json::json!("2024/03/04"),
            ],
            &[
                serde_json::json!("10"),
                serde_json::json!(true),
                serde_json::json!(20),
            ],
            &[
                serde_json::json!(""),
                Json::Null,
                serde_json::json!("none"),
                serde_json::json!("-"),
            ],
            &[
                serde_json::json!({"nested": 1}),
                serde_json::json!([1, 2]),
                serde_json::json!("text"),
            ],
        ];

        for values in cases {
            let date_order = infer_numeric_date_order(values.iter().filter_map(Json::as_str));
            let expected = sniff_json_with_order(values.iter(), date_order);
            let mut profile = JsonColumnProfile::default();
            for value in *values {
                profile.observe(value);
            }
            let actual = profile.finish();
            assert_eq!(actual.0, expected.0, "column values: {values:?}");
            assert_eq!(actual.1, expected.1, "column values: {values:?}");
            assert_eq!(actual.2, date_order, "column values: {values:?}");
        }
    }

    #[test]
    fn json_and_ndjson_readers_report_bounded_byte_progress() {
        let dir = ScratchDir::new();
        let cases = [
            ("records.json", false, r#"[{"x":1},{"x":2},{"x":3}]"#),
            ("records.ndjson", true, "{\"x\":1}\n{\"x\":2}\n{\"x\":3}\n"),
        ];

        for (name, ndjson, contents) in cases {
            let path = dir.0.join(name);
            std::fs::write(&path, contents).unwrap();
            let total_bytes = std::fs::metadata(&path).unwrap().len();
            let mut progress = Vec::new();
            let mut on_progress = |bytes_read| progress.push(bytes_read);
            let summary = visit_json_objects(
                path.to_str().unwrap(),
                ndjson,
                total_bytes,
                8,
                Some(&mut on_progress),
                |_| Ok(()),
            )
            .unwrap();

            assert_eq!(summary.objects, 3, "{name} retains all object rows");
            assert!(!progress.is_empty(), "{name} reports bytes read");
            assert!(
                progress.windows(2).all(|pair| pair[0] <= pair[1]),
                "{name} progress is monotonic"
            );
            assert!(progress.iter().all(|bytes| *bytes <= total_bytes));
            assert_eq!(progress.last(), Some(&total_bytes));
        }
    }

    #[test]
    fn json_source_ingest_reports_profiling_and_loading_progress() {
        let dir = ScratchDir::new();
        let records: Vec<Json> = (0..1_500)
            .map(|index| serde_json::json!({"index": index, "label": format!("sensor-{index}")}))
            .collect();
        let json = serde_json::to_vec(&records).unwrap();
        let ndjson = format!(
            "{}\n",
            records
                .iter()
                .map(serde_json::to_string)
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
                .join("\n")
        );

        for (index, (ndjson, contents)) in
            [(false, String::from_utf8(json).unwrap()), (true, ndjson)]
                .into_iter()
                .enumerate()
        {
            let path = dir.0.join(if ndjson {
                "records.ndjson"
            } else {
                "records.json"
            });
            std::fs::write(&path, &contents).unwrap();
            let total_bytes = contents.len() as u64;
            let database = dir.0.join(format!("database-{index}"));
            std::fs::create_dir_all(&database).unwrap();
            let mut engine = SqliteEngine::open(&database).unwrap();
            let mut progress = Vec::new();
            let mut on_progress = |update: SourceIngestProgress| {
                progress.push((update.stage, update.bytes_read));
            };
            let loaded = engine
                .add_json_source(
                    "records",
                    path.to_str().unwrap(),
                    JsonSourceOptions {
                        ndjson,
                        allow_progress: true,
                        progress_min_bytes: 1,
                        progress_interval_bytes: 1_024,
                    },
                    &mut on_progress,
                )
                .unwrap();

            assert_eq!(loaded.row_count, records.len() as i64);
            for stage in ["profiling", "loading"] {
                let bytes: Vec<u64> = progress
                    .iter()
                    .filter(|(observed_stage, _)| *observed_stage == stage)
                    .map(|(_, bytes_read)| *bytes_read)
                    .collect();
                assert!(bytes.len() > 2, "{stage} reports in-file progress");
                assert_eq!(bytes.first(), Some(&0), "{stage} reports its start");
                assert_eq!(
                    bytes.last(),
                    Some(&total_bytes),
                    "{stage} reports completion"
                );
                assert!(
                    bytes.windows(2).all(|window| window[0] <= window[1]),
                    "{stage} progress is monotonic"
                );
                assert!(bytes.iter().all(|read| *read <= total_bytes));
            }
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
