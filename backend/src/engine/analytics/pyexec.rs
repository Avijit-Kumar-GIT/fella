//! `run_python`: execute generated Python inside Fella's embedded sandbox.
//!
//! The Python interpreter is compiled once as a `wasm32-unknown-unknown`
//! module (`python-sandbox/`) and embedded in the desktop binary. Wasmi runs
//! that module without WASI imports, so the guest has no filesystem, network,
//! environment, clock, or subprocess capability. The host exposes only a
//! bounded stdout/stderr sink and a bounded read-only `sql()` bridge.
//!
//! This gives the personal app one capability model on Linux, macOS, and
//! Windows. The sandbox is still defense in depth: the Wasmi/RustPython
//! versions and the checked-in guest artifact are part of the trusted base.

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, OnceLock,
};
use std::time::{Duration, Instant};

use serde_json::json;
use wasmi::{
    Caller, CompilationMode, Config, Engine, Extern, Linker, Memory, Module, Store, StoreLimits,
    StoreLimitsBuilder, TypedResumableCall,
};

use crate::engine::analytics::chart::TabularResult;
use crate::engine::analytics::data::{self, PythonBridge, QueryOutcome};
use crate::engine::error::{EngineError, EngineResult};

// RustPython's VM initialization plus compilation is instruction-heavy when
// Wasmi fuel metering is enabled. This is a portable execution budget, not a
// wall clock; the SQL backend has its own query watchdog.
const FUEL: u64 = 1_000_000_000;
/// A resumable fuel slice bounds how long a pure-Python loop can ignore Stop.
/// Keep this conservative because a Python worker can run beside another
/// worker on a two-core machine. A larger slice can make cancellation miss its
/// UI budget when both workers are CPU-bound.
const FUEL_SLICE: u64 = 1_000_000;
const PYTHON_TIMEOUT_SECS: u64 = 60;
const CODE_CAP: usize = 64 * 1024;
const OUTPUT_CAP: usize = 64 * 1024;
const SQL_QUERY_CAP: usize = 64 * 1024;
const SQL_ROW_CAP: usize = 10_000;
const SQL_RESPONSE_CAP: usize = 1024 * 1024;
const SQL_TRACE_CAP: usize = 8;
const MEMORY_CAP_BYTES: usize = 256 * 1024 * 1024;
const STACK_CAP_BYTES: usize = 2 * 1024 * 1024;

// `build-python-sandbox.sh` (and the Electron build instructions) keep this
// artifact in the source tree so a clean desktop build does not need Python,
// WASI, or a platform-specific sandbox executable installed on the user's
// machine.
const PYTHON_WASM: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/resources/fella-python-sandbox.wasm"
));

fn sandbox_engine() -> &'static Engine {
    static ENGINE: OnceLock<Engine> = OnceLock::new();
    ENGINE.get_or_init(|| {
        let mut config = Config::default();
        config.consume_fuel(true);
        // Compile every guest function before execution starts. Wasmi's
        // default lazy translation can otherwise spend the execution slice
        // compiling a function discovered deep in RustPython, which makes a
        // valid resume fail with an out-of-fuel error before user code runs.
        config.compilation_mode(CompilationMode::Eager);
        config.set_max_stack_height(STACK_CAP_BYTES);
        Engine::new(&config)
    })
}

fn sandbox_module() -> EngineResult<Module> {
    static MODULE: OnceLock<Result<Module, String>> = OnceLock::new();
    match MODULE.get_or_init(|| {
        Module::new(sandbox_engine(), PYTHON_WASM).map_err(|error| error.to_string())
    }) {
        Ok(module) => Ok(module.clone()),
        Err(error) => Err(EngineError::msg(format!(
            "load embedded Python sandbox: {error}"
        ))),
    }
}

pub struct PyResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    /// The caller set Stop while the guest was running.
    pub cancelled: bool,
    /// Current Wasm linear-memory size at the end of this disposable run.
    /// Wasm memory only grows, so this is also the run's peak guest memory.
    pub guest_memory_bytes: usize,
    pub ms: u64,
    /// Read-only SQL calls made by the guest, captured with their bounded
    /// results so the runtime can replay Python-backed statistics.
    pub queries: Vec<PythonQueryTrace>,
    pub query_trace_complete: bool,
    /// Optional bounded table explicitly published by generated Python with
    /// `fella_table(columns, rows)` for reuse by charts.
    pub result_table: Option<TabularResult>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PythonQueryTrace {
    pub sql: String,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<serde_json::Value>>,
    pub row_count: usize,
    pub truncated: bool,
}

struct HostState {
    bridge: PythonBridge,
    stdout: CapturedOutput,
    stderr: CapturedOutput,
    limits: StoreLimits,
    cancel: Option<Arc<AtomicBool>>,
    queries: Vec<PythonQueryTrace>,
    query_trace_truncated: bool,
}

impl HostState {
    fn new(bridge: PythonBridge, cancel: Option<Arc<AtomicBool>>) -> Self {
        Self {
            bridge,
            stdout: CapturedOutput::default(),
            stderr: CapturedOutput::default(),
            limits: StoreLimitsBuilder::new()
                .memory_size(MEMORY_CAP_BYTES)
                // RustPython's frozen stdlib needs a few thousand table
                // elements at instantiation; this still prevents unbounded
                // table growth from user code.
                .table_elements(10_000)
                .instances(2)
                .tables(4)
                .memories(1)
                // A memory.grow can legitimately run out of the current
                // Wasmi fuel slice. Let that become a resumable fuel
                // boundary instead of translating it into the generic
                // GrowthOperationLimited trap.
                .trap_on_grow_failure(false)
                .build(),
            cancel,
            queries: Vec::new(),
            query_trace_truncated: false,
        }
    }
}

#[derive(Debug, Default)]
struct CapturedOutput {
    bytes: Vec<u8>,
    truncated: bool,
}

impl CapturedOutput {
    fn push(&mut self, bytes: &[u8]) {
        let remaining = OUTPUT_CAP.saturating_sub(self.bytes.len());
        let take = bytes.len().min(remaining);
        self.bytes.extend_from_slice(&bytes[..take]);
        if take < bytes.len() {
            self.truncated = true;
        }
    }

    fn mark_truncated(&mut self) {
        self.truncated = true;
    }

    fn into_text(self) -> String {
        let mut text = String::from_utf8_lossy(&self.bytes).into_owned();
        if self.truncated {
            text.push_str("\n…(output truncated)");
        }
        text
    }
}

/// Run one snippet with a fresh Wasmi Store. A fresh store is what makes the
/// guest's allocator and module state disposable between questions. The guest
/// invocation is resumable, which lets us inspect cancellation and a wall-clock
/// budget between fuel slices.
pub fn run(
    code: &str,
    bridge: PythonBridge,
    cancel: Option<Arc<AtomicBool>>,
) -> EngineResult<PyResult> {
    // Keep specialized statistical helpers out of ordinary Python snippets.
    // This is based on the generated program's referenced API, not on the
    // user's question or any properties of the mounted data.
    let forecast_helpers = if references_forecast_helper(code) {
        FORECAST_HELPERS
    } else {
        ""
    };
    let table_helpers = if code.contains("fella_table(") {
        TABLE_HELPERS
    } else {
        ""
    };
    let script = format!(
        "{STATS_HELPERS}\n{forecast_helpers}\n{table_helpers}\n# ---- user code ----\n{code}\n"
    );
    if script.len() > CODE_CAP {
        return Err(EngineError::msg(format!(
            "Python code is too large for the local sandbox ({} KiB maximum)",
            CODE_CAP / 1024
        )));
    }

    let started = Instant::now();
    let engine = sandbox_engine();
    let module = sandbox_module()?;

    let mut store = Store::new(engine, HostState::new(bridge, cancel.clone()));
    store.limiter(|state| &mut state.limits);
    store
        .set_fuel(FUEL)
        .map_err(|e| EngineError::msg(format!("configure Python sandbox fuel: {e}")))?;

    let mut linker = Linker::new(engine);
    linker
        .func_wrap(
            "fella",
            "write",
            |mut caller: Caller<'_, HostState>, ptr: i32, len: i32, stream: i32| {
                let Some(memory) = caller.get_export("memory").and_then(Extern::into_memory) else {
                    return;
                };
                if ptr < 0 || len < 0 {
                    return;
                }
                let original_len = len as usize;
                let read_len = original_len.min(OUTPUT_CAP);
                let Some((ptr, read_len)) = checked_range(ptr, read_len as i32, OUTPUT_CAP) else {
                    return;
                };
                let mut bytes = vec![0; read_len];
                if memory.read(&caller, ptr, &mut bytes).is_err() {
                    return;
                }
                if stream == 0 {
                    caller.data_mut().stdout.push(&bytes);
                    if original_len > read_len {
                        caller.data_mut().stdout.mark_truncated();
                    }
                } else {
                    caller.data_mut().stderr.push(&bytes);
                    if original_len > read_len {
                        caller.data_mut().stderr.mark_truncated();
                    }
                }
            },
        )
        .map_err(|e| EngineError::msg(format!("register Python output bridge: {e}")))?;
    linker
        .func_wrap(
            "fella",
            "random",
            |mut caller: Caller<'_, HostState>, ptr: i32, len: i32| -> i32 {
                let Some(memory) = caller.get_export("memory").and_then(Extern::into_memory) else {
                    return -1;
                };
                let Some((ptr, len)) = checked_range(ptr, len, 64 * 1024) else {
                    return -1;
                };
                let mut bytes = vec![0; len];
                if getrandom::fill(&mut bytes).is_err() {
                    return -1;
                }
                memory
                    .write(&mut caller, ptr, &bytes)
                    .map(|_| 0)
                    .unwrap_or(-1)
            },
        )
        .map_err(|e| EngineError::msg(format!("register Python entropy bridge: {e}")))?;
    linker
        .func_wrap(
            "fella",
            "sql",
            |mut caller: Caller<'_, HostState>,
             query_ptr: i32,
             query_len: i32,
             out_ptr: i32,
             out_cap: i32|
             -> i32 {
                let Some(memory) = caller.get_export("memory").and_then(Extern::into_memory) else {
                    return -1;
                };
                let Some((query_ptr, query_len)) =
                    checked_range(query_ptr, query_len, SQL_QUERY_CAP)
                else {
                    return -1;
                };
                let Some((out_ptr, out_cap)) = checked_range(out_ptr, out_cap, SQL_RESPONSE_CAP)
                else {
                    return -1;
                };

                let mut query = vec![0; query_len];
                if memory.read(&caller, query_ptr, &mut query).is_err() {
                    return -1;
                }
                let query = match std::str::from_utf8(&query) {
                    Ok(query) => query,
                    Err(_) => {
                        return write_json_error(
                            &memory,
                            &mut caller,
                            out_ptr,
                            out_cap,
                            "sql() query is not valid UTF-8",
                        )
                    }
                };

                let query_result =
                    query_bridge(&caller.data().bridge, query, caller.data().cancel.clone());
                if let Ok(result) = &query_result {
                    let host = caller.data_mut();
                    if host.queries.len() < SQL_TRACE_CAP {
                        host.queries.push(PythonQueryTrace {
                            sql: query.to_string(),
                            columns: result.columns.clone(),
                            rows: result.rows.clone(),
                            row_count: result.row_count,
                            truncated: result.truncated,
                        });
                    } else {
                        host.query_trace_truncated = true;
                    }
                };
                let response = match query_result {
                    Ok(result) => serde_json::to_vec(&result).unwrap_or_default(),
                    Err(error) => serde_json::to_vec(&json!({ "error": error.to_string() }))
                        .unwrap_or_default(),
                };
                if response.len() > out_cap {
                    return write_json_error(
                        &memory,
                        &mut caller,
                        out_ptr,
                        out_cap,
                        "sql() result is too large; add a narrower SELECT or LIMIT",
                    );
                }
                if memory.write(&mut caller, out_ptr, &response).is_err() {
                    return -1;
                }
                response.len() as i32
            },
        )
        .map_err(|e| EngineError::msg(format!("register Python SQL bridge: {e}")))?;

    let instance = linker
        .instantiate_and_start(&mut store, &module)
        .map_err(|e| EngineError::msg(format!("start embedded Python sandbox: {e}")))?;
    let memory = instance
        .get_export(&store, "memory")
        .and_then(Extern::into_memory)
        .ok_or_else(|| EngineError::msg("embedded Python sandbox has no exported memory"))?;
    let alloc = instance
        .get_typed_func::<i32, i32>(&store, "alloc")
        .map_err(|e| EngineError::msg(format!("embedded Python sandbox has no allocator: {e}")))?;
    let run_fn = instance
        .get_typed_func::<(i32, i32), i32>(&store, "run")
        .map_err(|e| EngineError::msg(format!("embedded Python sandbox has no runner: {e}")))?;
    let code_ptr = alloc
        .call(&mut store, script.len() as i32)
        .map_err(|e| EngineError::msg(format!("allocate Python input: {e}")))?;
    let Some((code_ptr, code_len)) = checked_range(code_ptr, script.len() as i32, CODE_CAP) else {
        return Err(EngineError::msg(
            "embedded Python sandbox returned an invalid input buffer",
        ));
    };
    memory
        .write(&mut store, code_ptr, script.as_bytes())
        .map_err(|e| EngineError::msg(format!("write Python input: {e}")))?;

    // Instantiation uses the full setup budget. Reset the store to a bounded
    // execution slice only after the interpreter has been created.
    store
        .set_fuel(FUEL_SLICE.min(FUEL))
        .map_err(|e| EngineError::msg(format!("configure Python execution slice: {e}")))?;
    let mut fuel_left = FUEL.saturating_sub(FUEL_SLICE.min(FUEL));
    let deadline = Instant::now()
        + Duration::from_secs(crate::engine::env::positive(
            "FELLA_PYTHON_TIMEOUT_SECS",
            PYTHON_TIMEOUT_SECS,
        ));
    let mut invocation = run_fn
        .call_resumable(&mut store, (code_ptr as i32, code_len as i32))
        .map_err(|e| EngineError::msg(format!("run embedded Python sandbox: {e}")))?;
    let mut cancelled = false;
    let mut timed_out = false;
    let mut host_error: Option<String> = None;
    let exit_code = loop {
        match invocation {
            TypedResumableCall::Finished(code) => break Some(code),
            TypedResumableCall::HostTrap(trap) => {
                host_error = Some(trap.host_error().to_string());
                break None;
            }
            TypedResumableCall::OutOfFuel(next) => {
                if cancel
                    .as_ref()
                    .is_some_and(|flag| flag.load(Ordering::Relaxed))
                {
                    cancelled = true;
                    break None;
                }
                if Instant::now() >= deadline {
                    timed_out = true;
                    break None;
                }
                let slice = fuel_left.min(FUEL_SLICE);
                if slice == 0 {
                    timed_out = true;
                    break None;
                }
                fuel_left -= slice;
                store
                    .set_fuel(slice)
                    .map_err(|e| EngineError::msg(format!("refill Python sandbox fuel: {e}")))?;
                invocation = next.resume(&mut store).map_err(|e| {
                    EngineError::msg(format!("resume embedded Python sandbox: {e}"))
                })?;
            }
        }
    };
    // Dropping the Store releases the guest input buffer and the remaining
    // RustPython heap in one cleanup boundary, including interrupted runs.
    let elapsed_ms = started.elapsed().as_millis() as u64;
    let guest_memory_bytes = memory.data_size(&store);
    let host = store.into_data();
    let queries = host.queries;
    let query_trace_complete = !host.query_trace_truncated;
    let (raw_stdout, stderr) = (host.stdout.into_text(), host.stderr.into_text());
    let result_table = extract_published_table(&raw_stdout);
    let stdout = visible_stdout(&raw_stdout);

    match (exit_code, cancelled, timed_out, host_error) {
        (Some(exit_code), false, false, None) => Ok(PyResult {
            stdout,
            stderr,
            exit_code: Some(exit_code),
            timed_out: false,
            cancelled: false,
            guest_memory_bytes,
            ms: elapsed_ms,
            queries,
            query_trace_complete,
            result_table,
        }),
        (exit_code, cancelled, timed_out, host_error) => {
            let mut stderr = stderr;
            if !stderr.is_empty() {
                stderr.push('\n');
            }
            if cancelled {
                stderr.push_str("Python stopped by you");
            } else if timed_out {
                stderr.push_str("Python stopped by the local sandbox execution limit");
            } else if let Some(error) = host_error {
                stderr.push_str("Python stopped inside the local sandbox: ");
                stderr.push_str(&error);
            } else {
                stderr.push_str("Python stopped inside the local sandbox");
            }
            Ok(PyResult {
                stdout,
                stderr,
                exit_code,
                timed_out,
                cancelled,
                guest_memory_bytes,
                ms: elapsed_ms,
                queries,
                query_trace_complete,
                result_table,
            })
        }
    }
}

const TABLE_MARKER: &str = "__FELLA_TABLE__:";

fn extract_published_table(stdout: &str) -> Option<TabularResult> {
    let mut records = stdout
        .lines()
        .filter_map(|line| line.strip_prefix(TABLE_MARKER));
    let encoded = records.next()?;
    if records.next().is_some() {
        return None;
    }
    let table: TabularResult = serde_json::from_str(encoded).ok()?;
    if table.columns.is_empty()
        || table.columns.len() > 64
        || table.rows.len() > 10_000
        || table
            .rows
            .iter()
            .any(|row| row.len() != table.columns.len())
    {
        return None;
    }
    Some(table)
}

fn visible_stdout(stdout: &str) -> String {
    stdout
        .lines()
        .filter(|line| !line.starts_with(TABLE_MARKER))
        .collect::<Vec<_>>()
        .join("\n")
}

fn references_forecast_helper(code: &str) -> bool {
    code.split(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))
        .any(|identifier| {
            matches!(
                identifier,
                "forecast_series" | "rolling_origin_backtest" | "forecast_error_bands"
            )
        })
}

fn checked_range(ptr: i32, len: i32, cap: usize) -> Option<(usize, usize)> {
    if ptr < 0 || len < 0 {
        return None;
    }
    let ptr = ptr as usize;
    let len = len as usize;
    (len <= cap).then_some((ptr, len))
}

fn write_json_error(
    memory: &Memory,
    caller: &mut Caller<'_, HostState>,
    out_ptr: usize,
    out_cap: usize,
    message: &str,
) -> i32 {
    let response = serde_json::to_vec(&json!({ "error": message })).unwrap_or_default();
    if response.len() > out_cap || memory.write(caller, out_ptr, &response).is_err() {
        return -1;
    }
    response.len() as i32
}

fn query_bridge(
    bridge: &PythonBridge,
    sql: &str,
    cancel: Option<Arc<AtomicBool>>,
) -> EngineResult<QueryOutcome> {
    data::ensure_read_only(sql)?;
    match bridge {
        PythonBridge::SqliteFile(path) => match cancel {
            Some(cancel) => {
                data::sqlite::query_read_only_cancellable(path, sql, SQL_ROW_CAP, cancel)
            }
            None => data::sqlite::query_read_only(path, sql, SQL_ROW_CAP),
        },
        #[cfg(feature = "duckdb")]
        PythonBridge::DuckReaders(readers) => {
            data::duck::query_read_only(readers, sql, SQL_ROW_CAP)
        }
    }
}

/// Small pure-Python analytics helpers. Keeping them in the preamble means the
/// guest can stay core-only: no standard-library package tree or third-party
/// data-science dependency is needed for the calculations this tool promises.
pub const STATS_HELPERS: &str = r#"
def median(values):
    "Median of a non-empty numeric sequence."
    values = sorted(values)
    n = len(values)
    if n == 0:
        raise ValueError("median needs at least one value")
    middle = n // 2
    if n % 2:
        return values[middle]
    return (values[middle - 1] + values[middle]) / 2

def stdev(values):
    "Sample standard deviation of a numeric sequence."
    values = list(values)
    n = len(values)
    if n < 2:
        raise ValueError("stdev needs at least two values")
    mean = sum(values) / n
    return (sum((value - mean) ** 2 for value in values) / (n - 1)) ** 0.5

def pearsonr(x, y):
    "Pearson correlation coefficient between two equal-length numeric sequences."
    n = len(x)
    if n != len(y) or n < 2:
        raise ValueError("pearsonr needs two equal-length sequences of at least 2 values")
    mx = sum(x) / n
    my = sum(y) / n
    cov = sum((a - mx) * (b - my) for a, b in zip(x, y))
    vx = sum((a - mx) ** 2 for a in x)
    vy = sum((b - my) ** 2 for b in y)
    denom = (vx * vy) ** 0.5
    return cov / denom if denom else 0.0

def linregress(x, y):
    "Simple linear regression. Returns (slope, intercept, r)."
    n = len(x)
    if n != len(y) or n < 2:
        raise ValueError("linregress needs two equal-length sequences of at least 2 values")
    mx = sum(x) / n
    my = sum(y) / n
    sxy = sum((a - mx) * (b - my) for a, b in zip(x, y))
    sxx = sum((a - mx) ** 2 for a in x)
    slope = sxy / sxx if sxx else 0.0
    intercept = my - slope * mx
    return slope, intercept, pearsonr(x, y)
"#;

/// Publish one machine-readable, scalar table without depending on a Python
/// JSON/dataframe package. The host parses this one bounded record; ordinary
/// stdout remains untouched and is still available for replay/debugging.
const TABLE_HELPERS: &str = r#"
def _fella_json_string(value):
    pieces = ['"']
    for character in value:
        code = ord(character)
        if character == '"': pieces.append('\\"')
        elif character == '\\': pieces.append('\\\\')
        elif character == '\n': pieces.append('\\n')
        elif character == '\r': pieces.append('\\r')
        elif character == '\t': pieces.append('\\t')
        elif code < 32: pieces.append('\\u%04x' % code)
        else: pieces.append(character)
    pieces.append('"')
    return ''.join(pieces)

def _fella_json(value):
    if value is None: return 'null'
    if type(value) is bool: return 'true' if value else 'false'
    if type(value) is int: return str(value)
    if type(value) is float:
        if value != value or value == float('inf') or value == float('-inf'):
            raise ValueError('fella_table cannot publish non-finite numbers')
        return repr(value)
    if type(value) is str: return _fella_json_string(value)
    if type(value) in (list, tuple):
        return '[' + ','.join(_fella_json(item) for item in value) + ']'
    raise TypeError('fella_table cells must be scalar strings, numbers, booleans, or None')

def fella_table(columns, rows):
    """Publish a bounded table; rows may be positional or keyed by column name."""
    columns = list(columns)
    if not columns or len(columns) > 64:
        raise ValueError('fella_table needs 1-64 columns')
    if any(type(column) is not str or not column.strip() for column in columns):
        raise ValueError('fella_table column names must be non-empty strings')
    rows = list(rows)
    if len(rows) > 10000:
        raise ValueError('fella_table supports at most 10000 rows')
    normalized_rows = []
    for row in rows:
        if isinstance(row, dict):
            if set(row.keys()) != set(columns):
                raise ValueError('dictionary rows must contain exactly the published column names')
            row = [row[column] for column in columns]
        else:
            row = list(row)
        normalized_rows.append(row)
    rows = normalized_rows
    if any(len(row) != len(columns) for row in rows):
        raise ValueError('fella_table rows must match the column count')
    payload = '{"columns":' + _fella_json(columns) + ',"rows":' + _fella_json(rows) + '}'
    print('__FELLA_TABLE__:' + payload)
    return len(rows)
"#;

/// Small, dependency-free time-series methods for model-authored analysis.
/// The model still chooses the source, time grain, filters, and method; these
/// helpers provide reproducible candidates and chronological evaluation.
pub const FORECAST_HELPERS: &str = r#"
_MAX_FORECAST_HORIZON = 1000
_MAX_BACKTEST_ORIGINS = 10
_MAX_BACKTEST_FORECASTS = 1000

def _forecast_values(values):
    result = []
    for value in values:
        if value is None:
            raise ValueError("forecast input contains a missing value; inspect and resolve missingness explicitly")
        number = float(value)
        if number != number or number == float("inf") or number == float("-inf"):
            raise ValueError("forecast input contains a non-finite value")
        result.append(number)
    if len(result) < 2:
        raise ValueError("forecast needs at least two observed values")
    return result

def _forecast_horizon(horizon):
    if type(horizon) is not int or horizon < 1:
        raise ValueError("forecast horizon must be a positive integer")
    if horizon > _MAX_FORECAST_HORIZON:
        raise ValueError("forecast horizon exceeds the helper limit of 1000; use a coarser time grain")
    return horizon

def forecast_series(values, horizon, method="naive", seasonal_period=None):
    """Forecast equally spaced numeric observations with a named simple method.

    Methods: naive (last value), mean, drift, linear_trend, seasonal_naive.
    Time parsing and equal spacing are the caller's responsibility.
    """
    values = _forecast_values(values)
    return _forecast_clean_series(values, horizon, method, seasonal_period)

def _forecast_clean_series(values, horizon, method, seasonal_period):
    horizon = _forecast_horizon(horizon)
    if method == "naive":
        return [values[-1] for _ in range(horizon)]
    if method == "mean":
        mean = sum(values) / len(values)
        return [mean for _ in range(horizon)]
    if method == "drift":
        slope = (values[-1] - values[0]) / (len(values) - 1)
        return [values[-1] + slope * step for step in range(1, horizon + 1)]
    if method == "linear_trend":
        slope, intercept, _ = linregress(list(range(len(values))), values)
        return [slope * (len(values) - 1 + step) + intercept
                for step in range(1, horizon + 1)]
    if method == "seasonal_naive":
        if type(seasonal_period) is not int or seasonal_period < 1:
            raise ValueError("seasonal_naive requires a positive integer seasonal_period")
        if len(values) < seasonal_period:
            raise ValueError("seasonal_naive needs at least one complete observed season")
        start = len(values) - seasonal_period
        return [values[start + (step % seasonal_period)] for step in range(horizon)]
    raise ValueError("unsupported method; use naive, mean, drift, linear_trend, or seasonal_naive")

def _forecast_prefix(values, end, horizon, method, seasonal_period,
                     prefix_sums, prefix_weighted_sums):
    """Forecast one historical prefix using O(1) work per point method."""
    horizon = _forecast_horizon(horizon)
    count = end
    if method == "naive":
        return [values[end - 1] for _ in range(horizon)]
    if method == "mean":
        mean = prefix_sums[end] / count
        return [mean for _ in range(horizon)]
    if method == "drift":
        slope = (values[end - 1] - values[0]) / (count - 1)
        return [values[end - 1] + slope * step for step in range(1, horizon + 1)]
    if method == "linear_trend":
        sum_x = count * (count - 1) / 2
        sum_x_squared = count * (count - 1) * (2 * count - 1) / 6
        denominator = count * sum_x_squared - sum_x * sum_x
        slope = ((count * prefix_weighted_sums[end] - sum_x * prefix_sums[end]) / denominator
                 if denominator else 0.0)
        intercept = (prefix_sums[end] - slope * sum_x) / count
        return [slope * (count - 1 + step) + intercept
                for step in range(1, horizon + 1)]
    if method == "seasonal_naive":
        if type(seasonal_period) is not int or seasonal_period < 1:
            raise ValueError("seasonal_naive requires a positive integer seasonal_period")
        if count < seasonal_period:
            raise ValueError("seasonal_naive needs at least one complete observed season")
        start = end - seasonal_period
        return [values[start + (step % seasonal_period)] for step in range(horizon)]
    raise ValueError("unsupported method; use naive, mean, drift, linear_trend, or seasonal_naive")

def _rolling_forecast_errors(values, method, horizon, seasonal_period, min_train,
                             baseline_method):
    values = _forecast_values(values)
    horizon = _forecast_horizon(horizon)
    if min_train is None:
        min_train = max(3, seasonal_period or 1)
    if type(min_train) is not int or min_train < 2:
        raise ValueError("min_train must be an integer of at least 2")
    if (method == "seasonal_naive" or baseline_method == "seasonal_naive") and seasonal_period is not None:
        min_train = max(min_train, seasonal_period)
    available_origins = len(values) - min_train - horizon + 1
    if available_origins < 1:
        raise ValueError("not enough history for even one chronological holdout at this horizon")

    first_origin = min_train
    last_origin = len(values) - horizon
    max_origins = min(_MAX_BACKTEST_ORIGINS, max(1, _MAX_BACKTEST_FORECASTS // horizon))
    if available_origins <= max_origins:
        selected_origins = list(range(first_origin, last_origin + 1))
    elif max_origins == 1:
        selected_origins = [last_origin]
    else:
        span = last_origin - first_origin
        selected_origins = [
            first_origin + (span * index // (max_origins - 1))
            for index in range(max_origins)
        ]

    if method in ("mean", "linear_trend") or baseline_method in ("mean", "linear_trend"):
        prefix_sums = [0.0]
        prefix_weighted_sums = [0.0]
        for index, value in enumerate(values):
            prefix_sums.append(prefix_sums[-1] + value)
            prefix_weighted_sums.append(prefix_weighted_sums[-1] + index * value)
    else:
        prefix_sums = None
        prefix_weighted_sums = None

    same_method = method == baseline_method
    errors = [[] for _ in range(horizon)]
    baseline_errors = errors if same_method else [[] for _ in range(horizon)]
    for origin in selected_origins:
        predictions = _forecast_prefix(
            values, origin, horizon, method, seasonal_period,
            prefix_sums, prefix_weighted_sums)
        baseline = predictions if same_method else _forecast_prefix(
            values, origin, horizon, baseline_method, seasonal_period,
            prefix_sums, prefix_weighted_sums)
        for lead in range(horizon):
            actual = values[origin + lead]
            error = actual - predictions[lead]
            errors[lead].append(error)
            if not same_method:
                baseline_errors[lead].append(actual - baseline[lead])
    return errors, baseline_errors, len(selected_origins), available_origins

def _forecast_error_metrics(errors):
    count = 0
    absolute_total = 0.0
    squared_total = 0.0
    for lead_errors in errors:
        for error in lead_errors:
            count += 1
            absolute_total += abs(error)
            squared_total += error * error
    if not count:
        raise ValueError("no chronological holdout errors were produced")
    return {
        "n_forecasts": count,
        "mae": absolute_total / count,
        "rmse": (squared_total / count) ** 0.5,
    }

def rolling_origin_backtest(values, method, horizon=1, seasonal_period=None, min_train=None,
                            baseline_method="naive"):
    """Compare a method with a baseline using expanding chronological origins."""
    values = _forecast_values(values)
    horizon = _forecast_horizon(horizon)
    if min_train is not None and (type(min_train) is not int or min_train < 2):
        raise ValueError("min_train must be an integer of at least 2")
    return _fella._rolling_origin_backtest(
        values, method, horizon, seasonal_period, min_train, baseline_method)

def _forecast_quantile(values, probability):
    ordered = sorted(values)
    position = (len(ordered) - 1) * probability
    lower = int(position)
    upper = min(lower + 1, len(ordered) - 1)
    fraction = position - lower
    return ordered[lower] * (1.0 - fraction) + ordered[upper] * fraction

def forecast_error_bands(values, method, horizon, level=0.8, seasonal_period=None,
                         min_train=None, min_errors=8):
    """Return empirical rolling-origin error bands, not guaranteed intervals.

    A band is omitted for any lead with too few calibration errors or no
    observed error spread; zero-width bands would imply unjustified certainty.
    """
    if type(min_errors) is not int or min_errors < 2:
        raise ValueError("min_errors must be an integer of at least 2")
    level = float(level)
    if level <= 0.5 or level >= 1.0:
        raise ValueError("level must be greater than 0.5 and less than 1")
    errors, _, origins, available_origins = _rolling_forecast_errors(
        values, method, horizon, seasonal_period, min_train, "naive")
    points = forecast_series(values, horizon, method, seasonal_period)
    tail = (1.0 - level) / 2.0
    steps = []
    for lead, point in enumerate(points):
        lead_errors = errors[lead]
        step = {"step": lead + 1, "point": point, "n_errors": len(lead_errors)}
        if len(lead_errors) < min_errors:
            step["lower"] = None
            step["upper"] = None
            step["reason"] = "too few rolling-origin errors to estimate a useful band"
        elif max(lead_errors) == min(lead_errors):
            step["lower"] = None
            step["upper"] = None
            step["reason"] = "rolling-origin errors have no observed spread"
        else:
            step["lower"] = point + _forecast_quantile(lead_errors, tail)
            step["upper"] = point + _forecast_quantile(lead_errors, 1.0 - tail)
            step["reason"] = None
        steps.append(step)
    return {
        "method": method,
        "level": level,
        "origins": origins,
        "available_origins": available_origins,
        "steps": steps,
        "interpretation": "empirical rolling-origin error bands, not guaranteed prediction intervals",
    }
"#;

#[cfg(test)]
mod tests {
    use super::{references_forecast_helper, visible_stdout, TABLE_MARKER};

    #[test]
    fn forecast_helpers_are_selected_by_python_api_use() {
        assert!(!references_forecast_helper("print(sum([1, 2, 3]))"));
        assert!(references_forecast_helper(
            "pred = forecast_series(values, 3)"
        ));
        assert!(references_forecast_helper(
            "metrics = rolling_origin_backtest(values, 'mean')"
        ));
        assert!(references_forecast_helper(
            "bands = forecast_error_bands(values, 'naive', 2)"
        ));
    }

    #[test]
    fn internal_published_table_record_is_not_user_facing_stdout() {
        let output = format!("printed summary\n{TABLE_MARKER}{{\"columns\":[],\"rows\":[]}}\n");
        assert_eq!(visible_stdout(&output), "printed summary");
    }
}
