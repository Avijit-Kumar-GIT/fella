//! Exercises the JSON-lines process boundary Electron uses to reach the Rust
//! engine, independently of any GUI shell.

#[test]
fn sample_report_seeds_from_the_mounted_engine_over_stdio() {
    let scratch = Scratch::new();
    let folder = scratch.0.join("Northstar sample with spaces");
    let files = [
        ("catalog/products.csv", "product_id,category\np1,Packs\n"),
        ("commerce/orders/orders_2023.csv", "order_id,order_date,currency,status\na,2023-07-01,USD,paid\n"),
        ("commerce/orders/orders_2024.csv", "order_id,order_date,currency,status\nb,2024-07-01,USD,paid\n"),
        ("commerce/orders/orders_2025.csv", "order_id,order_date,currency,status\nc,2025-07-01,USD,paid\nd,2025-10-01,USD,paid\n"),
        ("commerce/order_lines.csv", "order_id,product_id,quantity,unit_price_usd,line_discount_usd\na,p1,1,10,0\nb,p1,1,20,0\nc,p1,1,30,0\nd,p1,1,40,0\n"),
        ("finance/refund_transactions.csv", "refund_date,amount_usd,status,reason\n2025-10-02,5,posted,return\n"),
        ("docs/metric_policy_current.md", "Merchandise sales minus posted refunds.\n"),
    ];
    for (relative, body) in files {
        let path = folder.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, body).unwrap();
    }
    let mut child = Sidecar(
        Command::new(env!("CARGO_BIN_EXE_fella"))
            .args(["--engine-stdio", "--data-dir"])
            .arg(scratch.0.join("data"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let mut stdin = child.0.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.0.stdout.take().unwrap());
    let catalog = request(
        &mut stdin,
        &mut stdout,
        1,
        "open_workspace",
        json!({"path": folder}),
    );
    let params = json!({"workspacePath": catalog["workspace"]});
    let report = request(
        &mut stdin,
        &mut stdout,
        2,
        "studio_seed_northstar_sample",
        params.clone(),
    );
    assert!(!report["placements"].as_array().unwrap().is_empty());
    let placement = &report["placements"][0];
    let resolved = request(
        &mut stdin,
        &mut stdout,
        3,
        "artifact_resolve_saved",
        json!({"artifactId": placement["artifact_id"], "revision": placement["revision"]}),
    );
    assert_eq!(resolved["run"]["workspace"], catalog["workspace"]);
    assert_eq!(resolved["results"].as_array().unwrap().len(), 4);
    for result in resolved["results"].as_array().unwrap() {
        assert!(!result["rows"].as_array().unwrap().is_empty());
    }
    assert_eq!(
        request(
            &mut stdin,
            &mut stdout,
            4,
            "studio_seed_northstar_sample",
            params
        )["id"],
        report["id"]
    );
    // Seeding must not mount a folder into the independent General engine.
    assert!(request(&mut stdin, &mut stdout, 5, "get_catalog", json!({}))["workspace"].is_null());
}

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
    let second_workspace = scratch.0.join("second-workspace");
    fs::create_dir_all(&workspace).expect("create workspace");
    fs::create_dir_all(&second_workspace).expect("create second workspace");
    fs::write(
        workspace.join("sales.csv"),
        "month,amount\n2025-01,120\n2025-02,180\n",
    )
    .expect("write analytical fixture");
    fs::write(
        second_workspace.join("sales.csv"),
        "month,amount\n2025-01,1000\n2025-02,2000\n",
    )
    .expect("write second analytical fixture");

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
    let second_catalog = request(
        &mut stdin,
        &mut stdout,
        3,
        "open_workspace",
        json!({ "path": second_workspace }),
    );
    assert_eq!(second_catalog["sources"][0]["name"], "sales.csv");
    let first_id = catalog["workspace"]
        .as_str()
        .expect("stable workspace identity");
    let second_id = second_catalog["workspace"]
        .as_str()
        .expect("second workspace identity");
    assert_ne!(first_id, second_id);
    let unbound_catalog = request(&mut stdin, &mut stdout, 4, "get_catalog", json!({}));
    assert!(unbound_catalog["workspace"].is_null());
    assert!(unbound_catalog["sources"].as_array().unwrap().is_empty());

    let view = catalog["sources"][0]["view"]
        .as_str()
        .expect("catalog gives a queryable view name");
    let result = request(
        &mut stdin,
        &mut stdout,
        5,
        "run_sql_direct",
        json!({ "workspaceId": first_id, "sql": format!("SELECT SUM(amount) AS total FROM \"{view}\"") }),
    );
    assert_eq!(result["rows"][0][0], 300);
    let second_view = second_catalog["sources"][0]["view"].as_str().unwrap();
    let second_result = request(
        &mut stdin,
        &mut stdout,
        6,
        "run_sql_direct",
        json!({ "workspaceId": second_id, "sql": format!("SELECT SUM(amount) AS total FROM \"{second_view}\"") }),
    );
    assert_eq!(second_result["rows"][0][0], 3000);
    let focused_catalog = request(
        &mut stdin,
        &mut stdout,
        7,
        "get_catalog",
        json!({ "workspaceId": first_id }),
    );
    assert_eq!(focused_catalog["workspace"], first_id);
    assert_eq!(focused_catalog["sources"][0]["row_count"], 2);

    drop(stdin);
    let status = child.0.wait().expect("wait for sidecar shutdown");
    assert!(status.success(), "sidecar exited with {status}");
}
