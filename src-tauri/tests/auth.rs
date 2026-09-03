//! Provider selection + credential storage: the keychain-free `auth.json`
//! store, the one-shot migration of a legacy plaintext key, and the
//! `/login` / `/logout` surface.

use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use fella_lib::engine::EngineState;

fn scratch(tag: &str) -> PathBuf {
    let n = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let p = std::env::temp_dir().join(format!("fella-{tag}-{n}"));
    fs::create_dir_all(&p).unwrap();
    p
}

#[tokio::test]
async fn probe_ollama_targets_the_local_endpoint_and_never_reports_rejected() {
    let data = scratch("probe-ollama");
    let engine = EngineState::new(&data).unwrap();

    // Point the engine at a hosted provider it can't reach the unconditional
    // Ollama probe must still target localhost, not that provider.
    engine.set_api_key("openai", "sk-not-real").unwrap();

    let h = engine.probe_ollama().await;
    // Ollama needs no key, so a probe can be reachable-or-not but never a
    // 401/403 "rejected". (Reachability itself is environment-dependent.)
    assert!(!h.rejected, "local Ollama probe should never be a rejected key");

    let _ = fs::remove_dir_all(&data);
}

#[test]
fn set_key_switches_provider_and_persists_outside_the_db() {
    let data = scratch("auth-set");
    let engine = EngineState::new(&data).unwrap();

    // nothing configured yet
    assert_eq!(engine.settings().provider, "ollama");
    assert!(engine.list_providers().iter().any(|p| p.id == "vercel" && !p.authed));

    let s = engine.set_api_key("vercel", "  vk-123  ").unwrap();
    assert_eq!(s.provider, "vercel");
    assert_eq!(s.base_url, "https://ai-gateway.vercel.sh/v1");
    assert!(s.has_credential);
    // the row has no default chat model switching must not inherit ollama's
    assert_eq!(s.model, "");

    // the key lives in auth.json, not fella.db
    let auth = fs::read_to_string(data.join("auth.json")).unwrap();
    assert!(auth.contains("vk-123"));
    let db = fs::read(data.join("fella.db")).unwrap();
    assert!(!String::from_utf8_lossy(&db).contains("vk-123"));

    // a fresh engine over the same dir still sees it
    let again = EngineState::new(&data).unwrap();
    assert!(again.settings().has_credential);
    assert!(again.list_providers().iter().any(|p| p.id == "vercel" && p.authed && p.current));

    let _ = fs::remove_dir_all(&data);
}

#[test]
fn logout_forgets_only_that_provider() {
    let data = scratch("auth-logout");
    let engine = EngineState::new(&data).unwrap();

    engine.set_api_key("openai", "sk-openai").unwrap();
    engine.set_api_key("xai", "xai-key").unwrap();

    let s = engine.logout("openai").unwrap();
    // xai is still the active provider and still signed in
    assert_eq!(s.provider, "xai");
    assert!(s.has_credential);
    assert!(engine.list_providers().iter().any(|p| p.id == "openai" && !p.authed));
    assert!(engine.list_providers().iter().any(|p| p.id == "xai" && p.authed));

    let _ = fs::remove_dir_all(&data);
}

#[test]
fn a_keyed_provider_with_no_saved_key_reverts_to_local_on_startup() {
    let data = scratch("auth-reconcile");

    // First run: sign in to OpenAI, then delete the key file by hand
    // (a stale config left by an older build, or a hand-cleared auth.json).
    {
        let engine = EngineState::new(&data).unwrap();
        engine.set_api_key("openai", "sk-openai").unwrap();
        assert_eq!(engine.settings().provider, "openai");
    }
    fs::remove_file(data.join("auth.json")).unwrap();

    // Next run reconciles: keyed provider + no key -> back to the local default.
    let engine = EngineState::new(&data).unwrap();
    let s = engine.settings();
    assert_eq!(s.provider, "ollama");
    assert_eq!(s.base_url, "http://localhost:11434");
    assert_eq!(s.model, "llama3.1");

    let _ = fs::remove_dir_all(&data);
}

#[test]
fn an_unknown_stored_provider_reverts_to_local_on_startup() {
    let data = scratch("auth-reconcile-unknown");

    // A stray `/model provider <x>` from an older build leaves an id no
    // registry row matches; the sqlite layer doesn't validate it.
    {
        let engine = EngineState::new(&data).unwrap();
        let patch = serde_json::json!({ "provider": "some-old-gateway", "model": "x/y:free" });
        let s = engine.save_settings(patch.as_object().unwrap()).unwrap();
        assert_eq!(s.provider, "some-old-gateway");
    }

    // Next run can't use it and reverts to the local default.
    let engine = EngineState::new(&data).unwrap();
    assert_eq!(engine.settings().provider, "ollama");
    assert_eq!(engine.settings().model, "llama3.1");

    let _ = fs::remove_dir_all(&data);
}

#[test]
fn switching_provider_through_settings_moves_the_address_and_model() {
    let data = scratch("auth-switch-provider");
    let engine = EngineState::new(&data).unwrap();
    assert_eq!(engine.settings().provider, "ollama");

    // `/model provider openai` sends just { "provider": "openai" }.
    let patch = serde_json::json!({ "provider": "openai" });
    let s = engine.save_settings(patch.as_object().unwrap()).unwrap();

    assert_eq!(s.provider, "openai");
    assert_eq!(s.base_url, "https://api.openai.com/v1");
    assert_eq!(s.model, "gpt-4o-mini");

    let _ = fs::remove_dir_all(&data);
}

#[test]
fn logout_of_the_active_provider_resets_to_the_local_default() {
    let data = scratch("auth-logout-active");
    let engine = EngineState::new(&data).unwrap();

    engine.set_api_key("openai", "sk-openai").unwrap();
    let before = engine.settings();
    assert_eq!(before.provider, "openai");
    assert_eq!(before.base_url, "https://api.openai.com/v1");

    let s = engine.logout("openai").unwrap();
    // Back on the local default, with its address / model, and no stale key.
    assert_eq!(s.provider, "ollama");
    assert_eq!(s.base_url, "http://localhost:11434");
    assert_eq!(s.model, "llama3.1");
    assert!(!engine.list_providers().iter().any(|p| p.id == "openai" && p.authed));

    let _ = fs::remove_dir_all(&data);
}

#[test]
fn rejects_unknown_provider_and_keyless_provider() {
    let data = scratch("auth-reject");
    let engine = EngineState::new(&data).unwrap();

    assert!(engine.set_api_key("nope", "x").is_err());
    assert!(engine.set_api_key("ollama", "x").is_err()); // local, needs no key
    assert!(engine.set_api_key("openai", "   ").is_err()); // empty

    let _ = fs::remove_dir_all(&data);
}

#[test]
fn migrates_a_legacy_plaintext_key_out_of_the_settings_table() {
    let data = scratch("auth-migrate");

    // Simulate a pre-keychain install: a fella.db carrying provider +
    // api_key rows in the settings table.
    {
        let conn = rusqlite::Connection::open(data.join("fella.db")).unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);",
        )
        .unwrap();
        for (k, v) in [
            ("provider", "openai-compatible"),
            ("base_url", "https://example.test/v1"),
            ("model", "some-model"),
            ("api_key", "sk-legacy-secret"),
        ] {
            conn.execute("INSERT INTO settings (key, value) VALUES (?1, ?2)", (k, v))
                .unwrap();
        }
    }

    let engine = EngineState::new(&data).unwrap();

    // provider mapped onto the registry; credential now recognized
    let s = engine.settings();
    assert_eq!(s.provider, "openai-compatible"); // stored value is preserved
    assert!(s.has_credential);

    // key moved to auth.json, removed from the db
    let auth = fs::read_to_string(data.join("auth.json")).unwrap();
    assert!(auth.contains("sk-legacy-secret"));
    assert!(auth.contains("apikey:custom"));
    let conn = rusqlite::Connection::open(data.join("fella.db")).unwrap();
    let leftover: Option<String> = conn
        .query_row("SELECT value FROM settings WHERE key = 'api_key'", [], |r| r.get(0))
        .ok();
    assert_eq!(leftover, None);

    // idempotent: a second construction doesn't choke
    let _ = EngineState::new(&data).unwrap();

    let _ = fs::remove_dir_all(&data);
}
