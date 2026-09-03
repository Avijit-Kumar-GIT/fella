//! DuckDB data-engine backend (Cargo feature `duckdb`). Ported from the
//! original `engine/duck.rs`; the connection-taking helpers are unchanged and
//! `DuckEngine` wraps them behind the `DataEngine` trait.

use duckdb::types::{TimeUnit, Value};
use duckdb::Connection;
use serde_json::Value as Json;

use crate::engine::catalog::{ColumnInfo, SourceKind};
use crate::engine::data::{
    quote_ident, quote_str, Cell, ColType, DataEngine, PythonBridge, QueryOutcome, SourceLoad,
};
use crate::engine::error::{EngineError, EngineResult};

pub struct DuckEngine {
    conn: Connection,
    /// (table, FROM-expression) for `python_bridge`. Only path-backed sources.
    readers: Vec<(String, String)>,
}

impl DuckEngine {
    pub fn open() -> EngineResult<Self> {
        let conn = Connection::open_in_memory()?;
        let _ = conn.execute_batch(
            "SET threads TO 4; SET memory_limit = '2GB'; SET enable_progress_bar = false;",
        );
        Ok(Self { conn, readers: Vec::new() })
    }
}

impl DataEngine for DuckEngine {
    fn add_source(&mut self, name: &str, kind: SourceKind, path: &str) -> EngineResult<SourceLoad> {
        let reader = reader_expr(kind, path)
            .ok_or_else(|| EngineError::msg("not a path-readable tabular source"))?;
        self.conn.execute_batch(&format!(
            "CREATE OR REPLACE VIEW {} AS SELECT * FROM {reader};",
            quote_ident(name)
        ))?;
        self.readers.retain(|(n, _)| n != name);
        self.readers.push((name.to_string(), reader));
        Ok(SourceLoad {
            row_count: row_count(&self.conn, name)?,
            columns: columns(&self.conn, name)?,
            // DuckDB's read_csv_auto handles delimiter/BOM/encoding itself.
            note: None,
        })
    }

    fn add_rows(
        &mut self,
        name: &str,
        cols: &[(String, ColType)],
        rows: &[Vec<Cell>],
    ) -> EngineResult<i64> {
        let ident = quote_ident(name);
        let cols_sql = cols
            .iter()
            .map(|(c, t)| format!("{} {}", quote_ident(c), t.duckdb()))
            .collect::<Vec<_>>()
            .join(", ");
        self.conn.execute_batch(&format!(
            "DROP VIEW IF EXISTS {ident}; DROP TABLE IF EXISTS {ident}; CREATE TABLE {ident} ({cols_sql});"
        ))?;
        {
            let mut app = self.conn.appender(name)?;
            for row in rows {
                let vals: Vec<Value> = (0..cols.len())
                    .map(|i| cell_to_duck(row.get(i).unwrap_or(&Json::Null), cols[i].1))
                    .collect();
                app.append_row(duckdb::appender_params_from_iter(vals))?;
            }
            app.flush()?;
        }
        Ok(rows.len() as i64)
    }

    fn drop_source(&mut self, name: &str) {
        let ident = quote_ident(name);
        let _ = self
            .conn
            .execute_batch(&format!("DROP VIEW IF EXISTS {ident}; DROP TABLE IF EXISTS {ident};"));
        self.readers.retain(|(n, _)| n != name);
    }

    fn describe(&self, name: &str) -> EngineResult<Vec<ColumnInfo>> {
        describe(&self.conn, name)
    }

    fn query(&self, sql: &str, max_rows: usize) -> EngineResult<QueryOutcome> {
        query(&self.conn, sql, max_rows)
    }

    fn python_bridge(&self) -> PythonBridge {
        PythonBridge::DuckReaders(self.readers.clone())
    }
}

// --- reader expressions (also used to build the Python bridge) -------------

pub fn reader_expr(kind: SourceKind, path: &str) -> Option<String> {
    let p = quote_str(path);
    Some(match kind {
        SourceKind::Csv => format!("read_csv_auto({p}, sample_size = -1)"),
        SourceKind::Tsv => format!(r"read_csv_auto({p}, sample_size = -1, delim = '\t')"),
        SourceKind::Parquet => format!("read_parquet({p})"),
        SourceKind::Json => format!("read_json_auto({p})"),
        SourceKind::Ndjson => format!("read_json_auto({p}, format = 'newline_delimited')"),
        SourceKind::Xlsx | SourceKind::Pdf | SourceKind::Text => return None,
    })
}

// --- connection helpers (unchanged from the original engine/duck.rs) ------

fn row_count(conn: &Connection, view: &str) -> EngineResult<i64> {
    Ok(conn.query_row(&format!("SELECT count(*) FROM {}", quote_ident(view)), [], |r| {
        r.get::<_, i64>(0)
    })?)
}

// KNOWN GAP (parity with the SQLite backend): the SQLite path reads delimited
// files itself and coerces text-that-is-really-numbers ("$1,200.00", "1 200",
// "(45)") into real numeric columns at ingest, tagging the column with a
// `ColumnInfo.note`. DuckDB delegates to `read_csv_auto`, so the equivalent here
// is a post-load view rewrite (`TRY_CAST` over `regexp_replace`) plus the same
// note. Not done yet: this feature is `--features duckdb`, which nothing builds
// or tests (local box OOMs, CI skips it), so unverified parsing logic is not
// worth shipping blind. Until then the `run_sql` TEXT-aggregation warning
// (engine/tools.rs) and `describe_schema` still steer the model to CAST.
fn columns(conn: &Connection, view: &str) -> EngineResult<Vec<ColumnInfo>> {
    let out = query(conn, &format!("DESCRIBE SELECT * FROM {}", quote_ident(view)), 10_000)?;
    let ix = col_index(&out.columns);
    Ok(out
        .rows
        .iter()
        .map(|row| ColumnInfo {
            name: str_at(row, ix.get("column_name")).unwrap_or_default(),
            type_: str_at(row, ix.get("column_type")).unwrap_or_default(),
            null_fraction: None,
            distinct: None,
            min: None,
            max: None,
            example: None,
            note: None,
        })
        .collect())
}

fn describe(conn: &Connection, view: &str) -> EngineResult<Vec<ColumnInfo>> {
    let out = query(conn, &format!("SUMMARIZE SELECT * FROM {}", quote_ident(view)), 100_000)?;
    let ix = col_index(&out.columns);
    let mut cols = Vec::new();
    for row in &out.rows {
        let null_pct = str_at(row, ix.get("null_percentage"))
            .and_then(|s| s.trim_end_matches('%').trim().parse::<f64>().ok());
        cols.push(ColumnInfo {
            name: str_at(row, ix.get("column_name")).unwrap_or_default(),
            type_: str_at(row, ix.get("column_type")).unwrap_or_default(),
            null_fraction: null_pct.map(|p| (p / 100.0 * 1e6).round() / 1e6),
            distinct: str_at(row, ix.get("approx_unique")).and_then(|s| s.parse::<i64>().ok()),
            min: str_at(row, ix.get("min")),
            max: str_at(row, ix.get("max")),
            example: None,
            note: None,
        });
    }
    Ok(cols)
}

fn query(conn: &Connection, sql: &str, max_rows: usize) -> EngineResult<QueryOutcome> {
    let mut stmt = conn.prepare(sql)?;
    let mut rows = stmt.query([])?;
    let (columns, column_count): (Vec<String>, usize) = {
        let s = rows.as_ref().expect("statement is live after query()");
        (s.column_names(), s.column_count())
    };
    let mut rows_out: Vec<Vec<Json>> = Vec::new();
    let mut total = 0usize;
    let mut truncated = false;
    while let Some(row) = rows.next()? {
        total += 1;
        if rows_out.len() >= max_rows {
            truncated = true;
            continue;
        }
        let mut cells = Vec::with_capacity(column_count);
        for i in 0..column_count {
            let owned: Value = row.get_ref(i)?.into();
            cells.push(value_to_json(&owned));
        }
        rows_out.push(cells);
    }
    Ok(QueryOutcome { columns, rows: rows_out, row_count: total, truncated })
}

fn col_index(columns: &[String]) -> std::collections::HashMap<String, usize> {
    columns.iter().enumerate().map(|(i, c)| (c.to_lowercase(), i)).collect()
}

fn str_at(row: &[Json], idx: Option<&usize>) -> Option<String> {
    match row.get(*idx?)? {
        Json::Null => None,
        Json::String(s) => Some(s.clone()),
        other => Some(other.to_string()),
    }
}

fn json_f64(f: f64) -> Json {
    serde_json::Number::from_f64(f).map(Json::Number).unwrap_or(Json::Null)
}

fn cell_to_duck(v: &Json, ty: ColType) -> Value {
    match (v, ty) {
        (Json::Null, _) => Value::Null,
        (Json::Bool(b), ColType::Bool) => Value::Boolean(*b),
        (Json::Number(n), ColType::Int) => n.as_i64().map(Value::BigInt).unwrap_or(Value::Null),
        (Json::Number(n), ColType::Float) => n.as_f64().map(Value::Double).unwrap_or(Value::Null),
        (Json::String(s), _) => Value::Text(s.clone()),
        (other, ColType::Text) => Value::Text(other.to_string()),
        _ => Value::Null,
    }
}

fn value_to_json(v: &Value) -> Json {
    match v {
        Value::Null => Json::Null,
        Value::Boolean(b) => Json::Bool(*b),
        Value::TinyInt(n) => Json::from(*n),
        Value::SmallInt(n) => Json::from(*n),
        Value::Int(n) => Json::from(*n),
        Value::BigInt(n) => Json::from(*n),
        Value::UTinyInt(n) => Json::from(*n),
        Value::USmallInt(n) => Json::from(*n),
        Value::UInt(n) => Json::from(*n),
        Value::UBigInt(n) => Json::from(*n),
        Value::HugeInt(n) => i64::try_from(*n).map(Json::from).unwrap_or_else(|_| Json::from(n.to_string())),
        Value::UHugeInt(n) => u64::try_from(*n).map(Json::from).unwrap_or_else(|_| Json::from(n.to_string())),
        Value::Float(f) => json_f64(*f as f64),
        Value::Double(f) => json_f64(*f),
        Value::Decimal(d) => Json::from(d.to_string()),
        Value::Text(s) => Json::from(s.clone()),
        Value::Blob(b) => Json::from(format!("<{} bytes>", b.len())),
        Value::Geometry(b) => Json::from(format!("<geometry, {} bytes>", b.len())),
        Value::Timestamp(unit, n) => Json::from(fmt_timestamp(*unit, *n)),
        Value::Date32(d) => Json::from(fmt_date(*d as i64)),
        Value::Time64(unit, n) => Json::from(fmt_time(*unit, *n)),
        Value::Interval { months, days, nanos } => Json::from(fmt_interval(*months, *days, *nanos)),
        Value::List(xs) | Value::Array(xs) => Json::Array(xs.iter().map(value_to_json).collect()),
        Value::Enum(s) => Json::from(s.clone()),
        Value::Struct(m) => Json::Object(m.iter().map(|(k, v)| (k.clone(), value_to_json(v))).collect()),
        Value::Map(m) => Json::Array(
            m.iter()
                .map(|(k, v)| serde_json::json!({ "key": value_to_json(k), "value": value_to_json(v) }))
                .collect(),
        ),
        Value::Union(inner) => value_to_json(inner),
        other => Json::from(format!("{other:?}")),
    }
}

fn days_to_ymd(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

fn fmt_date(days: i64) -> String {
    let (y, m, d) = days_to_ymd(days);
    format!("{y:04}-{m:02}-{d:02}")
}

fn fmt_timestamp(unit: TimeUnit, v: i64) -> String {
    let per_sec: i64 = match unit {
        TimeUnit::Second => 1,
        TimeUnit::Millisecond => 1_000,
        TimeUnit::Microsecond => 1_000_000,
        TimeUnit::Nanosecond => 1_000_000_000,
    };
    let secs = v.div_euclid(per_sec);
    let frac = v.rem_euclid(per_sec);
    let days = secs.div_euclid(86_400);
    let tod = secs.rem_euclid(86_400);
    let (y, m, d) = days_to_ymd(days);
    let (hh, mm, ss) = (tod / 3600, (tod % 3600) / 60, tod % 60);
    let mut s = format!("{y:04}-{m:02}-{d:02} {hh:02}:{mm:02}:{ss:02}");
    let micros = match unit {
        TimeUnit::Second => 0,
        TimeUnit::Millisecond => frac * 1000,
        TimeUnit::Microsecond => frac,
        TimeUnit::Nanosecond => frac / 1000,
    };
    if micros != 0 {
        s.push_str(&format!(".{micros:06}"));
    }
    s
}

fn fmt_time(unit: TimeUnit, v: i64) -> String {
    let micros = unit.to_micros(v).rem_euclid(86_400_000_000);
    let secs = micros / 1_000_000;
    let (hh, mm, ss) = (secs / 3600, (secs % 3600) / 60, secs % 60);
    let rem = micros % 1_000_000;
    if rem != 0 {
        format!("{hh:02}:{mm:02}:{ss:02}.{rem:06}")
    } else {
        format!("{hh:02}:{mm:02}:{ss:02}")
    }
}

fn fmt_interval(months: i32, days: i32, nanos: i64) -> String {
    let mut parts = Vec::new();
    if months != 0 {
        parts.push(format!("{months} months"));
    }
    if days != 0 {
        parts.push(format!("{days} days"));
    }
    if nanos != 0 {
        parts.push(format!("{} seconds", nanos as f64 / 1e9));
    }
    if parts.is_empty() {
        "0 seconds".into()
    } else {
        parts.join(" ")
    }
}
