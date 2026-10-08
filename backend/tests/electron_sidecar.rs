//! Exercises the JSON-lines process boundary Electron uses to reach the Rust
//! engine, independently of any GUI shell.

use serde_json::{json, Value};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after the Unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("fella-electron-sidecar-{nonce}"));
        fs::create_dir_all(&path).expect("create isolated sidecar test directory");
        Self(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct Sidecar(Child);

impl Drop for Sidecar {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn request(
    stdin: &mut impl Write,
    stdout: &mut impl BufRead,
    id: u64,
    method: &str,
    params: Value,
) -> Value {
    serde_json::to_writer(
        &mut *stdin,
        &json!({ "id": id, "method": method, "params": params }),
    )
    .expect("serialize request");
    stdin.write_all(b"\n").expect("write request terminator");
    stdin.flush().expect("flush request");

    loop {
        let mut line = String::new();
        stdout.read_line(&mut line).expect("read sidecar response");
        assert!(
            !line.is_empty(),
            "sidecar closed stdout before replying to {method}"
        );
        let response: Value = serde_json::from_str(&line).expect("sidecar emits JSON lines");
        assert_eq!(response["id"], id);
        if response.get("event").is_some() {
            continue;
        }
        assert_eq!(response["ok"], true, "{method}: {}", response["error"]);
        return response["result"].clone();
    }
}

#[test]
fn electron_sidecar_opens_a_workspace_and_runs_a_read_only_query() {
    let scratch = Scratch::new();
    let data_dir = scratch.0.join("app-data");
    let workspace = scratch.0.join("workspace");
    fs::create_dir_all(&workspace).expect("create workspace");
    fs::write(
        workspace.join("sales.csv"),
        "month,amount\n2025-01,120\n2025-02,180\n",
    )
    .expect("write analytical fixture");

    let child = Command::new(env!("CARGO_BIN_EXE_fella"))
        .args(["--engine-stdio", "--data-dir"])
        .arg(&data_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("launch Electron's Rust sidecar");
    let mut child = Sidecar(child);
    let mut stdin = child.0.stdin.take().expect("sidecar stdin");
    let mut stdout = BufReader::new(child.0.stdout.take().expect("sidecar stdout"));

    assert_eq!(
        request(&mut stdin, &mut stdout, 1, "ping", json!({})),
        "pong"
    );
    let catalog = request(
        &mut stdin,
        &mut stdout,
        2,
        "open_workspace",
        json!({ "path": workspace }),
    );
    assert_eq!(catalog["sources"][0]["name"], "sales.csv");
    let view = catalog["sources"][0]["view"]
        .as_str()
        .expect("catalog gives a queryable view name");
    let result = request(
        &mut stdin,
        &mut stdout,
        3,
        "run_sql_direct",
        json!({ "sql": format!("SELECT SUM(amount) AS total FROM \"{view}\"") }),
    );
    assert_eq!(result["rows"][0][0], 300);

    drop(stdin);
    let status = child.0.wait().expect("wait for sidecar shutdown");
    assert!(status.success(), "sidecar exited with {status}");
}
