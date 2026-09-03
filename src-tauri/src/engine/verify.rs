//! Deterministic post-answer checks. Cheap, no extra LLM call:
//!   1. every table named in a cited query exists in the catalog
//!   2. re-running each cited query still gives the same result
//!   3. every number in the answer appears in some tool result

use std::collections::HashSet;

use serde_json::Value as Json;

use crate::engine::evidence::{EvidenceItem, VerificationCheck};
use crate::engine::state::EngineState;

pub fn run(engine: &EngineState, answer: &str, evidence: &[EvidenceItem]) -> Vec<VerificationCheck> {
    let mut checks = Vec::new();

    check_tables(engine, evidence, &mut checks);
    rerun_queries(engine, evidence, &mut checks);
    check_numbers(answer, evidence, &mut checks);
    check_text_agg(engine, evidence, &mut checks);

    checks
}

// --- 4. aggregates over a text column --------------------------------------

/// `(lowercased, original-case)` names of every catalogued `TEXT` column.
/// Shared by the `run_sql` tool's inline warning and `check_text_agg` so the
/// "collect the text columns" logic lives in one place.
pub(crate) fn text_columns(engine: &EngineState) -> Vec<(String, String)> {
    engine
        .catalog()
        .sources
        .iter()
        .filter_map(|s| s.columns.as_ref())
        .flatten()
        .filter(|c| c.type_.eq_ignore_ascii_case("text"))
        .map(|c| (c.name.to_lowercase(), c.name.clone()))
        .collect()
}

/// If `sql` applies `SUM`/`AVG`/`TOTAL` to one of `text_cols` (already
/// lowercased), return that column. Crude scan, same altitude as
/// `referenced_relations`: whitespace around the argument is tolerated, as is a
/// leading `distinct` and one `table.`/`alias.` qualifier, so `SUM( t."amount
/// paid" )` still matches. Shared with the `run_sql` tool.
pub(crate) fn aggregates_text_column<'a>(sql: &str, text_cols: &'a [String]) -> Option<&'a str> {
    let lower = sql.to_lowercase();
    let bytes = lower.as_bytes();
    for agg in ["sum", "avg", "total"] {
        let mut from = 0;
        while let Some(rel) = lower[from..].find(agg) {
            let mut i = from + rel + agg.len();
            from = i;
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            if bytes.get(i) != Some(&b'(') {
                continue;
            }
            i += 1;
            let Some(close) = lower[i..].find(')') else { break };
            let inner = lower[i..i + close].trim();
            let inner = inner.strip_prefix("distinct").map(str::trim_start).unwrap_or(inner);
            let arg = match inner.split_once('.') {
                Some((q, rest))
                    if !q.is_empty()
                        && q.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '"') =>
                {
                    rest.trim()
                }
                _ => inner,
            };
            let arg = arg.trim_matches('"');
            if let Some(hit) = text_cols.iter().find(|c| c.as_str() == arg) {
                return Some(hit.as_str());
            }
        }
    }
    None
}

/// Flag any cited query that sums/averages a column the catalog reports as
/// `TEXT` SQLite counts non-numeric text as 0, so the figure may be wrong.
fn check_text_agg(engine: &EngineState, evidence: &[EvidenceItem], out: &mut Vec<VerificationCheck>) {
    let cols = text_columns(engine);
    if cols.is_empty() {
        return;
    }
    let lowered: Vec<String> = cols.iter().map(|(l, _)| l.clone()).collect();
    for e in evidence.iter().filter(|e| e.tool == "run_sql" && e.error.is_none()) {
        let Some(sql) = &e.sql else { continue };
        if let Some(hit) = aggregates_text_column(sql, &lowered) {
            let name = cols.iter().find(|(l, _)| l == hit).map_or(hit, |(_, n)| n.as_str());
            out.push(warn(
                format!("a total here is computed over the text column `{name}`"),
                Some("non-numeric values count as 0 cast the column if the figure looks off".into()),
            ));
            return;
        }
    }
}

fn ok(label: impl Into<String>) -> VerificationCheck {
    VerificationCheck { label: label.into(), ok: true, detail: None }
}
fn warn(label: impl Into<String>, detail: Option<String>) -> VerificationCheck {
    VerificationCheck { label: label.into(), ok: false, detail }
}

// --- 1. table existence -------------------------------------------------

fn check_tables(engine: &EngineState, evidence: &[EvidenceItem], out: &mut Vec<VerificationCheck>) {
    let known: HashSet<String> = engine
        .catalog()
        .sources
        .iter()
        .filter_map(|s| s.view.clone())
        .collect();

    let mut bad = HashSet::new();
    for e in evidence.iter().filter(|e| e.tool == "run_sql") {
        let Some(sql) = &e.sql else { continue };
        for t in referenced_relations(sql) {
            if !known.contains(&t) && !t.contains('(') {
                bad.insert(t);
            }
        }
    }
    for t in bad {
        out.push(warn(
            format!("`{t}` used in a query is not a catalogued table"),
            None,
        ));
    }
}

/// Tokens that follow FROM / JOIN, lowercased and de-punctuated. Crude only
/// used to flag obviously-wrong table names.
fn referenced_relations(sql: &str) -> HashSet<String> {
    let lower = sql.to_lowercase();
    let toks: Vec<&str> = lower.split(|c: char| c.is_whitespace()).filter(|s| !s.is_empty()).collect();
    let mut out = HashSet::new();
    for (i, t) in toks.iter().enumerate() {
        if (*t == "from" || *t == "join") && i + 1 < toks.len() {
            let name = toks[i + 1].trim_matches(|c: char| !c.is_alphanumeric() && c != '_');
            if !name.is_empty() {
                out.insert(name.to_string());
            }
        }
    }
    out
}

// --- 2. re-run cited queries ------------------------------------------

fn rerun_queries(engine: &EngineState, evidence: &[EvidenceItem], out: &mut Vec<VerificationCheck>) {
    let mut matched = 0usize;
    let mut skipped_cost = 0usize;
    let mut seen: HashSet<&str> = HashSet::new();
    for e in evidence.iter().filter(|e| e.tool == "run_sql" && e.error.is_none()) {
        let Some(sql) = &e.sql else { continue };
        // One re-run per distinct query - the model often cites the same SQL twice.
        if !seen.insert(sql.as_str()) {
            continue;
        }
        // A query that was already slow, or came back truncated, is the
        // expensive kind; re-running it right before the answer renders is the
        // wrong trade. Trust the first execution.
        let truncated = matches!((e.row_count, &e.rows), (Some(n), Some(r)) if r.len() < n);
        if e.ms > 500 || truncated {
            skipped_cost += 1;
            continue;
        }
        match engine.run_sql(sql) {
            Ok(fresh) => {
                let same = e.row_count == Some(fresh.row_count)
                    && e.rows.as_ref().map(|r| r == &fresh.rows).unwrap_or(true);
                if same {
                    matched += 1;
                } else {
                    out.push(warn(
                        "a query behind this answer gives a different result now",
                        Some(truncate(sql, 120)),
                    ));
                }
            }
            Err(err) => out.push(warn(
                "a query behind this answer no longer runs",
                Some(format!("{}: {err}", truncate(sql, 100))),
            )),
        }
    }
    if matched > 0 {
        out.push(ok("re-checked the queries behind this answer  same results"));
    }
    if skipped_cost > 0 {
        out.push(ok(
            "an expensive query behind this answer was not re-run  trusting its first result",
        ));
    }
}

// --- 3. numbers in the answer are backed by evidence ------------------

fn check_numbers(answer: &str, evidence: &[EvidenceItem], out: &mut Vec<VerificationCheck>) {
    let mut supported: Vec<f64> = Vec::new();
    for e in evidence {
        collect_numbers(&e.result_summary, &mut supported);
        // `output` is where read_file / run_python put their text - an answer
        // that quotes a figure from a note or a Python print is still backed.
        if let Some(output) = &e.output {
            collect_numbers(output, &mut supported);
        }
        if let Some(rows) = &e.rows {
            for row in rows {
                for cell in row {
                    match cell {
                        Json::Number(n) => {
                            if let Some(f) = n.as_f64() {
                                supported.push(f);
                            }
                        }
                        Json::String(s) => collect_numbers(s, &mut supported),
                        _ => {}
                    }
                }
            }
        }
    }

    // A `Background:` line is explicitly model context, not a computed claim
    // (the system prompt bars specific figures from it). Don't hold its
    // numerals e.g. "a 1-10 scale" against the "every number came from the
    // data" check; everything else in the answer is still checked.
    let checked: String = answer
        .lines()
        .filter(|l| !l.trim_start().starts_with("Background:"))
        .collect::<Vec<_>>()
        .join("\n");

    let mut unsupported: Vec<String> = Vec::new();
    for (raw, val) in number_tokens(&checked) {
        if is_probable_year(val) {
            continue;
        }
        if !supported.iter().any(|s| close(*s, val)) {
            unsupported.push(raw);
        }
    }
    unsupported.dedup();

    if unsupported.is_empty() {
        if number_tokens(&checked).next().is_some() {
            out.push(ok("every number in the answer came from the data above"));
        }
    } else {
        let shown: Vec<_> = unsupported.iter().take(4).cloned().collect();
        out.push(warn(
            format!(
                "the answer mentions {} not found in any result",
                shown.join(", ")
            ),
            Some("check these against the evidence below".into()),
        ));
    }
}

fn collect_numbers(text: &str, out: &mut Vec<f64>) {
    for (_, v) in number_tokens(text) {
        out.push(v);
    }
}

/// Iterator of (raw substring, parsed value) for number-shaped runs in `text`.
/// Handles thousands separators, a leading `$`, a trailing `%`.
fn number_tokens(text: &str) -> impl Iterator<Item = (String, f64)> + '_ {
    let bytes = text.as_bytes();
    let mut i = 0;
    std::iter::from_fn(move || {
        while i < bytes.len() {
            let c = bytes[i];
            if c.is_ascii_digit() {
                let start = i;
                while i < bytes.len()
                    && (bytes[i].is_ascii_digit() || bytes[i] == b',' || bytes[i] == b'.')
                {
                    i += 1;
                }
                // Absorb space-separated 3-digit groups ("52 000" = 52000), so a
                // model that writes European-style grouping is still matched.
                while i + 4 <= bytes.len()
                    && bytes[i] == b' '
                    && bytes[i + 1].is_ascii_digit()
                    && bytes[i + 2].is_ascii_digit()
                    && bytes[i + 3].is_ascii_digit()
                    && (i + 4 == bytes.len() || !bytes[i + 4].is_ascii_digit())
                {
                    i += 4;
                }
                let mut raw = text[start..i].to_string();
                // don't swallow a sentence-ending period
                while raw.ends_with('.') {
                    raw.pop();
                    i -= 1;
                }
                let cleaned: String = raw.chars().filter(|c| *c != ',' && *c != ' ').collect();
                if let Ok(v) = cleaned.parse::<f64>() {
                    let mut display = raw.clone();
                    if start > 0 && bytes[start - 1] == b'$' {
                        display = format!("${raw}");
                    }
                    if i < bytes.len() && bytes[i] == b'%' {
                        display = format!("{raw}%");
                        i += 1;
                    }
                    return Some((display, v));
                }
            } else {
                i += 1;
            }
        }
        None
    })
}

fn is_probable_year(v: f64) -> bool {
    v.fract() == 0.0 && (1900.0..=2099.0).contains(&v)
}

/// Loose numeric match: exact, within a rounding step, or within 0.5%.
fn close(a: f64, b: f64) -> bool {
    if a == b {
        return true;
    }
    let diff = (a - b).abs();
    if diff < 0.5 {
        return true;
    }
    let scale = a.abs().max(b.abs()).max(1.0);
    diff / scale < 0.005
}

fn truncate(s: &str, n: usize) -> String {
    match s.char_indices().nth(n) {
        Some((idx, _)) => format!("{}…", &s[..idx]),
        None => s.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_table_names() {
        let r = referenced_relations("SELECT * FROM sales s JOIN people p ON s.id = p.id");
        assert!(r.contains("sales") && r.contains("people"));
    }

    #[test]
    fn spots_sum_over_text_column() {
        let cols = vec!["amount paid".to_string(), "method".to_string()];
        assert_eq!(
            aggregates_text_column(r#"SELECT SUM("Amount Paid") AS t FROM ledger"#, &cols),
            Some("amount paid")
        );
        assert_eq!(
            aggregates_text_column("select total(\"amount paid\") from x", &cols),
            Some("amount paid")
        );
        assert_eq!(aggregates_text_column("SELECT count(*) FROM ledger", &cols), None);
        assert_eq!(
            aggregates_text_column(r#"SELECT SUM(rent_total) FROM ledger"#, &cols),
            None
        );
        // Whitespace inside the call and a table/alias qualifier still match.
        assert_eq!(
            aggregates_text_column(
                r#"SELECT SUM( l."Amount Paid" ) FROM ledger l JOIN pay p ON p.id = l.id"#,
                &cols
            ),
            Some("amount paid")
        );
        assert_eq!(
            aggregates_text_column("select avg(distinct method) from x", &cols),
            Some("method")
        );
    }

    #[test]
    fn parses_numbers() {
        let got: Vec<_> = number_tokens("We spent $1,234.50 (up 12%) vs 2024, total 450.")
            .map(|(_, v)| v)
            .collect();
        assert_eq!(got, vec![1234.5, 12.0, 2024.0, 450.0]);
    }

    #[test]
    fn close_matches() {
        assert!(close(450.0, 450.0));
        assert!(close(450.0, 450.4));
        assert!(close(1000.0, 1004.0)); // within 0.5%
        assert!(!close(450.0, 470.0));
    }

    #[test]
    fn year_is_ignored() {
        assert!(is_probable_year(2024.0));
        assert!(!is_probable_year(2024.5));
        assert!(!is_probable_year(450.0));
    }

    #[test]
    fn background_line_numbers_are_not_flagged() {
        let ev = vec![EvidenceItem {
            tool: "run_sql".into(),
            args: Json::Object(Default::default()),
            note: None,
            sql: None,
            result_summary: "1 row: total 450".into(),
            columns: None,
            rows: None,
            row_count: Some(1),
            output: None,
            ms: 1,
            error: None,
        }];

        let answer = "Background: RPE is a 1-10 scale.\nYour total was 450, peaking at 999.";
        let mut out = Vec::new();
        check_numbers(answer, &ev, &mut out);

        let warns: Vec<_> = out.iter().filter(|c| !c.ok).collect();
        assert_eq!(warns.len(), 1, "only the body's stray 999 should warn: {out:?}");
        assert!(warns[0].label.contains("999"), "{}", warns[0].label);
    }
}
