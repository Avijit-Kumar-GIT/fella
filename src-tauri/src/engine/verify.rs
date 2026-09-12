//! Deterministic post-answer checks. Cheap, no extra LLM call:
//!   1. every table named in a cited query exists in the catalog
//!   2. re-running each cited query still gives the same result
//!   3. every number in the answer appears in some tool result
//!   4. the question's wording implies a SQL aggregate no cited query used
//!   5. a column named in the question is never mentioned in any cited query
//!   6. a question naming a shared join column was answered from one table
//!
//! Plus one cost-gated check that *does* spend an extra LLM round-trip, used
//! sparingly (agent.rs only calls it once a cheap check above already left a
//! warning standing):
//!   7. a stricter, independent second opinion agrees with the first answer

use std::collections::HashSet;

use serde_json::Value as Json;

use crate::engine::evidence::{EvidenceItem, VerificationCheck};
use crate::engine::state::EngineState;

pub fn run(
    engine: &EngineState,
    question: &str,
    answer: &str,
    evidence: &[EvidenceItem],
) -> Vec<VerificationCheck> {
    let mut checks = Vec::new();

    check_tables(engine, evidence, &mut checks);
    rerun_queries(engine, evidence, &mut checks);
    check_numbers(answer, evidence, &mut checks);
    check_text_agg(engine, evidence, &mut checks);
    check_case_filter(engine, evidence, &mut checks);
    check_aggregate_verb(question, evidence, &mut checks);
    check_dropped_column(engine, question, evidence, &mut checks);
    check_multi_table_join(engine, question, evidence, &mut checks);

    checks
}

fn first_bad(checks: &[VerificationCheck], labels: &[&str]) -> Option<String> {
    checks
        .iter()
        .find(|c| !c.ok && labels.iter().any(|h| c.label.contains(h)))
        .map(|c| match &c.detail {
            Some(d) => format!("{} ({d})", c.label),
            None => c.label.clone(),
        })
}

/// The subset of failures that mean the answer is probably *wrong*, not merely
/// worth a look: a cited query that now re-runs to a different result or won't
/// run, or a figure in the answer that appears in no tool result. A stray-year
/// nudge or a text-column-aggregation caution is a soft warning and does not
/// count. Returns the first such check's `label` with its `detail` folded in.
///
/// Matched on the label string, the same altitude as `is_schema_error`. Used by
/// the eval harness to separate hard misses from soft warnings.
pub fn hard_fail(checks: &[VerificationCheck]) -> Option<String> {
    first_bad(
        checks,
        &[
            "different result now",
            "no longer runs",
            "not found in any result",
            "disagrees with this one",
        ],
    )
}

/// True when a query behind the answer was actually re-executed and matched,
/// and nothing failed hard. The signal for "this answer is safe to learn from"
/// (per-folder memory records a recipe only when this holds).
pub fn reran_clean(checks: &[VerificationCheck]) -> bool {
    hard_fail(checks).is_none()
        && checks
            .iter()
            .any(|c| c.ok && c.label.contains("re-checked the queries behind this answer"))
}

/// The narrower subset the agent loop's corrective re-ask acts on: a cited query
/// that now re-runs differently, or no longer runs. These are precise the query
/// is re-executed so "restate your answer to match the re-run" is a safe,
/// tool-free fix. The fuzzier "a figure appears in no result" is deliberately
/// *not* here: it's a number-shape heuristic, and a tool-free reconcile there
/// tends to make the model parrot a raw evidence value (the ratio 0.176 instead
/// of the "18%" it correctly derived). That stays a fold warning only.
pub fn rerun_regression(checks: &[VerificationCheck]) -> Option<String> {
    first_bad(checks, &["different result now", "no longer runs"])
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

// --- 5. case-sensitive filter on a mixed-case label column ----------------

/// `(lowercased, original)` names of catalogued `TEXT` columns whose ingest note
/// says the values differ only in capitalisation (`case_collision`).
pub(crate) fn mixed_case_columns(engine: &EngineState) -> Vec<(String, String)> {
    engine
        .catalog()
        .sources
        .iter()
        .filter_map(|s| s.columns.as_ref())
        .flatten()
        .filter(|c| c.type_.eq_ignore_ascii_case("text"))
        .filter(|c| c.note.as_deref().is_some_and(|n| n.contains("capitalisation")))
        .map(|c| (c.name.to_lowercase(), c.name.clone()))
        .collect()
}

/// If `sql` filters one of `cols` (already lowercased) with an exact-case
/// `= '…'` or `IN (…)` that isn't wrapped in `lower(`/`upper(`, return that
/// column. Crude scan, same altitude as `aggregates_text_column`.
pub(crate) fn case_sensitive_label_filter<'a>(sql: &str, cols: &'a [String]) -> Option<&'a str> {
    let lower = sql.to_lowercase();
    for col in cols {
        for pat in [format!(" {col}"), format!("\"{col}\""), format!(".{col}"), format!("({col}")] {
            let mut from = 0;
            while let Some(rel) = lower[from..].find(&pat) {
                let start = from + rel;
                let end = start + pat.len();
                from = end;
                // not part of a longer identifier on either side
                if lower[end..].starts_with(|c: char| c.is_alphanumeric() || c == '_') {
                    continue;
                }
                let before = lower[..start + 1].trim_end();
                if before.ends_with("lower(") || before.ends_with("upper(") {
                    continue; // already case-folded
                }
                let after = lower[end..].trim_start().trim_start_matches('"').trim_start();
                let is_filter = after.starts_with("= '")
                    || after.starts_with("='")
                    || after.starts_with("in ('")
                    || after.starts_with("in('");
                if is_filter && !after.contains("collate nocase") {
                    return Some(col);
                }
            }
        }
    }
    None
}

/// Flag a cited query that filters a mixed-case label column by exact case one
/// `Rent` vs `rent` row silently drops out. A soft warning; the schema note is
/// where the model is meant to have folded case in the first place.
fn check_case_filter(engine: &EngineState, evidence: &[EvidenceItem], out: &mut Vec<VerificationCheck>) {
    let cols = mixed_case_columns(engine);
    if cols.is_empty() {
        return;
    }
    let lowered: Vec<String> = cols.iter().map(|(l, _)| l.clone()).collect();
    for e in evidence.iter().filter(|e| e.tool == "run_sql" && e.error.is_none()) {
        let Some(sql) = &e.sql else { continue };
        if let Some(hit) = case_sensitive_label_filter(sql, &lowered) {
            let name = cols.iter().find(|(l, _)| l == hit).map_or(hit, |(_, n)| n.as_str());
            out.push(warn(
                format!("a filter on `{name}` matches exact case"),
                Some(format!(
                    "`{name}` has values that differ only in capitalisation; \
                     rows like `Rent` vs `rent` may be excluded unless the filter folds case"
                )),
            ));
            return;
        }
    }
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

const AGGREGATE_VERBS: &[(&[&str], &str)] = &[
    (&["how many", "count of", "number of"], "COUNT("),
    (&["how much", "total ", " sum of"], "SUM("),
    (&["average", "avg "], "AVG("),
];

/// Which SQL aggregates the question's wording implies but no query in
/// `sql_texts` (already upper-cased) actually used. Pure so it's easy to
/// test independent of `EvidenceItem`; `check_aggregate_verb` is the wrapper
/// that pulls SQL text out of the evidence.
fn missing_aggregate_verbs<'a>(question: &str, sql_texts: &[String]) -> Vec<&'a str> {
    if sql_texts.is_empty() {
        return Vec::new();
    }
    let q = question.to_lowercase();
    AGGREGATE_VERBS
        .iter()
        .filter(|(phrases, func)| {
            phrases.iter().any(|p| q.contains(p)) && !sql_texts.iter().any(|s| s.contains(func))
        })
        .map(|(_, func)| func.trim_end_matches('('))
        .collect()
}

/// A question that says "how many" / "how much" / "average" implies a
/// specific SQL aggregate. If none of the answer's successful queries used
/// it, the model may have answered from a precomputed column or a different
/// computation than the question actually asked for (#67's "valid query,
/// wrong question" class — this catches only the crudest, lexical case of
/// it; #77 is the deeper fix). Soft warning: the aggregate can legitimately
/// be absent (e.g. the raw rows already answer it, or a subquery hides it).
fn check_aggregate_verb(question: &str, evidence: &[EvidenceItem], out: &mut Vec<VerificationCheck>) {
    let sql: Vec<String> = evidence
        .iter()
        .filter(|e| e.tool == "run_sql" && e.error.is_none())
        .filter_map(|e| e.sql.as_deref())
        .map(str::to_uppercase)
        .collect();
    for name in missing_aggregate_verbs(question, &sql) {
        out.push(warn(
            format!("question implies {name}() but no cited query used it"),
            Some(format!(
                "the wording suggests {name}, but none of the queries behind this answer \
                 contain {name}() it may come from a precomputed column or a different \
                 aggregate than the question asked for"
            )),
        ));
    }
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
/// used to flag obviously-wrong table names, and to tag a learned recipe with
/// the tables it touches.
pub(crate) fn referenced_relations(sql: &str) -> HashSet<String> {
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

/// True when `needle` occurs in `haystack` on a word boundary (not as part of
/// a longer identifier on either side). Both are expected already lowercased.
fn contains_word(haystack: &str, needle: &str) -> bool {
    let bytes = haystack.as_bytes();
    let mut from = 0;
    while let Some(rel) = haystack[from..].find(needle) {
        let start = from + rel;
        let end = start + needle.len();
        from = end;
        let before_ok = start == 0 || !bytes[start - 1].is_ascii_alphanumeric() && bytes[start - 1] != b'_';
        let after_ok = end == bytes.len() || !bytes[end].is_ascii_alphanumeric() && bytes[end] != b'_';
        if before_ok && after_ok {
            return true;
        }
    }
    false
}

/// Column names too generic to mean "filter on this" just because the word
/// shows up in the question — same list `shared_column_hints` (state.rs)
/// already uses to drop unhelpful join-key suggestions.
const GENERIC_COLUMN_NAMES: &[&str] = &[
    "id", "name", "title", "description", "note", "notes", "memo", "comment", "comments", "type",
    "status", "value", "amount", "total", "subtotal", "count", "price", "cost", "qty", "quantity",
    "label", "date",
];

/// Which of `table_columns` (real schema names, any case) are named in the
/// question but never mentioned anywhere in `sql_texts` — a sign the model
/// dropped a filter or grouping the question implied (e.g. "how many books
/// have I *finished*" answered without a `finished` filter anywhere in the
/// query). Lexical and conservative: word-boundary matched, generic names
/// filtered out, only meaningful for a single table (the caller resolves
/// that). Pure so it's testable without a real `EngineState`/catalog, same
/// pattern as `missing_aggregate_verbs`. Doesn't check filter *values*
/// (`tier = 'close'` vs `tier = 'active'`) only that the column was
/// referenced at all; see #67 for the harder cases this still misses.
fn dropped_columns<'a>(question: &str, sql_texts: &[String], table_columns: &'a [String]) -> Vec<&'a str> {
    if sql_texts.is_empty() {
        return Vec::new();
    }
    let q = question.to_lowercase();
    let sql_all = sql_texts.join(" ").to_lowercase();
    table_columns
        .iter()
        .filter(|name| {
            let n = name.to_lowercase();
            n.len() >= 4
                && !GENERIC_COLUMN_NAMES.contains(&n.as_str())
                && contains_word(&q, &n)
                && !contains_word(&sql_all, &n)
        })
        .map(String::as_str)
        .collect()
}

/// A column from the single table this answer's queries touch, named in the
/// question but never mentioned anywhere in the cited SQL. Only fires for a
/// single-table answer — multi-table questions are #79's job, and a second
/// table changes which column belongs where.
fn check_dropped_column(
    engine: &EngineState,
    question: &str,
    evidence: &[EvidenceItem],
    out: &mut Vec<VerificationCheck>,
) {
    let sql: Vec<String> = evidence
        .iter()
        .filter(|e| e.tool == "run_sql" && e.error.is_none())
        .filter_map(|e| e.sql.as_deref())
        .map(str::to_string)
        .collect();
    let tables: HashSet<String> =
        sql.iter().flat_map(|s| referenced_relations(&s.to_lowercase())).collect();
    let [table] = tables.iter().collect::<Vec<_>>()[..] else { return };
    let catalog = engine.catalog();
    let Some(cols) = catalog
        .sources
        .iter()
        .find(|s| s.view.as_deref().map(str::to_lowercase).as_deref() == Some(table.as_str()))
        .and_then(|s| s.columns.as_ref())
    else {
        return;
    };
    let names: Vec<String> = cols.iter().map(|c| c.name.clone()).collect();
    for name in dropped_columns(question, &sql, &names) {
        out.push(warn(
            format!("question names `{name}` but no cited query mentions it"),
            Some(format!(
                "the question's wording includes \"{name}\", which is a column on this table, \
                 but none of the queries behind this answer reference it — a filter or grouping \
                 the question implied may have been dropped"
            )),
        ));
    }
}

// --- 6. a question that named a join key answered from one table only ------

/// Lowercased names of every non-generic column that appears, by name, in at
/// least 2 of `tables` — the same "these tables share a join key" signal
/// `shared_column_hints` (state.rs) surfaces in the prompt, computed
/// independently here so this check stays a pure function of simple inputs.
/// `tables` is `(table_name, column_names)` pairs; pure and testable without a
/// real catalog, same pattern as `missing_aggregate_verbs`/`dropped_columns`.
fn multi_table_columns(tables: &[(&str, Vec<String>)]) -> HashSet<String> {
    let mut counts: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    for (_, cols) in tables {
        for c in cols {
            let low = c.to_lowercase();
            if low.len() < 4 || GENERIC_COLUMN_NAMES.contains(&low.as_str()) {
                continue;
            }
            *counts.entry(low).or_insert(0) += 1;
        }
    }
    counts.into_iter().filter(|&(_, n)| n >= 2).map(|(k, _)| k).collect()
}

/// True when the question's wording names a column shared across ≥2
/// catalogued tables (implying a join), but the cited SQL only touched one of
/// them. `all_tables` is every catalogued (name, columns) pair; `touched` the
/// distinct table(s) the cited SQL actually referenced.
fn looks_like_a_missed_join(
    question: &str,
    all_tables: &[(&str, Vec<String>)],
    touched: &HashSet<String>,
) -> bool {
    if touched.len() != 1 || all_tables.len() < 2 {
        return false;
    }
    let q = question.to_lowercase();
    multi_table_columns(all_tables).iter().any(|c| contains_word(&q, c))
}

/// Extends #58's prompt-side join hint with a check on the answer side: when
/// the question's wording looked like it needed a join (it names a column
/// that lives on more than one table) but the answer's evidence trail only
/// touched one table, that's a sign the steering didn't take. Soft warning
/// only — a real single-table answer to a question that merely echoes a
/// shared column name (e.g. every table has a `date`) is common and fine;
/// generic column names are filtered out for exactly that reason.
fn check_multi_table_join(
    engine: &EngineState,
    question: &str,
    evidence: &[EvidenceItem],
    out: &mut Vec<VerificationCheck>,
) {
    let sql: Vec<String> = evidence
        .iter()
        .filter(|e| e.tool == "run_sql" && e.error.is_none())
        .filter_map(|e| e.sql.as_deref())
        .map(str::to_string)
        .collect();
    if sql.is_empty() {
        return;
    }
    let touched: HashSet<String> =
        sql.iter().flat_map(|s| referenced_relations(&s.to_lowercase())).collect();
    let catalog = engine.catalog();
    let all_tables: Vec<(&str, Vec<String>)> = catalog
        .sources
        .iter()
        .filter_map(|s| {
            let view = s.view.as_deref()?;
            let cols = s.columns.as_ref()?;
            Some((view, cols.iter().map(|c| c.name.clone()).collect()))
        })
        .collect();
    if looks_like_a_missed_join(question, &all_tables, &touched) {
        out.push(warn(
            "this question looked like it needed a join across tables; the answer only used one",
            None,
        ));
    }
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
                    && e.rows.as_ref().map(|r| rows_match(r, &fresh.rows)).unwrap_or(true);
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
            // An aggregate over no matching rows comes back as one all-NULL row
            // (or zero rows). That result backs the answer "0" / "none" - so
            // the model reporting 0 here isn't an ungrounded figure.
            let empty_aggregate = e.tool == "run_sql"
                && e.error.is_none()
                && (rows.is_empty() || (rows.len() == 1 && rows[0].iter().all(Json::is_null)));
            if empty_aggregate {
                supported.push(0.0);
            }
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
                // A digit run glued to a letter or underscore is an identifier
                // fragment (txns_00.csv, q1), not a figure the model stated.
                let in_identifier = start > 0
                    && (bytes[start - 1].is_ascii_alphabetic() || bytes[start - 1] == b'_');
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
                if !in_identifier {
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
/// A JSON number, or a string that is wholly a number.
fn num_of(v: &Json) -> Option<f64> {
    match v {
        Json::Number(n) => n.as_f64(),
        Json::String(s) => s.trim().parse::<f64>().ok(),
        _ => None,
    }
}

/// Two result sets are "the same" for the re-run check if every cell matches
/// exactly, or is a number within `close()` tolerance. A float SUM/AVG can
/// serialise with a low-bit difference when the query runs again a
/// microseconds-later `738022.3` vs `738022.30000000001` is not a changed
/// answer, and shouldn't trip the corrective re-ask.
fn rows_match(a: &[Vec<Json>], b: &[Vec<Json>]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(ra, rb)| {
            ra.len() == rb.len()
                && ra.iter().zip(rb).all(|(ca, cb)| match (num_of(ca), num_of(cb)) {
                    (Some(x), Some(y)) => close(x, y),
                    _ => ca == cb,
                })
        })
}

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

// --- 4. self-consistency second opinion (#80) -------------------------

/// True when the numeric figures in `a` and `b` don't line up: some number in
/// one has no `close()` counterpart in the other. Ignores probable years (a
/// date isn't the "did we get the figure right" signal). `false` when neither
/// text has a comparable figure at all nothing numeric to compare is not
/// evidence of disagreement. Pure and testable without a real run.
fn answers_disagree(a: &str, b: &str) -> bool {
    let nums = |t: &str| -> Vec<f64> {
        number_tokens(t).map(|(_, v)| v).filter(|v| !is_probable_year(*v)).collect()
    };
    let (na, nb) = (nums(a), nums(b));
    if na.is_empty() || nb.is_empty() {
        return false;
    }
    let uncovered = |xs: &[f64], ys: &[f64]| xs.iter().any(|x| !ys.iter().any(|y| close(*x, *y)));
    uncovered(&na, &nb) || uncovered(&nb, &na)
}

/// The check pushed when a stricter, independent second opinion (the agent
/// loop's cost-gated self-consistency re-check, #80 fired only when a cheap
/// check above already left a warning standing) disagrees with the first
/// answer's figures. A residual "still might be wrong" signal for cases the
/// deterministic checks above can't fully resolve on their own (joins,
/// multi-step, anything outside the simple single-table shape). Its label
/// participates in `hard_fail` the point is to surface it, not bury it.
/// `None` when the two agree, or neither has a comparable figure.
pub fn self_consistency_check(first: &str, second: &str) -> Option<VerificationCheck> {
    if !answers_disagree(first, second) {
        return None;
    }
    Some(warn(
        "a second, independent answer disagrees with this one",
        Some(format!(
            "asked again with stricter instructions, the model answered: \"{}\"",
            truncate(second.trim(), 200)
        )),
    ))
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
    fn rows_match_tolerates_float_jitter_only() {
        let a = vec![vec![Json::from(738022.3)]];
        let b = vec![vec![Json::from(738022.3 + 1e-6)]];
        assert!(rows_match(&a, &b), "sub-cent float drift is the same result");

        let c = vec![vec![Json::from(752000.0)]];
        assert!(!rows_match(&a, &c), "a result past close() tolerance still trips");

        // non-numeric cells must still match exactly
        let m1 = vec![vec![Json::from("2024-03"), Json::from(10.0)]];
        let m2 = vec![vec![Json::from("2024-04"), Json::from(10.0)]];
        assert!(!rows_match(&m1, &m2));
        let n = vec![vec![Json::from("2024-03"), Json::from(10.0001)]];
        assert!(rows_match(&m1, &n));
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
    fn spots_case_sensitive_label_filter() {
        let cols = vec!["cat".to_string()];
        let f = |s: &str| case_sensitive_label_filter(s, &cols);

        assert_eq!(f("SELECT sum(amt) FROM s WHERE cat = 'rent'"), Some("cat"));
        assert_eq!(f("... WHERE cat IN ('Rent','HOUSING')"), Some("cat"));
        assert_eq!(f("... WHERE s.cat='rent'"), Some("cat"));
        assert_eq!(f(r#"... WHERE "cat" = 'rent'"#), Some("cat"));

        // already case-folded -> no flag
        assert_eq!(f("... WHERE lower(cat) = 'rent'"), None);
        assert_eq!(f("... WHERE cat = 'rent' COLLATE NOCASE"), None);
        assert_eq!(f("... WHERE UPPER(cat) IN ('RENT')"), None);
        // not an equality filter, and a longer identifier that merely contains "cat"
        assert_eq!(f("SELECT cat, sum(amt) FROM s GROUP BY cat"), None);
        assert_eq!(f("... WHERE category_code = 'x'"), None);
    }

    #[test]
    fn spots_missing_aggregate_verb() {
        let sql = |s: &str| vec![s.to_uppercase()];

        // wording implies COUNT, query doesn't have it -> flagged
        assert_eq!(
            missing_aggregate_verbs("how many books have I finished?", &sql("SELECT * FROM books")),
            vec!["COUNT"]
        );
        // query does have it -> not flagged
        assert!(missing_aggregate_verbs(
            "how many books have I finished?",
            &sql("SELECT COUNT(*) FROM books WHERE finished = 'yes'")
        )
        .is_empty());
        // "total" implies SUM
        assert_eq!(
            missing_aggregate_verbs("what's my total spend?", &sql("SELECT amount FROM spend")),
            vec!["SUM"]
        );
        // "average" implies AVG
        assert_eq!(
            missing_aggregate_verbs("what's my average rating?", &sql("SELECT MAX(rating) FROM books")),
            vec!["AVG"]
        );
        // no run_sql evidence at all -> nothing to flag (e.g. answered from schema/no-tool)
        assert!(missing_aggregate_verbs("how many books have I finished?", &[]).is_empty());
        // wording doesn't imply any of these verbs -> nothing flagged
        assert!(missing_aggregate_verbs("which genre did I read most?", &sql("SELECT genre FROM books")).is_empty());
        // a compound question can flag more than one verb
        let mut both = missing_aggregate_verbs(
            "what's the total and average rating?",
            &sql("SELECT rating FROM books"),
        );
        both.sort_unstable();
        assert_eq!(both, vec!["AVG", "SUM"]);
    }

    #[test]
    fn spots_a_dropped_column() {
        let cols = vec!["finished".to_string(), "genre".to_string(), "title".to_string()];

        // question names "finished", cited query never mentions it -> flagged
        assert_eq!(
            dropped_columns(
                "how many books have I finished?",
                &["SELECT COUNT(*) FROM books".to_string()],
                &cols
            ),
            vec!["finished"]
        );
        // query does reference it -> not flagged
        assert!(dropped_columns(
            "how many books have I finished?",
            &["SELECT COUNT(*) FROM books WHERE finished = 'yes'".to_string()],
            &cols
        )
        .is_empty());
        // short/generic-ish column not in the stoplist by name coincidence is still fine;
        // a genuinely generic name is skipped even when dropped
        let generic = vec!["type".to_string()];
        assert!(dropped_columns(
            "what type of book is this?",
            &["SELECT * FROM books".to_string()],
            &generic
        )
        .is_empty());
        // a column name embedded in a longer word doesn't count as a real mention,
        // on either side: "rate" inside "rated" (question), "genre" inside
        // "subgenre" (SQL) — "genre" is still correctly flagged as dropped since
        // "subgenre" isn't a real reference to the `genre` column.
        assert!(dropped_columns(
            "how is this rated?",
            &["SELECT * FROM books".to_string()],
            &["rate".to_string()]
        )
        .is_empty());
        assert_eq!(
            dropped_columns(
                "which genre did I read most?",
                &["SELECT subgenre FROM books".to_string()],
                &["genre".to_string()]
            ),
            vec!["genre"]
        );
        // no run_sql evidence at all -> nothing to flag
        assert!(dropped_columns("how many books have I finished?", &[], &cols).is_empty());
    }

    #[test]
    fn spots_a_missed_join() {
        let orders = ("orders", vec!["customer_id".to_string(), "amount".to_string()]);
        let customers = ("customers", vec!["customer_id".to_string(), "city".to_string()]);
        let tables = vec![orders.clone(), customers.clone()];

        // "customer_id" lives on both tables -> question naming it, answered
        // from one table only, is flagged.
        assert!(looks_like_a_missed_join(
            "what's the customer_id for the biggest order?",
            &tables,
            &HashSet::from(["orders".to_string()]),
        ));
        // both tables touched -> not flagged, whatever the question says.
        assert!(!looks_like_a_missed_join(
            "what's the customer_id for the biggest order?",
            &tables,
            &HashSet::from(["orders".to_string(), "customers".to_string()]),
        ));
        // question doesn't name a shared column -> not flagged.
        assert!(!looks_like_a_missed_join(
            "what's the total amount?",
            &tables,
            &HashSet::from(["orders".to_string()]),
        ));
        // only one catalogued table -> nothing could be shared, not flagged.
        assert!(!looks_like_a_missed_join(
            "what's the customer_id for the biggest order?",
            &[orders],
            &HashSet::from(["orders".to_string()]),
        ));
        // no table touched at all -> not flagged (nothing to compare against).
        assert!(!looks_like_a_missed_join(
            "what's the customer_id for the biggest order?",
            &tables,
            &HashSet::new(),
        ));
    }

    #[test]
    fn generic_shared_columns_dont_count() {
        // "id"/"amount"/"date" are stoplisted even though every table has one;
        // a genuinely shared non-generic column ("vendor") is still caught.
        let a = ("a", vec!["id".to_string(), "amount".to_string(), "vendor".to_string()]);
        let b = ("b", vec!["id".to_string(), "date".to_string(), "vendor".to_string()]);
        assert_eq!(multi_table_columns(&[a, b]), HashSet::from(["vendor".to_string()]));
    }

    #[test]
    fn word_boundaries_are_respected() {
        assert!(contains_word("how many books have i finished?", "finished"));
        assert!(!contains_word("unfinished business", "finished"));
        assert!(!contains_word("the finisher", "finish"));
        assert!(contains_word("select * from t where finished = 'yes'", "finished"));
    }

    #[test]
    fn parses_numbers() {
        let got: Vec<_> = number_tokens("We spent $1,234.50 (up 12%) vs 2024, total 450.")
            .map(|(_, v)| v)
            .collect();
        assert_eq!(got, vec![1234.5, 12.0, 2024.0, 450.0]);

        // a digit run inside an identifier (txns_00) is not a figure
        let got2: Vec<_> = number_tokens("Total spending in txns_00 was 738,022.3.")
            .map(|(_, v)| v)
            .collect();
        assert_eq!(got2, vec![738022.3]);
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
    fn hard_fail_separates_wrong_from_merely_noteworthy() {
        // A soft warning only -> no hard fail.
        let soft = vec![
            warn("a total here is computed over the text column `amount`", None),
            ok("every number in the answer came from the data above"),
        ];
        assert_eq!(hard_fail(&soft), None);

        // A re-run mismatch is a hard fail; label + detail (the SQL) come back.
        let hard = vec![
            ok("re-checked the queries behind this answer  same results"),
            warn(
                "a query behind this answer gives a different result now",
                Some("SELECT sum(amount) FROM t".into()),
            ),
        ];
        assert_eq!(
            hard_fail(&hard).as_deref(),
            Some("a query behind this answer gives a different result now (SELECT sum(amount) FROM t)")
        );

        // An unbacked figure is a hard fail; label used when there's no detail.
        let stray = vec![warn("the answer mentions 999 not found in any result", None)];
        assert_eq!(
            hard_fail(&stray).as_deref(),
            Some("the answer mentions 999 not found in any result")
        );

        // ...but the corrective re-ask only acts on the re-run checks.
        assert_eq!(rerun_regression(&stray), None, "unbacked figure is fold-only");
        assert!(rerun_regression(&hard).is_some(), "a changed re-run does trigger it");

        // A self-consistency disagreement is a hard fail too.
        let disagreed = vec![self_consistency_check("Total: $450", "Total: $600").unwrap()];
        assert!(hard_fail(&disagreed).is_some());
    }

    #[test]
    fn spots_a_disagreeing_second_opinion() {
        // Same figure, different wording -> no disagreement.
        assert!(self_consistency_check("You spent $450 total.", "Total spending: 450").is_none());
        // A close (rounding-level) figure isn't a disagreement either.
        assert!(self_consistency_check("About $1,000.", "$1,004").is_none());
        // A genuinely different figure -> flagged, with the second answer quoted.
        let check = self_consistency_check("Total: $450", "Total: $600").unwrap();
        assert!(!check.ok);
        assert!(check.label.contains("disagrees with this one"));
        assert!(check.detail.unwrap().contains("$600"));
        // A year in one and not the other doesn't count as a figure mismatch.
        assert!(self_consistency_check("In 2024, you spent $450.", "$450").is_none());
        // Neither answer has a comparable figure (e.g. both prose) -> nothing to compare.
        assert!(self_consistency_check("The files can't answer this.", "I'm not sure.").is_none());
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
            chart: None,
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

    #[test]
    fn an_empty_aggregate_backs_the_answer_zero() {
        let ev = vec![EvidenceItem {
            tool: "run_sql".into(),
            args: Json::Object(Default::default()),
            note: None,
            sql: Some("SELECT SUM(amount) FROM t WHERE category = 'healthcare'".into()),
            result_summary: "1 row".into(),
            columns: Some(vec!["SUM(amount)".into()]),
            rows: Some(vec![vec![Json::Null]]),
            row_count: Some(1),
            output: None,
            chart: None,
            ms: 1,
            error: None,
        }];
        let mut out = Vec::new();
        check_numbers("You spent $0 on healthcare.", &ev, &mut out);
        assert!(
            out.iter().all(|c| c.ok),
            "0 is backed by the empty aggregate, not a stray figure: {out:?}"
        );
    }
}
