//! `/update`'s version-check step against a fake GitHub releases API. Its
//! own test binary because it sets the process-global
//! `FELLA_RELEASE_API_URL` (same reasoning as packs_marketplace.rs); both
//! scenarios below share one test function rather than two, so two
//! concurrently-run tests can't race on that same env var.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;

use fella_lib::engine::update;

fn serve(listener: TcpListener, routes: HashMap<String, (u16, Vec<u8>)>) {
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));
            let mut request_line = String::new();
            if reader.read_line(&mut request_line).is_err() {
                continue;
            }
            let path = request_line.split_whitespace().nth(1).unwrap_or("/").to_string();
            loop {
                let mut h = String::new();
                if reader.read_line(&mut h).is_err() || h == "\r\n" || h.is_empty() {
                    break;
                }
            }
            let (status, body) = routes.get(&path).cloned().unwrap_or((404, b"not found".to_vec()));
            let head = format!(
                "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.write_all(&body);
            let _ = stream.flush();
        }
    });
}

fn start_server(routes: HashMap<String, (u16, Vec<u8>)>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    serve(listener, routes);
    base
}

#[tokio::test]
async fn reports_up_to_date_then_available_as_the_fake_release_changes() {
    // Normally installed once by EngineState::new(); this test builds a bare
    // client directly, so it needs the same one-time setup itself.
    let _ = rustls::crypto::ring::default_provider().install_default();
    let http = reqwest::Client::new();
    let current = env!("CARGO_PKG_VERSION");

    // Same version as this build: not available.
    let same_body = format!(r#"{{"tag_name":"v{current}","assets":[]}}"#).into_bytes();
    let base1 = start_server(HashMap::from([("/r1".to_string(), (200, same_body))]));
    std::env::set_var("FELLA_RELEASE_API_URL", format!("{base1}/r1"));
    let status = update::check(&http).await.unwrap();
    assert_eq!(status.current, current);
    assert_eq!(status.latest, current);
    assert!(!status.available, "same version must not be reported as available");

    // A clearly newer version: available.
    let newer_body = br#"{"tag_name":"v99.0.0","assets":[]}"#.to_vec();
    let base2 = start_server(HashMap::from([("/r2".to_string(), (200, newer_body))]));
    std::env::set_var("FELLA_RELEASE_API_URL", format!("{base2}/r2"));
    let status = update::check(&http).await.unwrap();
    assert_eq!(status.latest, "99.0.0");
    assert!(status.available, "a newer tag must be reported as available");

    // An older version (e.g. a stale cache, or a rollback): not available.
    let older_body = br#"{"tag_name":"v0.0.1","assets":[]}"#.to_vec();
    let base3 = start_server(HashMap::from([("/r3".to_string(), (200, older_body))]));
    std::env::set_var("FELLA_RELEASE_API_URL", format!("{base3}/r3"));
    let status = update::check(&http).await.unwrap();
    assert!(!status.available, "an older tag must not be reported as available");
}
