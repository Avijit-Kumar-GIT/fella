//! Deterministic saved-result comparison and inert portable report snapshots.
//!
//! A report package is NOT an AnalysisTurn import. Its result snapshots and
//! recorded checks are untrusted historical content, never new source authority.
//! There is deliberately no target-workspace or local-artifact import operation.
//! Imported presentation JSON is bounded declarative content, never evidence.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::io::Write;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value as Json;

use super::artifacts::{self, BoundResult, ResolvedArtifact, StudioDocument, StudioPlacementWidth};
use super::error::{EngineError, EngineResult};
use super::evidence::{VerificationCheck, VerificationStatus};
use super::runtime::{AnalysisContract, AnalysisTurn, ContractMeasure, ContractTime, TurnState};

pub const PACKAGE_VERSION: u32 = 2;
pub const MAX_PACKAGE_BYTES: usize = 10 * 1024 * 1024;
const MAX_ITEMS: usize = 256;
const MAX_SPEC_BYTES: usize = 256 * 1024;
const MAX_BODY_BYTES: usize = 512 * 1024;
const MAX_CELLS: usize = 500_000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NumericDelta {
    /// Decimal strings also protect the IPC consumer from JS integer rounding.
    ExactInteger {
        delta: String,
    },
    /// Explicitly approximate; never used for large integers mixed with floats.
    FiniteFloat {
        delta: f64,
    },
    Unavailable {
        reason: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ValueComparison {
    pub key: Vec<String>,
    pub field: String,
    pub unit: String,
    pub before: String,
    pub after: String,
    pub delta: NumericDelta,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TableComparison {
    pub before_result_id: String,
    pub after_result_id: String,
    pub key_fields: Vec<String>,
    pub values: Vec<ValueComparison>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ArtifactComparison {
    pub compatible: bool,
    pub reasons: Vec<String>,
    pub warnings: Vec<String>,
    pub before_revision: u64,
    pub after_revision: u64,
    pub source_revision_changed: bool,
    pub definitions_or_filters_changed: bool,
    pub tables: Vec<TableComparison>,
    /// Additive metadata; old reports remain readable. None for untrusted inputs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub analytical_diff: Option<AnalyticalDiff>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DiffStatus {
    Changed,
    Unchanged,
    Unavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiffItem {
    pub label: String,
    pub before: Option<String>,
    pub after: Option<String>,
    pub changed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiffDimension {
    pub status: DiffStatus,
    pub summary: String,
    pub items: Vec<DiffItem>,
    pub limitations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiffRevision {
    pub artifact_id: String,
    pub revision: u64,
    pub run_id: String,
    /// current, proposed (not adopted), or historical. Never inferred from prose.
    pub role: String,
    pub change_note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiffConsumer {
    pub document_id: String,
    pub document_title: String,
    pub placement_id: String,
    pub checkpoint_id: Option<String>,
    pub checkpoint_title: Option<String>,
    pub pinned_revision: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiffImpact {
    pub scanned: bool,
    pub complete: bool,
    pub summary: String,
    pub consumers: Vec<DiffConsumer>,
    pub limitations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AnalyticalDiff {
    pub version: u32,
    pub before: DiffRevision,
    pub after: DiffRevision,
    pub meaning: DiffDimension,
    pub inputs: DiffDimension,
    pub method: DiffDimension,
    pub results: DiffDimension,
    pub impact: DiffImpact,
}

fn recorded(value: impl Into<String>) -> Option<String> {
    Some(value.into())
}

fn item(label: &str, before: Option<String>, after: Option<String>) -> DiffItem {
    DiffItem {
        label: label.into(),
        changed: before != after,
        before,
        after,
    }
}

fn json_text(value: &impl Serialize) -> Option<String> {
    serde_json::to_string(value).ok()
}

fn dimension(items: Vec<DiffItem>, noun: &str, limitations: Vec<String>) -> DiffDimension {
    let changed = items.iter().filter(|item| item.changed).count();
    let available = items
        .iter()
        .any(|item| item.before.is_some() || item.after.is_some());
    DiffDimension {
        status: if !available {
            DiffStatus::Unavailable
        } else if changed > 0 {
            DiffStatus::Changed
        } else {
            DiffStatus::Unchanged
        },
        summary: if !available {
            format!("No recorded {noun} to compare.")
        } else if changed == 0 {
            format!("No recorded {noun} changed.")
        } else {
            format!("{changed} recorded {noun} changed.")
        },
        items,
        limitations,
    }
}

fn measures(run: &AnalysisTurn) -> Option<String> {
    run.contract.as_ref().map(|contract| {
        contract
            .measures
            .iter()
            .map(|m| {
                format!(
                    "{}: {}({}) [{}]",
                    m.concept,
                    m.operation,
                    m.field.as_deref().unwrap_or("field not recorded"),
                    m.unit.as_deref().unwrap_or("unit not recorded")
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    })
}

fn filters(run: &AnalysisTurn) -> Option<String> {
    run.contract.as_ref().map(|contract| {
        if contract.filters.is_empty() {
            "No filters recorded".into()
        } else {
            contract
                .filters
                .iter()
                .map(|filter| {
                    format!(
                        "{} {}: {}",
                        if filter.exclude { "Exclude" } else { "Include" },
                        filter.field.as_deref().unwrap_or(&filter.concept),
                        if filter.resolved_values.is_empty() {
                            format!("{} (unresolved)", filter.candidate_values.join(", "))
                        } else {
                            filter.resolved_values.join(", ")
                        }
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        }
    })
}

fn definitions(run: &AnalysisTurn) -> Option<String> {
    run.investigation_context.as_ref().map(|context| {
        if context.definitions.is_empty() {
            "No saved definitions".into()
        } else {
            context
                .definitions
                .iter()
                .map(|definition| {
                    format!(
                        "{} · revision {}: {}",
                        definition.id, definition.revision, definition.text
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        }
    })
}

fn sources(run: &AnalysisTurn) -> Option<String> {
    source_keys(run).map(|sources| {
        sources
            .into_iter()
            .map(|(source, table)| {
                table.map_or(source.clone(), |table| format!("{source} · {table}"))
            })
            .collect::<Vec<_>>()
            .join("\n")
    })
}

fn selection(run: &AnalysisTurn) -> Option<String> {
    selected_sources(run).map(|(all, paths)| {
        if all {
            "All workspace sources eligible".into()
        } else {
            paths.into_iter().collect::<Vec<_>>().join("\n")
        }
    })
}

fn methods(run: &AnalysisTurn) -> Option<String> {
    let entries = run
        .result
        .evidence
        .iter()
        .filter_map(|evidence| {
            let tool = evidence.get("tool").and_then(Json::as_str)?;
            if !matches!(tool, "run_sql" | "run_python") {
                return None;
            }
            let sql = evidence
                .get("sql")
                .or_else(|| evidence.get("args").and_then(|args| args.get("sql")))
                .and_then(Json::as_str);
            let code = evidence
                .get("python_calculation")
                .and_then(|python| python.get("code"))
                .or_else(|| evidence.get("args").and_then(|args| args.get("code")))
                .and_then(Json::as_str);
            Some(format!(
                "{} · {tool}\n{}",
                evidence
                    .get("id")
                    .and_then(Json::as_str)
                    .unwrap_or("unlabelled evidence"),
                sql.or(code).unwrap_or("Computation text not recorded")
            ))
        })
        .collect::<Vec<_>>();
    (!entries.is_empty()).then(|| entries.join("\n\n"))
}

fn revision_label(value: &ResolvedArtifact) -> DiffRevision {
    DiffRevision {
        artifact_id: value.artifact.id.clone(),
        revision: value.revision.revision,
        run_id: value.run.id.clone(),
        role: if value.revision.revision == value.artifact.current_revision {
            "current"
        } else if value.revision.revision > value.artifact.current_revision {
            "proposed"
        } else {
            "historical"
        }
        .into(),
        change_note: value.revision.change_note.clone(),
    }
}

fn analytical_diff(
    before: &ResolvedArtifact,
    after: &ResolvedArtifact,
    report: &ArtifactComparison,
) -> AnalyticalDiff {
    let a = &before.run;
    let b = &after.run;
    let field = |run: &AnalysisTurn, get: fn(&AnalysisContract) -> Option<String>| {
        run.contract.as_ref().and_then(get)
    };
    let mut meaning_limits = vec!["Recorded interpretation only; this comparison does not independently establish business meaning or causation.".into()];
    if a.contract.is_none() || b.contract.is_none() {
        meaning_limits.push("At least one analytical contract was not recorded.".into());
    }
    if a.investigation_context.is_none() || b.investigation_context.is_none() {
        meaning_limits.push("At least one run has no Investigation definition snapshot.".into());
    }
    let meaning = dimension(
        vec![
            item(
                "Question",
                recorded(a.question.clone()),
                recorded(b.question.clone()),
            ),
            item(
                "Subject",
                field(a, |c| c.subject.clone()),
                field(b, |c| c.subject.clone()),
            ),
            item(
                "Population",
                field(a, |c| c.population.clone()),
                field(b, |c| c.population.clone()),
            ),
            item("Measures and units", measures(a), measures(b)),
            item(
                "Grain",
                field(a, |c| c.grain.clone()),
                field(b, |c| c.grain.clone()),
            ),
            item(
                "Grouping keys",
                field(a, |c| Some(c.group_by.join(", "))),
                field(b, |c| Some(c.group_by.join(", "))),
            ),
            item("Filters", filters(a), filters(b)),
            item(
                "Period and timezone",
                a.contract
                    .as_ref()
                    .and_then(|c| c.time.as_ref())
                    .and_then(json_text),
                b.contract
                    .as_ref()
                    .and_then(|c| c.time.as_ref())
                    .and_then(json_text),
            ),
            item("Definitions", definitions(a), definitions(b)),
            item(
                "Assumptions",
                field(a, |c| Some(c.assumptions.join("\n"))),
                field(b, |c| Some(c.assumptions.join("\n"))),
            ),
            item(
                "Value semantics",
                field(a, |c| c.value_semantics.clone()),
                field(b, |c| c.value_semantics.clone()),
            ),
            item(
                "Denominator",
                field(a, |c| c.denominator.clone()),
                field(b, |c| c.denominator.clone()),
            ),
            item(
                "Missing-value policy",
                field(a, |c| c.missing_policy.clone()),
                field(b, |c| c.missing_policy.clone()),
            ),
        ],
        "meaning fields",
        meaning_limits,
    );
    let inputs = dimension(vec![
        item("Used sources", sources(a), sources(b)),
        item("Eligible source scope", selection(a), selection(b)),
        item("Source revision", a.workspace_revision.clone(), b.workspace_revision.clone()),
    ], "input fields", vec!["Source revisions identify retained snapshots; source files were not queried or replayed.".into()]);
    let method = dimension(
        vec![
            item("Saved computations", methods(a), methods(b)),
            item(
                "Joins",
                a.contract.as_ref().and_then(|c| json_text(&c.joins)),
                b.contract.as_ref().and_then(|c| json_text(&c.joins)),
            ),
            item(
                "Derived metrics",
                a.contract
                    .as_ref()
                    .and_then(|c| json_text(&c.derived_metrics)),
                b.contract
                    .as_ref()
                    .and_then(|c| json_text(&c.derived_metrics)),
            ),
            item(
                "Presentation JSON spec",
                json_text(&before.revision.spec),
                json_text(&after.revision.spec),
            ),
        ],
        "method fields",
        {
            let mut limits = vec![
                "Computation and presentation text are shown as saved, never executed by the diff."
                    .into(),
            ];
            if methods(a).is_none() || methods(b).is_none() {
                limits.push("At least one run has no saved SQL/Python computation details.".into());
            }
            limits
        },
    );
    let changed = report
        .tables
        .iter()
        .flat_map(|table| &table.values)
        .filter(|value| value.before != value.after)
        .count();
    let mut results = dimension(
        vec![
            item(
                "Saved answer",
                recorded(a.result.text.clone()),
                recorded(b.result.text.clone()),
            ),
            item(
                "Verification status",
                json_text(&a.result.status),
                json_text(&b.result.status),
            ),
            item(
                "Recorded checks",
                json_text(&a.result.verification),
                json_text(&b.result.verification),
            ),
            item(
                "Run checks",
                a.verification.as_ref().and_then(json_text),
                b.verification.as_ref().and_then(json_text),
            ),
        ],
        "result fields",
        report.reasons.clone(),
    );
    results.summary = if report.compatible {
        format!(
            "{changed} aligned saved values differ. Recorded verification is unchanged by review."
        )
    } else {
        "Numeric comparison is unavailable. Saved answers and checks remain inspectable.".into()
    };
    if !report.compatible {
        results.status = DiffStatus::Unavailable;
    } else if changed > 0 {
        results.status = DiffStatus::Changed;
    }
    AnalyticalDiff { version: 1, before: revision_label(before), after: revision_label(after), meaning, inputs, method, results,
        impact: DiffImpact { scanned: false, complete: false, summary: "Dependencies have not been scanned.".into(), consumers: vec![], limitations: vec!["Only explicit Studio placements/checkpoints can be indexed; Findings, Questions, prose references and exported reports/snapshots are not tracked here.".into(), "Existing pins and snapshots never update automatically.".into()] }
    }
}

fn err(message: impl Into<String>) -> EngineError {
    EngineError::msg(message)
}

fn identity(id: &str) -> EngineResult<()> {
    if id.is_empty()
        || id.len() > 128
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(err("invalid report identity"));
    }
    Ok(())
}

fn digest(bytes: &[u8]) -> String {
    ring::digest::digest(&ring::digest::SHA256, bytes)
        .as_ref()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn canonical(value: &impl Serialize) -> EngineResult<Vec<u8>> {
    // Conversion through Value recursively orders object maps with this crate's
    // serde_json configuration, including arbitrary saved display selections.
    Ok(serde_json::to_vec(&serde_json::to_value(value)?)?)
}

fn validated_resolved(value: &ResolvedArtifact) -> EngineResult<Vec<BoundResult>> {
    if value.artifact.workspace.is_empty()
        || value.run.workspace.as_deref() != Some(value.artifact.workspace.as_str())
        || value.run.id != value.revision.run_id
        || !value
            .artifact
            .revisions
            .iter()
            .any(|r| r == &value.revision)
    {
        return Err(err(
            "resolved artifact does not match its canonical run and revision",
        ));
    }
    let results = artifacts::eligible_results(&value.run)?;
    if results != value.results {
        return Err(err("resolved values differ from canonical saved evidence"));
    }
    Ok(results)
}

fn source_keys(run: &AnalysisTurn) -> Option<BTreeSet<(String, Option<String>)>> {
    run.source_scope.as_ref().map(|scope| {
        scope
            .used_sources
            .iter()
            .map(|s| (s.source.clone(), s.table.clone()))
            .collect()
    })
}

fn selected_sources(run: &AnalysisTurn) -> Option<(bool, BTreeSet<String>)> {
    run.investigation_context.as_ref().map(|c| {
        (
            c.source_selection.is_all_sources(),
            c.source_selection.included_paths.iter().cloned().collect(),
        )
    })
}

fn semantics(
    before: &AnalysisTurn,
    after: &AnalysisTurn,
) -> EngineResult<(AnalysisContract, AnalysisContract)> {
    let a = before.contract.as_ref().ok_or_else(||err("The earlier run has no declared analytical contract; keys, grain, and units cannot be established."))?;
    let b = after.contract.as_ref().ok_or_else(||err("The later run has no declared analytical contract; keys, grain, and units cannot be established."))?;
    if a.grain.as_deref().is_none_or(|g| g.trim().is_empty())
        || a.grain != b.grain
        || a.group_by != b.group_by
    {
        return Err(err(
            "Declared grain or grouping keys differ or are missing.",
        ));
    }
    if a.time != b.time || a.comparison_spec != b.comparison_spec || a.comparison != b.comparison {
        return Err(err("Declared periods, time buckets, or timezones differ; no implicit period alignment was performed."));
    }
    if a.measures.is_empty()
        || a.measures != b.measures
        || a.derived_metrics != b.derived_metrics
        || a.denominator != b.denominator
        || a.missing_policy != b.missing_policy
        || a.value_semantics != b.value_semantics
    {
        return Err(err(
            "Measures, units, denominators, or value semantics are missing or incompatible.",
        ));
    }
    if source_keys(before).is_none()
        || source_keys(before) != source_keys(after)
        || selected_sources(before) != selected_sources(after)
    {
        return Err(err("Recorded source identities or Investigation source boundaries differ or are unavailable."));
    }
    Ok((a.clone(), b.clone()))
}

fn integer(value: &Json) -> Option<i128> {
    value
        .as_i64()
        .map(i128::from)
        .or_else(|| value.as_u64().map(i128::from))
}

fn numeric_delta(before: &Json, after: &Json) -> NumericDelta {
    if before.is_null() || after.is_null() {
        return NumericDelta::Unavailable {
            reason: "At least one saved value is null; missing values are not zero.".into(),
        };
    }
    if let (Some(a), Some(b)) = (integer(before), integer(after)) {
        return NumericDelta::ExactInteger {
            delta: (b - a).to_string(),
        };
    }
    if [before, after]
        .iter()
        .any(|v| integer(v).is_some_and(|n| n.unsigned_abs() > (1_u128 << 53)))
    {
        return NumericDelta::Unavailable {
            reason: "A large integer cannot be safely mixed with floating-point values.".into(),
        };
    }
    match (before.as_f64(), after.as_f64()) {
        (Some(a), Some(b)) if a.is_finite() && b.is_finite() && (b - a).is_finite() => {
            NumericDelta::FiniteFloat { delta: b - a }
        }
        _ => NumericDelta::Unavailable {
            reason: "A finite numeric difference is unavailable for these saved values.".into(),
        },
    }
}

fn row_keys(table: &BoundResult, keys: &[usize]) -> EngineResult<BTreeMap<String, usize>> {
    let mut map = BTreeMap::new();
    if keys.is_empty() && table.rows.len() != 1 {
        return Err(err("A result without declared keys must be a single scalar row; row positions are not identities."));
    }
    for (index, row) in table.rows.iter().enumerate() {
        let values: Vec<_> = keys.iter().map(|&k| &row[k]).collect();
        if values.iter().any(|v| v.is_null()) {
            return Err(err(
                "A result contains null keys; rows cannot be aligned safely.",
            ));
        }
        let key = serde_json::to_string(&values)?;
        if map.insert(key, index).is_some() {
            return Err(err(
                "Grouping keys are not unique; result grain is ambiguous.",
            ));
        }
    }
    Ok(map)
}

fn unit_for(contract: &AnalysisContract, column: &str) -> EngineResult<String> {
    let matches: Vec<_> = contract
        .measures
        .iter()
        .filter(|m| m.concept == column || m.field.as_deref() == Some(column))
        .collect();
    if matches.len() != 1 {
        return Err(err(format!(
            "Field {column:?} cannot be mapped unambiguously to a declared measure."
        )));
    }
    matches[0]
        .unit
        .clone()
        .filter(|u| !u.trim().is_empty())
        .ok_or_else(|| err(format!("Field {column:?} has no declared unit.")))
}

fn compare_tables(
    before: &[BoundResult],
    after: &[BoundResult],
    contract: &AnalysisContract,
) -> EngineResult<Vec<TableComparison>> {
    if before.is_empty() || before.len() != after.len() {
        return Err(err(
            "The runs do not contain matching saved analytical tables.",
        ));
    }
    let mut a = BTreeMap::new();
    let mut b = BTreeMap::new();
    for (tables, map) in [(before, &mut a), (after, &mut b)] {
        for table in tables {
            if !table.complete {
                return Err(err(
                    "A saved result is incomplete or sampled; no full-result delta is available.",
                ));
            }
            if map.insert(table.columns.clone(), table).is_some() {
                return Err(err("Several results have the same schema; corresponding computations cannot be identified unambiguously."));
            }
        }
    }
    if a.keys().ne(b.keys()) {
        return Err(err("Saved result fields or column order differ."));
    }
    let mut comparisons = Vec::new();
    for (schema, previous) in a {
        let next = b[&schema];
        let keys = contract
            .group_by
            .iter()
            .map(|key| {
                schema.iter().position(|c| c == key).ok_or_else(|| {
                    err(format!(
                        "Declared grouping key {key:?} is missing from the result."
                    ))
                })
            })
            .collect::<EngineResult<Vec<_>>>()?;
        let key_set: HashSet<_> = keys.iter().copied().collect();
        if key_set.len() != keys.len() {
            return Err(err("Declared grouping keys are duplicated."));
        }
        let previous_keys = row_keys(previous, &keys)?;
        let next_keys = row_keys(next, &keys)?;
        if previous_keys.keys().ne(next_keys.keys()) {
            return Err(err("The saved results contain different entity keys; absent rows were not replaced with zero."));
        }
        let numeric: Vec<_> = schema
            .iter()
            .enumerate()
            .filter(|(i, _)| !key_set.contains(i))
            .collect();
        if numeric.is_empty() {
            return Err(err("The result contains no comparable measure fields."));
        }
        let mut values = Vec::new();
        for (col, field) in numeric {
            let unit = unit_for(contract, field)?;
            if previous
                .rows
                .iter()
                .chain(&next.rows)
                .any(|row| !row[col].is_number() && !row[col].is_null())
            {
                return Err(err(format!("Measure {field:?} includes nonnumeric values; text was not coerced into numbers.")));
            }
            for (key, &old_row) in &previous_keys {
                let new_row = next_keys[key];
                values.push(ValueComparison {
                    key: keys
                        .iter()
                        .map(|&i| previous.exact_rows[old_row][i].clone())
                        .collect(),
                    field: field.clone(),
                    unit: unit.clone(),
                    before: previous.exact_rows[old_row][col].clone(),
                    after: next.exact_rows[new_row][col].clone(),
                    delta: numeric_delta(&previous.rows[old_row][col], &next.rows[new_row][col]),
                });
            }
        }
        comparisons.push(TableComparison {
            before_result_id: previous.id.clone(),
            after_result_id: next.id.clone(),
            key_fields: contract.group_by.clone(),
            values,
        });
    }
    Ok(comparisons)
}

/// Compare declared-compatible saved computations, not the truth of a model's
/// interpretation. Any structural incompatibility returns reasons and NO deltas.
pub fn compare(before: &ResolvedArtifact, after: &ResolvedArtifact) -> ArtifactComparison {
    let mut report = ArtifactComparison {
        compatible:false,reasons:vec![],warnings:vec!["Compatibility follows recorded contracts, not an independent judgment that the interpretation is correct. Numerical changes do not establish causation.".into()],
        before_revision:before.revision.revision,after_revision:after.revision.revision,
        source_revision_changed:before.run.workspace_revision != after.run.workspace_revision,
        definitions_or_filters_changed:before.run.investigation_context.as_ref().map(|c|&c.definitions) != after.run.investigation_context.as_ref().map(|c|&c.definitions)
            || before.run.contract.as_ref().map(|c|&c.filters) != after.run.contract.as_ref().map(|c|&c.filters),tables:vec![],analytical_diff:None,
    };
    let mut trusted = false;
    let attempt = (|| {
        if before.artifact.workspace != after.artifact.workspace {
            return Err(err("Artifacts belong to different workspaces."));
        }
        let a = validated_resolved(before)?;
        let b = validated_resolved(after)?;
        trusted = true;
        let (contract, _) = semantics(&before.run, &after.run)?;
        compare_tables(&a, &b, &contract)
    })();
    match attempt {
        Ok(tables) => {
            report.compatible = true;
            report.tables = tables;
        }
        Err(error) => report.reasons.push(error.to_string()),
    }
    if report.source_revision_changed {
        report.warnings.push("Source revisions differ; the comparison uses retained values, not a fresh query or an exact source replay.".into());
    }
    if report.definitions_or_filters_changed {
        report.warnings.push("Recorded definitions or filters changed; these are before/after outcomes, not identical semantic conditions.".into());
    }
    if before.run.result.status != VerificationStatus::Verified
        || after.run.result.status != VerificationStatus::Verified
    {
        report.warnings.push("At least one run has unresolved verification limitations. Comparison does not clear its recorded checks.".into());
    }
    // Semantic detail is useful even when numeric alignment fails, but must not
    // leak renderer-forged data or another workspace's source/method metadata.
    if trusted {
        report.analytical_diff = Some(analytical_diff(before, after, &report));
    }
    report
}

/// Add only known, explicit baseline-revision consumers. This does not change
/// adoption, pinned placements, checkpoints, runs, or downloaded report files.
pub fn compare_with_dependencies(
    data_dir: &Path,
    before: &ResolvedArtifact,
    after: &ResolvedArtifact,
) -> EngineResult<ArtifactComparison> {
    let mut report = compare(before, after);
    let Some(diff) = report.analytical_diff.as_mut() else {
        return Ok(report);
    };
    let documents = artifacts::studio_list_all(data_dir)?;
    let mut impact = diff.impact.clone();
    impact.scanned = true;
    impact.complete = documents.len() <= MAX_ITEMS;
    for document in documents.iter().take(MAX_ITEMS) {
        let references = std::iter::once((None, None, &document.placements)).chain(
            document.checkpoints.iter().map(|checkpoint| {
                (
                    Some(checkpoint.id.clone()),
                    Some(checkpoint.title.clone()),
                    &checkpoint.placements,
                )
            }),
        );
        for (checkpoint_id, checkpoint_title, placements) in references {
            for placement in placements {
                if placement.artifact_id != before.artifact.id
                    || placement.revision != before.revision.revision
                {
                    continue;
                }
                if impact.consumers.len() >= 512 {
                    impact.complete = false;
                    break;
                }
                impact.consumers.push(DiffConsumer {
                    document_id: document.id.clone(),
                    document_title: document.title.clone(),
                    placement_id: placement.id.clone(),
                    checkpoint_id: checkpoint_id.clone(),
                    checkpoint_title: checkpoint_title.clone(),
                    pinned_revision: placement.revision,
                });
            }
        }
    }
    impact.consumers.sort_by(|a, b| {
        (&a.document_id, &a.checkpoint_id, &a.placement_id).cmp(&(
            &b.document_id,
            &b.checkpoint_id,
            &b.placement_id,
        ))
    });
    impact.summary = if impact.consumers.is_empty() {
        "No baseline-revision pins found in the indexed Studio records.".into()
    } else {
        format!("{} Studio placements/checkpoint references remain pinned to revision {}. Adoption will not move these pins.", impact.consumers.len(), before.revision.revision)
    };
    if !impact.complete {
        impact.limitations.push("Dependency scan reached its document/reference budget; additional Studio consumers may exist.".into());
    }
    diff.impact = impact;
    Ok(report)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PackageManifest {
    pub generator: String,
    pub origin_workspace_fingerprint: String,
    pub snapshot_at_ms: u64,
    pub artifact_count: usize,
    pub run_count: usize,
    /// Corruption detection only. A checksum is NOT an authenticity signature.
    pub content_sha256: String,
    pub exclusions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PackagePlacement {
    pub id: String,
    pub artifact_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub label: String,
    #[serde(default, skip_serializing_if = "is_full_package_width")]
    pub width: StudioPlacementWidth,
    pub view_state: Json,
}

fn is_full_package_width(width: &StudioPlacementWidth) -> bool {
    *width == StudioPlacementWidth::Full
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PackageDocument {
    pub origin_document_id: String,
    pub origin_version: u64,
    pub title: String,
    pub body: String,
    pub placements: Vec<PackagePlacement>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PackageArtifact {
    pub id: String,
    pub origin_artifact_id: String,
    pub origin_revision: u64,
    pub title: String,
    pub run_id: String,
    /// Declarative view over the accompanying saved result; never evidence.
    pub spec: Json,
    pub schema_version: String,
    pub change_note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PackageSemantics {
    pub grain: Option<String>,
    pub keys: Vec<String>,
    pub measures: Vec<ContractMeasure>,
    pub time: Option<ContractTime>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PackageRun {
    pub id: String,
    pub origin_run_id: String,
    pub original_question: String,
    pub answer: String,
    pub state: TurnState,
    pub result_status: VerificationStatus,
    pub verification_status: Option<VerificationStatus>,
    pub checks: Vec<VerificationCheck>,
    pub result_checks: Vec<VerificationCheck>,
    pub source_revision: Option<String>,
    pub semantics: Option<PackageSemantics>,
    pub definitions: Vec<String>,
    pub source_labels: Vec<String>,
    pub methods: Vec<PackageMethod>,
    pub results: Vec<BoundResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PackageMethod {
    pub evidence_id: String,
    pub tool: String,
    pub sql: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ReportPackage {
    pub format: String,
    pub version: u32,
    pub manifest: PackageManifest,
    pub document: PackageDocument,
    pub artifacts: Vec<PackageArtifact>,
    pub runs: Vec<PackageRun>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReportPreview {
    pub read_only: bool,
    pub source_authority: bool,
    pub title: String,
    pub artifact_count: usize,
    pub run_count: usize,
    pub warnings: Vec<String>,
    pub package: ReportPackage,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ImportedReport {
    pub id: String,
    pub preview: ReportPreview,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ImportedReportSummary {
    pub id: String,
    pub title: String,
    pub artifact_count: usize,
    pub run_count: usize,
    pub modified_at_ms: u64,
}

fn exclusions() -> Vec<String> {
    [
        "raw_source_files",
        "conversation_transcripts",
        "unselected_revisions",
        "checkpoint_history",
        "filesystem_paths",
        "execution_permissions",
    ]
    .map(str::to_owned)
    .to_vec()
}

fn content_digest(package: &ReportPackage) -> EngineResult<String> {
    Ok(digest(&canonical(&(
        &package.document,
        &package.artifacts,
        &package.runs,
    ))?))
}

fn relative_label(source: &str, workspace: &str) -> String {
    let normalized = source.replace('\\', "/");
    let root = workspace.replace('\\', "/");
    let candidate = normalized
        .strip_prefix(&format!("{}/", root.trim_end_matches('/')))
        .unwrap_or(&normalized);
    if candidate.is_empty()
        || candidate.starts_with('/')
        || candidate.contains(':')
        || candidate.chars().any(char::is_control)
        || candidate
            .split('/')
            .any(|s| s.is_empty() || s == "." || s == "..")
    {
        "[local source identity omitted]".into()
    } else {
        candidate.into()
    }
}

fn packaged_run(id: String, value: &ResolvedArtifact) -> EngineResult<PackageRun> {
    let results = validated_resolved(value)?;
    let eligible: HashSet<_> = results.iter().map(|r| r.id.as_str()).collect();
    let methods = value
        .run
        .result
        .evidence
        .iter()
        .filter_map(|item| {
            let id = item.get("id")?.as_str()?;
            eligible.contains(id).then(|| PackageMethod {
                evidence_id: id.into(),
                tool: item.get("tool").and_then(Json::as_str).unwrap_or("").into(),
                sql: item.get("sql").and_then(Json::as_str).map(str::to_owned),
            })
        })
        .collect();
    Ok(PackageRun {
        id,
        origin_run_id: value.run.id.clone(),
        original_question: value.run.question.clone(),
        answer: value.run.result.text.clone(),
        state: value.run.state,
        result_status: value.run.result.status,
        verification_status: value.run.verification.as_ref().map(|v| v.status),
        checks: value
            .run
            .verification
            .as_ref()
            .map(|v| v.checks.clone())
            .unwrap_or_default(),
        result_checks: value.run.result.verification.clone(),
        source_revision: value.run.workspace_revision.clone(),
        semantics: value.run.contract.as_ref().map(|c| PackageSemantics {
            grain: c.grain.clone(),
            keys: c.group_by.clone(),
            measures: c.measures.clone(),
            time: c.time.clone(),
        }),
        definitions: value
            .run
            .investigation_context
            .as_ref()
            .map(|c| c.definitions.iter().map(|d| d.text.clone()).collect())
            .unwrap_or_default(),
        source_labels: value
            .run
            .source_scope
            .as_ref()
            .map(|scope| {
                scope
                    .used_sources
                    .iter()
                    .map(|s| relative_label(&s.source, &value.artifact.workspace))
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect()
            })
            .unwrap_or_default(),
        methods,
        results,
    })
}

/// Build a self-contained, pathless-by-structure snapshot. Narrative, SQL, and
/// labels can still contain private text: export UI must disclose included data.
/// All required values are re-derived from canonical run evidence, not renderer
/// payloads. Extra artifacts not pinned by the document are rejected.
pub fn export_snapshot(
    document: &StudioDocument,
    resolved: &[ResolvedArtifact],
) -> EngineResult<String> {
    if document.workspace.is_empty()
        || document.placements.len() > MAX_ITEMS
        || resolved.len() > MAX_ITEMS
    {
        return Err(err(
            "report snapshot has invalid workspace or too many items",
        ));
    }
    let needed: BTreeSet<_> = document
        .placements
        .iter()
        .map(|p| (p.artifact_id.clone(), p.revision))
        .collect();
    let mut supplied = BTreeMap::new();
    for value in resolved {
        let key = (value.artifact.id.clone(), value.revision.revision);
        if supplied.insert(key, value).is_some() {
            return Err(err("report artifact pins are duplicated"));
        }
    }
    if needed.iter().ne(supplied.keys()) {
        return Err(err(
            "report does not resolve exactly its pinned artifact revisions",
        ));
    }
    let run_ids: BTreeMap<_, _> = supplied
        .values()
        .map(|r| r.run.id.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .enumerate()
        .map(|(i, id)| (id, format!("run_{i:04}")))
        .collect();
    let mut pins = BTreeMap::new();
    let mut artifacts = Vec::new();
    let mut runs = BTreeMap::new();
    for (index, (key, value)) in supplied.into_iter().enumerate() {
        let id = format!("artifact_{index:04}");
        let run_id = run_ids[&value.run.id].clone();
        let run = packaged_run(run_id.clone(), value)?;
        if runs.get(&run_id).is_some_and(|old| old != &run) {
            return Err(err(
                "one report run identity contains inconsistent saved content",
            ));
        }
        runs.insert(run_id.clone(), run);
        pins.insert(key, id.clone());
        artifacts.push(PackageArtifact {
            id,
            origin_artifact_id: value.artifact.id.clone(),
            origin_revision: value.revision.revision,
            title: value.artifact.title.clone(),
            run_id,
            spec: value.revision.spec.clone(),
            schema_version: value.revision.schema_version.clone(),
            change_note: value.revision.change_note.clone(),
        });
    }
    let package_document = PackageDocument {
        origin_document_id: document.id.clone(),
        origin_version: document.version,
        title: document.title.clone(),
        body: document.body.clone(),
        placements: document
            .placements
            .iter()
            .enumerate()
            .map(|(i, p)| PackagePlacement {
                id: format!("placement_{i:04}"),
                artifact_id: pins[&(p.artifact_id.clone(), p.revision)].clone(),
                label: p.label.clone(),
                width: p.width,
                view_state: p.view_state.clone(),
            })
            .collect(),
    };
    let mut package = ReportPackage {
        format: "fella-report".into(),
        version: PACKAGE_VERSION,
        manifest: PackageManifest {
            generator: "fella-report-v2".into(),
            origin_workspace_fingerprint: digest(document.workspace.as_bytes()),
            snapshot_at_ms: document.updated_at_ms,
            artifact_count: artifacts.len(),
            run_count: runs.len(),
            content_sha256: String::new(),
            exclusions: exclusions(),
        },
        document: package_document,
        artifacts,
        runs: runs.into_values().collect(),
    };
    package.manifest.content_sha256 = content_digest(&package)?;
    let bytes = canonical(&package)?;
    if bytes.len() > MAX_PACKAGE_BYTES {
        return Err(err("report package exceeds the 10 MiB limit"));
    }
    let json = String::from_utf8(bytes).map_err(|_| err("report snapshot is not UTF-8"))?;
    package_validate(&json)?;
    Ok(json)
}

/// Optimistic snapshot read: pins are immutable, and a concurrent document edit
/// invalidates this export rather than mixing narrative from different versions.
pub fn export_studio(data_dir: &Path, workspace: &str, document_id: &str) -> EngineResult<String> {
    let document = artifacts::studio_load(data_dir, workspace, document_id)?;
    let pins: BTreeSet<_> = document
        .placements
        .iter()
        .map(|p| (p.artifact_id.clone(), p.revision))
        .collect();
    let resolved = pins
        .into_iter()
        .map(|(id, revision)| artifacts::resolve_saved(data_dir, &id, Some(revision)))
        .collect::<EngineResult<Vec<_>>>()?;
    let json = export_snapshot(&document, &resolved)?;
    if artifacts::studio_load(data_dir, workspace, document_id)?.version != document.version {
        return Err(err(
            "Studio changed while exporting; export the current document again",
        ));
    }
    Ok(json)
}

fn bounded_text(value: &str, max: usize) -> EngineResult<()> {
    if value.len() > max || value.contains('\0') {
        return Err(err("report text exceeds its limit or contains NUL"));
    }
    Ok(())
}

fn validate_state(value: &Json, depth: usize) -> EngineResult<()> {
    if depth > 16 {
        return Err(err("report view state is nested too deeply"));
    }
    match value {
        Json::Object(map) => {
            for (key, value) in map {
                if matches!(key.as_str(), "__proto__" | "prototype" | "constructor") {
                    return Err(err("unsafe report view-state key"));
                }
                bounded_text(key, 256)?;
                validate_state(value, depth + 1)?;
            }
        }
        Json::Array(items) => {
            for item in items {
                validate_state(item, depth + 1)?;
            }
        }
        Json::String(s) => bounded_text(s, 16 * 1024)?,
        _ => {}
    }
    Ok(())
}

fn validate_results(results: &[BoundResult], cells: &mut usize) -> EngineResult<()> {
    if results.len() > MAX_ITEMS {
        return Err(err("report contains too many result tables"));
    }
    let mut seen = HashSet::new();
    for result in results {
        identity(&result.id)?;
        if !seen.insert(&result.id)
            || result.columns.is_empty()
            || result.columns.len() > 1024
            || result.columns.iter().collect::<HashSet<_>>().len() != result.columns.len()
            || result.rows.len() > 100_000
            || result.rows.len() != result.exact_rows.len()
            || result.row_count < result.rows.len()
            || (result.complete && result.row_count != result.rows.len())
        {
            return Err(err(
                "report result identities, shape, or completeness are invalid",
            ));
        }
        for col in &result.columns {
            bounded_text(col, 4096)?;
            if col.is_empty() {
                return Err(err("report column identity is empty"));
            }
        }
        for (row, exact) in result.rows.iter().zip(&result.exact_rows) {
            if row.len() != result.columns.len() || exact.len() != row.len() {
                return Err(err("report row width is invalid"));
            }
            *cells = cells
                .checked_add(row.len())
                .ok_or_else(|| err("report cell count overflow"))?;
            if *cells > MAX_CELLS {
                return Err(err("report result cell budget exceeded"));
            }
            for (value, stored) in row.iter().zip(exact) {
                if value.is_array() || value.is_object() {
                    return Err(err("report table contains nonscalar data"));
                }
                let expected = value
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| value.to_string());
                if &expected != stored {
                    return Err(err(
                        "report exact values disagree with the saved scalar representation",
                    ));
                }
                bounded_text(stored, MAX_BODY_BYTES)?;
            }
        }
    }
    Ok(())
}

/// Validate only. Checksums detect corruption, not truth/authenticity. This
/// operation has no filesystem, network, renderer, model, or execution access.
pub fn package_validate(json: &str) -> EngineResult<ReportPackage> {
    if json.len() > MAX_PACKAGE_BYTES {
        return Err(err("report package exceeds the 10 MiB limit"));
    }
    let package: ReportPackage = serde_json::from_str(json)?;
    if package.version != PACKAGE_VERSION
        || package.format != "fella-report"
        || package.manifest.generator != "fella-report-v2"
    {
        return Err(err("unsupported report package format or version"));
    }
    if package.artifacts.len() > MAX_ITEMS
        || package.runs.len() > MAX_ITEMS
        || package.document.placements.len() > MAX_ITEMS
        || package.manifest.artifact_count != package.artifacts.len()
        || package.manifest.run_count != package.runs.len()
        || package.manifest.exclusions != exclusions()
    {
        return Err(err("report manifest counts or exclusions are invalid"));
    }
    if package.manifest.origin_workspace_fingerprint.len() != 64
        || !package
            .manifest
            .origin_workspace_fingerprint
            .bytes()
            .all(|b| b.is_ascii_hexdigit())
    {
        return Err(err("report workspace fingerprint is invalid"));
    }
    identity(&package.document.origin_document_id)?;
    if package.document.origin_version == 0 || package.document.title.trim().is_empty() {
        return Err(err("report document identity or title is invalid"));
    }
    bounded_text(&package.document.title, 1024)?;
    bounded_text(&package.document.body, MAX_BODY_BYTES)?;
    let mut artifacts = BTreeMap::new();
    let mut origins = BTreeSet::new();
    for artifact in &package.artifacts {
        identity(&artifact.id)?;
        identity(&artifact.origin_artifact_id)?;
        identity(&artifact.run_id)?;
        if artifacts.insert(&artifact.id, artifact).is_some()
            || !origins.insert((&artifact.origin_artifact_id, artifact.origin_revision))
            || artifact.origin_revision == 0
            || artifact.schema_version != artifacts::SCHEMA_VERSION
        {
            return Err(err(
                "report artifact revisions are duplicated or unsupported",
            ));
        }
        bounded_text(&artifact.title, 1024)?;
        artifacts::validate_spec(&artifact.spec)?;
        if serde_json::to_vec(&artifact.spec).map_or(true, |bytes| bytes.len() > MAX_SPEC_BYTES) {
            return Err(err("report presentation spec exceeds its size limit"));
        }
        bounded_text(&artifact.change_note, 4096)?;
    }
    let mut runs = BTreeMap::new();
    let mut run_origins = HashSet::new();
    let mut cells = 0;
    for run in &package.runs {
        identity(&run.id)?;
        identity(&run.origin_run_id)?;
        if runs.insert(&run.id, run).is_some() || !run_origins.insert(&run.origin_run_id) {
            return Err(err("report run identities are duplicated"));
        }
        bounded_text(&run.original_question, MAX_BODY_BYTES)?;
        bounded_text(&run.answer, MAX_BODY_BYTES)?;
        if run.checks.len() > 1024
            || run.result_checks.len() > 1024
            || run.definitions.len() > 1024
            || run.source_labels.len() > 1024
            || run.methods.len() > MAX_ITEMS
        {
            return Err(err("report provenance exceeds its item budget"));
        }
        for label in &run.source_labels {
            bounded_text(label, 8192)?;
            if relative_label(label, "") != *label {
                return Err(err("report source labels must be normalized relative identities, never filesystem paths"));
            }
        }
        for definition in &run.definitions {
            bounded_text(definition, MAX_BODY_BYTES)?;
        }
        validate_results(&run.results, &mut cells)?;
        let ids: HashSet<_> = run.results.iter().map(|r| r.id.as_str()).collect();
        let mut methods = HashSet::new();
        for method in &run.methods {
            if !ids.contains(method.evidence_id.as_str())
                || !methods.insert(&method.evidence_id)
                || !matches!(
                    method.tool.as_str(),
                    "run_sql" | "run_python" | "forecast_analysis"
                )
            {
                return Err(err(
                    "report method references unavailable or ineligible evidence",
                ));
            }
            if let Some(sql) = &method.sql {
                bounded_text(sql, MAX_BODY_BYTES)?;
            }
        }
        if methods.len() != ids.len() {
            return Err(err(
                "report result is missing its saved computation reference",
            ));
        }
    }
    let mut used_artifacts = HashSet::new();
    let mut placement_ids = HashSet::new();
    for placement in &package.document.placements {
        identity(&placement.id)?;
        identity(&placement.artifact_id)?;
        if !placement_ids.insert(&placement.id) || !artifacts.contains_key(&placement.artifact_id) {
            return Err(err(
                "report placement identities or artifact references are invalid",
            ));
        }
        bounded_text(&placement.label, 160)?;
        if !placement.view_state.is_object() || canonical(&placement.view_state)?.len() > 16 * 1024
        {
            return Err(err("report view state is invalid or too large"));
        }
        validate_state(&placement.view_state, 0)?;
        used_artifacts.insert(&placement.artifact_id);
    }
    if used_artifacts.len() != artifacts.len() {
        return Err(err("report includes unpinned artifacts"));
    }
    let mut used_runs = HashSet::new();
    for artifact in artifacts.values() {
        if !runs.contains_key(&artifact.run_id) {
            return Err(err("report artifact references a missing run snapshot"));
        }
        used_runs.insert(&artifact.run_id);
    }
    if used_runs.len() != runs.len() {
        return Err(err("report includes unrelated run snapshots"));
    }
    if package.manifest.content_sha256 != content_digest(&package)? {
        return Err(err("report content checksum does not match"));
    }
    Ok(package)
}

pub fn package_preview(json: &str) -> EngineResult<ReportPreview> {
    let package = package_validate(json)?;
    Ok(ReportPreview {
        read_only:true,source_authority:false,title:package.document.title.clone(),
        artifact_count:package.artifacts.len(),run_count:package.runs.len(),
        warnings:vec![
            "This is an untrusted, read-only report snapshot. Recorded checks are not independently reverified.".into(),
            "Presentation specs and view state are declarative content; they do not execute SQL, code, or remote requests.".into(),
            "The checksum detects changed content but does not authenticate its author or analytical correctness.".into(),
            "Selected result values, narrative, definitions, and SQL may contain sensitive information. Raw source files and other history are not included.".into(),
        ],package,
    })
}

fn report_id() -> EngineResult<String> {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes).map_err(|e| err(format!("create imported report id: {e}")))?;
    Ok(format!(
        "report_{}",
        bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()
    ))
}

/// Persist only in the dedicated report snapshot store. No artifact/run/source
/// store is modified, no workspace is remapped, and package fields are not paths.
pub fn package_import(data_dir: &Path, json: &str) -> EngineResult<ImportedReport> {
    let preview = package_preview(json)?;
    let id = report_id()?;
    let directory = data_dir.join("report_packages");
    std::fs::create_dir_all(&directory)
        .map_err(|e| EngineError::io("create report snapshot directory", e))?;
    let temporary = directory.join(format!(".{id}.tmp"));
    let destination = directory.join(format!("{id}.json"));
    let bytes = canonical(&preview.package)?;
    let written: EngineResult<()> = (|| {
        let mut file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)
            .map_err(|e| EngineError::io("create report snapshot", e))?;
        file.write_all(&bytes)
            .map_err(|e| EngineError::io("write report snapshot", e))?;
        file.sync_all()
            .map_err(|e| EngineError::io("sync report snapshot", e))?;
        drop(file);
        std::fs::hard_link(&temporary, &destination)
            .map_err(|e| EngineError::io("publish report snapshot", e))?;
        Ok(())
    })();
    let _ = std::fs::remove_file(&temporary);
    written?;
    Ok(ImportedReport { id, preview })
}

pub fn package_load(data_dir: &Path, import_id: &str) -> EngineResult<ImportedReport> {
    identity(import_id)?;
    if !import_id.starts_with("report_")
        || import_id.len() != 39
        || !import_id[7..].bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err(err("invalid imported report id"));
    }
    let path = data_dir
        .join("report_packages")
        .join(format!("{import_id}.json"));
    let metadata = std::fs::symlink_metadata(&path)
        .map_err(|e| EngineError::io("inspect imported report", e))?;
    if !metadata.is_file() || metadata.len() > MAX_PACKAGE_BYTES as u64 {
        return Err(err("imported report is not a bounded regular file"));
    }
    let json =
        std::fs::read_to_string(&path).map_err(|e| EngineError::io("read imported report", e))?;
    Ok(ImportedReport {
        id: import_id.into(),
        preview: package_preview(&json)?,
    })
}

/// Lists bounded, validated report snapshots without promoting their contents
/// into a workspace or canonical analysis history.
pub fn package_list(data_dir: &Path) -> EngineResult<Vec<ImportedReportSummary>> {
    let directory = data_dir.join("report_packages");
    let metadata = match std::fs::symlink_metadata(&directory) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(EngineError::io("inspect imported report directory", error)),
    };
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(err("imported report directory is not a regular directory"));
    }
    let mut reports = Vec::new();
    for entry in std::fs::read_dir(&directory)
        .map_err(|error| EngineError::io("list imported reports", error))?
    {
        let entry = entry.map_err(|error| EngineError::io("read imported report entry", error))?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if !name.starts_with("report_") || !name.ends_with(".json") {
            continue;
        }
        let id = &name[..name.len() - ".json".len()];
        if id.len() != 39 || !id[7..].bytes().all(|byte| byte.is_ascii_hexdigit()) {
            continue;
        }
        let metadata = std::fs::symlink_metadata(entry.path())
            .map_err(|error| EngineError::io("inspect imported report", error))?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err(err("imported report is not a regular file"));
        }
        if metadata.len() > MAX_PACKAGE_BYTES as u64 {
            return Err(err("imported report exceeds the 10 MiB limit"));
        }
        let json = std::fs::read_to_string(entry.path())
            .map_err(|error| EngineError::io("read imported report", error))?;
        let preview = package_preview(&json)?;
        let modified_at_ms = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|duration| duration.as_millis().min(u64::MAX as u128) as u64)
            .unwrap_or(0);
        reports.push(ImportedReportSummary {
            id: id.to_owned(),
            title: preview.title,
            artifact_count: preview.artifact_count,
            run_count: preview.run_count,
            modified_at_ms,
        });
        if reports.len() > 200 {
            return Err(err("too many imported reports; keep at most 200 snapshots"));
        }
    }
    reports.sort_by(|left, right| {
        right
            .modified_at_ms
            .cmp(&left.modified_at_ms)
            .then_with(|| left.id.cmp(&right.id))
    });
    Ok(reports)
}

#[cfg(test)]
mod tests {
    use super::super::artifacts::{
        ArtifactRecord, ArtifactRevision, StudioPlacement, StudioPlacementWidth, StudioUpdate,
        SCHEMA_VERSION,
    };
    use super::*;
    use serde_json::json;
    use std::path::PathBuf;

    const WORKSPACE: &str = "/private/offline-fixture";

    struct Profile(PathBuf);
    impl Profile {
        fn new() -> Self {
            let directory = std::env::temp_dir().join(report_id().unwrap());
            std::fs::create_dir_all(&directory).unwrap();
            Self(directory)
        }
    }
    impl Drop for Profile {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn resolved(id: &str, rows: Json) -> ResolvedArtifact {
        let run:AnalysisTurn = serde_json::from_value(json!({
            "id":id,"conversation_id":"chat-private","question":"Compare revenue by region",
            "workspace":WORKSPACE,"workspace_revision":"source-v1","state":"accepted",
            "source_scope":{"used_sources":[{"source":"sales.csv","table":"sales","evidence_ids":["result-1"]}],"skipped_sources":[]},
            "contract":{"grain":"region","group_by":["region"],"measures":[{"concept":"revenue","field":"revenue","operation":"sum","unit":"USD"}]},
            "plan":{"strategy":"direct_tools","steps":[]},
            "trace":{"id":"trace-private","turn_id":id,"workspace_revision":"source-v1","steps":[]},
            "verification":{"status":"verified","checks":[{"label":"Rerun matched","ok":true}]},
            "result":{"text":"Saved revenue analysis.","status":"verified","verification":[],"evidence":[{
                "id":"result-1","tool":"run_sql","sql":"SELECT region, SUM(amount) AS revenue FROM sales GROUP BY region",
                "args":{"path":"/private/rawsource.csv"},"sources":[{"source":"sales.csv","table":"sales"}],
                "columns":["region","revenue"],"rows":rows,"row_count":rows.as_array().unwrap().len(),"result_summary":"query returned rows"
            }]}
        })).unwrap();
        let revision = ArtifactRevision {
            revision: 1,
            parent_revision: None,
            run_id: run.id.clone(),
            spec: json!({"root":"doc","elements":{"doc":{"type":"Document","props":{},"children":["title"]},"title":{"type":"Title","props":{"content":"Revenue"},"children":[]}}}),
            schema_version: SCHEMA_VERSION.into(),
            change_note: "Initial result".into(),
            created_at_ms: 10,
        };
        let artifact = ArtifactRecord {
            id: "artifact-source".into(),
            workspace: WORKSPACE.into(),
            title: "Revenue".into(),
            current_revision: 1,
            saved: true,
            revisions: vec![revision.clone()],
            created_at_ms: 10,
            updated_at_ms: 10,
        };
        let results = artifacts::eligible_results(&run).unwrap();
        ResolvedArtifact {
            artifact,
            revision,
            run,
            results,
        }
    }

    fn changed(mut value: ResolvedArtifact) -> ResolvedArtifact {
        value.results = artifacts::eligible_results(&value.run).unwrap();
        value
    }

    fn document(values: &[ResolvedArtifact]) -> StudioDocument {
        StudioDocument {
            id:"studio-original".into(),workspace:WORKSPACE.into(),title:"Quarterly review".into(),version:7,body:"# Saved investigation\n\nOriginal narrative.".into(),
            placements:values.iter().enumerate().map(|(i,value)|StudioPlacement { id:format!("placement-{i}"),artifact_id:value.artifact.id.clone(),revision:value.revision.revision,label:String::new(),width:StudioPlacementWidth::Full,view_state:json!({"filter:region":{"resultId":"result-1","field":"region","value":"East"}}) }).collect(),
            checkpoints:vec![],created_at_ms:10,updated_at_ms:100,
        }
    }

    fn package_json() -> String {
        let item = resolved(
            "run-one",
            json!([["East", 9007199254740993_u64], ["West", 0]]),
        );
        export_snapshot(&document(std::slice::from_ref(&item)), &[item]).unwrap()
    }

    fn rechecksum(package: &mut ReportPackage) -> String {
        package.manifest.content_sha256 = content_digest(package).unwrap();
        String::from_utf8(canonical(package).unwrap()).unwrap()
    }

    // Analytical diff contract v1: new assertions are independent of the
    // pre-existing numerical-comparison and portable-report contracts below.
    #[test]
    fn analytical_diff_records_five_dimensions_without_mutating_revisions() {
        let a = resolved("diff-before", json!([["East", 10]]));
        let mut b = resolved("diff-after", json!([["East", 8]]));
        b.revision.revision = 2;
        b.revision.parent_revision = Some(1);
        b.revision.change_note = "Exclude refunded orders".into();
        b.artifact.revisions = vec![a.revision.clone(), b.revision.clone()];
        b.run.contract.as_mut().unwrap().population = Some("Non-refunded orders".into());
        b.run.contract.as_mut().unwrap().filters = serde_json::from_value(
            json!([{"concept":"refunded","exclude":true,"resolved_values":["true"]}]),
        )
        .unwrap();
        b.run.contract.as_mut().unwrap().assumptions = vec!["Posted refunds only".into()];
        b.run.workspace_revision = Some("source-v2".into());
        // Contract v1.1: the source snapshot and its trace describe one run.
        b.run.trace.workspace_revision = Some("source-v2".into());
        b.run.result.evidence[0]["sql"] = json!(
            "SELECT region, SUM(amount) AS revenue FROM sales WHERE NOT refunded GROUP BY region"
        );
        let original = canonical(&b).unwrap();
        let report = compare(&a, &b);
        assert!(report.compatible);
        let diff = report.analytical_diff.unwrap();
        assert_eq!(diff.before.role, "current");
        assert_eq!(diff.after.role, "proposed");
        assert_eq!(diff.meaning.status, DiffStatus::Changed);
        assert!(diff
            .meaning
            .items
            .iter()
            .any(|i| i.label == "Filters" && i.changed));
        assert!(diff
            .inputs
            .items
            .iter()
            .any(|i| i.label == "Source revision" && i.changed));
        assert!(diff
            .method
            .items
            .iter()
            .any(|i| i.label == "Saved computations"
                && i.after.as_ref().unwrap().contains("WHERE NOT refunded")));
        assert_eq!(diff.results.status, DiffStatus::Changed);
        assert!(!diff.impact.scanned);
        assert!(!diff.impact.complete);
        assert_eq!(
            report.tables[0].values[0].delta,
            NumericDelta::ExactInteger { delta: "-2".into() }
        );
        assert_eq!(canonical(&b).unwrap(), original);
        assert_eq!(b.artifact.current_revision, 1);
    }

    #[test]
    fn analytical_diff_presentation_changes_do_not_claim_recomputation() {
        let a = resolved("diff-same-run", json!([["East", 10]]));
        let mut b = a.clone();
        b.revision.revision = 2;
        b.revision.parent_revision = Some(1);
        b.revision.spec = json!({"root":"doc","elements":{"doc":{"type":"Document","props":{},"children":["title"]},"title":{"type":"Title","props":{"content":"A clearer layout"},"children":[]}}});
        b.artifact.revisions.push(b.revision.clone());
        let report = compare(&a, &b);
        assert!(report.compatible);
        let diff = report.analytical_diff.unwrap();
        assert_eq!(diff.before.run_id, diff.after.run_id);
        assert_eq!(diff.meaning.status, DiffStatus::Unchanged);
        assert_eq!(diff.inputs.status, DiffStatus::Unchanged);
        assert_eq!(diff.method.status, DiffStatus::Changed);
        assert_eq!(diff.results.status, DiffStatus::Unchanged);
        assert!(diff
            .results
            .summary
            .starts_with("0 aligned saved values differ"));
        assert_eq!(
            report.tables[0].values[0].delta,
            NumericDelta::ExactInteger { delta: "0".into() }
        );
    }

    #[test]
    fn analytical_diff_missing_contract_explains_limits_without_fabricating_meaning() {
        let mut a = resolved("diff-untyped-before", json!([["East", 10]]));
        let mut b = resolved("diff-untyped-after", json!([["East", 20]]));
        a.run.contract = None;
        b.run.contract = None;
        let report = compare(&a, &b);
        assert!(!report.compatible);
        assert!(report.tables.is_empty());
        let diff = report.analytical_diff.unwrap();
        assert_eq!(diff.results.status, DiffStatus::Unavailable);
        assert!(diff
            .meaning
            .items
            .iter()
            .find(|i| i.label == "Measures and units")
            .unwrap()
            .before
            .is_none());
        assert!(diff
            .meaning
            .limitations
            .iter()
            .any(|l| l.contains("contract was not recorded")));
    }

    #[test]
    fn analytical_diff_untrusted_or_cross_workspace_records_expose_no_details() {
        let a = resolved("diff-private-before", json!([["East", 10]]));
        let mut forged = a.clone();
        forged.results[0].rows[0][1] = json!(999);
        let report = compare(&a, &forged);
        assert!(report.analytical_diff.is_none());
        assert!(report.tables.is_empty());
        let mut foreign = a.clone();
        foreign.artifact.workspace = "/different-workspace".into();
        foreign.run.workspace = Some("/different-workspace".into());
        assert!(compare(&a, &foreign).analytical_diff.is_none());
    }

    #[test]
    fn analytical_diff_impact_indexes_exact_studio_and_checkpoint_pins_without_adopting() {
        let profile = Profile::new();
        let first = resolved("diff-durable-before", json!([["East", 10]]));
        let second = resolved("diff-durable-after", json!([["East", 8]]));
        super::super::analysis_store::create(&profile.0, &first.run).unwrap();
        super::super::analysis_store::create(&profile.0, &second.run).unwrap();
        let record = artifacts::create_from_run(
            &profile.0,
            WORKSPACE,
            &first.run.id,
            "Revenue",
            &json!({"root":"doc","elements":{"doc":{"type":"Document","props":{},"children":[]}}}),
        )
        .unwrap();
        let record = artifacts::revise(
            &profile.0,
            WORKSPACE,
            &record.id,
            1,
            Some(&second.run.id),
            Some(json!({"root":"doc","elements":{"doc":{"type":"Document","props":{},"children":[]}}})),
            "Candidate",
        )
        .unwrap();
        let doc = artifacts::studio_create(&profile.0, WORKSPACE, "Pinned review").unwrap();
        let doc = artifacts::studio_save(
            &profile.0,
            WORKSPACE,
            &doc.id,
            1,
            StudioUpdate {
                title: doc.title.clone(),
                body: "Preserve this review".into(),
                placements: vec![StudioPlacement {
                    id: "baseline-placement".into(),
                    artifact_id: record.id.clone(),
                    revision: 1,
                    label: String::new(),
                    width: StudioPlacementWidth::Full,
                    view_state: json!({}),
                }],
            },
        )
        .unwrap();
        let doc = artifacts::studio_checkpoint(
            &profile.0,
            WORKSPACE,
            &doc.id,
            doc.version,
            "Before changes",
        )
        .unwrap();
        let foreign =
            artifacts::studio_create(&profile.0, "/other-workspace", "Private other workspace")
                .unwrap();
        let a = artifacts::resolve(&profile.0, WORKSPACE, &record.id, Some(1)).unwrap();
        let b = artifacts::resolve(&profile.0, WORKSPACE, &record.id, Some(2)).unwrap();
        // Contract v1.1: report creation intentionally retains the artifact.
        // Capture the baseline after setup, immediately before comparison.
        let record = artifacts::load(&profile.0, WORKSPACE, &record.id).unwrap();
        let original_run = std::fs::read(
            profile
                .0
                .join("analysis/turns")
                .join(format!("{}.json", first.run.id)),
        )
        .unwrap();
        let report = compare_with_dependencies(&profile.0, &a, &b).unwrap();
        let repeated = compare_with_dependencies(&profile.0, &a, &b).unwrap();
        assert_eq!(report, repeated);
        let impact = report.analytical_diff.unwrap().impact;
        assert!(impact.scanned && impact.complete);
        assert_eq!(impact.consumers.len(), 2);
        assert!(impact.consumers.iter().all(|c| c.document_id == doc.id
            && c.document_id != foreign.id
            && c.pinned_revision == 1));
        assert!(impact.consumers.iter().any(|c| c.checkpoint_id.is_some()));
        assert!(impact
            .limitations
            .iter()
            .any(|l| l.contains("exported reports")));
        assert_eq!(
            artifacts::load(&profile.0, WORKSPACE, &record.id).unwrap(),
            record
        );
        assert_eq!(
            artifacts::studio_load(&profile.0, WORKSPACE, &doc.id).unwrap(),
            doc
        );
        assert_eq!(
            std::fs::read(
                profile
                    .0
                    .join("analysis/turns")
                    .join(format!("{}.json", first.run.id))
            )
            .unwrap(),
            original_run
        );
        assert_eq!(record.current_revision, 1);
    }

    #[test]
    fn integers_above_js_precision_have_exact_deltas_and_rows_align_by_keys() {
        let a = resolved(
            "run-one",
            json!([["East", 9007199254740993_u64], ["West", 5]]),
        );
        let b = resolved(
            "run-two",
            json!([["West", 8], ["East", 9007199254740994_u64]]),
        );
        let comparison = compare(&a, &b);
        assert!(comparison.compatible, "{:?}", comparison.reasons);
        assert_eq!(comparison.tables[0].values[0].key, vec!["East"]);
        assert_eq!(comparison.tables[0].values[0].before, "9007199254740993");
        assert_eq!(
            comparison.tables[0].values[0].delta,
            NumericDelta::ExactInteger { delta: "1".into() }
        );
        assert_eq!(
            comparison.tables[0].values[1].delta,
            NumericDelta::ExactInteger { delta: "3".into() }
        );
        assert_eq!(comparison.tables[0].values[0].unit, "USD");
    }

    #[test]
    fn integer_extrema_do_not_overflow_or_fall_back_to_float() {
        let a = resolved("run-one", json!([["East", i64::MIN]]));
        let b = resolved("run-two", json!([["East", u64::MAX]]));
        let report = compare(&a, &b);
        assert!(report.compatible);
        assert_eq!(
            report.tables[0].values[0].delta,
            NumericDelta::ExactInteger {
                delta: "27670116110564327423".into()
            }
        );
    }

    #[test]
    fn null_is_not_zero_and_float_or_mixed_precision_is_explicit() {
        let a = resolved(
            "run-one",
            json!([
                ["East", null],
                ["West", 0],
                ["North", 1.25],
                ["South", 9007199254740993_u64]
            ]),
        );
        let b = resolved(
            "run-two",
            json!([
                ["East", 0],
                ["West", 3],
                ["North", 2.5],
                ["South", 9007199254740994.0]
            ]),
        );
        let report = compare(&a, &b);
        assert!(report.compatible);
        let values = &report.tables[0].values;
        assert_eq!(values[0].before, "null");
        assert!(matches!(values[0].delta, NumericDelta::Unavailable { .. }));
        assert_eq!(values[1].delta, NumericDelta::FiniteFloat { delta: 1.25 });
        assert!(matches!(values[2].delta, NumericDelta::Unavailable { .. }));
        assert_eq!(
            values[3].delta,
            NumericDelta::ExactInteger { delta: "3".into() }
        );
    }

    #[test]
    fn units_grain_periods_scope_missing_contract_and_ambiguous_keys_are_incompatible() {
        let a = resolved("run-one", json!([["East", 10]]));
        let mut unit = resolved("run-two", json!([["East", 20]]));
        unit.run.contract.as_mut().unwrap().measures[0].unit = Some("EUR".into());
        let mut grain = unit.clone();
        grain.run.contract.as_mut().unwrap().grain = Some("order".into());
        let mut period = a.clone();
        period.run.contract.as_mut().unwrap().time = Some(ContractTime {
            range: Some("Q4 2025".into()),
            ..Default::default()
        });
        let mut scope = a.clone();
        scope.run.source_scope.as_mut().unwrap().used_sources[0].source = "other.csv".into();
        let mut no_contract = a.clone();
        no_contract.run.contract = None;
        let duplicate_keys = resolved("run-two", json!([["East", 20], ["East", 30]]));
        let missing_keys = resolved("run-two", json!([["West", 20]]));
        for candidate in [
            unit,
            grain,
            period,
            scope,
            no_contract,
            duplicate_keys,
            missing_keys,
        ] {
            let report = compare(&a, &candidate);
            assert!(!report.compatible);
            assert!(!report.reasons.is_empty());
            assert!(
                report.tables.is_empty(),
                "incompatible comparisons must not contain plausible-looking deltas"
            );
        }
    }

    #[test]
    fn partial_results_and_renderer_forged_values_never_produce_deltas() {
        let a = resolved("run-one", json!([["East", 10]]));
        let mut partial = resolved("run-two", json!([["East", 20]]));
        partial.run.result.evidence[0]["row_count"] = json!(5);
        partial = changed(partial);
        let report = compare(&a, &partial);
        assert!(!report.compatible);
        assert!(report.tables.is_empty());
        let mut forged = a.clone();
        forged.results[0].rows[0][1] = json!(999);
        let report = compare(&a, &forged);
        assert!(!report.compatible);
        assert!(report.tables.is_empty());
    }

    #[test]
    fn filter_changes_show_semantic_warning_and_verification_limits_are_preserved() {
        let a = resolved("run-one", json!([["East", 10]]));
        let mut b = resolved("run-two", json!([["East", 8]]));
        b.run.contract.as_mut().unwrap().filters =
            serde_json::from_value(json!([{"concept":"refunds","exclude":true}])).unwrap();
        b.run.result.status = VerificationStatus::NeedsReview;
        b.run.workspace_revision = Some("source-v2".into());
        b.run.trace.workspace_revision = b.run.workspace_revision.clone();
        let report = compare(&a, &b);
        assert!(report.compatible);
        assert!(report.source_revision_changed);
        assert!(report.definitions_or_filters_changed);
        assert!(report
            .warnings
            .iter()
            .any(|w| w.contains("unresolved verification")));
        assert_eq!(b.run.result.status, VerificationStatus::NeedsReview);
    }

    #[test]
    fn snapshot_export_is_deterministic_deduplicated_and_pathless_by_structure() {
        let first = resolved("run-one", json!([["East", 9007199254740993_u64]]));
        let mut second = first.clone();
        second.artifact.id = "artifact-other".into();
        second.artifact.title = "Another view".into();
        let doc = document(&[second.clone(), first.clone()]);
        let one = export_snapshot(&doc, &[first.clone(), second.clone()]).unwrap();
        let two = export_snapshot(&doc, &[second, first]).unwrap();
        assert_eq!(one, two);
        let package = package_validate(&one).unwrap();
        assert_eq!(package.artifacts.len(), 2);
        assert_eq!(package.runs.len(), 1);
        assert_eq!(
            package.runs[0].results[0].exact_rows[0][1],
            "9007199254740993"
        );
        assert_eq!(package.document.body, doc.body);
        assert_eq!(
            package.document.placements[0].view_state,
            doc.placements[0].view_state
        );
        assert_eq!(package.runs[0].checks[0].label, "Rerun matched");
        assert!(!one.contains(WORKSPACE));
        assert!(!one.contains("chat-private"));
        assert!(!one.contains("/private/rawsource.csv"));
        assert!(package
            .manifest
            .exclusions
            .contains(&"raw_source_files".into()));
        assert!(package
            .manifest
            .exclusions
            .contains(&"checkpoint_history".into()));
    }

    #[test]
    fn outdated_presentation_schemas_are_rejected_by_report_export() {
        let mut item = resolved("legacy-run", json!([["East", 12]]));
        item.revision.schema_version = "fella-json-render-v0".into();
        item.artifact.revisions[0].schema_version = "fella-json-render-v0".into();
        assert!(export_snapshot(
            &document(std::slice::from_ref(&item)),
            std::slice::from_ref(&item),
        )
        .is_err());
    }

    #[test]
    fn packages_preserve_partial_extent_recorded_checks_and_nulls_without_promotion() {
        let mut item = resolved("run-one", json!([["East", null]]));
        item.run.result.evidence[0]["row_count"] = json!(25);
        item.run.state = TurnState::NeedsReview;
        item.run.result.status = VerificationStatus::NeedsReview;
        item.run.result.verification = serde_json::from_value(json!([{"label":"Missing rows","ok":false,"outcome":"flagged","detail":"Incomplete source coverage"}])).unwrap();
        item = changed(item);
        let json = export_snapshot(&document(std::slice::from_ref(&item)), &[item]).unwrap();
        let package = package_validate(&json).unwrap();
        assert!(!package.runs[0].results[0].complete);
        assert_eq!(package.runs[0].results[0].row_count, 25);
        assert_eq!(package.runs[0].results[0].exact_rows[0][1], "null");
        assert_eq!(package.runs[0].state, TurnState::NeedsReview);
        assert!(!package.runs[0].result_checks[0].ok);
    }

    #[test]
    fn package_validation_rejects_unknown_versions_paths_dangling_refs_and_inconsistent_values() {
        let original = package_validate(&package_json()).unwrap();
        let mut version = original.clone();
        version.version = 1;
        let mut dangling = original.clone();
        dangling.artifacts[0].run_id = "missing-run".into();
        let mut exact = original.clone();
        exact.runs[0].results[0].exact_rows[0][1] = "999".into();
        let mut path = original.clone();
        path.runs[0].source_labels = vec!["../../outside.csv".into()];
        let mut windows = original.clone();
        windows.runs[0].source_labels = vec!["C:\\Users\\private.csv".into()];
        let mut state = original.clone();
        state.document.placements[0].view_state = json!({"__proto__":{"polluted":true}});
        for mut bad in [version, dangling, exact, path, windows, state] {
            let encoded = rechecksum(&mut bad);
            assert!(package_validate(&encoded).is_err());
        }
        let mut unknown = serde_json::to_value(original).unwrap();
        unknown["files"] = json!([{"path":"../../escape","content":"bad"}]);
        assert!(package_validate(&unknown.to_string()).is_err());
    }

    #[test]
    fn checksum_changes_are_detected_but_not_presented_as_authentication() {
        let mut package = package_validate(&package_json()).unwrap();
        package.document.body = "Changed narrative".into();
        assert!(package_validate(&serde_json::to_string(&package).unwrap()).is_err());
        let json = rechecksum(&mut package);
        let preview = package_preview(&json).unwrap();
        assert!(preview
            .warnings
            .iter()
            .any(|w| w.contains("does not authenticate")));
        assert!(!preview.source_authority);
    }

    #[test]
    fn packages_enforce_payload_bounds_and_reject_unused_unrelated_runs() {
        assert!(package_validate(&" ".repeat(MAX_PACKAGE_BYTES + 1)).is_err());
        let mut package = package_validate(&package_json()).unwrap();
        let mut extra = package.runs[0].clone();
        extra.id = "run-extra".into();
        extra.origin_run_id = "extra-original".into();
        package.runs.push(extra);
        package.manifest.run_count += 1;
        assert!(package_validate(&rechecksum(&mut package)).is_err());
        let item = resolved("run-one", json!([["East", 1]]));
        let mut doc = document(std::slice::from_ref(&item));
        doc.body = "x".repeat(MAX_BODY_BYTES + 1);
        assert!(export_snapshot(&doc, &[item]).is_err());
    }

    #[test]
    fn offline_import_has_new_report_identity_and_no_workspace_or_run_writes() {
        let profile = Profile::new();
        let package = package_validate(&package_json()).unwrap();
        let json = serde_json::to_string(&package).unwrap();
        let first = package_import(&profile.0, &json).unwrap();
        let second = package_import(&profile.0, &json).unwrap();
        assert_ne!(first.id, second.id);
        assert!(first.preview.read_only);
        assert!(!first.preview.source_authority);
        assert_eq!(
            first.preview.package.artifacts[0].spec,
            package.artifacts[0].spec
        );
        assert!(!profile.0.join("analysis").exists());
        assert!(!profile.0.join("conversations").exists());
        assert_eq!(package_load(&profile.0, &first.id).unwrap(), first);
        assert!(package_load(&profile.0, "../escape").is_err());
        assert_eq!(
            std::fs::read_dir(profile.0.join("report_packages"))
                .unwrap()
                .count(),
            2
        );
        let listed = package_list(&profile.0).unwrap();
        assert_eq!(listed.len(), 2);
        assert!(listed.iter().all(|item| item.title == "Quarterly review"));
        assert!(listed.iter().any(|item| item.id == first.id));
        assert!(listed.iter().any(|item| item.id == second.id));
    }

    #[test]
    fn durable_studio_export_needs_no_live_sources_or_model_and_preserves_pins() {
        let profile = Profile::new();
        let item = resolved("run-one", json!([["East", 10]]));
        super::super::analysis_store::create(&profile.0, &item.run).unwrap();
        let artifact = artifacts::create_from_run(
            &profile.0,
            WORKSPACE,
            &item.run.id,
            "Revenue",
            &item.revision.spec,
        )
        .unwrap();
        let doc = artifacts::studio_create(&profile.0, WORKSPACE, "Saved report").unwrap();
        let doc = artifacts::studio_save(
            &profile.0,
            WORKSPACE,
            &doc.id,
            1,
            StudioUpdate {
                title: doc.title.clone(),
                body: "Review narrative".into(),
                placements: vec![StudioPlacement {
                    id: "p-1".into(),
                    artifact_id: artifact.id.clone(),
                    revision: 1,
                    label: String::new(),
                    width: StudioPlacementWidth::Full,
                    view_state: json!({}),
                }],
            },
        )
        .unwrap();
        let json = export_studio(&profile.0, WORKSPACE, &doc.id).unwrap();
        let package = package_validate(&json).unwrap();
        assert_eq!(package.document.body, "Review narrative");
        assert_eq!(package.artifacts[0].origin_artifact_id, artifact.id);
        assert_eq!(package.artifacts[0].origin_revision, 1);
        assert_eq!(package.runs[0].origin_run_id, item.run.id);
        assert!(export_studio(&profile.0, "/other", &doc.id).is_err());
        assert_eq!(
            super::super::analysis_store::delete_conversation(&profile.0, "chat-private").unwrap(),
            0
        );
        assert_eq!(export_studio(&profile.0, WORKSPACE, &doc.id).unwrap(), json);
        assert!(package_preview(&json).is_ok());
    }

    #[test]
    fn conflicting_or_extra_snapshot_inputs_are_rejected_not_exported() {
        let item = resolved("run-one", json!([["East", 10]]));
        let doc = document(std::slice::from_ref(&item));
        assert!(export_snapshot(&doc, &[]).is_err());
        let mut other = item.clone();
        other.artifact.id = "unselected".into();
        assert!(export_snapshot(&doc, &[item.clone(), other]).is_err());
        let mut wrong = item.clone();
        wrong.results[0].rows[0][1] = json!(9999);
        assert!(export_snapshot(&doc, &[wrong]).is_err());
        let mut foreign = item.clone();
        foreign.artifact.workspace = "/other".into();
        assert!(export_snapshot(&doc, &[foreign]).is_err());
    }
}
