//! The Python guest used by Fella's embedded analytics sandbox.
//!
//! This crate is deliberately a `wasm32-unknown-unknown` module rather than a
//! WASI program. It has no filesystem, network, environment, clock, or
//! subprocess imports. The host provides three narrow capabilities:
//!
//! - `fella._write(text, stream)` for captured stdout/stderr
//! - `fella._sql(query)` for a bounded, read-only workspace query
//! - a small entropy bridge used by RustPython's hash maps at startup
//!
//! The host ABI is kept small so the desktop runtime can use the same module
//! on Linux, macOS, and Windows.

use core::alloc::Layout;

use rustpython::vm::{
    compiler::Mode, convert::ToPyObject, pymodule, PyObjectRef, PyResult, VirtualMachine,
};

const SQL_RESPONSE_CAP: usize = 1024 * 1024;
const MAX_FORECAST_HORIZON: usize = 1000;
const MAX_BACKTEST_ORIGINS: usize = 10;
const MAX_BACKTEST_FORECASTS: usize = 1000;

/// Satisfy getrandom's bare-WASM build contract with one narrowly scoped host
/// import. RustPython's hash maps need entropy during interpreter startup; the
/// host import provides bytes from the OS CSPRNG, but gives the guest no data,
/// path, network, or process capability.
#[no_mangle]
unsafe extern "Rust" fn __getrandom_v03_custom(
    dest: *mut u8,
    len: usize,
) -> Result<(), getrandom::Error> {
    if len > i32::MAX as usize || (dest.is_null() && len != 0) {
        return Err(getrandom::Error::UNEXPECTED);
    }
    (fella_random(dest as i32, len as i32) == 0)
        .then_some(())
        .ok_or(getrandom::Error::UNEXPECTED)
}

#[link(wasm_import_module = "fella")]
unsafe extern "C" {
    #[link_name = "write"]
    fn fella_write(ptr: i32, len: i32, stream: i32);
    #[link_name = "sql"]
    fn fella_sql(query_ptr: i32, query_len: i32, out_ptr: i32, out_cap: i32) -> i32;
    #[link_name = "random"]
    fn fella_random(ptr: i32, len: i32) -> i32;
}

/// Allocate a byte range in the guest's linear memory for the host ABI. The
/// enclosing Wasmi Store owns the allocation and releases it when the Python
/// run ends.
#[no_mangle]
pub extern "C" fn alloc(len: i32) -> i32 {
    if len <= 0 {
        return 0;
    }
    let Ok(layout) = Layout::from_size_align(len as usize, 8) else {
        return 0;
    };
    unsafe { std::alloc::alloc(layout) as i32 }
}

/// Execute one user snippet. A zero return means success; a nonzero return is
/// a normal Python exception that the host turns into a failed tool result.
#[no_mangle]
pub extern "C" fn run(code_ptr: i32, code_len: i32) -> i32 {
    if code_ptr < 0 || code_len < 0 {
        return 2;
    }

    let code = unsafe { core::slice::from_raw_parts(code_ptr as *const u8, code_len as usize) };
    let Ok(code) = core::str::from_utf8(code) else {
        return 2;
    };

    let builder = rustpython::Interpreter::builder(Default::default());
    let module = fella::module_def(&builder.ctx);
    let interp = builder.add_native_module(module).build();

    let result = interp.enter(|vm| {
        let scope = vm.new_scope_with_builtins();
        run_source(vm, scope.clone(), BOOTSTRAP, "<fella bootstrap>")?;
        run_source(vm, scope, code, "<fella user code>")
    });

    match result {
        Ok(_) => 0,
        Err(exc) => {
            // The bootstrap has already redirected sys.stderr to the host, so
            // tracebacks are visible in the tool evidence panel.
            interp.enter(|vm| vm.print_exception(exc));
            1
        }
    }
}

fn run_source(
    vm: &VirtualMachine,
    scope: rustpython::vm::scope::Scope,
    source: &str,
    filename: &str,
) -> PyResult {
    let code = vm
        .compile(source, Mode::Exec, filename.to_owned())
        .map_err(|error| vm.new_syntax_error(&error, Some(source)))?;
    vm.run_code_obj(code, scope)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ForecastMethod {
    Naive,
    Mean,
    Drift,
    LinearTrend,
    SeasonalNaive,
}

impl ForecastMethod {
    fn parse(name: &str) -> Result<Self, String> {
        match name {
            "naive" => Ok(Self::Naive),
            "mean" => Ok(Self::Mean),
            "drift" => Ok(Self::Drift),
            "linear_trend" => Ok(Self::LinearTrend),
            "seasonal_naive" => Ok(Self::SeasonalNaive),
            _ => Err(
                "unsupported method; use naive, mean, drift, linear_trend, or seasonal_naive"
                    .to_owned(),
            ),
        }
    }
}

#[derive(Debug)]
struct BacktestResult {
    n_forecasts: usize,
    mae: f64,
    rmse: f64,
    method: String,
    baseline_method: String,
    horizon: usize,
    origins: usize,
    available_origins: usize,
    baseline_mae: f64,
    baseline_rmse: f64,
    mae_skill_vs_baseline: Option<f64>,
}

#[derive(Default)]
struct ErrorTotals {
    count: usize,
    absolute: f64,
    squared: f64,
}

impl ErrorTotals {
    fn push(&mut self, error: f64) {
        self.count += 1;
        self.absolute += error.abs();
        self.squared += error * error;
    }

    fn metrics(&self) -> (f64, f64) {
        let count = self.count as f64;
        (self.absolute / count, (self.squared / count).sqrt())
    }
}

/// Run the bounded rolling-origin forecast evaluation in native WASM code.
/// Keeping the per-point loop out of interpreted Python avoids spending the
/// sandbox's instruction budget on arithmetic and list bookkeeping.
fn rolling_backtest(
    values: &[f64],
    method_name: &str,
    horizon: usize,
    seasonal_period: Option<usize>,
    min_train: Option<usize>,
    baseline_name: &str,
) -> Result<BacktestResult, String> {
    if horizon == 0 {
        return Err("forecast horizon must be a positive integer".to_owned());
    }
    if horizon > MAX_FORECAST_HORIZON {
        return Err(
            "forecast horizon exceeds the helper limit of 1000; use a coarser time grain"
                .to_owned(),
        );
    }
    if values.len() < 2 {
        return Err("forecast needs at least two observed values".to_owned());
    }
    if values.iter().any(|value| !value.is_finite()) {
        return Err("forecast input contains a non-finite value".to_owned());
    }

    let mut training_size = min_train.unwrap_or_else(|| 3.max(seasonal_period.unwrap_or(1)));
    if training_size < 2 {
        return Err("min_train must be an integer of at least 2".to_owned());
    }
    if (method_name == "seasonal_naive" || baseline_name == "seasonal_naive")
        && seasonal_period.is_some()
    {
        training_size = training_size.max(seasonal_period.unwrap_or_default());
    }

    let available_origins = values
        .len()
        .checked_sub(training_size)
        .and_then(|remaining| remaining.checked_sub(horizon))
        .and_then(|remaining| remaining.checked_add(1))
        .unwrap_or(0);
    if available_origins == 0 {
        return Err(
            "not enough history for even one chronological holdout at this horizon".to_owned(),
        );
    }

    let method = ForecastMethod::parse(method_name)?;
    let baseline_method = ForecastMethod::parse(baseline_name)?;
    let max_origins = MAX_BACKTEST_ORIGINS.min((MAX_BACKTEST_FORECASTS / horizon).max(1));
    let first_origin = training_size;
    let last_origin = values.len() - horizon;
    let selected_origins = if available_origins <= max_origins {
        (first_origin..=last_origin).collect::<Vec<_>>()
    } else if max_origins == 1 {
        vec![last_origin]
    } else {
        let span = last_origin - first_origin;
        (0..max_origins)
            .map(|index| {
                first_origin + ((span as u128 * index as u128) / (max_origins - 1) as u128) as usize
            })
            .collect()
    };

    let mut prefix_sums = Vec::with_capacity(values.len() + 1);
    let mut prefix_weighted_sums = Vec::with_capacity(values.len() + 1);
    prefix_sums.push(0.0);
    prefix_weighted_sums.push(0.0);
    for (index, value) in values.iter().copied().enumerate() {
        prefix_sums.push(prefix_sums[index] + value);
        prefix_weighted_sums.push(prefix_weighted_sums[index] + index as f64 * value);
    }

    let same_method = method == baseline_method;
    let mut errors = ErrorTotals::default();
    let mut baseline_errors = ErrorTotals::default();
    for origin in &selected_origins {
        for lead in 0..horizon {
            let prediction = forecast_at(
                values,
                *origin,
                lead,
                method,
                seasonal_period,
                &prefix_sums,
                &prefix_weighted_sums,
            )?;
            let actual = values[*origin + lead];
            errors.push(actual - prediction);
            if !same_method {
                let baseline = forecast_at(
                    values,
                    *origin,
                    lead,
                    baseline_method,
                    seasonal_period,
                    &prefix_sums,
                    &prefix_weighted_sums,
                )?;
                baseline_errors.push(actual - baseline);
            }
        }
    }

    let (mae, rmse) = errors.metrics();
    let (baseline_mae, baseline_rmse) = if same_method {
        (mae, rmse)
    } else {
        baseline_errors.metrics()
    };

    Ok(BacktestResult {
        n_forecasts: errors.count,
        mae,
        rmse,
        method: method_name.to_owned(),
        baseline_method: baseline_name.to_owned(),
        horizon,
        origins: selected_origins.len(),
        available_origins,
        baseline_mae,
        baseline_rmse,
        mae_skill_vs_baseline: (baseline_mae > 0.0).then_some(1.0 - mae / baseline_mae),
    })
}

fn forecast_at(
    values: &[f64],
    count: usize,
    lead: usize,
    method: ForecastMethod,
    seasonal_period: Option<usize>,
    prefix_sums: &[f64],
    prefix_weighted_sums: &[f64],
) -> Result<f64, String> {
    let last = values[count - 1];
    let step = (lead + 1) as f64;
    Ok(match method {
        ForecastMethod::Naive => last,
        ForecastMethod::Mean => prefix_sums[count] / count as f64,
        ForecastMethod::Drift => {
            let slope = (last - values[0]) / (count - 1) as f64;
            last + slope * step
        }
        ForecastMethod::LinearTrend => {
            let count_f = count as f64;
            let sum_x = count_f * (count_f - 1.0) / 2.0;
            let sum_x_squared = count_f * (count_f - 1.0) * (2.0 * count_f - 1.0) / 6.0;
            let denominator = count_f * sum_x_squared - sum_x * sum_x;
            let slope = if denominator != 0.0 {
                (count_f * prefix_weighted_sums[count] - sum_x * prefix_sums[count]) / denominator
            } else {
                0.0
            };
            let intercept = (prefix_sums[count] - slope * sum_x) / count_f;
            slope * (count_f - 1.0 + step) + intercept
        }
        ForecastMethod::SeasonalNaive => {
            let period = seasonal_period
                .filter(|period| *period > 0)
                .ok_or_else(|| {
                    "seasonal_naive requires a positive integer seasonal_period".to_owned()
                })?;
            if count < period {
                return Err("seasonal_naive needs at least one complete observed season".to_owned());
            }
            values[count - period + (lead % period)]
        }
    })
}

fn backtest_to_python(result: BacktestResult, vm: &VirtualMachine) -> PyResult<PyObjectRef> {
    let dict = vm.ctx.new_dict();
    dict.set_item(
        "n_forecasts",
        (result.n_forecasts as u64).to_pyobject(vm),
        vm,
    )?;
    dict.set_item("mae", result.mae.to_pyobject(vm), vm)?;
    dict.set_item("rmse", result.rmse.to_pyobject(vm), vm)?;
    dict.set_item("method", result.method.to_pyobject(vm), vm)?;
    dict.set_item(
        "baseline_method",
        result.baseline_method.to_pyobject(vm),
        vm,
    )?;
    dict.set_item("horizon", (result.horizon as u64).to_pyobject(vm), vm)?;
    dict.set_item("origins", (result.origins as u64).to_pyobject(vm), vm)?;
    dict.set_item(
        "available_origins",
        (result.available_origins as u64).to_pyobject(vm),
        vm,
    )?;
    dict.set_item("baseline_mae", result.baseline_mae.to_pyobject(vm), vm)?;
    dict.set_item("baseline_rmse", result.baseline_rmse.to_pyobject(vm), vm)?;
    let skill = result
        .mae_skill_vs_baseline
        .map_or_else(|| vm.ctx.none(), |value| value.to_pyobject(vm));
    dict.set_item("mae_skill_vs_baseline", skill, vm)?;
    Ok(dict.into())
}

#[cfg(test)]
mod forecast_tests {
    use super::rolling_backtest;

    fn close(actual: f64, expected: f64) {
        assert!((actual - expected).abs() < 1e-10, "{actual} != {expected}");
    }

    #[test]
    fn rolling_backtest_uses_only_the_training_prefix() {
        let result = rolling_backtest(
            &[1.0, 2.0, 3.0, 4.0, 5.0, 100.0],
            "linear_trend",
            1,
            None,
            Some(5),
            "naive",
        )
        .expect("one chronological holdout should be available");

        assert_eq!(result.origins, 1);
        assert_eq!(result.available_origins, 1);
        assert_eq!(result.n_forecasts, 1);
        close(result.mae, 94.0);
        close(result.baseline_mae, 95.0);
    }

    #[test]
    fn rolling_backtest_reports_candidate_and_baseline_metrics() {
        let result = rolling_backtest(
            &[1.0, 2.0, 3.0, 4.0, 5.0, 6.0],
            "mean",
            1,
            None,
            Some(2),
            "naive",
        )
        .expect("four chronological holdouts should be available");

        assert_eq!(result.origins, 4);
        assert_eq!(result.n_forecasts, 4);
        close(result.mae, 2.25);
        close(result.baseline_mae, 1.0);
        close(
            result
                .mae_skill_vs_baseline
                .expect("baseline has nonzero error"),
            -1.25,
        );
    }

    #[test]
    fn long_horizon_backtest_respects_the_forecast_point_budget() {
        let values = (0..505).map(f64::from).collect::<Vec<_>>();
        let result = rolling_backtest(&values, "naive", 501, None, Some(2), "naive")
            .expect("one 501-step chronological holdout should be available");

        assert_eq!(result.available_origins, 3);
        assert_eq!(result.origins, 1);
        assert_eq!(result.n_forecasts, 501);
        close(result.mae, 251.0);
        close(result.baseline_mae, result.mae);
    }
}

#[pymodule]
mod fella {
    use super::*;

    #[pyfunction]
    fn _write(text: String, stream: i32) {
        unsafe { fella_write(text.as_ptr() as i32, text.len() as i32, stream) };
    }

    #[pyfunction]
    fn _rolling_origin_backtest(
        values: Vec<f64>,
        method: String,
        horizon: usize,
        seasonal_period: Option<usize>,
        min_train: Option<usize>,
        baseline_method: String,
        vm: &VirtualMachine,
    ) -> PyResult<PyObjectRef> {
        let result = rolling_backtest(
            &values,
            &method,
            horizon,
            seasonal_period,
            min_train,
            &baseline_method,
        )
        .map_err(|error| vm.new_value_error(error))?;
        backtest_to_python(result, vm)
    }

    #[pyfunction]
    fn _sql(query: String, vm: &VirtualMachine) -> PyResult<PyObjectRef> {
        if query.len() > 64 * 1024 {
            return Err(vm.new_exception_msg(
                vm.ctx.exceptions.value_error.to_owned(),
                "sql() query is too large".into(),
            ));
        }

        let query_ptr = query.as_ptr() as i32;
        let out_ptr = super::alloc(SQL_RESPONSE_CAP as i32);
        if out_ptr == 0 {
            return Err(vm.new_exception_msg(
                vm.ctx.exceptions.memory_error.to_owned(),
                "sql() could not allocate a response buffer".into(),
            ));
        }
        let size = unsafe {
            fella_sql(
                query_ptr,
                query.len() as i32,
                out_ptr,
                SQL_RESPONSE_CAP as i32,
            )
        };
        if size < 0 || size as usize > SQL_RESPONSE_CAP {
            return Err(vm.new_exception_msg(
                vm.ctx.exceptions.runtime_error.to_owned(),
                "sql() could not read the workspace result".into(),
            ));
        }

        let value = {
            let bytes = unsafe { core::slice::from_raw_parts(out_ptr as *const u8, size as usize) };
            serde_json::from_slice(bytes)
        };
        let value: serde_json::Value = value.map_err(|_| {
            vm.new_exception_msg(
                vm.ctx.exceptions.runtime_error.to_owned(),
                "sql() returned invalid data".into(),
            )
        })?;
        python_rows(value, vm)
    }
}

fn python_rows(value: serde_json::Value, vm: &VirtualMachine) -> PyResult<PyObjectRef> {
    let object = value.as_object().ok_or_else(|| {
        vm.new_exception_msg(
            vm.ctx.exceptions.runtime_error.to_owned(),
            "sql() returned an invalid result".into(),
        )
    })?;
    if let Some(error) = object.get("error").and_then(serde_json::Value::as_str) {
        return Err(vm.new_exception_msg(vm.ctx.exceptions.runtime_error.to_owned(), error.into()));
    }

    let columns = object
        .get("columns")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| vm.new_runtime_error("sql() returned no columns"))?;
    let rows = object
        .get("rows")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| vm.new_runtime_error("sql() returned no rows"))?;

    let mut result = Vec::with_capacity(rows.len());
    for row in rows {
        let cells = row
            .as_array()
            .ok_or_else(|| vm.new_runtime_error("sql() returned an invalid row"))?;
        let dict = vm.ctx.new_dict();
        for (index, column) in columns.iter().enumerate() {
            let Some(column) = column.as_str() else {
                continue;
            };
            let value = cells.get(index).cloned().unwrap_or(serde_json::Value::Null);
            dict.set_item(column, json_to_python(value, vm)?, vm)?;
        }
        result.push(dict.into());
    }
    Ok(vm.ctx.new_list(result).into())
}

fn json_to_python(value: serde_json::Value, vm: &VirtualMachine) -> PyResult<PyObjectRef> {
    Ok(match value {
        serde_json::Value::Null => vm.ctx.none(),
        serde_json::Value::Bool(value) => value.to_pyobject(vm),
        serde_json::Value::Number(value) => {
            if let Some(value) = value.as_i64() {
                value.to_pyobject(vm)
            } else if let Some(value) = value.as_u64() {
                value.to_pyobject(vm)
            } else if let Some(value) = value.as_f64() {
                value.to_pyobject(vm)
            } else {
                vm.ctx.none()
            }
        }
        serde_json::Value::String(value) => value.to_pyobject(vm),
        serde_json::Value::Array(values) => vm
            .ctx
            .new_list(
                values
                    .into_iter()
                    .map(|value| json_to_python(value, vm))
                    .collect::<PyResult<Vec<_>>>()?,
            )
            .into(),
        serde_json::Value::Object(values) => {
            let dict = vm.ctx.new_dict();
            for (key, value) in values {
                dict.set_item(key.as_str(), json_to_python(value, vm)?, vm)?;
            }
            dict.into()
        }
    })
}

/// Small Python-side compatibility layer. `sql()` deliberately returns a
/// list of dictionaries instead of pretending a third-party DataFrame package
/// is present inside the embedded runtime.
const BOOTSTRAP: &str = r#"
import sys as _sys
import fella as _fella

class _FellaStream:
    def __init__(self, stream):
        self._stream = stream

    def write(self, text):
        text = str(text)
        _fella._write(text, self._stream)
        return len(text)

    def flush(self):
        return None

_sys.stdout = _FellaStream(0)
_sys.stderr = _FellaStream(1)

def sql(query):
    """Run a read-only query against the mounted workspace."""
    return _fella._sql(query)
"#;
