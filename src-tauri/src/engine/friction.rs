//! Local, telemetry-free log of moments the harness hit a wall: a `hard_fail`
//! that survives the corrective re-ask, or repeated tool errors within one
//! answer. Never transmitted — a plain JSONL file the user (or the
//! maintainer, dogfooding their own sessions) can choose to open. Deliberately
//! coarse: no question text, no answer text, no file paths, no SQL, no tool
//! names, no data values — only the trigger reason and small counts.

use std::path::{Path, PathBuf};

use serde_json::json;

use crate::engine::evidence::{EvidenceItem, VerificationCheck};
use crate::engine::verify;

/// Keep the log from growing unbounded on a long-lived install, same cap
/// style as `memory::record_episode`'s `.episodes.jsonl`.
const MAX_SIGNALS: usize = 500;

/// Which trigger, if any, this answer hits. `checks` is the *final* set (post
/// corrective re-ask); `evidence` this answer's tool calls. Pure so it's
/// testable without touching disk.
pub(crate) fn trigger(checks: &[VerificationCheck], evidence: &[EvidenceItem]) -> Option<&'static str> {
    if verify::hard_fail(checks).is_some() {
        return Some("hard_fail_unresolved");
    }
    if evidence.iter().filter(|e| e.error.is_some()).count() >= 2 {
        return Some("repeated_tool_errors");
    }
    None
}

fn signals_path(data_dir: &Path) -> PathBuf {
    data_dir.join("signals.jsonl")
}

/// Append one coarse record. Best-effort: a write failure here must never
/// fail the actual answer.
pub(crate) fn record(data_dir: &Path, reason: &str, evidence: &[EvidenceItem]) {
    let p = signals_path(data_dir);
    let line = json!({
        "at_ms": std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0),
        "reason": reason,
        "steps": evidence.len(),
        "errors": evidence.iter().filter(|e| e.error.is_some()).count(),
    });
    let mut lines: Vec<String> =
        std::fs::read_to_string(&p).unwrap_or_default().lines().map(str::to_string).collect();
    lines.push(line.to_string());
    let n = lines.len();
    if n > MAX_SIGNALS {
        lines.drain(0..n - MAX_SIGNALS);
    }
    let _ = std::fs::write(&p, lines.join("\n") + "\n");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(error: Option<&str>) -> EvidenceItem {
        EvidenceItem {
            tool: "run_sql".into(),
            args: serde_json::json!({}),
            note: None,
            sql: None,
            result_summary: String::new(),
            columns: None,
            rows: None,
            row_count: None,
            output: None,
            ms: 0,
            error: error.map(String::from),
        }
    }

    fn ok(label: &str) -> VerificationCheck {
        VerificationCheck { label: label.into(), ok: true, detail: None }
    }
    fn warn(label: &str) -> VerificationCheck {
        VerificationCheck { label: label.into(), ok: false, detail: None }
    }

    #[test]
    fn hard_fail_wins_over_repeated_errors() {
        let checks = vec![warn("a query behind this answer gives a different result now")];
        let evidence = vec![ev(Some("timeout")), ev(Some("timeout"))];
        assert_eq!(trigger(&checks, &evidence), Some("hard_fail_unresolved"));
    }

    #[test]
    fn repeated_tool_errors_fire_without_a_hard_fail() {
        let checks = vec![ok("every table named in a cited query exists")];
        let evidence = vec![ev(Some("no such table")), ev(Some("timeout"))];
        assert_eq!(trigger(&checks, &evidence), Some("repeated_tool_errors"));
    }

    #[test]
    fn one_error_alone_does_not_fire() {
        let checks = vec![ok("every table named in a cited query exists")];
        let evidence = vec![ev(Some("timeout")), ev(None)];
        assert_eq!(trigger(&checks, &evidence), None);
    }

    #[test]
    fn a_clean_answer_does_not_fire() {
        let checks = vec![ok("every number in the answer came from the data above")];
        let evidence = vec![ev(None), ev(None)];
        assert_eq!(trigger(&checks, &evidence), None);
    }

    #[test]
    fn record_appends_a_coarse_line_and_trims_to_the_cap() {
        let dir = std::env::temp_dir().join(format!("fella-friction-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        for _ in 0..(MAX_SIGNALS + 5) {
            record(&dir, "hard_fail_unresolved", &[ev(None)]);
        }
        let text = std::fs::read_to_string(signals_path(&dir)).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), MAX_SIGNALS);
        let first: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(first["reason"], "hard_fail_unresolved");
        assert_eq!(first["steps"], 1);
        // no question/answer/tool/file content anywhere in the record
        let mut keys: Vec<&str> = first.as_object().unwrap().keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(keys, vec!["at_ms", "errors", "reason", "steps"]);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
