//! `run_python`: execute a short Python snippet against the workspace data.
//!
//! This is a guard rail, not a security sandbox the user is running their own
//! code against their own files on their own machine. We isolate best-effort:
//! a fresh temp cwd, a stripped environment, wall-clock + CPU + memory limits
//! on Unix, and captured/[capped] output. Not restricted: filesystem reads
//! outside the workspace, and network access (an unprivileged net namespace
//! needs a user namespace + uid mapping, which would break reading pip packages
//! from a mode-700 home). Treat a snippet as code the user chose to run.

use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::engine::data::PythonBridge;
use crate::engine::error::{EngineError, EngineResult};

const TIMEOUT: Duration = Duration::from_secs(20);
const OUTPUT_CAP: usize = 64 * 1024;
#[cfg(unix)]
const MEM_LIMIT_BYTES: u64 = 1024 * 1024 * 1024; // 1 GiB address space
#[cfg(unix)]
const CPU_LIMIT_SECS: u64 = 15;
#[cfg(unix)]
const FSIZE_LIMIT_BYTES: u64 = 64 * 1024 * 1024;

pub struct PyResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    pub ms: u64,
}

/// `bridge` is how the generated `sql()` helper reaches the workspace data —
/// a read-only SQLite file (default), or DuckDB file-reader expressions.
/// Directories to search for `python3`/`python`, and the `PATH` the sandboxed
/// child gets: common install locations (a no-op on platforms they don't
/// apply to) plus the caller's own `PATH`, so a Homebrew / Nix / pyenv /
/// python.org install not in the OS default path is still found. Built and
/// read with `join_paths`/`split_paths` the platform list separator (`:` on
/// Unix, `;` on Windows), never a hardcoded `:` a previous version used `:`
/// unconditionally, so Python was never found on Windows even when installed
/// (splitting `C:\Python312\...` on `:` doesn't yield a real directory).
fn search_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    #[cfg(unix)]
    dirs.extend(["/usr/bin", "/bin", "/usr/local/bin", "/opt/homebrew/bin"].map(PathBuf::from));
    if let Some(path) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&path));
    }
    dirs
}

fn child_path() -> std::ffi::OsString {
    std::env::join_paths(search_dirs()).unwrap_or_default()
}

pub fn run(code: &str, bridge: PythonBridge) -> EngineResult<PyResult> {
    let Some(python) = resolve_python() else {
        return Err(EngineError::msg(
            "This question needs Python, which isn't installed on this computer.",
        ));
    };

    let workdir = std::env::temp_dir().join(format!(
        "fella-py-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&workdir)
        .map_err(|e| EngineError::io("create python workdir", e))?;

    let script = build_script(code, &bridge);

    let mut cmd = Command::new(&python);
    cmd.arg("-I") // isolated: ignore env vars, user site, no implicit cwd on path
        .arg("-")
        .current_dir(&workdir)
        .env_clear()
        .env("PATH", child_path())
        .env("HOME", &workdir)
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .env("MPLBACKEND", "Agg")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    #[cfg(unix)]
    apply_rlimits(&mut cmd);

    let started = Instant::now();
    let mut child = cmd
        .spawn()
        .map_err(|e| EngineError::io("spawn python3", e))?;

    child
        .stdin
        .take()
        .unwrap()
        .write_all(script.as_bytes())
        .map_err(|e| EngineError::io("write python script", e))?;

    let mut out_pipe = child.stdout.take().unwrap();
    let mut err_pipe = child.stderr.take().unwrap();
    let out_h = std::thread::spawn(move || read_capped(&mut out_pipe));
    let err_h = std::thread::spawn(move || read_capped(&mut err_pipe));

    let mut timed_out = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break Some(s),
            Ok(None) => {
                if started.elapsed() > TIMEOUT {
                    let _ = child.kill();
                    let _ = child.wait();
                    timed_out = true;
                    break None;
                }
                std::thread::sleep(Duration::from_millis(40));
            }
            Err(_) => break None,
        }
    };

    let stdout = out_h.join().unwrap_or_default();
    let stderr = err_h.join().unwrap_or_default();
    let _ = std::fs::remove_dir_all(&workdir);

    Ok(PyResult {
        stdout,
        stderr,
        exit_code: status.and_then(|s| s.code()),
        timed_out,
        ms: started.elapsed().as_millis() as u64,
    })
}

/// First `python3` (or `python`) on `search_dirs()` that answers `--version`.
/// Returned as an absolute path so `run()` spawns exactly what it checked.
fn resolve_python() -> Option<PathBuf> {
    let names: &[&str] = if cfg!(windows) {
        &["python3.exe", "python.exe", "python3", "python"]
    } else {
        &["python3", "python"]
    };
    for dir in search_dirs() {
        for name in names {
            let cand = dir.join(name);
            let ok = Command::new(&cand)
                .arg("--version")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
            if ok {
                return Some(cand);
            }
        }
    }
    None
}

fn read_capped(r: &mut impl Read) -> String {
    let mut buf = Vec::with_capacity(8192);
    let _ = r.take(OUTPUT_CAP as u64).read_to_end(&mut buf);
    let mut rest = Vec::new();
    let _ = r.read_to_end(&mut rest); // drain so the child isn't stuck on a full pipe
    let mut s = String::from_utf8_lossy(&buf).into_owned();
    if !rest.is_empty() {
        s.push_str("\n…(output truncated)");
    }
    s
}

fn py_str(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

fn build_script(user_code: &str, bridge: &PythonBridge) -> String {
    // `sql(query)` returns a pandas DataFrame if pandas is installed, else a
    // list of dicts. The SQLite path needs nothing beyond the Python stdlib.
    let setup = match bridge {
        PythonBridge::SqliteFile(path) => format!(
            r#"import sqlite3 as _sqlite3
_con = _sqlite3.connect("file:" + {p} + "?mode=ro", uri=True)
_con.row_factory = _sqlite3.Row
def sql(q):
    "Run read-only SQL against the workspace tables."
    _rows = [dict(_r) for _r in _con.execute(q).fetchall()]
    try:
        import pandas as _pd
        return _pd.DataFrame(_rows)
    except ModuleNotFoundError:
        return _rows
"#,
            p = py_str(&path.to_string_lossy())
        ),
        #[cfg(feature = "duckdb")]
        PythonBridge::DuckReaders(views) => {
            let mut list = String::new();
            for (name, reader) in views {
                list.push_str(&format!("    ({}, {}),\n", py_str(name), py_str(reader)));
            }
            format!(
                r#"_VIEWS = [
{list}]
try:
    import duckdb as _duckdb
    _con = _duckdb.connect()
    for _n, _r in _VIEWS:
        try:
            _con.execute('CREATE VIEW "' + _n + '" AS SELECT * FROM ' + _r)
        except Exception as _e:
            print("fella: could not load table " + _n + ": " + str(_e), file=_sys.stderr)
    def sql(q):
        return _con.sql(q).df()
except ModuleNotFoundError as _e:
    print("fella: " + str(_e) + " -- sql() needs `pip install duckdb pandas`", file=_sys.stderr)
    def sql(q):
        raise RuntimeError("sql() needs the duckdb package")
"#
            )
        }
    };

    format!("import sys as _sys\n{setup}\n# ---- user code ----\n{user_code}\n")
}

#[cfg(unix)]
fn apply_rlimits(cmd: &mut Command) {
    use std::os::unix::process::CommandExt;
    // SAFETY: only async-signal-safe setrlimit calls run in the forked child.
    unsafe {
        cmd.pre_exec(|| {
            use nix::sys::resource::{setrlimit, Resource};
            let _ = setrlimit(Resource::RLIMIT_CPU, CPU_LIMIT_SECS, CPU_LIMIT_SECS);
            let _ = setrlimit(Resource::RLIMIT_AS, MEM_LIMIT_BYTES, MEM_LIMIT_BYTES);
            let _ = setrlimit(Resource::RLIMIT_FSIZE, FSIZE_LIMIT_BYTES, FSIZE_LIMIT_BYTES);
            Ok(())
        });
    }
}
