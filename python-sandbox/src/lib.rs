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

#[pymodule]
mod fella {
    use super::*;

    #[pyfunction]
    fn _write(text: String, stream: i32) {
        unsafe { fella_write(text.as_ptr() as i32, text.len() as i32, stream) };
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
