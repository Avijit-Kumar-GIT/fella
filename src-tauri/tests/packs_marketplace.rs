//! `/packs install <id>`: fetch the catalog, download each file, verify its
//! SHA-256, write the pack. Its own test binary because it sets the
//! process-global `FELLA_CATALOG_URL` (same reasoning as `sql_timeout.rs`).

use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use fella_lib::engine::EngineState;

fn scratch(tag: &str) -> PathBuf {
    let n = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let p = std::env::temp_dir().join(format!("fella-{tag}-{n}"));
    fs::create_dir_all(&p).unwrap();
    p
}

/// A throwaway HTTP server: one canned response per exact path, served until
/// the process exits. Returns `http://127.0.0.1:<port>`.
fn serve(listener: TcpListener, routes: HashMap<String, (u16, Vec<u8>)>) {
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let mut reader = BufReader::new(
                stream.try_clone().expect("clone stream"),
            );
            let mut request_line = String::new();
            if reader.read_line(&mut request_line).is_err() {
                continue;
            }
            let path = request_line
                .split_whitespace()
                .nth(1)
                .unwrap_or("/")
                .to_string();
            loop {
                let mut h = String::new();
                if reader.read_line(&mut h).is_err() || h == "\r\n" || h.is_empty() {
                    break;
                }
            }
            let (status, body) = routes
                .get(&path)
                .cloned()
                .unwrap_or((404, b"not found".to_vec()));
            let head = format!(
                "HTTP/1.1 {status} X\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.write_all(&body);
            let _ = stream.flush();
        }
    });
}

const MANIFEST: &str = r#"{"schema":1,"id":"nord-mkt","kind":"theme","name":"Nord","version":"1.0.0","description":"d","payload":"theme.json"}"#;
const MANIFEST_SHA: &str = "5611b8ba0b3addc739dc11905114f969259a23ee0af8debd424171a146199f4b";
const PAYLOAD: &str = r##"{"--bg":"#101010","--text":"#fafafa"}"##;
const PAYLOAD_SHA: &str = "bcd6fd2af2022e2c37982bf5c21cae4272c03deded1498519af6861ebdb94cc9";

fn catalog(base: &str, payload_sha: &str) -> String {
    format!(
        r#"{{"schema":1,"packs":[
          {{"id":"nord-mkt","kind":"theme","name":"Nord","description":"d","author":"fella",
            "files":[
              {{"path":"fella-pack.json","url":"{base}/p/fella-pack.json","sha256":"{MANIFEST_SHA}"}},
              {{"path":"theme.json","url":"{base}/p/theme.json","sha256":"{payload_sha}"}}
            ]}}
        ]}}"#
    )
}

#[tokio::test]
async fn marketplace_install_verifies_hashes() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());

    let mut routes = HashMap::new();
    routes.insert("/catalog.json".into(), (200u16, catalog(&base, PAYLOAD_SHA).into_bytes()));
    routes.insert("/p/fella-pack.json".into(), (200, MANIFEST.as_bytes().to_vec()));
    routes.insert("/p/theme.json".into(), (200, PAYLOAD.as_bytes().to_vec()));
    // A second catalog whose theme.json hash is wrong.
    routes.insert(
        "/bad-catalog.json".into(),
        (200, catalog(&base, "deadbeef").into_bytes()),
    );
    serve(listener, routes);

    std::env::set_var("FELLA_CATALOG_URL", format!("{base}/catalog.json"));
    let data = scratch("pk-mkt-data");
    let engine = EngineState::new(&data).unwrap();

    // --- happy path ---
    let list = engine.packs_install("nord-mkt").await.unwrap();
    let p = list.iter().find(|p| p.id == "nord-mkt").expect("installed");
    assert_eq!(p.source, "marketplace");
    assert!(p.verified);
    assert_eq!(
        fs::read_to_string(data.join("extensions/nord-mkt/theme.json")).unwrap(),
        PAYLOAD
    );
    engine.packs_set_enabled("nord-mkt", true).unwrap();
    assert_eq!(
        engine.packs_theme().unwrap().get("--bg").map(String::as_str),
        Some("#101010")
    );

    // --- unknown id ---
    assert!(engine.packs_install("does-not-exist").await.is_err());

    // --- checksum mismatch: rejected, nothing written ---
    std::env::set_var("FELLA_CATALOG_URL", format!("{base}/bad-catalog.json"));
    let data2 = scratch("pk-mkt-data2");
    let engine2 = EngineState::new(&data2).unwrap();
    assert!(engine2.packs_install("nord-mkt").await.is_err());
    assert!(!data2.join("extensions/nord-mkt").exists());
    assert!(engine2.packs_list().is_empty());

    let _ = fs::remove_dir_all(&data);
    let _ = fs::remove_dir_all(&data2);
}
