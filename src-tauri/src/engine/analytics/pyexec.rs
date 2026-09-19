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
const MEMORY_CAP_BYTES: usize = 256 * 1024 * 1024;
const STACK_CAP_BYTES: usize = 2 * 1024 * 1024;

// `build-python-sandbox.sh` (and the Tauri build instructions) keep this
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
}

struct HostState {
    bridge: PythonBridge,
    stdout: CapturedOutput,
    stderr: CapturedOutput,
    limits: StoreLimits,
    cancel: Option<Arc<AtomicBool>>,
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
    let script = format!("{STATS_HELPERS}\n# ---- user code ----\n{code}\n");
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

                let response = match query_bridge(
                    &caller.data().bridge,
                    query,
                    caller.data().cancel.clone(),
                ) {
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
    let (stdout, stderr) = (host.stdout.into_text(), host.stderr.into_text());

    match (exit_code, cancelled, timed_out, host_error) {
        (Some(exit_code), false, false, None) => Ok(PyResult {
            stdout,
            stderr,
            exit_code: Some(exit_code),
            timed_out: false,
            cancelled: false,
            guest_memory_bytes,
            ms: elapsed_ms,
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
            })
        }
    }
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
