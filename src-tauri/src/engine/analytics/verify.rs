//! Verifier checks produce typed findings with explicit authority and scope.
//! Objective defects can exclude a specific evidence item or artifact; they do
//! not implicitly invalidate unrelated results or block the whole answer.
//! Whole-answer blocking is reserved for explicit answer-scoped findings such
//! as a workspace revision changing during the turn. Same-model disagreement
//! remains advisory, not an independent correctness oracle.

use std::collections::HashSet;

use serde_json::Value as Json;

use crate::engine::analytics::chart;
use crate::engine::analytics::data::quote_str;
use crate::engine::analytics::AnalyticsSource;
use crate::engine::catalog::{source_scope, SourceScope};
use crate::engine::evidence::{
    EvidenceItem, VerificationCheck, VerificationEffect, VerificationFinding,
    VerificationFindingCode, VerificationStatus, VerificationTarget,
};
use crate::engine::grounding::{GroundingReport, ProbeOutcome};
use crate::engine::risk;
use crate::engine::runtime::{
    AnalysisContract, ContractDerivedMetric, ContractJoin, ContractOrder, InterpretationStatus,
    SortDirection, TimeBucket,
};

fn is_sql_evidence(evidence: &EvidenceItem) -> bool {
    matches!(
        evidence.tool.as_str(),
        "run_sql" | "make_chart" | "forecast_analysis"
    )
}

pub fn run(
    engine: &dyn AnalyticsSource,
    question: &str,
    answer: &str,
    evidence: &[EvidenceItem],
    contract: Option<&AnalysisContract>,
) -> Vec<VerificationCheck> {
    let mut checks = Vec::new();

    check_tables(engine, evidence, &mut checks);
    check_source_scope(engine, question, evidence, &mut checks);
    rerun_queries(engine, evidence, &mut checks);
    rerun_python_inputs(engine, evidence, &mut checks);
    check_numbers(question, answer, evidence, contract, &mut checks);
    check_signed_semantics(question, evidence, &mut checks);
    check_text_agg(engine, evidence, &mut checks);
    check_case_filter(engine, evidence, &mut checks);
    check_aggregate_verb(question, evidence, &mut checks);
    check_dropped_column(engine, question, evidence, &mut checks);
    check_null_group_key(evidence, &mut checks);
    check_incomplete_quality_audit(question, evidence, &mut checks);
    check_row_value_labels(answer, evidence, &mut checks);
    check_chart_values(evidence, &mut checks);
    check_empty_aggregate(engine, evidence, &mut checks);
    check_forecast_evaluation(question, evidence, &mut checks);

    checks
}

/// Full final-answer verification, including contract-level checks that need
/// independent SQL probes. The agent loop uses this before repair so the
/// findings can guide a targeted revision rather than appearing only after
/// the answer is already final.
pub fn run_for_answer(
    engine: &dyn AnalyticsSource,
    question: &str,
    answer: &str,
    evidence: &[EvidenceItem],
    contract: Option<&AnalysisContract>,
    grounding: Option<&GroundingReport>,
) -> Vec<VerificationCheck> {
    let mut checks = run(engine, question, answer, evidence, contract);
    if let Some(contract) =
        contract.filter(|value| value.interpretation == InterpretationStatus::Grounded)
    {
        checks.extend(execution_checks(engine, contract, grounding, evidence));
    }
    checks
}

/// Whole-answer gates are deliberately rare and opt-in. Evidence replay,
/// unsupported claims, and broken artifacts are scoped to their target and
/// must not turn unrelated supported output into a failed answer.
pub fn hard_fail(checks: &[VerificationCheck]) -> Option<String> {
    checks
        .iter()
        .find(|check| {
            !check.ok
                && check
                    .finding
                    .as_ref()
                    .is_some_and(|finding| finding.effect == VerificationEffect::BlockAnswer)
        })
        .map(|check| match &check.detail {
            Some(detail) => format!("{} ({detail})", check.label),
            None => check.label.clone(),
        })
}

/// A bounded, tool-backed repair pass handles semantic mistakes that cannot
/// safely be fixed by asking for prose alone. Any unbacked numeric
/// claim needs another look: it may be an omitted derived computation or an
/// unsupported claim, regardless of whether earlier evidence was scalar or
/// grouped. The model can compute the requested value or retract it; the
/// runtime does not guess which correction is right.
pub fn semantic_repair_hint(checks: &[VerificationCheck]) -> Option<String> {
    semantic_repair_check(checks).map(|check| {
        let finding = check
            .finding
            .as_ref()
            .expect("repair checks carry a finding");
        let guidance = finding
            .guidance
            .clone()
            .or_else(|| check.detail.clone())
            .unwrap_or_else(|| "repair only the identified target".into());
        if guidance == check.label {
            check.label.clone()
        } else {
            format!("{}: {guidance}", check.label)
        }
    })
}

/// Return the first actionable check. Repairs and evidence/artifact
/// invalidations are intentionally represented separately so the loop never
/// has to guess the target from user-facing label text.
pub fn semantic_repair_check(checks: &[VerificationCheck]) -> Option<&VerificationCheck> {
    [
        VerificationEffect::ExcludeEvidence,
        VerificationEffect::WithholdArtifact,
        VerificationEffect::Repair,
    ]
    .into_iter()
    .find_map(|effect| {
        checks.iter().find(|check| {
            !check.ok
                && check
                    .finding
                    .as_ref()
                    .is_some_and(|finding| finding.effect == effect)
        })
    })
}

/// True when a query behind the answer was actually re-executed and matched,
/// and nothing failed hard. Recorded on the folder's episode log as
/// `"verified"` a label, not something memory acts on (`engine::memory`
/// caches no query, however cleanly it verified).
pub fn reran_clean(checks: &[VerificationCheck]) -> bool {
    hard_fail(checks).is_none()
        && !checks.iter().any(|check| {
            !check.ok
                && check
                    .finding
                    .as_ref()
                    .is_some_and(|finding| finding.code == VerificationFindingCode::ReplayMismatch)
        })
        && checks.iter().any(|c| {
            c.ok && c
                .finding
                .as_ref()
                .is_some_and(|finding| finding.code == VerificationFindingCode::ReplayMatch)
        })
}

fn scoped_finding(
    code: VerificationFindingCode,
    effect: VerificationEffect,
    target: VerificationTarget,
    target_id: Option<String>,
    evidence_ids: Vec<String>,
    guidance: Option<String>,
) -> VerificationFinding {
    VerificationFinding {
        code,
        effect,
        target,
        target_id,
        evidence_ids,
        guidance,
    }
}

fn targeted_warning(
    label: impl Into<String>,
    detail: Option<String>,
    finding: VerificationFinding,
) -> VerificationCheck {
    VerificationCheck {
        label: label.into(),
        ok: false,
        detail,
        finding: Some(finding),
    }
}

fn warn_with_effect(
    label: impl Into<String>,
    detail: Option<String>,
    code: VerificationFindingCode,
    effect: VerificationEffect,
    target: VerificationTarget,
    target_id: Option<String>,
    evidence_ids: Vec<String>,
    guidance: Option<String>,
) -> VerificationCheck {
    targeted_warning(
        label,
        detail,
        scoped_finding(code, effect, target, target_id, evidence_ids, guidance),
    )
}

fn successful_replay_check(label: impl Into<String>) -> VerificationCheck {
    VerificationCheck {
        label: label.into(),
        ok: true,
        detail: None,
        finding: Some(scoped_finding(
            VerificationFindingCode::ReplayMatch,
            VerificationEffect::Informational,
            VerificationTarget::Answer,
            None,
            Vec::new(),
            None,
        )),
    }
}

fn repair_claim_warning(
    label: impl Into<String>,
    detail: Option<String>,
    code: VerificationFindingCode,
    claim: impl Into<String>,
    evidence_ids: Vec<String>,
    guidance: impl Into<String>,
) -> VerificationCheck {
    warn_with_effect(
        label,
        detail,
        code,
        VerificationEffect::Repair,
        VerificationTarget::Claim,
        Some(claim.into()),
        evidence_ids,
        Some(guidance.into()),
    )
}

fn exclude_evidence_warning(
    label: impl Into<String>,
    detail: Option<String>,
    code: VerificationFindingCode,
    evidence_ids: Vec<String>,
    guidance: impl Into<String>,
) -> VerificationCheck {
    warn_with_effect(
        label,
        detail,
        code,
        VerificationEffect::ExcludeEvidence,
        VerificationTarget::Evidence,
        evidence_ids.first().cloned(),
        evidence_ids,
        Some(guidance.into()),
    )
}

fn withhold_artifact_warning(
    label: impl Into<String>,
    detail: Option<String>,
    code: VerificationFindingCode,
    artifact: impl Into<String>,
    evidence_ids: Vec<String>,
    guidance: impl Into<String>,
) -> VerificationCheck {
    warn_with_effect(
        label,
        detail,
        code,
        VerificationEffect::WithholdArtifact,
        VerificationTarget::Artifact,
        Some(artifact.into()),
        evidence_ids,
        Some(guidance.into()),
    )
}

fn withhold_chart_warning(
    item: &EvidenceItem,
    source: &EvidenceItem,
    label: impl Into<String>,
    detail: Option<String>,
) -> VerificationCheck {
    withhold_artifact_warning(
        label,
        detail,
        VerificationFindingCode::ChartMismatch,
        item.id.clone(),
        vec![source.id.clone()],
        "withhold or rebuild only this chart; retain its source result and answer text",
    )
}

/// Classify the complete answer result once. A clean replay proves that the
/// computation is reproducible, not that the model selected the right meaning
/// or scope. Reserve `Verified` for a replayed result whose interpretation was
/// separately grounded against the current workspace (or resolved by the
/// user and then grounded by the runtime).
pub fn status(
    checks: &[VerificationCheck],
    evidence: &[EvidenceItem],
    semantics_grounded: bool,
) -> VerificationStatus {
    if evidence.is_empty() || !evidence.iter().any(EvidenceItem::is_accepted) {
        return VerificationStatus::InsufficientData;
    }
    if hard_fail(checks).is_some() {
        return VerificationStatus::Failed;
    }
    if checks.iter().any(|check| !check.ok) {
        return VerificationStatus::NeedsReview;
    }
    let withheld_chart_without_replacement = evidence.iter().any(|item| {
        item.artifact_is_withheld("chart")
            && item.tool == "make_chart"
            && !evidence.iter().any(|replacement| {
                replacement.tool == "make_chart"
                    && replacement.is_accepted()
                    && !replacement.artifact_is_withheld("chart")
            })
    });
    if withheld_chart_without_replacement {
        return VerificationStatus::NeedsReview;
    }
    if semantics_grounded && reran_clean(checks) {
        VerificationStatus::Verified
    } else {
        VerificationStatus::NeedsReview
    }
}

fn scope_word(question: &str, words: &[&str]) -> bool {
    let question = question.to_ascii_lowercase();
    words.iter().any(|word| contains_word(&question, word))
}

fn requested_scope(question: &str) -> Option<SourceScope> {
    let current = scope_word(
        question,
        &["current", "latest", "active", "live", "present", "primary"],
    );
    let historical = scope_word(
        question,
        &[
            "archive",
            "archived",
            "old",
            "legacy",
            "historical",
            "backup",
            "snapshot",
            "prior",
            "previous",
        ],
    );
    if current && historical {
        let lower = question.to_ascii_lowercase();
        let compares = [
            "compare",
            "versus",
            " vs ",
            "both",
            "each",
            "difference",
            "between",
            "all files",
            "all exports",
        ]
        .iter()
        .any(|marker| lower.contains(marker));
        if compares {
            return None;
        }
    }
    if current {
        Some(SourceScope::Current)
    } else if historical {
        Some(SourceScope::Historical)
    } else {
        None
    }
}

fn check_source_scope(
    engine: &dyn AnalyticsSource,
    question: &str,
    evidence: &[EvidenceItem],
    out: &mut Vec<VerificationCheck>,
) {
    let Some(requested) = requested_scope(question) else {
        return;
    };
    let catalog = engine.catalog();
    let mut mismatched = Vec::new();
    let mut mismatched_items = Vec::new();
    for item in evidence
        .iter()
        .filter(|item| is_sql_evidence(item) && item.is_accepted())
    {
        for source in &item.sources {
            let Some(info) = catalog.sources.iter().find(|candidate| {
                candidate.name == source.source
                    || candidate.view.as_deref() == Some(source.table.as_str())
            }) else {
                continue;
            };
            let scope = source_scope(&info.name, &info.path, info.view.as_deref());
            let mismatch = matches!(
                (requested, scope),
                (SourceScope::Current, SourceScope::Historical)
                    | (SourceScope::Historical, SourceScope::Current)
            );
            if mismatch {
                mismatched.push(info.name.clone());
                mismatched_items.push(item.id.clone());
            }
        }
    }
    mismatched.sort();
    mismatched.dedup();
    if mismatched.is_empty() {
        return;
    }
    let requested_label = match requested {
        SourceScope::Current => "current",
        SourceScope::Historical => "historical",
        SourceScope::Unknown => return,
    };
    mismatched_items.sort();
    mismatched_items.dedup();
    out.push(exclude_evidence_warning(
        format!("requested {requested_label} scope was not preserved by the query"),
        Some(format!(
            "the evidence also used {} source file(s): {}",
            mismatched.len(),
            mismatched.join(", ")
        )),
        VerificationFindingCode::SemanticExecutionMismatch,
        mismatched_items,
        "rerun only the affected query against the requested source scope; keep unrelated results",
    ));
}

pub(crate) fn is_aggregate_sql(sql: &str) -> bool {
    let lower = sql.to_ascii_lowercase();
    [
        "sum(", "avg(", "total(", "count(", "min(", "max(", "median(",
    ]
    .iter()
    .any(|function| lower.contains(function))
}

fn null_result(rows: &[Vec<Json>]) -> bool {
    rows.is_empty() || (rows.len() == 1 && rows[0].iter().all(Json::is_null))
}

/// A non-empty source plus a null aggregate result is usually a bad semantic
/// filter, not a measured zero. Restrict this to filtered aggregate queries so
/// an intentionally empty/all-null measure remains a reviewable result rather
/// than causing a repair loop on every dataset.
fn check_empty_aggregate(
    engine: &dyn AnalyticsSource,
    evidence: &[EvidenceItem],
    out: &mut Vec<VerificationCheck>,
) {
    let catalog = engine.catalog();
    for item in evidence
        .iter()
        .filter(|item| is_sql_evidence(item) && item.is_accepted())
    {
        let Some(sql) = item.sql.as_deref() else {
            continue;
        };
        let lower = sql.to_ascii_lowercase();
        let filtered = lower.contains(" where ")
            || lower.contains(" where\n")
            || lower.contains(" having ")
            || lower.contains("case when ");
        let Some(rows) = item.rows.as_deref() else {
            continue;
        };
        if !filtered || !is_aggregate_sql(sql) || !null_result(rows) {
            continue;
        }
        let non_empty_source = item.sources.iter().any(|source| {
            catalog
                .sources
                .iter()
                .find(|candidate| {
                    candidate.name == source.source
                        || candidate.view.as_deref() == Some(source.table.as_str())
                })
                .and_then(|source| source.row_count)
                .is_some_and(|row_count| row_count > 0)
        });
        if non_empty_source {
            out.push(exclude_evidence_warning(
                "aggregate query matched no rows in a non-empty source",
                Some(
                    "the query returned NULL for a filtered aggregate; inspect the filter and source values before treating this as zero"
                        .into(),
                ),
                VerificationFindingCode::SemanticExecutionMismatch,
                vec![item.id.clone()],
                "inspect the affected filter and rerun this aggregate; do not treat NULL as zero",
            ));
            return;
        }
    }
}

/// A forecast built from workspace observations should compare against
/// chronological holdouts when they can be formed. This is a soft nudge, not
/// a reason to suppress the point estimate: the model can add the dedicated
/// forecast tool or a replayable custom Python backtest, and short-history
/// cases remain answerable with an explicit limitation.
fn check_forecast_evaluation(
    question: &str,
    evidence: &[EvidenceItem],
    out: &mut Vec<VerificationCheck>,
) {
    let predictive = risk::assess(question)
        .signals
        .iter()
        .any(|signal| signal == "forecast");
    if !predictive {
        return;
    }

    // Showing a forecast already present in a mounted table is not a new
    // forecast from Fella. Do not create a needless evaluation/backtest step
    // after the requested chart has already reused those supplied values.
    if charts_precomputed_forecast(evidence) {
        return;
    }

    let has_workspace_computation = evidence.iter().any(|item| {
        item.is_accepted()
            && matches!(
                item.tool.as_str(),
                "run_sql" | "run_python" | "forecast_analysis"
            )
    });
    if !has_workspace_computation {
        return;
    }

    let has_evaluation = evidence.iter().any(|item| {
        if !item.is_accepted() {
            return false;
        }
        if item.tool == "forecast_analysis" {
            return item
                .output
                .as_deref()
                .is_some_and(|output| output.contains("backtest="));
        }
        item.tool == "run_python"
            && item
                .args
                .get("code")
                .and_then(Json::as_str)
                .is_some_and(|code| code.contains("rolling_origin_backtest"))
            && item
                .output
                .as_deref()
                .is_some_and(|output| output.contains("baseline_mae") && output.contains("mae"))
            && item.python_queries_complete == Some(true)
            && item
                .python_queries
                .as_ref()
                .is_some_and(|queries| !queries.is_empty())
    });
    if !has_evaluation {
        out.push(warn(
            "forecast lacks chronological evaluation",
            Some(
                "the workspace-backed point estimate can remain, but add a chronological rolling-origin comparison against a baseline with an interpretable error metric when history permits; use forecast_analysis for a regular series, or a replayable run_python backtest for a custom series, and state when no holdout is available".into(),
            ),
        ));
    }
}

fn charts_precomputed_forecast(evidence: &[EvidenceItem]) -> bool {
    evidence.iter().any(|chart_item| {
        if chart_item.tool != "make_chart"
            || !chart_item.is_accepted()
            || chart_item.artifact_is_withheld("chart")
        {
            return false;
        }
        let Some(chart) = chart_item.chart.as_ref() else {
            return false;
        };
        if chart.kind != chart::ChartKind::Forecast
            || !matches!(
                chart.payload.as_ref(),
                Some(chart::ChartPayload::Forecast { .. })
            )
        {
            return false;
        }
        let metadata = chart.metadata.as_ref();
        let has_required_fields = metadata.is_some_and(|metadata| {
            ["observed", "forecast"].iter().all(|field| {
                metadata
                    .fields
                    .iter()
                    .any(|candidate| candidate.eq_ignore_ascii_case(field))
            })
        });
        if !has_required_fields {
            return false;
        }

        let source = metadata
            .and_then(|metadata| metadata.source_evidence_id.as_deref())
            .and_then(|id| evidence.iter().find(|candidate| candidate.id == id))
            .unwrap_or(chart_item);
        let is_reused_tabular_source =
            source.tool == "run_sql" || (source.tool == "make_chart" && source.id == chart_item.id);
        is_reused_tabular_source
            && source.is_accepted()
            && source.sql.as_deref().is_some_and(|sql| {
                !is_aggregate_sql(sql)
                    && !sql
                        .split_whitespace()
                        .collect::<Vec<_>>()
                        .windows(2)
                        .any(|words| {
                            words[0].eq_ignore_ascii_case("group")
                                && words[1].eq_ignore_ascii_case("by")
                        })
            })
    })
}

/// Verify that a semantic contract survived the transition into physical
/// evidence. This is intentionally a lexical/structural gate, not a second
/// interpretation model: the grounding layer resolves names, and this layer
/// checks that the executed query actually used those bindings.
pub fn contract_checks(
    contract: &AnalysisContract,
    grounding: Option<&GroundingReport>,
    evidence: &[EvidenceItem],
) -> Vec<VerificationCheck> {
    let mut checks = Vec::new();
    match contract.interpretation {
        InterpretationStatus::Grounded => checks.push(ok("the semantic contract was grounded")),
        InterpretationStatus::Ambiguous => checks.push(warn(
            "the semantic contract remains ambiguous",
            Some(contract.unresolved.join("; ")),
        )),
        InterpretationStatus::Unsupported => checks.push(warn(
            "the semantic contract is unsupported",
            Some("the runtime could not map this request to a supported analysis path".into()),
        )),
        InterpretationStatus::Assumed | InterpretationStatus::Unresolved => checks.push(warn(
            "the semantic contract was not grounded",
            Some("the runtime has not established a data-backed interpretation".into()),
        )),
    }

    if let Some(report) = grounding {
        for probe in &report.probes {
            match probe.outcome {
                ProbeOutcome::Ambiguous | ProbeOutcome::Unavailable => checks.push(warn(
                    format!("grounding probe needs review: {}", probe.target),
                    Some(probe.detail.clone()),
                )),
                ProbeOutcome::Resolved | ProbeOutcome::NotObserved => {}
            }
        }
    } else {
        checks.push(warn(
            "the semantic contract has no grounding report",
            Some("a contract cannot be accepted without a revision-bound grounding step".into()),
        ));
    }

    let sql: Vec<String> = evidence
        .iter()
        .filter(|item| is_sql_evidence(item) && item.is_accepted())
        .filter_map(|item| item.sql.clone())
        .map(|query| query.to_ascii_lowercase())
        .collect();

    for measure in &contract.measures {
        if let Some(field) = measure.field.as_deref() {
            check_binding_usage(&mut checks, "measure", field, &sql);
        }
        check_measure_operation(&mut checks, &measure.operation, &sql, evidence);
    }
    for filter in &contract.filters {
        if let Some(field) = filter.field.as_deref() {
            check_binding_usage(&mut checks, "filter", field, &sql);
        }
        for value in &filter.resolved_values {
            check_literal_usage(
                &mut checks,
                "filter value",
                filter.field.as_deref(),
                value,
                &sql,
            );
        }
        check_filter_polarity(&mut checks, filter, &sql);
    }
    if let Some(time) = &contract.time {
        // A bare time field can describe the source's temporal grain without
        // being a predicate in the query. Require SQL usage only when the
        // contract actually asks for a range, bucket, or typed comparison.
        if time.range.is_some() || time.bucket.is_some() || contract.comparison_spec.is_some() {
            if let Some(field) = time.field.as_deref() {
                check_binding_usage(&mut checks, "time field", field, &sql);
            }
        }
        if let Some(bucket) = time.bucket {
            check_time_bucket_usage(
                &mut checks,
                bucket,
                &sql,
                contract.comparison_spec.is_some(),
            );
        }
    }
    for field in &contract.group_by {
        check_binding_usage(&mut checks, "grouping field", field, &sql);
    }
    if let Some(order) = &contract.order_by {
        check_order_usage(&mut checks, contract, order, &sql);
    }
    if let Some(limit) = contract.limit {
        check_limit_usage(&mut checks, limit, &sql);
    }
    for (index, derived) in contract.derived_metrics.iter().enumerate() {
        check_derived_usage(&mut checks, contract, index, derived, &sql);
        check_derived_arithmetic(&mut checks, contract, index, derived, evidence);
        check_derived_population(&mut checks, contract, index, derived, evidence);
    }
    for join in &contract.joins {
        check_join_usage(&mut checks, join, &sql);
    }
    if let Some(comparison) = &contract.comparison_spec {
        check_comparison_usage(&mut checks, contract, comparison, &sql);
        check_comparison_arithmetic(&mut checks, contract, evidence);
    }
    if contract.interpretation == InterpretationStatus::Grounded {
        let related_evidence: Vec<String> = evidence
            .iter()
            .filter(|item| item.is_accepted() && is_sql_evidence(item))
            .map(|item| item.id.clone())
            .collect();
        for check in &mut checks {
            if !check.ok
                && check
                    .finding
                    .as_ref()
                    .is_some_and(|finding| finding.code == VerificationFindingCode::Advisory)
            {
                check.finding = Some(scoped_finding(
                    VerificationFindingCode::ContractMismatch,
                    VerificationEffect::Repair,
                    VerificationTarget::Claim,
                    Some(check.label.clone()),
                    related_evidence.clone(),
                    Some("reconcile this grounded contract requirement with the query and revise only the affected part".into()),
                ));
            }
        }
    }
    checks
}

/// Rebuild chart payloads from their source evidence so a malformed bridge
/// payload cannot make a chart disagree with the result it represents. This
/// also covers Python-published tables and forecast output, not only SQL.
fn check_chart_values(evidence: &[EvidenceItem], checks: &mut Vec<VerificationCheck>) {
    for item in evidence.iter().filter(|item| {
        item.tool == "make_chart" && item.is_accepted() && !item.artifact_is_withheld("chart")
    }) {
        let Some(chart_data) = item.chart.as_ref() else {
            continue;
        };
        let source = chart_data
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.source_evidence_id.as_deref())
            .and_then(|id| evidence.iter().find(|candidate| candidate.id == id))
            .unwrap_or(item);
        if !source.is_accepted() {
            checks.push(withhold_chart_warning(
                item,
                source,
                "chart source result could not be checked",
                Some("the referenced analytical result failed".into()),
            ));
            continue;
        }

        let table = if let Some(table) = &source.result_table {
            table.clone()
        } else if let (Some(columns), Some(rows)) = (&source.columns, &source.rows) {
            if source.row_count.is_some_and(|count| count != rows.len()) {
                checks.push(withhold_chart_warning(
                    item,
                    source,
                    "chart source result could not be checked",
                    Some("the referenced result contained an incomplete row set".into()),
                ));
                continue;
            }
            chart::TabularResult {
                columns: columns.clone(),
                rows: rows.clone(),
            }
        } else {
            checks.push(withhold_chart_warning(
                item,
                source,
                "chart source result could not be checked",
                Some("the referenced result did not include a reusable typed table".into()),
            ));
            continue;
        };
        let args = &item.args;
        let arg_string = |name: &str| {
            args.get(name)
                .and_then(Json::as_str)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
        };
        let arg_strings = |name: &str| {
            args.get(name)
                .and_then(Json::as_array)
                .map(|values| {
                    values
                        .iter()
                        .filter_map(Json::as_str)
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default()
        };
        let requested_kind = args
            .get("kind")
            .cloned()
            .and_then(|value| serde_json::from_value(value).ok())
            .unwrap_or(chart_data.kind);
        let missing_treatment = args
            .get("missing_treatment")
            .cloned()
            .and_then(|value| serde_json::from_value(value).ok())
            .unwrap_or_default();
        let expected = match chart::from_table(
            &table,
            chart::ChartRequest {
                kind: requested_kind,
                title: arg_string("title").or_else(|| chart_data.title.clone()),
                unit: arg_string("unit").or_else(|| chart_data.unit.clone()),
                x_field: arg_string("x_field"),
                y_field: arg_string("y_field"),
                value_field: arg_string("value_field"),
                group_field: arg_string("group_field"),
                label_field: arg_string("label_field"),
                series_fields: arg_strings("series_fields"),
                bin_count: args
                    .get("bin_count")
                    .and_then(Json::as_u64)
                    .map(|v| v as usize),
                missing_treatment,
                metadata: chart_data.metadata.clone().unwrap_or_default(),
            },
        ) {
            Ok(expected) => expected,
            Err(error) => {
                checks.push(withhold_chart_warning(
                    item,
                    source,
                    "chart values did not match source query",
                    Some(error),
                ));
                continue;
            }
        };
        if serde_json::to_value(expected).ok() == serde_json::to_value(chart_data).ok() {
            checks.push(ok("chart values matched the source query"));
        } else {
            checks.push(withhold_chart_warning(
                item,
                source,
                "chart values did not match source query",
                Some(
                    "the chart specification differs from a fresh projection of its source result"
                        .into(),
                ),
            ));
        }
    }
}

/// Run contract checks that need the read-only execution seam as well as the
/// returned evidence. These checks deliberately keep their own verification
/// query out of `EvidenceItem`: it is an internal acceptance check, not a
/// second piece of user-facing evidence.
pub fn execution_checks(
    engine: &dyn AnalyticsSource,
    contract: &AnalysisContract,
    grounding: Option<&GroundingReport>,
    evidence: &[EvidenceItem],
) -> Vec<VerificationCheck> {
    let mut checks = contract_checks(contract, grounding, evidence);
    check_grouped_totals(&mut checks, engine, contract, grounding, evidence);
    check_average_bounds(&mut checks, engine, contract, grounding, evidence);
    checks
}

/// Reconcile an unbounded grouped result with an independently compiled total
/// for additive measures. This catches a class of plausible-looking answers
/// where a grouping expression silently drops rows or partitions the result
/// incorrectly. Limited rankings, non-additive measures, and incomplete result
/// sets are intentionally left for a later invariant rather than guessed at.
fn check_grouped_totals(
    checks: &mut Vec<VerificationCheck>,
    engine: &dyn AnalyticsSource,
    contract: &AnalysisContract,
    grounding: Option<&GroundingReport>,
    evidence: &[EvidenceItem],
) {
    if contract.interpretation != InterpretationStatus::Grounded
        || contract.group_by.is_empty()
        || contract.limit.is_some()
        || contract.measures.is_empty()
        || contract
            .measures
            .iter()
            .any(|measure| !is_additive_operation(&measure.operation))
    {
        return;
    }

    let aliases = grouped_measure_aliases(contract);
    let Some(grouped) = evidence.iter().find(|item| {
        is_sql_evidence(item)
            && item.is_accepted()
            && item
                .row_count
                .is_some_and(|count| item.rows.as_ref().is_some_and(|rows| count == rows.len()))
            && item.columns.as_ref().is_some_and(|columns| {
                aliases.iter().all(|measure_aliases| {
                    measure_aliases.iter().all(|alias| {
                        columns
                            .iter()
                            .any(|column| column.eq_ignore_ascii_case(alias))
                    })
                })
            })
    }) else {
        return;
    };

    let mut total_contract = contract.clone();
    total_contract.group_by.clear();
    total_contract.order_by = None;
    total_contract.limit = None;
    if let Some(time) = total_contract.time.as_mut() {
        time.bucket = None;
    }
    let source_hint = grounding.and_then(|report| report.source.as_deref());
    let total_plan =
        match crate::engine::planner::compile(&engine.catalog(), &total_contract, source_hint) {
            Ok(plan) => plan,
            Err(_) => return,
        };
    let total = match engine.run_sql(&total_plan.sql) {
        Ok(result) => result,
        Err(error) => {
            checks.push(warn(
                "grouped totals could not be independently checked",
                Some(error.to_string()),
            ));
            return;
        }
    };
    let Some(total_row) = total.rows.first() else {
        checks.push(warn(
            "grouped totals could not be independently checked",
            Some("the independent total query returned no row".into()),
        ));
        return;
    };

    let columns = grouped.columns.as_deref().unwrap_or_default();
    let total_columns = total.columns.as_slice();
    let rows = grouped.rows.as_deref().unwrap_or_default();
    let mut mismatches = Vec::new();
    let mut incomplete = false;
    for measure_aliases in aliases {
        for alias in measure_aliases {
            let Some(grouped_index) = columns
                .iter()
                .position(|column| column.eq_ignore_ascii_case(&alias))
            else {
                incomplete = true;
                continue;
            };
            let Some(total_index) = total_columns
                .iter()
                .position(|column| column.eq_ignore_ascii_case(&alias))
            else {
                incomplete = true;
                continue;
            };
            let Some(grouped_sum) = rows
                .iter()
                .map(|row| row.get(grouped_index).and_then(num_of))
                .collect::<Option<Vec<_>>>()
                .map(|values| values.into_iter().sum::<f64>())
            else {
                incomplete = true;
                continue;
            };
            let Some(total_value) = total_row.get(total_index).and_then(num_of) else {
                incomplete = true;
                continue;
            };
            if !invariant_close(grouped_sum, total_value) {
                mismatches.push(format!(
                    "{alias}: grouped sum {grouped_sum} != independent total {total_value}"
                ));
            }
        }
    }

    if !mismatches.is_empty() {
        checks.push(exclude_evidence_warning(
            "grouped totals did not reconcile",
            Some(mismatches.join("; ")),
            VerificationFindingCode::ContractMismatch,
            vec![grouped.id.clone()],
            "recompute this grouped result so its included groups reconcile with the independent total",
        ));
    } else if incomplete {
        checks.push(warn(
            "grouped totals could not be independently checked",
            Some("one or more grouped or total cells were not numeric".into()),
        ));
    } else {
        checks.push(ok("grouped totals reconciled with an independent total"));
    }
}

fn is_additive_operation(operation: &str) -> bool {
    matches!(
        operation.trim().to_ascii_lowercase().as_str(),
        "sum" | "total" | "count" | "number"
    )
}

fn grouped_measure_aliases(contract: &AnalysisContract) -> Vec<Vec<String>> {
    contract
        .measures
        .iter()
        .enumerate()
        .map(|(index, _)| {
            if contract.comparison_spec.is_some() {
                vec![
                    format!("measure_{index}_current"),
                    format!("measure_{index}_previous"),
                ]
            } else {
                vec![format!("measure_{index}")]
            }
        })
        .collect()
}

/// Check every returned average against independently computed min/max values
/// under the same grounded filters, time range, grouping, and joins. A bound
/// violation is a correctness failure; inability to run the bound plans is a
/// review warning so fallback SQL remains usable without a false guarantee.
fn check_average_bounds(
    checks: &mut Vec<VerificationCheck>,
    engine: &dyn AnalyticsSource,
    contract: &AnalysisContract,
    grounding: Option<&GroundingReport>,
    evidence: &[EvidenceItem],
) {
    if contract.interpretation != InterpretationStatus::Grounded
        || contract.comparison_spec.is_some()
        || contract.comparison.is_some()
    {
        return;
    }
    let source_hint = grounding.and_then(|report| report.source.as_deref());
    for (index, measure) in contract.measures.iter().enumerate() {
        if !is_average_operation(&measure.operation) {
            continue;
        }
        let alias = format!("measure_{index}");
        let Some(average_evidence) = evidence.iter().find(|item| {
            is_sql_evidence(item)
                && item.is_accepted()
                && item
                    .row_count
                    .is_some_and(|count| item.rows.as_ref().is_some_and(|rows| count == rows.len()))
                && item.columns.as_ref().is_some_and(|columns| {
                    columns
                        .iter()
                        .any(|column| column.eq_ignore_ascii_case(&alias))
                })
        }) else {
            continue;
        };
        let mut min_contract = contract.clone();
        min_contract.measures = vec![measure.clone()];
        min_contract.measures[0].operation = "min".into();
        min_contract.derived_metrics.clear();
        min_contract.order_by = None;
        min_contract.limit = None;
        let mut max_contract = min_contract.clone();
        max_contract.measures[0].operation = "max".into();

        let min_plan =
            match crate::engine::planner::compile(&engine.catalog(), &min_contract, source_hint) {
                Ok(plan) => plan,
                Err(error) => {
                    checks.push(warn(
                        format!(
                            "average bounds could not be checked for `{}`",
                            measure.concept
                        ),
                        Some(error),
                    ));
                    continue;
                }
            };
        let max_plan =
            match crate::engine::planner::compile(&engine.catalog(), &max_contract, source_hint) {
                Ok(plan) => plan,
                Err(error) => {
                    checks.push(warn(
                        format!(
                            "average bounds could not be checked for `{}`",
                            measure.concept
                        ),
                        Some(error),
                    ));
                    continue;
                }
            };
        let min_result = match engine.run_sql(&min_plan.sql) {
            Ok(result) => result,
            Err(error) => {
                checks.push(warn(
                    format!(
                        "average bounds could not be checked for `{}`",
                        measure.concept
                    ),
                    Some(error.to_string()),
                ));
                continue;
            }
        };
        let max_result = match engine.run_sql(&max_plan.sql) {
            Ok(result) => result,
            Err(error) => {
                checks.push(warn(
                    format!(
                        "average bounds could not be checked for `{}`",
                        measure.concept
                    ),
                    Some(error.to_string()),
                ));
                continue;
            }
        };
        if !complete_query_result(&min_result) || !complete_query_result(&max_result) {
            checks.push(warn(
                format!(
                    "average bounds could not be checked for `{}`",
                    measure.concept
                ),
                Some("a bound query returned an incomplete result set".into()),
            ));
            continue;
        }

        let Some(average_columns) = average_evidence.columns.as_deref() else {
            continue;
        };
        let Some(average_rows) = average_evidence.rows.as_deref() else {
            continue;
        };
        if average_rows.is_empty() || min_result.rows.is_empty() || max_result.rows.is_empty() {
            checks.push(warn(
                format!(
                    "average bounds could not be checked for `{}`",
                    measure.concept
                ),
                Some("the average or bound query returned no rows".into()),
            ));
            continue;
        }
        let Some(average_index) = average_columns
            .iter()
            .position(|column| column.eq_ignore_ascii_case(&alias))
        else {
            continue;
        };
        let Some(min_index) = min_result
            .columns
            .iter()
            .position(|column| column.eq_ignore_ascii_case("measure_0"))
        else {
            checks.push(warn(
                format!(
                    "average bounds could not be checked for `{}`",
                    measure.concept
                ),
                Some("the minimum bound query omitted its measure".into()),
            ));
            continue;
        };
        let Some(max_index) = max_result
            .columns
            .iter()
            .position(|column| column.eq_ignore_ascii_case("measure_0"))
        else {
            checks.push(warn(
                format!(
                    "average bounds could not be checked for `{}`",
                    measure.concept
                ),
                Some("the maximum bound query omitted its measure".into()),
            ));
            continue;
        };
        let min_keys = matching_key_positions(&min_result.columns, average_columns);
        let max_keys = matching_key_positions(&max_result.columns, average_columns);
        if min_keys.is_none() || max_keys.is_none() {
            checks.push(warn(
                format!(
                    "average bounds could not be checked for `{}`",
                    measure.concept
                ),
                Some("the bound query and answer used different grouping columns".into()),
            ));
            continue;
        }
        let min_keys = min_keys.unwrap();
        let max_keys = max_keys.unwrap();
        let mut mismatches = Vec::new();
        let mut incomplete = false;
        for (row_index, average_row) in average_rows.iter().enumerate() {
            let Some(average) = average_row.get(average_index).and_then(num_of) else {
                incomplete = true;
                continue;
            };
            let Some(min_row) = matching_row(&min_result.rows, &min_keys, average_row) else {
                incomplete = true;
                continue;
            };
            let Some(max_row) = matching_row(&max_result.rows, &max_keys, average_row) else {
                incomplete = true;
                continue;
            };
            let Some(minimum) = min_row.get(min_index).and_then(num_of) else {
                incomplete = true;
                continue;
            };
            let Some(maximum) = max_row.get(max_index).and_then(num_of) else {
                incomplete = true;
                continue;
            };
            let within_bounds = (average >= minimum || invariant_close(average, minimum))
                && (average <= maximum || invariant_close(average, maximum));
            if !within_bounds {
                mismatches.push(format!(
                    "row {} average {average} outside [{minimum}, {maximum}]",
                    row_index + 1
                ));
                if mismatches.len() >= 3 {
                    break;
                }
            }
        }
        if !mismatches.is_empty() {
            checks.push(exclude_evidence_warning(
                format!(
                    "average fell outside observed bounds for `{}`",
                    measure.concept
                ),
                Some(mismatches.join("; ")),
                VerificationFindingCode::ContractMismatch,
                vec![average_evidence.id.clone()],
                "recompute only this average using the same population as its observed bounds",
            ));
        } else if incomplete {
            checks.push(warn(
                format!(
                    "average bounds could not be checked for `{}`",
                    measure.concept
                ),
                Some("one or more average or bound cells could not be matched".into()),
            ));
        } else {
            checks.push(ok(format!(
                "average `{}` stayed within observed bounds",
                measure.concept
            )));
        }
    }
}

fn is_average_operation(operation: &str) -> bool {
    matches!(
        operation.trim().to_ascii_lowercase().as_str(),
        "avg" | "average" | "mean"
    )
}

fn complete_query_result(result: &crate::engine::state::QueryResult) -> bool {
    result.row_count == result.rows.len()
}

fn matching_key_positions(
    bound_columns: &[String],
    evidence_columns: &[String],
) -> Option<Vec<(usize, usize)>> {
    bound_columns
        .iter()
        .enumerate()
        .filter(|(_, column)| !column.eq_ignore_ascii_case("measure_0"))
        .map(|(bound_index, column)| {
            evidence_columns
                .iter()
                .position(|candidate| candidate.eq_ignore_ascii_case(column))
                .map(|evidence_index| (bound_index, evidence_index))
        })
        .collect()
}

fn matching_row<'a>(
    rows: &'a [Vec<Json>],
    key_positions: &[(usize, usize)],
    evidence_row: &[Json],
) -> Option<&'a Vec<Json>> {
    rows.iter().find(|row| {
        key_positions.iter().all(|(bound_index, evidence_index)| {
            row.get(*bound_index) == evidence_row.get(*evidence_index)
        })
    })
}

fn check_comparison_usage(
    checks: &mut Vec<VerificationCheck>,
    contract: &AnalysisContract,
    comparison: &crate::engine::runtime::ContractComparison,
    sql: &[String],
) {
    let ranges_used = contract
        .time
        .as_ref()
        .and_then(|time| time.field.as_deref())
        .and_then(|field| {
            Some((
                crate::engine::planner::time_predicate(field, &comparison.current_range).ok()?,
                crate::engine::planner::time_predicate(field, &comparison.previous_range).ok()?,
            ))
        })
        .is_some_and(|(current, previous)| {
            sql.iter().any(|query| {
                query.contains("case when")
                    && query.contains(&current.to_ascii_lowercase())
                    && query.contains(&previous.to_ascii_lowercase())
            })
        });
    let output_shape_used = contract
        .measures
        .iter()
        .enumerate()
        .map(|(index, _)| format!("measure_{index}"))
        .chain(
            contract
                .derived_metrics
                .iter()
                .enumerate()
                .map(|(index, _)| format!("derived_{index}")),
        )
        .all(|alias| {
            sql.iter().any(|query| {
                query.contains(&format!("{alias}_current"))
                    && query.contains(&format!("{alias}_previous"))
                    && query.contains(&format!("{alias}_change"))
                    && query.contains(&format!("{alias}_change_pct"))
            })
        });
    if ranges_used && output_shape_used {
        checks.push(ok(format!(
            "period comparison `{}` vs `{}` was used by the query",
            comparison.current_range, comparison.previous_range
        )));
    } else {
        checks.push(warn(
            "requested period comparison was not used by the query",
            Some("the executed evidence did not carry both explicit comparison windows".into()),
        ));
    }
}

fn check_comparison_arithmetic(
    checks: &mut Vec<VerificationCheck>,
    contract: &AnalysisContract,
    evidence: &[EvidenceItem],
) {
    let mut saw_shape = false;
    let mut mismatches = Vec::new();
    let mut affected_ids = Vec::new();
    for item in evidence
        .iter()
        .filter(|item| is_sql_evidence(item) && item.is_accepted())
    {
        let Some(columns) = item.columns.as_deref() else {
            continue;
        };
        let Some(rows) = item.rows.as_deref() else {
            continue;
        };
        let metrics = contract
            .measures
            .iter()
            .enumerate()
            .map(|(index, measure)| (format!("measure_{index}"), measure.concept.clone()))
            .chain(
                contract
                    .derived_metrics
                    .iter()
                    .enumerate()
                    .map(|(index, derived)| (format!("derived_{index}"), derived.concept.clone())),
            );
        for (alias, label) in metrics {
            let aliases = [
                format!("{alias}_current"),
                format!("{alias}_previous"),
                format!("{alias}_change"),
                format!("{alias}_change_pct"),
            ];
            let Some(column_indexes) = aliases
                .iter()
                .map(|alias| {
                    columns
                        .iter()
                        .position(|column| column.eq_ignore_ascii_case(alias))
                })
                .collect::<Option<Vec<_>>>()
            else {
                continue;
            };
            saw_shape = true;
            for (row_index, row) in rows.iter().enumerate() {
                let current = row.get(column_indexes[0]).and_then(num_of);
                let previous = row.get(column_indexes[1]).and_then(num_of);
                let actual_change = row.get(column_indexes[2]).and_then(num_of);
                let actual_percent = row.get(column_indexes[3]).and_then(num_of);
                let expected_change = current
                    .zip(previous)
                    .map(|(current, previous)| current - previous);
                let expected_percent = expected_change.and_then(|change| {
                    previous.and_then(|previous| {
                        if previous.abs() <= f64::EPSILON {
                            None
                        } else {
                            Some(change / previous.abs() * 100.0)
                        }
                    })
                });
                let change_ok = match (expected_change, actual_change) {
                    (Some(expected), Some(actual)) => invariant_close(expected, actual),
                    (None, None) => true,
                    _ => false,
                };
                let percent_ok = match (expected_percent, actual_percent) {
                    (Some(expected), Some(actual)) => invariant_close(expected, actual),
                    (None, None) => true,
                    _ => false,
                };
                if !change_ok || !percent_ok {
                    affected_ids.push(item.id.clone());
                    mismatches.push(format!(
                        "comparison metric `{}` row {} expected change={:?}, change_pct={:?}; got change={:?}, change_pct={:?}",
                        label,
                        row_index + 1,
                        expected_change,
                        expected_percent,
                        actual_change,
                        actual_percent
                    ));
                    if mismatches.len() >= 3 {
                        break;
                    }
                }
            }
        }
        if mismatches.len() >= 3 {
            break;
        }
    }
    if !saw_shape {
        return;
    }
    if mismatches.is_empty() {
        checks.push(ok(
            "period comparison arithmetic reconciled from returned rows",
        ));
    } else {
        affected_ids.sort();
        affected_ids.dedup();
        checks.push(repair_claim_warning(
            "period comparison arithmetic did not reconcile",
            Some(mismatches.join("; ")),
            VerificationFindingCode::ContractMismatch,
            "period comparison change or percent",
            affected_ids,
            "recompute only the comparison claim from its current and prior values; keep those values",
        ));
    }
}

fn invariant_close(actual: f64, expected: f64) -> bool {
    if actual == expected {
        return true;
    }
    let scale = actual.abs().max(expected.abs()).max(1.0);
    (actual - expected).abs() <= scale * 1e-8
}

fn check_order_usage(
    checks: &mut Vec<VerificationCheck>,
    contract: &AnalysisContract,
    order: &ContractOrder,
    sql: &[String],
) {
    let direction = match order.direction {
        SortDirection::Asc => "asc",
        SortDirection::Desc => "desc",
    };
    let mut targets = vec![order.by.to_ascii_lowercase()];
    for (index, measure) in contract.measures.iter().enumerate() {
        if measure.concept.eq_ignore_ascii_case(&order.by)
            || measure
                .field
                .as_deref()
                .is_some_and(|field| field.eq_ignore_ascii_case(&order.by))
        {
            targets.push(format!("measure_{index}"));
            if let Some(field) = &measure.field {
                targets.push(field.to_ascii_lowercase());
            }
        }
    }
    for (index, derived) in contract.derived_metrics.iter().enumerate() {
        if derived.concept.eq_ignore_ascii_case(&order.by)
            || format!("derived_{index}").eq_ignore_ascii_case(&order.by)
        {
            targets.push(format!("derived_{index}"));
        }
    }
    if let Some(time) = &contract.time {
        if let Some(bucket) = time.bucket {
            let alias = format!("time_{}", format_time_bucket(bucket));
            if order.by.eq_ignore_ascii_case(&alias)
                || time
                    .field
                    .as_deref()
                    .is_some_and(|field| field.eq_ignore_ascii_case(&order.by))
            {
                targets.push(alias);
            }
        }
    }
    let used = sql.iter().any(|query| {
        let Some(order_start) = query.find("order by") else {
            return false;
        };
        let ordered = &query[order_start..];
        ordered.contains(direction)
            && targets
                .iter()
                .any(|target| contains_field_reference(ordered, target))
    });
    if used {
        checks.push(ok(format!(
            "requested order by `{}` was used by the query",
            order.by
        )));
    } else {
        checks.push(warn(
            format!(
                "requested order by `{}` was not used by the query",
                order.by
            ),
            Some("the executed evidence did not carry the requested ranking".into()),
        ));
    }
}

fn check_limit_usage(checks: &mut Vec<VerificationCheck>, limit: u16, sql: &[String]) {
    let expected = format!("limit {limit}");
    let used = sql.iter().any(|query| {
        query
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .contains(&expected)
    });
    if used {
        checks.push(ok(format!(
            "requested row limit `{limit}` was used by the query"
        )));
    } else {
        checks.push(warn(
            format!("requested row limit `{limit}` was not used by the query"),
            Some("the executed evidence returned an unbounded ranking".into()),
        ));
    }
}

fn check_derived_usage(
    checks: &mut Vec<VerificationCheck>,
    contract: &AnalysisContract,
    index: usize,
    derived: &ContractDerivedMetric,
    sql: &[String],
) {
    let alias = format!("derived_{index}");
    let used = sql.iter().any(|query| {
        let lowered = query.to_ascii_lowercase();
        let guarded_compiled_metric = (contains_word(query, &alias)
            || lowered.contains(&format!("{alias}_current"))
            || lowered.contains(&format!("{alias}_previous")))
            && match derived.kind {
                crate::engine::runtime::DerivedMetricKind::Ratio => {
                    lowered.contains('/') && lowered.contains("nullif")
                }
                crate::engine::runtime::DerivedMetricKind::Difference => lowered.contains('-'),
            };
        let direct_metric = match derived.kind {
            crate::engine::runtime::DerivedMetricKind::Ratio => query.contains('/'),
            crate::engine::runtime::DerivedMetricKind::Difference => query.contains('-'),
        } && measure_reference_used(contract, &derived.numerator, query)
            && measure_reference_used(contract, &derived.denominator, query);
        guarded_compiled_metric || direct_metric
    });
    if used {
        checks.push(ok(format!(
            "derived metric `{}` was used by the query",
            derived.concept
        )));
    } else {
        checks.push(warn(
            format!(
                "derived metric `{}` was not used by the query",
                derived.concept
            ),
            Some("the executed evidence did not carry the requested derived calculation".into()),
        ));
    }
}

fn check_derived_arithmetic(
    checks: &mut Vec<VerificationCheck>,
    contract: &AnalysisContract,
    index: usize,
    derived: &ContractDerivedMetric,
    evidence: &[EvidenceItem],
) {
    let Some(numerator_index) = measure_index_for_reference(contract, &derived.numerator) else {
        return;
    };
    let Some(denominator_index) = measure_index_for_reference(contract, &derived.denominator)
    else {
        return;
    };
    let derived_alias = format!("derived_{index}");
    let numerator_alias = format!("measure_{numerator_index}");
    let denominator_alias = format!("measure_{denominator_index}");
    let mut saw_shape = false;
    let mut mismatches = Vec::new();
    let mut affected_ids = Vec::new();
    for item in evidence
        .iter()
        .filter(|item| is_sql_evidence(item) && item.is_accepted())
    {
        let Some(columns) = item.columns.as_deref() else {
            continue;
        };
        let Some(rows) = item.rows.as_deref() else {
            continue;
        };
        let Some(numerator_column) = columns
            .iter()
            .position(|column| column.eq_ignore_ascii_case(&numerator_alias))
        else {
            continue;
        };
        let Some(denominator_column) = columns
            .iter()
            .position(|column| column.eq_ignore_ascii_case(&denominator_alias))
        else {
            continue;
        };
        let Some(derived_column) = columns
            .iter()
            .position(|column| column.eq_ignore_ascii_case(&derived_alias))
        else {
            continue;
        };
        saw_shape = true;
        for (row_index, row) in rows.iter().enumerate() {
            let numerator = row.get(numerator_column).and_then(num_of);
            let denominator = row.get(denominator_column).and_then(num_of);
            let actual = row.get(derived_column).and_then(num_of);
            let expected = numerator
                .zip(denominator)
                .and_then(|(numerator, denominator)| match derived.kind {
                    crate::engine::runtime::DerivedMetricKind::Ratio => {
                        if denominator.abs() <= f64::EPSILON {
                            None
                        } else {
                            let ratio = numerator / denominator;
                            Some(if is_percentage_unit(derived.unit.as_deref()) {
                                ratio * 100.0
                            } else {
                                ratio
                            })
                        }
                    }
                    crate::engine::runtime::DerivedMetricKind::Difference => {
                        Some(numerator - denominator)
                    }
                });
            let matches = match (expected, actual) {
                (Some(expected), Some(actual)) => invariant_close(expected, actual),
                (None, None) => true,
                _ => false,
            };
            if !matches {
                affected_ids.push(item.id.clone());
                mismatches.push(format!(
                    "derived `{}` row {} expected {:?}, got {:?}",
                    derived.concept,
                    row_index + 1,
                    expected,
                    actual
                ));
                if mismatches.len() >= 3 {
                    break;
                }
            }
        }
        if mismatches.len() >= 3 {
            break;
        }
    }
    if !saw_shape {
        return;
    }
    if mismatches.is_empty() {
        checks.push(ok(format!(
            "derived metric `{}` reconciled from returned rows",
            derived.concept
        )));
    } else {
        affected_ids.sort();
        affected_ids.dedup();
        checks.push(repair_claim_warning(
            format!(
                "derived metric arithmetic did not reconcile for `{}`",
                derived.concept
            ),
            Some(mismatches.join("; ")),
            VerificationFindingCode::ContractMismatch,
            derived.concept.clone(),
            affected_ids,
            "recompute or correct only this derived metric from the retained operand values",
        ));
    }
}

/// A ratio is only as meaningful as the population shared by its operands.
/// The structured compiler naturally emits one aggregate scope; for
/// model-authored SQL, flag nested/separate SELECT or FROM scopes instead of
/// pretending that a reproducible ratio has the same denominator semantics.
/// This stays a review warning because explicit cross-population ratios are a
/// valid future contract feature, not automatically an error.
fn check_derived_population(
    checks: &mut Vec<VerificationCheck>,
    contract: &AnalysisContract,
    index: usize,
    derived: &ContractDerivedMetric,
    evidence: &[EvidenceItem],
) {
    let Some(numerator_index) = measure_index_for_reference(contract, &derived.numerator) else {
        return;
    };
    let Some(denominator_index) = measure_index_for_reference(contract, &derived.denominator)
    else {
        return;
    };
    let derived_alias = format!("derived_{index}");
    let numerator_alias = format!("measure_{numerator_index}");
    let denominator_alias = format!("measure_{denominator_index}");
    let mut saw_shape = false;
    let mut incompatible = Vec::new();
    let mut related_evidence = Vec::new();
    for item in evidence
        .iter()
        .filter(|item| is_sql_evidence(item) && item.is_accepted())
    {
        let Some(sql) = item.sql.as_deref() else {
            continue;
        };
        if !contains_word(sql, &derived_alias)
            || !contains_word(sql, &numerator_alias)
            || !contains_word(sql, &denominator_alias)
        {
            continue;
        }
        saw_shape = true;
        if !has_single_population_scope(sql) {
            incompatible.push("the ratio uses nested or separate SQL scopes".to_string());
            related_evidence.push(item.id.clone());
        }
    }
    if !saw_shape {
        return;
    }
    if incompatible.is_empty() {
        checks.push(ok(format!(
            "derived metric `{}` uses a shared query population",
            derived.concept
        )));
    } else {
        checks.push(warn_with_effect(
            format!(
                "derived metric `{}` may use incompatible populations",
                derived.concept
            ),
            Some(incompatible.join("; ")),
            VerificationFindingCode::SemanticCaveat,
            VerificationEffect::Informational,
            VerificationTarget::Claim,
            Some(derived.concept.clone()),
            related_evidence,
            Some(
                "this is a caution, not a rejection; cross-population ratios can be intentional"
                    .into(),
            ),
        ));
    }
}

fn has_single_population_scope(sql: &str) -> bool {
    sql_keyword_count(sql, "select") == 1 && sql_keyword_count(sql, "from") == 1
}

fn sql_keyword_count(sql: &str, keyword: &str) -> usize {
    sql.split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
        .filter(|token| token.eq_ignore_ascii_case(keyword))
        .count()
}

fn measure_index_for_reference(contract: &AnalysisContract, requested: &str) -> Option<usize> {
    let matches: Vec<_> = contract
        .measures
        .iter()
        .enumerate()
        .filter(|(_, measure)| {
            measure.concept.eq_ignore_ascii_case(requested)
                || measure
                    .field
                    .as_deref()
                    .is_some_and(|field| field.eq_ignore_ascii_case(requested))
        })
        .map(|(index, _)| index)
        .collect();
    matches
        .as_slice()
        .first()
        .copied()
        .filter(|_| matches.len() == 1)
}

fn check_join_usage(checks: &mut Vec<VerificationCheck>, join: &ContractJoin, sql: &[String]) {
    let left = format!("{}.{}", join.left_source, join.left_field);
    let right = format!("{}.{}", join.right_source, join.right_field);
    let used = sql.iter().any(|query| {
        (query.contains(" join ") || query.contains("left join "))
            && contains_field_reference(query, &left.to_ascii_lowercase())
            && contains_field_reference(query, &right.to_ascii_lowercase())
    });
    let relationship = format!("{left} ↔ {right}");
    if used {
        checks.push(ok(format!(
            "declared join `{relationship}` was used by the query"
        )));
    } else {
        checks.push(warn(
            format!("declared join `{relationship}` was not used by the query"),
            Some("the executed evidence did not carry the grounded relationship".into()),
        ));
    }
}

fn measure_reference_used(contract: &AnalysisContract, requested: &str, query: &str) -> bool {
    contract.measures.iter().any(|measure| {
        let matches = measure.concept.eq_ignore_ascii_case(requested)
            || measure
                .field
                .as_deref()
                .is_some_and(|field| field.eq_ignore_ascii_case(requested));
        if !matches {
            return false;
        }
        if let Some(field) = &measure.field {
            contains_field_reference(query, &field.to_ascii_lowercase())
        } else {
            contains_function_call(query, &measure.operation.to_ascii_lowercase())
        }
    })
}

fn check_time_bucket_usage(
    checks: &mut Vec<VerificationCheck>,
    bucket: TimeBucket,
    sql: &[String],
    comparison: bool,
) {
    let formats: &[&str] = if comparison {
        match bucket {
            TimeBucket::Year => &["%y"],
            TimeBucket::Month => &["%m"],
            TimeBucket::Week => &["%w"],
            TimeBucket::Day => &["%m-%d"],
        }
    } else {
        match bucket {
            TimeBucket::Year => &["%y"],
            TimeBucket::Month => &["%y-%m"],
            TimeBucket::Week => &["%y-%w"],
            TimeBucket::Day => &["%y-%m-%d"],
        }
    };
    if sql.iter().any(|query| {
        query.contains("strftime") && formats.iter().any(|format| query.contains(format))
    }) {
        checks.push(ok(format!(
            "time bucket `{}` was used by the query",
            format_time_bucket(bucket)
        )));
    } else {
        checks.push(warn(
            format!(
                "time bucket `{}` was not used by the query",
                format_time_bucket(bucket)
            ),
            Some("the executed evidence did not carry the requested time grouping".into()),
        ));
    }
}

fn format_time_bucket(bucket: TimeBucket) -> &'static str {
    match bucket {
        TimeBucket::Year => "year",
        TimeBucket::Month => "month",
        TimeBucket::Week => "week",
        TimeBucket::Day => "day",
    }
}

fn check_measure_operation(
    checks: &mut Vec<VerificationCheck>,
    operation: &str,
    sql: &[String],
    evidence: &[EvidenceItem],
) {
    let normalized = operation.trim().to_ascii_lowercase();
    if matches!(
        normalized.as_str(),
        "raw"
            | "raw values"
            | "raw distribution"
            | "raw observation"
            | "raw observations"
            | "observations"
            | "identity"
    ) {
        let projected_without_aggregation = sql.iter().any(|query| {
            let query = query.trim_start();
            !is_aggregate_sql(query)
                && !query
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .windows(2)
                    .any(|words| {
                        words[0].eq_ignore_ascii_case("group")
                            && words[1].eq_ignore_ascii_case("by")
                    })
        });
        if projected_without_aggregation {
            checks.push(ok(
                "grounded raw observations were selected without aggregation",
            ));
        } else {
            checks.push(warn(
                "grounded raw observation operation was not used by the query",
                Some("the executed evidence did not preserve row-level observations".into()),
            ));
        }
        return;
    }
    let functions = match normalized.as_str() {
        "sum" | "total" => Some(&["sum", "total"][..]),
        "avg" | "average" | "mean" | "arithmetic mean" => Some(&["avg"][..]),
        "count" | "number" => Some(&["count"][..]),
        "min" | "minimum" => Some(&["min"][..]),
        "max" | "maximum" => Some(&["max"][..]),
        _ => None,
    };
    let Some(functions) = functions else {
        checks.push(warn(
            format!("measure operation `{operation}` is unsupported"),
            Some("the executed evidence cannot be matched to this semantic operation".into()),
        ));
        return;
    };
    let count_case = matches!(normalized.as_str(), "count" | "number");
    let forecast_mean = matches!(
        normalized.as_str(),
        "avg" | "average" | "mean" | "arithmetic mean"
    ) && evidence.iter().any(|item| {
        item.tool == "forecast_analysis"
            && item.is_accepted()
            && item.args.get("method").and_then(Json::as_str) == Some("mean")
    });
    if forecast_mean
        || sql.iter().any(|query| {
            functions
                .iter()
                .any(|function| contains_function_call(query, function))
                || (count_case
                    && contains_function_call(query, "sum")
                    && query.to_ascii_lowercase().contains("then 1 else 0 end"))
        })
    {
        checks.push(ok(format!(
            "grounded measure operation `{operation}` was used by the query"
        )));
    } else {
        checks.push(warn(
            format!("grounded measure operation `{operation}` was not used by the query"),
            Some("the executed evidence did not carry the requested aggregation".into()),
        ));
    }
}

fn check_literal_usage(
    checks: &mut Vec<VerificationCheck>,
    kind: &str,
    field: Option<&str>,
    value: &str,
    sql: &[String],
) {
    let literal = quote_str(value).to_ascii_lowercase();
    let numeric = value.trim().parse::<f64>().ok();
    if sql.iter().any(|query| {
        if let Some(value) = numeric {
            field.map_or_else(
                || query.contains(&literal),
                |field| numeric_filter_uses_value(query, field, value),
            )
        } else {
            field.map_or(true, |field| {
                contains_field_reference(query, &field.to_ascii_lowercase())
            }) && query.contains(&literal)
        }
    }) {
        checks.push(ok(format!(
            "grounded {kind} `{value}` was used by the query"
        )));
    } else {
        checks.push(warn(
            format!("grounded {kind} `{value}` was not used by the query"),
            Some("the executed evidence did not carry the observed filter value".into()),
        ));
    }
}

/// Match a numeric category filter to a comparison on that same column.
/// Merely finding the number elsewhere in SQL is not enough (for example,
/// date components must not satisfy a year filter).
fn numeric_filter_uses_value(query: &str, field: &str, expected: f64) -> bool {
    let column = field
        .rsplit('.')
        .next()
        .unwrap_or(field)
        .trim_matches(['"', '`', '[', ']']);
    if column.is_empty() {
        return false;
    }
    let pattern = format!(
        r#"(?i)(?:^|[^a-z0-9_])(?:"{}"|\x60{}\x60|\[{}\]|{})\s*(between\s+|==|<>|!=|<=|>=|=|<|>|in\s*\()\s*"#,
        regex::escape(column),
        regex::escape(column),
        regex::escape(column),
        regex::escape(column)
    );
    let Ok(field_filter) = regex::Regex::new(&pattern) else {
        return false;
    };
    let found = field_filter.captures_iter(query).any(|captures| {
        let Some(matched) = captures.get(0) else {
            return false;
        };
        let Some(operator) = captures.get(1) else {
            return false;
        };
        let suffix = &query[matched.end()..];
        if operator
            .as_str()
            .trim_start()
            .to_ascii_lowercase()
            .starts_with("between")
        {
            let bounds = number_tokens(suffix)
                .take(2)
                .map(|(_, value)| value)
                .collect::<Vec<_>>();
            bounds.len() == 2 && expected >= bounds[0] && expected <= bounds[1]
        } else {
            let candidates = if operator.as_str().trim_end().ends_with('(') {
                suffix.split(')').next().unwrap_or(suffix)
            } else {
                suffix.split_whitespace().next().unwrap_or(suffix)
            };
            number_tokens(candidates).any(|(_, value)| (value - expected).abs() <= 1e-9)
        }
    });
    found
}

fn check_filter_polarity(
    checks: &mut Vec<VerificationCheck>,
    filter: &crate::engine::runtime::ContractFilter,
    sql: &[String],
) {
    if !filter.exclude || filter.resolved_values.is_empty() {
        return;
    }
    let Some(field) = filter.field.as_deref() else {
        return;
    };
    let field = field.to_ascii_lowercase();
    let uses_exclusion = sql.iter().any(|query| {
        let query = query.to_ascii_lowercase();
        let mentions_field = contains_field_reference(&query, &field);
        mentions_field && (query.contains("not in") || query.contains("<>") || query.contains("!="))
    });
    if uses_exclusion {
        checks.push(ok(format!(
            "excluding filter `{}` was preserved by the query",
            filter.concept
        )));
    } else {
        checks.push(warn(
            format!("excluding filter `{}` was not preserved by the query", filter.concept),
            Some("the contract excludes observed values but the executed query did not carry that polarity".into()),
        ));
    }
}

fn check_signed_semantics(
    question: &str,
    evidence: &[EvidenceItem],
    checks: &mut Vec<VerificationCheck>,
) {
    let lower = question.to_ascii_lowercase();
    let words: Vec<&str> = lower
        .split(|character: char| !character.is_ascii_alphanumeric())
        .collect();
    let signed_question = [
        "net", "spending", "spent", "refund", "refunds", "credit", "credits",
    ]
    .iter()
    .any(|word| words.contains(word))
        || lower.contains("excluding income")
        || lower.contains("after refunds")
        || lower.contains("including refunds");
    if !signed_question
        || lower.contains("absolute")
        || lower.contains("absolute value")
        || lower.contains("magnitude")
    {
        return;
    }
    let discarded: Vec<_> = evidence
        .iter()
        .filter(|item| {
            item.is_accepted()
                && is_sql_evidence(item)
                && item
                    .sql
                    .as_deref()
                    .is_some_and(signed_value_abs_is_discarding)
        })
        .collect();
    if let Some(item) = discarded.first() {
        checks.push(exclude_evidence_warning(
            "signed values were discarded by ABS in the executed query",
            Some(format!(
                "this question requires signed amounts; revise the semantic plan instead of summing absolute magnitudes: {}",
                truncate(item.sql.as_deref().unwrap_or_default(), 140)
            )),
            VerificationFindingCode::SemanticExecutionMismatch,
            discarded.iter().map(|item| item.id.clone()).collect(),
            "replace only the affected computation with signed values; preserve other results",
        ));
    }
}

fn signed_value_abs_is_discarding(sql: &str) -> bool {
    let lower = sql.to_ascii_lowercase();
    let mut offset = 0;
    while let Some(relative) = lower[offset..].find("abs(") {
        let start = offset + relative;
        let prefix = lower[..start].trim_end();
        // Period-over-period percent change uses ABS only to keep a negative
        // prior-period denominator from flipping the displayed percentage.
        // That does not discard the signed measure used for the change itself.
        if !prefix.ends_with("nullif(") {
            return true;
        }
        offset = start + 4;
    }
    false
}

fn contains_function_call(haystack: &str, function: &str) -> bool {
    let haystack = haystack.to_ascii_lowercase();
    let bytes = haystack.as_bytes();
    let mut from = 0;
    while let Some(relative) = haystack[from..].find(function) {
        let start = from + relative;
        let end = start + function.len();
        let before_ok =
            start == 0 || (!bytes[start - 1].is_ascii_alphanumeric() && bytes[start - 1] != b'_');
        if !before_ok {
            from = end;
            continue;
        }
        let mut cursor = end;
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        if bytes.get(cursor) == Some(&b'(') {
            return true;
        }
        from = end;
    }
    false
}

fn check_binding_usage(
    checks: &mut Vec<VerificationCheck>,
    kind: &str,
    field: &str,
    sql: &[String],
) {
    let used = sql
        .iter()
        .any(|query| contains_field_reference(query, &field.to_ascii_lowercase()));
    if used {
        checks.push(ok(format!(
            "grounded {kind} `{field}` was used by the query"
        )));
    } else {
        checks.push(warn(
            format!("grounded {kind} `{field}` was not used by the query"),
            Some("the executed evidence did not carry the contract binding".into()),
        ));
    }
}

/// Return a replay mismatch for focused telemetry. The agent loop consumes its
/// typed evidence scope and performs a tool-backed repair rather than asking
/// for prose against stale results.
pub fn rerun_regression(checks: &[VerificationCheck]) -> Option<String> {
    checks
        .iter()
        .find(|check| {
            !check.ok
                && check
                    .finding
                    .as_ref()
                    .is_some_and(|finding| finding.code == VerificationFindingCode::ReplayMismatch)
        })
        .map(|check| match &check.detail {
            Some(detail) => format!("{} ({detail})", check.label),
            None => check.label.clone(),
        })
}

// --- 4. aggregates over a text column --------------------------------------

/// `(lowercased, original-case)` names of every catalogued `TEXT` column.
/// Shared by the `run_sql` tool's inline warning and `check_text_agg` so the
/// "collect the text columns" logic lives in one place.
pub(crate) fn text_columns(engine: &dyn AnalyticsSource) -> Vec<(String, String)> {
    engine
        .catalog()
        .sources
        .iter()
        .filter_map(|s| s.columns.as_ref())
        .flatten()
        .filter(|c| is_text_type(&c.type_))
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
            let Some(close) = lower[i..].find(')') else {
                break;
            };
            let inner = lower[i..i + close].trim();
            let inner = inner
                .strip_prefix("distinct")
                .map(str::trim_start)
                .unwrap_or(inner);
            let arg = match inner.split_once('.') {
                Some((q, rest))
                    if !q.is_empty()
                        && q.chars()
                            .all(|c| c.is_alphanumeric() || c == '_' || c == '"') =>
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

/// `(lowercased, original)` names of catalogued text columns whose ingest note
/// says the values differ only in capitalisation (`case_collision`).
pub(crate) fn mixed_case_columns(engine: &dyn AnalyticsSource) -> Vec<(String, String)> {
    engine
        .catalog()
        .sources
        .iter()
        .filter_map(|s| s.columns.as_ref())
        .flatten()
        .filter(|c| is_text_type(&c.type_))
        .filter(|c| {
            c.note
                .as_deref()
                .is_some_and(|n| n.contains("capitalisation"))
        })
        .map(|c| (c.name.to_lowercase(), c.name.clone()))
        .collect()
}

/// `(lowercased, original)` names of text columns where an exact-value filter
/// is likely to be a category/status/label lookup. This is intentionally a
/// little broader than [`mixed_case_columns`]: a uniformly cased column can
/// still lose every row when the model writes `Leisure` for stored `leisure`.
/// The name and cardinality filters keep free-form text, ids, dates, and other
/// high-cardinality columns from adding a warning to every ordinary query.
pub(crate) fn filterable_text_columns(engine: &dyn AnalyticsSource) -> Vec<(String, String)> {
    engine
        .catalog()
        .sources
        .iter()
        .filter_map(|s| s.columns.as_ref())
        .flatten()
        .filter(|c| is_filterable_text_column(c))
        .map(|c| (c.name.to_lowercase(), c.name.clone()))
        .collect()
}

fn is_text_type(type_: &str) -> bool {
    let upper = type_.trim().to_ascii_uppercase();
    matches!(
        upper.as_str(),
        "TEXT" | "VARCHAR" | "STRING" | "CHAR" | "BPCHAR"
    ) || upper.starts_with("VARCHAR(")
        || upper.starts_with("CHAR(")
}

fn is_filterable_text_column(column: &crate::engine::catalog::ColumnInfo) -> bool {
    if !is_text_type(&column.type_) {
        return false;
    }
    if column.distinct.is_some_and(|distinct| distinct > 100) {
        return false;
    }
    if column
        .note
        .as_deref()
        .is_some_and(|note| note.contains("ISO-8601"))
    {
        return false;
    }

    // These names usually identify free-form text, dates, paths, or opaque
    // identifiers. A column with no stats is still eligible when its name
    // looks like a useful label; stats only narrow the high-cardinality case.
    const NON_LABEL_TOKENS: &[&str] = &[
        "address",
        "body",
        "comment",
        "content",
        "created",
        "date",
        "description",
        "details",
        "email",
        "hash",
        "id",
        "key",
        "memo",
        "note",
        "notes",
        "path",
        "summary",
        "text",
        "time",
        "timestamp",
        "updated",
        "uri",
        "url",
        "uuid",
    ];
    let normalized = column.name.to_ascii_lowercase();
    !normalized
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
        .any(|token| NON_LABEL_TOKENS.contains(&token))
}

/// If `sql` filters one of `cols` (already lowercased) with an exact-case
/// `= '…'` or `IN (…)` that isn't wrapped in `lower(`/`upper(`, return that
/// column. Crude scan, same altitude as `aggregates_text_column`.
pub(crate) fn case_sensitive_label_filter<'a>(sql: &str, cols: &'a [String]) -> Option<&'a str> {
    let lower = sql.to_lowercase();
    for col in cols {
        for pat in [
            format!(" {col}"),
            format!("\"{col}\""),
            format!(".{col}"),
            format!("({col}"),
        ] {
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
                let after = lower[end..]
                    .trim_start()
                    .trim_start_matches('"')
                    .trim_start();
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

/// Flag a cited query that filters a likely label column by exact case. A soft
/// warning; this catches both mixed-case data and a model inventing `Leisure`
/// for a uniformly lowercase `leisure` column.
fn check_case_filter(
    engine: &dyn AnalyticsSource,
    evidence: &[EvidenceItem],
    out: &mut Vec<VerificationCheck>,
) {
    let cols = mixed_case_columns(engine);
    if cols.is_empty() {
        return;
    }
    let lowered: Vec<String> = cols.iter().map(|(l, _)| l.clone()).collect();
    for e in evidence
        .iter()
        .filter(|e| is_sql_evidence(e) && e.is_accepted())
    {
        let Some(sql) = &e.sql else { continue };
        if let Some(hit) = case_sensitive_label_filter(sql, &lowered) {
            let name = cols
                .iter()
                .find(|(l, _)| l == hit)
                .map_or(hit, |(_, n)| n.as_str());
            out.push(exclude_evidence_warning(
                format!("a filter on `{name}` matches exact case"),
                Some(format!(
                    "`{name}` has values that differ only in capitalisation; \
                     rows like `Rent` vs `rent` may be excluded unless the filter folds case"
                )),
                VerificationFindingCode::SemanticExecutionMismatch,
                vec![e.id.clone()],
                "revise only this filter to match the observed label values without losing valid rows",
            ));
            return;
        }
    }
}

/// Flag any cited query that sums/averages a column the catalog reports as
/// `TEXT` SQLite counts non-numeric text as 0, so the figure may be wrong.
fn check_text_agg(
    engine: &dyn AnalyticsSource,
    evidence: &[EvidenceItem],
    out: &mut Vec<VerificationCheck>,
) {
    let cols = text_columns(engine);
    if cols.is_empty() {
        return;
    }
    let lowered: Vec<String> = cols.iter().map(|(l, _)| l.clone()).collect();
    for e in evidence
        .iter()
        .filter(|e| is_sql_evidence(e) && e.is_accepted())
    {
        let Some(sql) = &e.sql else { continue };
        if let Some(hit) = aggregates_text_column(sql, &lowered) {
            let name = cols
                .iter()
                .find(|(l, _)| l == hit)
                .map_or(hit, |(_, n)| n.as_str());
            out.push(warn(
                format!("a total here is computed over the text column `{name}`"),
                Some(
                    "non-numeric values count as 0 cast the column if the figure looks off".into(),
                ),
            ));
            return;
        }
    }
}

/// A model will often pair a SUM/AVG with a quality count such as
/// `COUNT(*) - COUNT(parse_num(amount)) AS unparseable_rows`. That is good
/// analytical practice, but a positive quality count means a total over the
/// usable rows is not a complete answer unless the question explicitly scopes
/// itself to observed/available values. Keep this as a soft acceptance warning:
/// the computation is reproducible, but its completeness needs to be visible
/// to the user.
fn check_incomplete_quality_audit(
    question: &str,
    evidence: &[EvidenceItem],
    out: &mut Vec<VerificationCheck>,
) {
    if question_explicitly_scopes_missing_values(question) {
        return;
    }

    for item in evidence
        .iter()
        .filter(|item| is_sql_evidence(item) && item.is_accepted())
    {
        let Some(columns) = item.columns.as_deref() else {
            continue;
        };
        let Some(rows) = item.rows.as_deref() else {
            continue;
        };
        let quality_positions: Vec<usize> = columns
            .iter()
            .enumerate()
            .filter_map(|(index, name)| {
                let normalized = name.to_ascii_lowercase();
                (normalized.contains("missing")
                    || normalized.contains("unparse")
                    || normalized.contains("invalid")
                    || normalized.contains("null_count"))
                .then_some(index)
            })
            .collect();

        for index in quality_positions {
            let positive = rows.iter().any(|row| {
                row.get(index)
                    .and_then(|value| match value {
                        Json::Number(number) => number.as_f64(),
                        Json::String(value) => value.trim().parse::<f64>().ok(),
                        _ => None,
                    })
                    .is_some_and(|value| value > 0.0)
            });
            if positive {
                out.push(warn(
                    "the result excludes rows with missing or unusable values",
                    Some(format!(
                        "the query reported a positive `{}` count; this is a partial result and should not be labelled complete",
                        columns[index]
                    )),
                ));
                return;
            }
        }
    }
}

fn question_explicitly_scopes_missing_values(question: &str) -> bool {
    let question = question.to_ascii_lowercase();
    [
        "missing",
        "unparse",
        "null",
        "invalid",
        "available values",
        "available data",
        "observed values",
        "observed data",
        "measured values",
        "measured data",
        "valid values",
        "valid data",
        "non-null",
        "non null",
        "not null",
    ]
    .iter()
    .any(|term| question.contains(term))
}

fn ok(label: impl Into<String>) -> VerificationCheck {
    VerificationCheck {
        label: label.into(),
        ok: true,
        detail: None,
        finding: None,
    }
}

fn ok_with_detail(label: impl Into<String>, detail: Option<String>) -> VerificationCheck {
    VerificationCheck {
        label: label.into(),
        ok: true,
        detail,
        finding: None,
    }
}

fn warn(label: impl Into<String>, detail: Option<String>) -> VerificationCheck {
    VerificationCheck {
        label: label.into(),
        ok: false,
        detail,
        finding: Some(crate::engine::evidence::VerificationFinding {
            code: crate::engine::evidence::VerificationFindingCode::Advisory,
            effect: crate::engine::evidence::VerificationEffect::Informational,
            target: crate::engine::evidence::VerificationTarget::Answer,
            target_id: None,
            evidence_ids: Vec::new(),
            guidance: None,
        }),
    }
}

// --- 6. question implies an aggregate no cited query used ------------------

const AGGREGATE_VERBS: &[(&[&str], &str)] = &[
    (&["count of", "number of"], "COUNT("),
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

/// Explicit aggregate terminology can reveal a query that selected a different
/// operation. Colloquial quantity questions such as "how many" are deliberately
/// left to the model: they can ask for a row count or the sum of a measured
/// quantity ("how many total trip-nights/minutes/miles"). Soft warning: the
/// aggregate can legitimately be absent (e.g. raw rows already answer it, or a
/// subquery hides it).
fn check_aggregate_verb(
    question: &str,
    evidence: &[EvidenceItem],
    out: &mut Vec<VerificationCheck>,
) {
    let sql: Vec<String> = evidence
        .iter()
        .filter(|e| is_sql_evidence(e) && e.is_accepted())
        .filter_map(|e| e.sql.as_deref())
        .map(str::to_uppercase)
        .collect();
    let mean_forecast = evidence.iter().any(|item| {
        item.tool == "forecast_analysis"
            && item.is_accepted()
            && item.args.get("method").and_then(Json::as_str) == Some("mean")
    });
    for name in missing_aggregate_verbs(question, &sql) {
        if name == "AVG" && mean_forecast {
            continue;
        }
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

fn check_tables(
    engine: &dyn AnalyticsSource,
    evidence: &[EvidenceItem],
    out: &mut Vec<VerificationCheck>,
) {
    let known: HashSet<String> = engine
        .catalog()
        .sources
        .iter()
        .filter_map(|s| s.view.clone())
        .collect();

    let mut bad = HashSet::new();
    for e in evidence.iter().filter(|e| is_sql_evidence(e)) {
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

/// Physical relation names following FROM / JOIN, lowercased and
/// de-punctuated. This lightweight recognizer is used to flag obviously-wrong
/// names and check multi-table joins; parenthesized subqueries are not tables.
pub(crate) fn referenced_relations(sql: &str) -> HashSet<String> {
    let lower = sql.to_lowercase();
    let ctes = cte_names(&lower);
    let toks: Vec<&str> = lower
        .split(|c: char| c.is_whitespace())
        .filter(|s| !s.is_empty())
        .collect();
    let mut out = HashSet::new();
    for (i, t) in toks.iter().enumerate() {
        if (*t == "from" || *t == "join") && i + 1 < toks.len() {
            let mut relation_index = i + 1;
            while relation_index < toks.len()
                && toks[relation_index]
                    .chars()
                    .all(|character| matches!(character, '(' | ')'))
            {
                relation_index += 1;
            }
            let Some(raw_name) = toks.get(relation_index) else {
                continue;
            };
            // `FROM (SELECT ...` and `FROM (( SELECT ...` begin a derived
            // relation, not a physical table named `select`.
            if raw_name.trim_start_matches('(').eq("select")
                || raw_name.trim_start_matches('(').eq("with")
                || raw_name.contains('(')
            {
                continue;
            }
            let name = raw_name.trim_matches(|c: char| !c.is_alphanumeric() && c != '_');
            if !name.is_empty() && !ctes.contains(name) {
                out.insert(name.to_string());
            }
        }
    }
    out
}

/// Names introduced by a `WITH` clause are query-local relations, not
/// catalogued sources. Keep them out of `referenced_relations` so a perfectly
/// valid CTE does not produce a missing-table warning or look like a second
/// physical table to the join checks.
fn cte_names(sql: &str) -> HashSet<String> {
    let re =
        regex::Regex::new(r"(?i)(?:\bwith\s+(?:recursive\s+)?|,\s*)([a-z_][a-z0-9_]*)\s+as\s*\(")
            .expect("CTE name pattern is valid");
    re.captures_iter(sql)
        .filter_map(|capture| capture.get(1).map(|name| name.as_str().to_lowercase()))
        .collect()
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
        let before_ok =
            start == 0 || !bytes[start - 1].is_ascii_alphanumeric() && bytes[start - 1] != b'_';
        let after_ok =
            end == bytes.len() || !bytes[end].is_ascii_alphanumeric() && bytes[end] != b'_';
        if before_ok && after_ok {
            return true;
        }
    }
    false
}

fn contains_field_reference(haystack: &str, field: &str) -> bool {
    if contains_word(haystack, field) {
        return true;
    }
    let Some((source, column)) = field.rsplit_once('.') else {
        return false;
    };
    contains_word(haystack, source) && contains_word(haystack, column)
}

// --- 7. a column named in the question is missing from every cited query ---

/// Column names too generic to mean "filter on this" just because the word
/// shows up in the question — same list `shared_column_hints` (state.rs)
/// already uses to drop unhelpful join-key suggestions.
const GENERIC_COLUMN_NAMES: &[&str] = &[
    "id",
    "name",
    "title",
    "description",
    "note",
    "notes",
    "memo",
    "comment",
    "comments",
    "type",
    "status",
    "value",
    "amount",
    "total",
    "subtotal",
    "count",
    "price",
    "cost",
    "qty",
    "quantity",
    "label",
    "date",
];

/// Which of `table_columns` (real schema names, any case) are named in the
/// question but never mentioned anywhere in `sql_texts` — a sign the model
/// dropped a filter or grouping the question implied (e.g. "how many books
/// have I *finished*" answered without a `finished` filter anywhere in the
/// query). Lexical and conservative: word-boundary matched, generic names
/// filtered out, only meaningful for a single table (the caller resolves
/// that). Pure so it's testable without a real `AnalyticsSource`/catalog, same
/// pattern as `missing_aggregate_verbs`. Doesn't check filter *values*
/// (`tier = 'close'` vs `tier = 'active'`) only that the column was
/// referenced at all; see #67 for the harder cases this still misses.
fn dropped_columns<'a>(
    question: &str,
    sql_texts: &[String],
    table_columns: &'a [String],
) -> Vec<&'a str> {
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
    engine: &dyn AnalyticsSource,
    question: &str,
    evidence: &[EvidenceItem],
    out: &mut Vec<VerificationCheck>,
) {
    let sql: Vec<String> = evidence
        .iter()
        .filter(|e| is_sql_evidence(e) && e.is_accepted())
        .filter_map(|e| e.sql.as_deref())
        .map(str::to_string)
        .collect();
    let tables: HashSet<String> = sql
        .iter()
        .flat_map(|s| referenced_relations(&s.to_lowercase()))
        .collect();
    let [table] = tables.iter().collect::<Vec<_>>()[..] else {
        return;
    };
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

// --- Legacy candidate-signal helpers, not used for runtime acceptance ------

/// Test-only legacy heuristic. Shared field names can suggest a relationship,
/// but cannot establish that a user's question requires a join. Production
/// verification uses an explicitly grounded contract's declared joins instead.
#[cfg(test)]
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
    counts
        .into_iter()
        .filter(|&(_, n)| n >= 2)
        .map(|(k, _)| k)
        .collect()
}

/// Retained for tests of the old lexical signal; this must not gate an answer
/// or trigger a repair because a shared column name is not user intent.
#[cfg(test)]
fn looks_like_a_missed_join(
    question: &str,
    all_tables: &[(&str, Vec<String>)],
    touched: &HashSet<String>,
) -> bool {
    if touched.len() != 1 || all_tables.len() < 2 {
        return false;
    }
    let q = question.to_lowercase();
    multi_table_columns(all_tables)
        .iter()
        .any(|c| contains_word(&q, c))
}

// --- 2. re-run cited queries ------------------------------------------

fn rerun_queries(
    engine: &dyn AnalyticsSource,
    evidence: &[EvidenceItem],
    out: &mut Vec<VerificationCheck>,
) {
    let mut matched = 0usize;
    let mut skipped_cost = 0usize;
    let mut seen: HashSet<&str> = HashSet::new();
    for e in evidence
        .iter()
        .filter(|e| e.is_accepted() && is_sql_evidence(e) && e.tool != "forecast_analysis")
    {
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
        let row_limit = if e.tool == "make_chart" {
            chart::MAX_SOURCE_ROWS
        } else {
            crate::engine::analytics::data::DEFAULT_ROW_CAP
        };
        match engine.run_sql_with_limit(sql, row_limit) {
            Ok(fresh) => {
                let same = e.row_count == Some(fresh.row_count)
                    && e.rows
                        .as_ref()
                        .map(|r| rows_match(r, &fresh.rows))
                        .unwrap_or(true);
                if same {
                    matched += 1;
                } else {
                    out.push(exclude_evidence_warning(
                        "a query behind this answer gives a different result now",
                        Some(truncate(sql, 120)),
                        VerificationFindingCode::ReplayMismatch,
                        vec![e.id.clone()],
                        "rerun this query and revise claims that depended on its stale result",
                    ));
                }
            }
            Err(err) => out.push(exclude_evidence_warning(
                "a query behind this answer no longer runs",
                Some(format!("{}: {err}", truncate(sql, 100))),
                VerificationFindingCode::ReplayMismatch,
                vec![e.id.clone()],
                "replace only this result with a query that runs on the current workspace",
            )),
        }
    }
    if matched > 0 {
        out.push(successful_replay_check(
            "re-checked the queries behind this answer  same results",
        ));
    }
    if skipped_cost > 0 {
        out.push(ok(
            "an expensive query behind this answer was not re-run  trusting its first result",
        ));
    }
}

/// Python is a presentation path, not an unverifiable escape hatch. The
/// sandbox records every bounded SQL input, and the verifier replays those
/// inputs against the same workspace revision before accepting the statistic.
/// This checks the data boundary independently of the generated Python code;
/// the sandbox itself remains responsible for the arithmetic.
fn rerun_python_inputs(
    engine: &dyn AnalyticsSource,
    evidence: &[EvidenceItem],
    out: &mut Vec<VerificationCheck>,
) {
    let mut matched = 0usize;
    let mut saw_python = false;
    for item in evidence.iter().filter(|item| {
        matches!(item.tool.as_str(), "run_python" | "forecast_analysis") && item.is_accepted()
    }) {
        saw_python = true;
        if item.python_queries_complete != Some(true) {
            out.push(warn(
                "Python computation SQL trace was incomplete",
                Some("the runtime capped the recorded inputs, so this result cannot be accepted as fully replayable".into()),
            ));
            continue;
        }
        let Some(queries) = item.python_queries.as_deref() else {
            out.push(warn(
                "Python computation had no replayable SQL input",
                Some("the result was not grounded to a recorded workspace query".into()),
            ));
            continue;
        };
        if queries.is_empty() {
            out.push(warn(
                "Python computation had no replayable SQL input",
                Some("the result was not grounded to a recorded workspace query".into()),
            ));
            continue;
        }
        let mut item_ok = true;
        for query in queries {
            match engine.run_sql(&query.sql) {
                Ok(fresh) => {
                    let same = query.columns == fresh.columns
                        && query.row_count == fresh.row_count
                        && query.truncated == fresh.truncated
                        && rows_match(&query.rows, &fresh.rows);
                    if same {
                        matched += 1;
                    } else {
                        item_ok = false;
                        out.push(exclude_evidence_warning(
                            "a SQL input to the Python computation gives a different result now",
                            Some(truncate(&query.sql, 120)),
                            VerificationFindingCode::ReplayMismatch,
                            vec![item.id.clone()],
                            "recompute only this Python result using current SQL inputs",
                        ));
                    }
                }
                Err(error) => {
                    item_ok = false;
                    out.push(exclude_evidence_warning(
                        "a SQL input to the Python computation no longer runs",
                        Some(format!("{}: {error}", truncate(&query.sql, 100))),
                        VerificationFindingCode::ReplayMismatch,
                        vec![item.id.clone()],
                        "repair the recorded input query and recompute this Python result",
                    ));
                }
            }
        }
        if !item_ok {
            continue;
        }
    }
    if saw_python && matched > 0 {
        out.push(successful_replay_check(
            "re-checked the SQL inputs to this Python computation  same results",
        ));
    }
}

// --- 3. numbers in the answer are backed by evidence ------------------

fn check_numbers(
    question: &str,
    answer: &str,
    evidence: &[EvidenceItem],
    contract: Option<&AnalysisContract>,
    out: &mut Vec<VerificationCheck>,
) {
    // No tool ran at all -- a general-knowledge or conversational answer, not
    // a data claim. Every number in it would otherwise flag as "unsupported"
    // by definition (there's no evidence to check against), which reads as
    // Fella distrusting its own small talk. Nothing to check, so no check.
    if evidence.is_empty() {
        return;
    }

    let mut supported: Vec<f64> = Vec::new();
    for e in evidence {
        // Failed or superseded calls are execution history, not valid backing
        // for the final answer. In particular, a rejected query must not
        // suppress a repair merely because its retained rows happen to
        // contain the answer's number.
        if !e.is_accepted() {
            continue;
        }
        // SQL summaries contain execution metadata such as `2 rows in 50ms`.
        // Those numbers describe the tool run, not the analyzed population;
        // only returned cells may support a numerical data claim.
        if !is_sql_evidence(e) {
            collect_numbers(&e.result_summary, &mut supported);
        }
        // `output` is where read_file / run_python put their text - an answer
        // that quotes a figure from a note or a Python print is still backed.
        if let Some(output) = &e.output {
            collect_numbers(output, &mut supported);
        }
        if let Some(rows) = &e.rows {
            // An aggregate over no matching rows comes back as one all-NULL row
            // (or zero rows). That result backs the answer "0" / "none" - so
            // the model reporting 0 here isn't an ungrounded figure.
            let empty_aggregate = is_sql_evidence(e)
                && e.is_accepted()
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
        if let Some(chart) = e
            .chart
            .as_ref()
            .filter(|_| !e.artifact_is_withheld("chart"))
        {
            collect_chart_numbers(chart, &mut supported);
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

    let answer_numbers = answer_number_tokens(&checked);
    let question_numbers = answer_number_tokens(question);
    let assumption_numbers = contract
        .into_iter()
        .flat_map(|contract| contract.assumptions.iter())
        .flat_map(|assumption| answer_number_tokens(assumption))
        .map(|(_, value)| value)
        .collect::<Vec<_>>();
    let mut unsupported: Vec<String> = Vec::new();
    let mut user_inputs: Vec<String> = Vec::new();
    for (raw, val) in &answer_numbers {
        if is_probable_year(*val) {
            continue;
        }
        if supported
            .iter()
            .any(|supported| matches_displayed_precision(raw, *supported, *val))
        {
            continue;
        }
        let input_is_declared = question_numbers
            .iter()
            .any(|(_, input)| close(*input, *val))
            && assumption_numbers.iter().any(|input| close(*input, *val));
        if input_is_declared {
            user_inputs.push(raw.clone());
        } else {
            unsupported.push(raw.clone());
        }
    }
    unsupported.dedup();

    if unsupported.is_empty() {
        if !user_inputs.is_empty() {
            user_inputs.dedup();
            out.push(ok_with_detail(
                "user-supplied assumption values are inputs, not workspace observations",
                Some(format!(
                    "{} is recorded in the question and analysis contract",
                    user_inputs.join(", ")
                )),
            ));
        } else if !answer_numbers.is_empty() {
            out.push(ok("every number in the answer came from the data above"));
        }
    } else {
        let shown: Vec<_> = unsupported.iter().take(4).cloned().collect();
        let evidence_ids = evidence
            .iter()
            .filter(|item| item.is_accepted())
            .map(|item| item.id.clone())
            .collect();
        out.push(repair_claim_warning(
            format!(
                "the answer mentions {} not found in any result",
                shown.join(", ")
            ),
            Some("check these against the evidence below".into()),
            VerificationFindingCode::UnsupportedClaim,
            shown.join(", "),
            evidence_ids,
            "revise, qualify, or omit only these unsupported figures; keep valid source results",
        ));
    }
}

/// Typed chart evidence supports the values it actually plots. A validated
/// pie/donut also has a deterministic common whole, so its slice percentages
/// and 100% composition are checkable claims—not unsupported model arithmetic.
fn collect_chart_numbers(data: &chart::ChartData, supported: &mut Vec<f64>) {
    let part_to_whole = matches!(data.kind, chart::ChartKind::Pie | chart::ChartKind::Donut)
        && data.metadata.as_ref().is_some_and(|metadata| {
            metadata.part_to_whole
                && metadata
                    .denominator
                    .as_deref()
                    .is_some_and(|denominator| !denominator.trim().is_empty())
        });

    for series in &data.series {
        let values: Vec<f64> = series
            .values
            .iter()
            .flatten()
            .copied()
            .filter(|value| value.is_finite())
            .collect();
        supported.extend(values.iter().copied());

        if !part_to_whole
            || values.len() != series.values.len()
            || values.iter().any(|value| *value < 0.0)
        {
            continue;
        }
        let whole: f64 = values.iter().sum();
        if !whole.is_finite() || whole <= 0.0 {
            continue;
        }
        supported.push(whole);
        supported.push(100.0);
        supported.extend(values.into_iter().map(|value| value * 100.0 / whole));
    }

    for label in &data.labels {
        collect_numbers(label, supported);
    }
    match data.payload.as_ref() {
        Some(chart::ChartPayload::Scatter { points }) => {
            for point in points {
                supported.extend([point.x, point.y]);
            }
        }
        Some(chart::ChartPayload::BoxPlot { groups }) => {
            for group in groups {
                supported.extend([
                    group.low_whisker,
                    group.q1,
                    group.median,
                    group.q3,
                    group.high_whisker,
                    group.n as f64,
                ]);
                supported.extend(group.outliers.iter().copied());
            }
        }
        Some(chart::ChartPayload::Heatmap { values, .. }) => {
            supported.extend(values.iter().flatten().flatten().copied());
        }
        Some(chart::ChartPayload::Forecast {
            observed,
            forecast,
            lower,
            upper,
            ..
        }) => {
            supported.extend(
                observed
                    .iter()
                    .chain(forecast)
                    .chain(lower)
                    .chain(upper)
                    .flatten()
                    .copied(),
            );
        }
        None => {}
    }
}

/// Number-shaped tokens in answer prose, excluding the numeric part of a
/// leading ordered-list marker (`1.`, `1)`, or `(1)`). Those digits describe
/// the answer's structure rather than a claim about the user's data.
fn answer_number_tokens(text: &str) -> Vec<(String, f64)> {
    let date = regex::Regex::new(r"\b(?:19|20)\d{2}(?:[-/]\d{1,2}){1,2}\b")
        .expect("ISO date pattern is valid");
    text.lines()
        .flat_map(|line| {
            // Dates are labels/context, not standalone claims about a metric.
            // Mask common ISO forms before extracting figures so a failed chart
            // that is explained with its date does not flag the month/day.
            let line_without_dates = date.replace_all(line, " ");
            let line = line_without_dates.as_ref();
            let skip_first = is_ordered_list_marker(line);
            number_tokens(line)
                .enumerate()
                .filter_map(move |(index, token)| (index != 0 || !skip_first).then_some(token))
                .collect::<Vec<_>>()
        })
        .collect()
}

fn is_ordered_list_marker(line: &str) -> bool {
    let line = line.trim_start();
    let bytes = line.as_bytes();
    let (mut index, closing) = if bytes.first() == Some(&b'(') {
        (1, b')')
    } else {
        (0, 0)
    };
    let start = index;
    while index < bytes.len() && bytes[index].is_ascii_digit() {
        index += 1;
    }
    if index == start {
        return false;
    }
    let expected = if closing == 0 {
        bytes.get(index).copied()
    } else {
        Some(closing)
    };
    if !matches!(expected, Some(b'.' | b')')) {
        return false;
    }
    if closing != 0 && bytes.get(index) != Some(&closing) {
        return false;
    }
    let after = index + 1;
    after == bytes.len() || bytes[after].is_ascii_whitespace()
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

fn is_percentage_unit(unit: Option<&str>) -> bool {
    unit.is_some_and(|unit| {
        let unit = unit.to_ascii_lowercase();
        unit.contains('%') || unit.contains("percent") || unit.contains("percentage")
    })
}

/// Two result sets are "the same" for the re-run check if every cell matches
/// exactly, or is a number within `close()` tolerance. A float SUM/AVG can
/// serialise with a low-bit difference when the query runs again a
/// microseconds-later `738022.3` vs `738022.30000000001` is not a changed
/// answer, and shouldn't invalidate the result or prompt a repair.
fn rows_match(a: &[Vec<Json>], b: &[Vec<Json>]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(ra, rb)| {
            ra.len() == rb.len()
                && ra
                    .iter()
                    .zip(rb)
                    .all(|(ca, cb)| match (num_of(ca), num_of(cb)) {
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

/// A reported number is supported when it is the evidence value rounded to
/// the precision actually shown. The general `close()` tolerance is useful
/// for replaying floating-point computations, but a relative tolerance can
/// accept a visibly incorrect whole-number result for a very large total.
fn matches_displayed_precision(raw: &str, evidence_value: f64, reported: f64) -> bool {
    if evidence_value == reported {
        return true;
    }
    let numeric = raw
        .trim_start_matches('$')
        .trim_end_matches('%')
        .replace([',', ' '], "");
    let decimal_places = numeric
        .split_once('.')
        .map(|(_, fraction)| fraction.len())
        .unwrap_or(0)
        .min(15) as i32;
    let rounding_tolerance = 0.5 * 10_f64.powi(-decimal_places);
    let floating_tolerance = evidence_value.abs().max(reported.abs()).max(1.0) * f64::EPSILON * 4.0;
    (evidence_value - reported).abs() <= rounding_tolerance + floating_tolerance
}

pub(crate) fn truncate(s: &str, n: usize) -> String {
    match s.char_indices().nth(n) {
        Some((idx, _)) => format!("{}…", &s[..idx]),
        None => s.to_string(),
    }
}

// --- 10. self-consistency second opinion (#80), cost-gated, not in run() --

/// True when the numeric figures in `a` and `b` don't line up: some number in
/// one has no `close()` counterpart in the other. Ignores probable years (a
/// date isn't the "did we get the figure right" signal). `false` when neither
/// text has a comparable figure at all nothing numeric to compare is not
/// evidence of disagreement. Pure and testable without a real run.
fn answers_disagree(a: &str, b: &str) -> bool {
    let nums = |t: &str| -> Vec<f64> {
        number_tokens(t)
            .map(|(_, v)| v)
            .filter(|v| !is_probable_year(*v))
            .collect()
    };
    let (na, nb) = (nums(a), nums(b));
    if na.is_empty() || nb.is_empty() {
        return false;
    }
    let uncovered = |xs: &[f64], ys: &[f64]| xs.iter().any(|x| !ys.iter().any(|y| close(*x, *y)));
    uncovered(&na, &nb) || uncovered(&nb, &na)
}

/// The check pushed when a second pass from the same model, still conditioned
/// on the conversation, disagrees with the first answer's figures. It is a
/// soft signal rather than independent evidence or proof of error. `None` when
/// the two agree, one pass has no comparable figures, or the second pass failed.
pub fn self_consistency_check(first: &str, second: &str) -> Option<VerificationCheck> {
    if !answers_disagree(first, second) {
        return None;
    }
    let mut check = warn(
        "a second model pass disagrees with this answer",
        Some(format!(
            "asked again with stricter instructions, the model answered: \"{}\"",
            truncate(second.trim(), 200)
        )),
    );
    check.finding = Some(scoped_finding(
        VerificationFindingCode::ModelDisagreement,
        VerificationEffect::Informational,
        VerificationTarget::Answer,
        None,
        Vec::new(),
        Some("treat this as a review signal, not proof that either answer is wrong".into()),
    ));
    Some(check)
}

// --- 9. a date/time GROUP BY collapsed to a NULL bucket --------------------

/// True when `sql`'s `GROUP BY` clause groups by a date/time expression
/// (`strftime(...)`/`date(...)`/`datetime(...)`) -- the shape every "monthly/
/// weekly/yearly breakdown" question's query takes. A crude substring scan on
/// the clause, same altitude as `referenced_relations` -- only the presence
/// of the pattern matters, not a full parse.
fn groups_by_date_expr(sql: &str) -> bool {
    let lower = sql.to_lowercase();
    let Some(gb) = lower.find("group by") else {
        return false;
    };
    let clause = &lower[gb..];
    clause.contains("strftime(") || clause.contains("date(") || clause.contains("datetime(")
}

/// True when at least one result row's first column -- the grouping key, by
/// the "group expression selected first" convention every case this check
/// has ever seen follows -- is NULL.
fn has_null_group_key(rows: &[Vec<Json>]) -> bool {
    rows.iter().any(|r| r.first().is_some_and(Json::is_null))
}

/// Catches the "valid query, wrong question" failure mode #67 was filed for:
/// a query that runs cleanly, reruns to the same result, and only cites
/// numbers actually in its own output -- every earlier check in this file
/// passes -- but a date/time `GROUP BY` produced a NULL key for at least one
/// row, meaning rows that should have landed in separate months/weeks/years
/// instead collapsed into one ungrouped bucket. Found from a real report: a
/// ledger with non-ISO dates ("Aug 1, 2026") made every `strftime()` call
/// return NULL, so "monthly breakdown of my rent" answered with the grand
/// total under a single blank month, and nothing above caught it since the
/// total genuinely was the number the query returned.
///
/// Deliberately narrow: only fires on a NULL key, never merely a single
/// *row* (a dataset that legitimately spans one real month still has a
/// real, non-null key in its one row and is left alone).
fn check_null_group_key(evidence: &[EvidenceItem], out: &mut Vec<VerificationCheck>) {
    for e in evidence {
        if !is_sql_evidence(e) || !e.is_accepted() {
            continue;
        }
        let Some(sql) = &e.sql else { continue };
        if !groups_by_date_expr(sql) {
            continue;
        }
        let Some(rows) = &e.rows else { continue };
        if has_null_group_key(rows) {
            out.push(exclude_evidence_warning(
                "a query behind this answer groups by a date expression that returned no \
value for at least one row",
                Some(
                    "the column this groups by likely isn't in a format strftime()/date() can \
parse for every row, so those rows collapsed into one ungrouped bucket instead of their real \
month/week/year check the raw column's values"
                        .into(),
                ),
                VerificationFindingCode::SemanticExecutionMismatch,
                vec![e.id.clone()],
                "inspect the raw date values and repair only this grouped result; keep other evidence",
            ));
            return;
        }
    }
}

// --- 10. a value in a multi-aggregate row mislabeled with another column's name

/// Split a SQL column alias into lowercase words on non-alphanumeric
/// boundaries (`avg_sleep` -> ["avg", "sleep"]). Single-character
/// fragments are dropped -- too easy to coincidentally match.
fn alias_words(alias: &str) -> Vec<String> {
    alias
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| word.len() > 1 && !word.chars().all(|character| character.is_ascii_digit()))
        .map(|w| w.to_lowercase())
        .collect()
}

/// Generic aggregation/shape words describe how a result was computed, not
/// which result column a prose value refers to. They are too weak to establish
/// a column identity on their own (for example, "annual total" may describe
/// either an observed total or a hypothetical total).
fn is_generic_result_word(word: &str) -> bool {
    matches!(
        word,
        "amount"
            | "avg"
            | "average"
            | "count"
            | "maximum"
            | "max"
            | "measure"
            | "mean"
            | "metric"
            | "minimum"
            | "min"
            | "result"
            | "sum"
            | "total"
            | "value"
    )
}

fn is_arithmetic_expression_line(line: &str) -> bool {
    [" + ", " - ", " * ", " / ", " = ", " × ", " ÷ "]
        .iter()
        .any(|operator| line.contains(operator))
}

fn number_token_spans(text: &str) -> Vec<(String, f64, usize, usize)> {
    let mut cursor = 0;
    number_tokens(text)
        .map(|(raw, value)| {
            let start = cursor + text[cursor..].find(&raw).unwrap_or(0);
            let end = start + raw.len();
            cursor = end;
            (raw, value, start, end)
        })
        .collect()
}

fn word_spans(text: &str) -> Vec<(String, usize, usize)> {
    let mut words = Vec::new();
    let mut start = None;
    for (index, character) in text.char_indices() {
        if character.is_alphanumeric() || character == '_' {
            start.get_or_insert(index);
        } else if let Some(word_start) = start.take() {
            words.push((
                text[word_start..index].to_ascii_lowercase(),
                word_start,
                index,
            ));
        }
    }
    if let Some(word_start) = start {
        words.push((
            text[word_start..].to_ascii_lowercase(),
            word_start,
            text.len(),
        ));
    }
    words
}

/// Column words should be near the number they label, not merely elsewhere
/// on a sentence that mentions another measure or sample size. If a word is
/// equally close to adjacent figures, associate it with the following figure,
/// matching common prose such as "rent $450, groceries $50".
fn alias_word_is_local_to_number(
    word: &str,
    words: &[(String, usize, usize)],
    numbers: &[(String, f64, usize, usize)],
    target_number: usize,
) -> bool {
    const MAX_LABEL_DISTANCE: usize = 5;
    words
        .iter()
        .filter(|(candidate, _, _)| candidate == word)
        .any(|(_, word_start, word_end)| {
            let distances: Vec<usize> = numbers
                .iter()
                .map(|(_, _, number_start, number_end)| {
                    if word_end <= number_start {
                        words
                            .iter()
                            .filter(|(_, start, end)| *start >= *word_end && *end <= *number_start)
                            .count()
                    } else if number_end <= word_start {
                        words
                            .iter()
                            .filter(|(_, start, end)| *start >= *number_end && *end <= *word_start)
                            .count()
                    } else {
                        0
                    }
                })
                .collect();
            let nearest_distance = distances.iter().copied().min().unwrap_or(usize::MAX);
            let nearest_number = distances
                .iter()
                .enumerate()
                .filter(|(_, distance)| **distance == nearest_distance)
                .map(|(index, _)| index)
                .max();
            nearest_distance <= MAX_LABEL_DISTANCE && nearest_number == Some(target_number)
        })
}

/// A query that combines several unrelated aggregates into one row (`SELECT
/// (SELECT ...) AS a, (SELECT ...) AS b, ...`) is the one shape where the
/// model has to correctly remember which value belongs to which column
/// after the fact, with no per-row label to anchor it -- the real,
/// live-observed failure: the same value ("202.78") labeled "Sleep" in one
/// run, "Dining out" in another, "Running" in a third; only one of them is
/// actually `max_dining`. `check_numbers` doesn't catch this: it pools
/// every cell from every row into one flat set and only checks whether a
/// stated number is *somewhere* in it, with no column identity at all.
///
/// Deliberately narrow, precision over recall: only a single-row, 2+
/// column result (a normal multi-row breakdown already carries its own
/// label per row, a different shape); only a value that matches exactly
/// one column (an ambiguous or unsupported value is left to
/// `check_numbers`); only flagged when the line naming it contains some
/// *other* column's own words and not its true column's -- a line that
/// names no column at all is left alone.
fn check_row_value_labels(
    answer: &str,
    evidence: &[EvidenceItem],
    out: &mut Vec<VerificationCheck>,
) {
    for e in evidence {
        if !is_sql_evidence(e) || !e.is_accepted() {
            continue;
        }
        let (Some(columns), Some(rows)) = (&e.columns, &e.rows) else {
            continue;
        };
        if columns.len() < 2 || rows.len() != 1 {
            continue;
        }
        let col_vals: Vec<(&str, f64)> = columns
            .iter()
            .zip(rows[0].iter())
            .filter_map(|(c, v)| num_of(v).map(|n| (c.as_str(), n)))
            .collect();
        if col_vals.len() < 2 {
            continue;
        }
        // A shared token such as "rentals", or a generic aggregate word such
        // as "total", is not enough to infer which result a nearby number
        // describes. Only use column-specific semantic words.
        let aliases: Vec<Vec<String>> = col_vals
            .iter()
            .map(|(column, _)| alias_words(column))
            .collect();
        let distinctive: Vec<Vec<&str>> = aliases
            .iter()
            .enumerate()
            .map(|(index, words)| {
                words
                    .iter()
                    .filter(|word| {
                        !is_generic_result_word(word)
                            && !aliases.iter().enumerate().any(|(other_index, other)| {
                                other_index != index && other.contains(word)
                            })
                    })
                    .map(String::as_str)
                    .collect()
            })
            .collect();

        for line in answer.lines() {
            // In a formula, a line-level label commonly describes the result
            // at the right of the operator, not every input value on the line.
            if is_arithmetic_expression_line(line) {
                continue;
            }
            // A year is temporal context, not a competing result value. Do not
            // let it steal a nearby label such as "annual total" from the
            // figure that follows it.
            let numbers: Vec<_> = number_token_spans(line)
                .into_iter()
                .filter(|(_, value, _, _)| !is_probable_year(*value))
                .collect();
            let words = word_spans(line);
            for (number_index, (raw, val, _, _)) in numbers.iter().enumerate() {
                let matches: Vec<&str> = col_vals
                    .iter()
                    .filter(|(_, v)| close(*v, *val))
                    .map(|(c, _)| *c)
                    .collect();
                if matches.len() != 1 {
                    continue; // unsupported or ambiguous -- not this check's business
                }
                let true_index = col_vals
                    .iter()
                    .position(|(column, _)| *column == matches[0])
                    .expect("matched column belongs to the result");
                let repeated_representation =
                    numbers
                        .iter()
                        .take(number_index)
                        .any(|(_, previous_value, _, _)| {
                            let previous_matches: Vec<usize> = col_vals
                                .iter()
                                .enumerate()
                                .filter(|(_, (_, value))| close(*value, *previous_value))
                                .map(|(index, _)| index)
                                .collect();
                            previous_matches.len() == 1
                                && previous_matches[0] == true_index
                                && close(*previous_value, *val)
                        });
                if repeated_representation {
                    // A rounded restatement of the same result is not a new
                    // column claim; judge its label at the first mention.
                    continue;
                }
                let true_col = matches[0];
                if distinctive[true_index]
                    .iter()
                    .any(|word| alias_word_is_local_to_number(word, &words, &numbers, number_index))
                {
                    continue; // correctly labeled, or at least not contradicted
                }
                let swap = col_vals.iter().enumerate().find(|(index, (c, v))| {
                    *c != true_col
                        && !close(*v, *val)
                        && distinctive[*index].iter().any(|word| {
                            alias_word_is_local_to_number(word, &words, &numbers, number_index)
                        })
                });
                if let Some((_, (other_col, _))) = swap {
                    out.push(repair_claim_warning(
                        format!(
                            "\"{raw}\" is labeled like {other_col} but actually came from {true_col}"
                        ),
                        Some(
                            "this query returned several figures in one row check which one \
this line is actually quoting"
                                .into(),
                        ),
                        VerificationFindingCode::ContractMismatch,
                        raw.to_string(),
                        vec![e.id.clone()],
                        "correct only the claim-to-column attribution; retain the query result",
                    ));
                    return;
                }
            }
        }
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
    fn cte_names_are_not_catalogued_tables() {
        let r = referenced_relations(
            "WITH sorted AS (SELECT * FROM mood), scored AS (SELECT * FROM sorted) \
             SELECT * FROM scored JOIN costs ON scored.day = costs.day",
        );
        assert!(r.contains("mood") && r.contains("costs"));
        assert!(!r.contains("sorted") && !r.contains("scored"));
    }

    #[test]
    fn derived_subqueries_are_not_reported_as_physical_tables() {
        let r = referenced_relations(
            "SELECT ranked.name FROM ((SELECT name, total FROM sales)) ranked \
             JOIN regions ON ranked.name = regions.name",
        );
        assert!(r.contains("sales"));
        assert!(r.contains("regions"));
        assert!(!r.contains("select"));
    }

    #[test]
    fn rows_match_tolerates_float_jitter_only() {
        let a = vec![vec![Json::from(738022.3)]];
        let b = vec![vec![Json::from(738022.3 + 1e-6)]];
        assert!(
            rows_match(&a, &b),
            "sub-cent float drift is the same result"
        );

        let c = vec![vec![Json::from(752000.0)]];
        assert!(
            !rows_match(&a, &c),
            "a result past close() tolerance still trips"
        );

        // non-numeric cells must still match exactly
        let m1 = vec![vec![Json::from("2024-03"), Json::from(10.0)]];
        let m2 = vec![vec![Json::from("2024-04"), Json::from(10.0)]];
        assert!(!rows_match(&m1, &m2));
        let n = vec![vec![Json::from("2024-03"), Json::from(10.0001)]];
        assert!(rows_match(&m1, &n));
    }

    #[test]
    fn signed_questions_reject_absolute_value_sql() {
        let evidence = vec![run_sql_ev(
            "SELECT SUM(ABS(amount)) AS total FROM ledger",
            &["total"],
            vec![vec![Json::from(100)]],
        )];
        let mut checks = Vec::new();
        check_signed_semantics(
            "what was net spending after refunds?",
            &evidence,
            &mut checks,
        );
        assert!(checks
            .iter()
            .any(|check| { !check.ok && check.label.contains("signed values were discarded") }));
        assert!(checks.iter().any(|check| {
            check.finding.as_ref().is_some_and(|finding| {
                finding.effect == VerificationEffect::ExcludeEvidence
                    && finding.target == VerificationTarget::Evidence
                    && finding.evidence_ids == vec![evidence[0].id.clone()]
            })
        }));
        assert!(hard_fail(&checks).is_none());

        let mut explicit_absolute = Vec::new();
        check_signed_semantics(
            "what was the absolute magnitude of spending?",
            &evidence,
            &mut explicit_absolute,
        );
        assert!(explicit_absolute.is_empty());
    }

    #[test]
    fn signed_check_allows_absolute_prior_period_denominator() {
        assert!(!signed_value_abs_is_discarding(
            "SELECT ((current - previous) / NULLIF(ABS(previous), 0)) * 100 AS change_pct"
        ));
        assert!(signed_value_abs_is_discarding(
            "SELECT SUM(ABS(amount)) AS spending FROM ledger"
        ));
    }

    #[test]
    fn python_sql_inputs_can_be_replayed_for_acceptance() {
        struct ReplaySource;

        impl AnalyticsSource for ReplaySource {
            fn catalog(&self) -> crate::engine::catalog::Catalog {
                crate::engine::catalog::Catalog::default()
            }

            fn run_sql(
                &self,
                sql: &str,
            ) -> crate::engine::error::EngineResult<crate::engine::state::QueryResult> {
                assert_eq!(sql, "SELECT value FROM measurements");
                Ok(crate::engine::state::QueryResult {
                    columns: vec!["value".into()],
                    rows: vec![vec![Json::from(6)], vec![Json::from(8)]],
                    row_count: 2,
                    ms: 1,
                    truncated: false,
                })
            }

            fn run_sql_with_limit(
                &self,
                sql: &str,
                _max_rows: usize,
            ) -> crate::engine::error::EngineResult<crate::engine::state::QueryResult> {
                self.run_sql(sql)
            }
        }

        let mut evidence = run_sql_ev(
            "SELECT value FROM measurements",
            &["value"],
            vec![vec![Json::from(6)], vec![Json::from(8)]],
        );
        evidence.tool = "run_python".into();
        evidence.sql = None;
        evidence.output = Some("7".into());
        evidence.python_queries = Some(vec![crate::engine::analytics::pyexec::PythonQueryTrace {
            sql: "SELECT value FROM measurements".into(),
            columns: vec!["value".into()],
            rows: vec![vec![Json::from(6)], vec![Json::from(8)]],
            row_count: 2,
            truncated: false,
        }]);
        evidence.python_queries_complete = Some(true);

        let mut checks = Vec::new();
        rerun_python_inputs(&ReplaySource, &[evidence], &mut checks);
        assert!(checks.iter().any(|check| {
            check.ok
                && check
                    .label
                    .contains("re-checked the SQL inputs to this Python computation")
        }));
        assert!(reran_clean(&checks));
    }

    #[test]
    fn replay_mismatch_excludes_only_the_result_that_changed() {
        struct ReplaySource;

        impl AnalyticsSource for ReplaySource {
            fn catalog(&self) -> crate::engine::catalog::Catalog {
                crate::engine::catalog::Catalog::default()
            }

            fn run_sql(
                &self,
                sql: &str,
            ) -> crate::engine::error::EngineResult<crate::engine::state::QueryResult> {
                let (columns, rows) = match sql {
                    "SELECT amount FROM current" => {
                        (vec!["amount".into()], vec![vec![Json::from(100)]])
                    }
                    "SELECT COUNT(*) AS rows FROM other" => {
                        (vec!["rows".into()], vec![vec![Json::from(2)]])
                    }
                    _ => panic!("unexpected replay query: {sql}"),
                };
                Ok(crate::engine::state::QueryResult {
                    row_count: rows.len(),
                    columns,
                    rows,
                    ms: 1,
                    truncated: false,
                })
            }

            fn run_sql_with_limit(
                &self,
                sql: &str,
                _max_rows: usize,
            ) -> crate::engine::error::EngineResult<crate::engine::state::QueryResult> {
                self.run_sql(sql)
            }
        }

        let mut changed = run_sql_ev(
            "SELECT amount FROM current",
            &["amount"],
            vec![vec![Json::from(90)]],
        );
        changed.id = "changed-query".into();
        let mut stable = run_sql_ev(
            "SELECT COUNT(*) AS rows FROM other",
            &["rows"],
            vec![vec![Json::from(2)]],
        );
        stable.id = "stable-query".into();

        let mut checks = Vec::new();
        rerun_queries(&ReplaySource, &[changed, stable], &mut checks);

        let mismatch = checks
            .iter()
            .find(|check| {
                check
                    .finding
                    .as_ref()
                    .is_some_and(|finding| finding.code == VerificationFindingCode::ReplayMismatch)
            })
            .expect("changed query should create a replay finding");
        let finding = mismatch.finding.as_ref().unwrap();
        assert_eq!(finding.effect, VerificationEffect::ExcludeEvidence);
        assert_eq!(finding.target, VerificationTarget::Evidence);
        assert_eq!(finding.evidence_ids, vec!["changed-query"]);
        assert!(checks.iter().any(|check| {
            check.ok
                && check
                    .label
                    .contains("re-checked the queries behind this answer")
        }));
        assert!(hard_fail(&checks).is_none());
    }

    #[test]
    fn chart_source_replay_uses_the_chart_row_limit() {
        struct ReplaySource;

        impl AnalyticsSource for ReplaySource {
            fn catalog(&self) -> crate::engine::catalog::Catalog {
                crate::engine::catalog::Catalog::default()
            }

            fn run_sql(
                &self,
                _sql: &str,
            ) -> crate::engine::error::EngineResult<crate::engine::state::QueryResult> {
                panic!("chart replay should use the explicit row limit")
            }

            fn run_sql_with_limit(
                &self,
                sql: &str,
                max_rows: usize,
            ) -> crate::engine::error::EngineResult<crate::engine::state::QueryResult> {
                assert_eq!(sql, "SELECT day FROM daily");
                assert_eq!(max_rows, chart::MAX_SOURCE_ROWS);
                let rows = (0..1_001)
                    .map(|day| vec![Json::from(day)])
                    .collect::<Vec<_>>();
                Ok(crate::engine::state::QueryResult {
                    columns: vec!["day".into()],
                    row_count: rows.len(),
                    rows,
                    ms: 1,
                    truncated: false,
                })
            }
        }

        let sql = "SELECT day FROM daily";
        let rows = (0..1_001)
            .map(|day| vec![Json::from(day)])
            .collect::<Vec<_>>();
        let mut evidence = run_sql_ev(sql, &["day"], rows);
        evidence.tool = "make_chart".into();

        let mut checks = Vec::new();
        rerun_queries(&ReplaySource, &[evidence], &mut checks);

        assert!(
            checks.iter().any(|check| {
                check.ok
                    && check
                        .label
                        .contains("re-checked the queries behind this answer")
            }),
            "the full chart result should replay without a false changed-result warning: {checks:?}"
        );
        assert!(hard_fail(&checks).is_none(), "{checks:?}");
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
        assert_eq!(
            aggregates_text_column("SELECT count(*) FROM ledger", &cols),
            None
        );
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
    fn positive_quality_count_keeps_a_partial_total_out_of_verified_status() {
        let evidence = vec![run_sql_ev(
            "SELECT SUM(parse_num(amount)) AS total, \
             COUNT(*) - COUNT(parse_num(amount)) AS unparseable_rows FROM ledger",
            &["total", "unparseable_rows"],
            vec![vec![Json::from(3434.94), Json::from(2)]],
        )];
        let mut checks = Vec::new();
        check_incomplete_quality_audit("what was total spending?", &evidence, &mut checks);
        assert!(
            checks
                .iter()
                .any(|check| !check.ok && check.label.contains("excludes rows")),
            "a positive quality count should make the result visibly partial: {checks:?}"
        );

        let mut scoped_checks = Vec::new();
        check_incomplete_quality_audit(
            "what was the average measured sleep excluding missing values?",
            &evidence,
            &mut scoped_checks,
        );
        assert!(scoped_checks.is_empty());
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
    fn selects_label_columns_for_inline_case_guidance() {
        use crate::engine::catalog::ColumnInfo;

        assert!(is_filterable_text_column(&ColumnInfo::bare(
            "purpose", "TEXT"
        )));
        assert!(is_filterable_text_column(&ColumnInfo::bare(
            "status", "VARCHAR"
        )));
        assert!(!is_filterable_text_column(&ColumnInfo::bare(
            "created_at",
            "TEXT"
        )));
        assert!(!is_filterable_text_column(&ColumnInfo::bare(
            "description",
            "TEXT"
        )));
        assert!(!is_filterable_text_column(&ColumnInfo::bare(
            "amount", "REAL"
        )));

        let mut many_values = ColumnInfo::bare("merchant", "TEXT");
        many_values.distinct = Some(101);
        assert!(!is_filterable_text_column(&many_values));
    }

    #[test]
    fn spots_missing_aggregate_verb() {
        let sql = |s: &str| vec![s.to_uppercase()];

        // Explicit count wording implies COUNT, query doesn't have it -> flagged
        assert_eq!(
            missing_aggregate_verbs(
                "what is the count of books I finished?",
                &sql("SELECT * FROM books")
            ),
            vec!["COUNT"]
        );
        // query does have it -> not flagged
        assert!(missing_aggregate_verbs(
            "what is the count of books I finished?",
            &sql("SELECT COUNT(*) FROM books WHERE finished = 'yes'")
        )
        .is_empty());
        // "How many" can request a summed measure, not just row cardinality.
        // Leave that interpretation to the model instead of forcing COUNT.
        assert!(missing_aggregate_verbs(
            "how many total trip-nights did I spend?",
            &sql("SELECT SUM(nights) FROM trips")
        )
        .is_empty());
        assert!(missing_aggregate_verbs(
            "how many books have I finished?",
            &sql("SELECT * FROM books")
        )
        .is_empty());
        // "total" implies SUM
        assert_eq!(
            missing_aggregate_verbs("what's my total spend?", &sql("SELECT amount FROM spend")),
            vec!["SUM"]
        );
        // "average" implies AVG
        assert_eq!(
            missing_aggregate_verbs(
                "what's my average rating?",
                &sql("SELECT MAX(rating) FROM books")
            ),
            vec!["AVG"]
        );
        // no run_sql evidence at all -> nothing to flag (e.g. answered from schema/no-tool)
        assert!(missing_aggregate_verbs("what is the count of books I finished?", &[]).is_empty());
        // wording doesn't imply any of these verbs -> nothing flagged
        assert!(missing_aggregate_verbs(
            "which genre did I read most?",
            &sql("SELECT genre FROM books")
        )
        .is_empty());
        // a compound question can flag more than one verb
        let mut both = missing_aggregate_verbs(
            "what's the total and average rating?",
            &sql("SELECT rating FROM books"),
        );
        both.sort_unstable();
        assert_eq!(both, vec!["AVG", "SUM"]);
    }

    #[test]
    fn chart_queries_use_the_same_sql_checks() {
        let mut evidence = vec![run_sql_ev(
            "SELECT 'all' AS bucket, COUNT(*) AS count FROM books",
            &["bucket", "count"],
            vec![vec![Json::from("all"), Json::from(4)]],
        )];
        evidence[0].tool = "make_chart".into();

        let mut out = Vec::new();
        check_aggregate_verb("how many books are there?", &evidence, &mut out);
        assert!(
            out.is_empty(),
            "chart SQL should count as a cited aggregate: {out:?}"
        );
    }

    #[test]
    fn chart_values_must_match_the_source_rows() {
        let mut evidence = vec![run_sql_ev(
            "SELECT category, amount FROM spend ORDER BY category",
            &["category", "amount"],
            vec![
                vec![Json::from("dining"), Json::from(12)],
                vec![Json::from("rent"), Json::from(24)],
            ],
        )];
        evidence[0].tool = "make_chart".into();
        evidence[0].args = serde_json::json!({
            "kind": "bar",
            "title": "Spending",
            "series_fields": ["amount"]
        });
        evidence[0].chart = Some(
            chart::from_table(
                &chart::TabularResult {
                    columns: evidence[0].columns.clone().unwrap(),
                    rows: evidence[0].rows.clone().unwrap(),
                },
                chart::ChartRequest {
                    kind: chart::ChartKind::Bar,
                    title: Some("Spending".into()),
                    series_fields: vec!["amount".into()],
                    ..Default::default()
                },
            )
            .unwrap(),
        );
        let mut good = Vec::new();
        check_chart_values(&evidence, &mut good);
        assert_eq!(good, vec![ok("chart values matched the source query")]);

        evidence[0].chart.as_mut().unwrap().series[0].values[0] = Some(99.0);
        let mut bad = Vec::new();
        check_chart_values(&evidence, &mut bad);
        let finding = bad
            .iter()
            .find(|check| !check.ok && check.label == "chart values did not match source query")
            .and_then(|check| check.finding.as_ref())
            .expect("chart mismatch carries a scoped finding");
        assert_eq!(finding.effect, VerificationEffect::WithholdArtifact);
        assert_eq!(finding.target, VerificationTarget::Artifact);
        assert_eq!(finding.target_id.as_deref(), Some(evidence[0].id.as_str()));
        assert_eq!(finding.evidence_ids, vec!["evidence-test"]);
        assert!(hard_fail(&bad).is_none());
    }

    #[test]
    fn specialized_chart_payloads_and_bin_labels_ground_numeric_claims() {
        let charts = vec![
            chart::ChartData {
                kind: chart::ChartKind::Histogram,
                title: None,
                labels: vec!["10–25".into()],
                series: vec![chart::Series {
                    name: "Count".into(),
                    values: vec![Some(4.0)],
                }],
                unit: None,
                x_label: None,
                y_label: None,
                payload: None,
                metadata: None,
            },
            chart::ChartData {
                kind: chart::ChartKind::Scatter,
                title: None,
                labels: Vec::new(),
                series: Vec::new(),
                unit: None,
                x_label: None,
                y_label: None,
                payload: Some(chart::ChartPayload::Scatter {
                    points: vec![chart::ScatterPoint {
                        x: 6.0,
                        y: 7.0,
                        label: "point".into(),
                        group: None,
                    }],
                }),
                metadata: None,
            },
            chart::ChartData {
                kind: chart::ChartKind::BoxPlot,
                title: None,
                labels: Vec::new(),
                series: Vec::new(),
                unit: None,
                x_label: None,
                y_label: None,
                payload: Some(chart::ChartPayload::BoxPlot {
                    groups: vec![chart::BoxSummary {
                        label: "A".into(),
                        low_whisker: 1.0,
                        q1: 2.0,
                        median: 3.0,
                        q3: 4.0,
                        high_whisker: 5.0,
                        outliers: vec![20.0],
                        n: 5,
                    }],
                }),
                metadata: None,
            },
            chart::ChartData {
                kind: chart::ChartKind::Heatmap,
                title: None,
                labels: Vec::new(),
                series: Vec::new(),
                unit: None,
                x_label: None,
                y_label: None,
                payload: Some(chart::ChartPayload::Heatmap {
                    x_labels: vec!["Jan".into()],
                    y_labels: vec!["East".into()],
                    values: vec![vec![Some(8.0)]],
                }),
                metadata: None,
            },
            chart::ChartData {
                kind: chart::ChartKind::Forecast,
                title: None,
                labels: Vec::new(),
                series: Vec::new(),
                unit: None,
                x_label: None,
                y_label: None,
                payload: Some(chart::ChartPayload::Forecast {
                    observed: vec![Some(9.0)],
                    forecast: vec![None],
                    lower: vec![Some(8.0)],
                    upper: vec![Some(10.0)],
                    uncertainty_note: None,
                }),
                metadata: None,
            },
        ];
        let evidence = charts
            .into_iter()
            .enumerate()
            .map(|(index, chart)| {
                let mut item = run_sql_ev("", &[], Vec::new());
                item.id = format!("chart-{index}");
                item.tool = "make_chart".into();
                item.chart = Some(chart);
                item
            })
            .collect::<Vec<_>>();
        let mut checks = Vec::new();
        check_numbers(
            "Summarize the chart values.",
            "Histogram bin 10–25 has count 4. Scatter x is 6 and y is 7. Box quartiles are 2, 3, and 4 with outlier 20. Heatmap value is 8. Forecast is 9 with bounds 8 and 10.",
            &evidence,
            None,
            &mut checks,
        );

        assert!(
            checks.iter().all(|check| check.ok),
            "typed chart data should support each plotted numerical claim: {checks:?}"
        );
    }

    #[test]
    fn chart_verification_rebuilds_a_specialized_view_from_referenced_python_output() {
        let mut source = run_sql_ev("", &[], vec![]);
        source.id = "evidence-1".into();
        source.tool = "run_python".into();
        source.sql = None;
        source.columns = None;
        source.rows = None;
        source.row_count = None;
        source.result_table = Some(chart::TabularResult {
            columns: vec!["units".into(), "revenue".into(), "region".into()],
            rows: vec![
                vec![Json::from(2), Json::from(20), Json::from("East")],
                vec![Json::from(3), Json::from(35), Json::from("West")],
                vec![Json::from(5), Json::from(50), Json::from("East")],
            ],
        });
        let source_table = source.result_table.as_ref().unwrap();
        let request = chart::ChartRequest {
            kind: chart::ChartKind::Scatter,
            x_field: Some("units".into()),
            y_field: Some("revenue".into()),
            group_field: Some("region".into()),
            metadata: chart::ChartMetadata {
                source_evidence_id: Some("evidence-1".into()),
                source_label: Some("run_python".into()),
                ..Default::default()
            },
            ..Default::default()
        };
        let chart_data = chart::from_table(source_table, request).unwrap();
        let mut chart_item = run_sql_ev("", &[], vec![]);
        chart_item.id = "evidence-2".into();
        chart_item.tool = "make_chart".into();
        chart_item.args = serde_json::json!({
            "kind": "scatter",
            "source_evidence_id": "evidence-1",
            "x_field": "units",
            "y_field": "revenue",
            "group_field": "region"
        });
        chart_item.chart = Some(chart_data);
        let evidence = vec![source, chart_item];

        let mut good = Vec::new();
        check_chart_values(&evidence, &mut good);
        assert_eq!(good, vec![ok("chart values matched the source query")]);

        let mut corrupted = evidence;
        if let Some(chart::ChartPayload::Scatter { points }) = corrupted[1]
            .chart
            .as_mut()
            .and_then(|spec| spec.payload.as_mut())
        {
            points[0].y = 999.0;
        }
        let mut bad = Vec::new();
        check_chart_values(&corrupted, &mut bad);
        assert!(bad.iter().any(|check| {
            !check.ok && check.label == "chart values did not match source query"
        }));
    }

    #[test]
    fn spots_a_dropped_column() {
        let cols = vec![
            "finished".to_string(),
            "genre".to_string(),
            "title".to_string(),
        ];

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
        let orders = (
            "orders",
            vec!["customer_id".to_string(), "amount".to_string()],
        );
        let customers = (
            "customers",
            vec!["customer_id".to_string(), "city".to_string()],
        );
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
    fn shared_column_name_signal_alone_does_not_trigger_semantic_repair() {
        let checks = vec![warn(
            "this question looked like it needed a join across tables; the answer only used one",
            None,
        )];
        assert!(semantic_repair_hint(&checks).is_none());
    }

    #[test]
    fn generic_shared_columns_dont_count() {
        // "id"/"amount"/"date" are stoplisted even though every table has one;
        // a genuinely shared non-generic column ("vendor") is still caught.
        let a = (
            "a",
            vec!["id".to_string(), "amount".to_string(), "vendor".to_string()],
        );
        let b = (
            "b",
            vec!["id".to_string(), "date".to_string(), "vendor".to_string()],
        );
        assert_eq!(
            multi_table_columns(&[a, b]),
            HashSet::from(["vendor".to_string()])
        );
    }

    #[test]
    fn word_boundaries_are_respected() {
        assert!(contains_word("how many books have i finished?", "finished"));
        assert!(!contains_word("unfinished business", "finished"));
        assert!(!contains_word("the finisher", "finish"));
        assert!(contains_word(
            "select * from t where finished = 'yes'",
            "finished"
        ));
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

        let grouped_with_spacing: Vec<_> =
            number_tokens("The average used (38, 189 + 48, 215) / 2.")
                .map(|(_, value)| value)
                .collect();
        assert_eq!(grouped_with_spacing, vec![38189.0, 48215.0, 2.0]);
    }

    #[test]
    fn numeric_filter_values_must_be_used_on_the_grounded_field() {
        assert_eq!(
            number_tokens("1 AND 6")
                .map(|(_, value)| value)
                .collect::<Vec<_>>(),
            vec![1.0, 6.0]
        );
        assert!(numeric_filter_uses_value(
            "SELECT sum(cnt) FROM daily WHERE yr = 1",
            "yr",
            1.0
        ));
        assert!(numeric_filter_uses_value(
            "SELECT sum(cnt) FROM daily WHERE yr IN ('1')",
            "yr",
            1.0
        ));
        assert!(numeric_filter_uses_value(
            "SELECT sum(cnt) FROM daily WHERE month BETWEEN 1 AND 6",
            "month",
            4.0
        ));
        assert!(!numeric_filter_uses_value(
            "SELECT sum(cnt) FROM daily WHERE month BETWEEN 1 AND 6",
            "month",
            7.0
        ));
        assert!(!numeric_filter_uses_value(
            "SELECT sum(cnt) FROM daily WHERE dteday >= '2012-01-01'",
            "yr",
            1.0
        ));
        assert!(!numeric_filter_uses_value(
            "SELECT yr FROM daily WHERE dteday >= '2012-01-01' AND cnt = 1",
            "yr",
            1.0
        ));
    }

    #[test]
    fn arithmetic_mean_is_a_supported_average_operation() {
        let mut checks = Vec::new();
        check_measure_operation(
            &mut checks,
            "arithmetic mean",
            &["SELECT AVG(rides) FROM daily".to_string()],
            &[],
        );
        assert_eq!(checks.len(), 1);
        assert!(checks[0].ok, "{checks:?}");
    }

    #[test]
    fn raw_observation_measure_requires_a_non_aggregate_projection() {
        for operation in ["raw", "raw distribution", "raw observations"] {
            let mut checks = Vec::new();
            check_measure_operation(
                &mut checks,
                operation,
                &["SELECT cohort, score FROM readings ORDER BY cohort, score".into()],
                &[],
            );
            assert_eq!(checks.len(), 1);
            assert!(checks[0].ok, "{operation}: {checks:?}");
        }

        let mut aggregated = Vec::new();
        check_measure_operation(
            &mut aggregated,
            "raw",
            &["SELECT AVG(score) FROM readings".into()],
            &[],
        );
        assert_eq!(aggregated.len(), 1);
        assert!(!aggregated[0].ok, "{aggregated:?}");
    }

    #[test]
    fn forecast_without_evaluation_is_advisory_and_keeps_the_point_result() {
        let point_only = run_sql_ev(
            "SELECT AVG(value) AS estimate FROM monthly_values",
            &["estimate"],
            vec![vec![Json::from(35)]],
        );
        let mut checks = Vec::new();
        check_forecast_evaluation(
            "Forecast next month from the workspace series",
            &[point_only.clone()],
            &mut checks,
        );
        assert_eq!(checks.len(), 1);
        assert!(!checks[0].ok);
        assert!(checks[0]
            .label
            .contains("forecast lacks chronological evaluation"));
        assert!(semantic_repair_hint(&checks).is_none());
        assert_eq!(
            checks[0].finding.as_ref().unwrap().effect,
            VerificationEffect::Informational
        );
        assert!(point_only.is_accepted());

        let mut evaluated = run_sql_ev(
            "SELECT period, value FROM monthly_values ORDER BY period",
            &["period", "value"],
            vec![vec![Json::String("2025-01".into()), Json::from(10)]],
        );
        evaluated.tool = "forecast_analysis".into();
        evaluated.args = serde_json::json!({"method":"mean", "horizon":1});
        evaluated.output = Some(
            "forecast_values=[10.0]\nbacktest={'mae': 2.0, 'baseline_mae': 3.0}\nempirical_error_bands={'available': False}".into(),
        );
        let mut evaluated_checks = Vec::new();
        check_forecast_evaluation(
            "Forecast next month from the workspace series",
            &[evaluated],
            &mut evaluated_checks,
        );
        assert!(evaluated_checks.is_empty(), "{evaluated_checks:?}");

        let mut unrelated_checks = Vec::new();
        check_forecast_evaluation(
            "What was the average value last month?",
            &[],
            &mut unrelated_checks,
        );
        assert!(unrelated_checks.is_empty());
    }

    #[test]
    fn rendering_a_precomputed_forecast_does_not_require_a_second_backtest() {
        let sql = "SELECT period, observed, forecast, lower, upper FROM forecast_values";
        let rows = vec![
            vec![
                Json::from("Jan"),
                Json::from(10),
                Json::Null,
                Json::Null,
                Json::Null,
            ],
            vec![
                Json::from("Feb"),
                Json::Null,
                Json::from(12),
                Json::from(11),
                Json::from(13),
            ],
        ];
        let mut source = run_sql_ev(
            sql,
            &["period", "observed", "forecast", "lower", "upper"],
            rows.clone(),
        );
        source.id = "evidence-source".into();
        source.tool = "run_sql".into();
        let chart_data = chart::from_table(
            &chart::TabularResult {
                columns: source.columns.clone().unwrap(),
                rows,
            },
            chart::ChartRequest {
                kind: chart::ChartKind::Forecast,
                x_field: Some("period".into()),
                series_fields: vec![
                    "observed".into(),
                    "forecast".into(),
                    "lower".into(),
                    "upper".into(),
                ],
                metadata: chart::ChartMetadata {
                    source_evidence_id: Some(source.id.clone()),
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .unwrap();
        let mut chart_item = run_sql_ev("", &[], Vec::new());
        chart_item.id = "evidence-chart".into();
        chart_item.tool = "make_chart".into();
        chart_item.chart = Some(chart_data);

        let mut checks = Vec::new();
        check_forecast_evaluation(
            "Render the supplied forecast as a chart; do not recompute it",
            &[source, chart_item],
            &mut checks,
        );

        assert!(
            checks.is_empty(),
            "rendering provided forecast rows should not trigger a second analysis: {checks:?}"
        );
    }

    #[test]
    fn replayable_python_backtest_is_an_allowed_custom_forecast_path() {
        let sql = "SELECT period, value FROM monthly_values ORDER BY period";
        let rows = vec![
            vec![Json::String("2025-01".into()), Json::from(10)],
            vec![Json::String("2025-02".into()), Json::from(12)],
        ];
        let mut python = run_sql_ev(sql, &["period", "value"], rows.clone());
        python.tool = "run_python".into();
        python.args = serde_json::json!({
            "code": "result = rolling_origin_backtest(values, 'linear_trend', baseline_method='naive')"
        });
        python.output = Some("{'mae': 2.0, 'baseline_mae': 3.0}".into());
        python.python_queries_complete = Some(true);
        python.python_queries = Some(vec![crate::engine::analytics::pyexec::PythonQueryTrace {
            sql: sql.into(),
            columns: vec!["period".into(), "value".into()],
            rows,
            row_count: 2,
            truncated: false,
        }]);

        let mut checks = Vec::new();
        check_forecast_evaluation(
            "Predict next month using a custom method",
            &[python],
            &mut checks,
        );
        assert!(checks.is_empty(), "{checks:?}");
    }

    #[test]
    fn a_typed_mean_forecast_satisfies_the_average_contract_without_sql_avg() {
        let mut forecast = run_sql_ev(
            "SELECT strftime('%Y-%m', day) AS period, SUM(rentals) AS value FROM daily GROUP BY period ORDER BY period",
            &["period", "value"],
            vec![vec![Json::String("2011-01".into()), Json::from(10)]],
        );
        forecast.tool = "forecast_analysis".into();
        forecast.args = serde_json::json!({"method":"mean", "horizon":1});

        let mut checks = Vec::new();
        check_measure_operation(
            &mut checks,
            "arithmetic mean",
            &[forecast.sql.clone().unwrap()],
            &[forecast.clone()],
        );
        assert_eq!(checks.len(), 1);
        assert!(checks[0].ok, "{checks:?}");

        let mut aggregate_checks = Vec::new();
        check_aggregate_verb(
            "What is the average forecast?",
            &[forecast],
            &mut aggregate_checks,
        );
        assert!(aggregate_checks.is_empty(), "{aggregate_checks:?}");
    }

    #[test]
    fn ordered_list_markers_are_not_treated_as_figures() {
        let got: Vec<_> = answer_number_tokens(
            "(1) A chart can make a pattern easier to understand.\n\
             2. A chart is unnecessary for one isolated value.\n\
             3) Keep the analysis local.",
        )
        .into_iter()
        .map(|(_, value)| value)
        .collect();
        assert!(
            got.is_empty(),
            "list markers leaked into answer figures: {got:?}"
        );

        let got_with_data: Vec<_> = answer_number_tokens("1. Total spending was $450.")
            .into_iter()
            .map(|(_, value)| value)
            .collect();
        assert_eq!(got_with_data, vec![450.0]);
    }

    #[test]
    fn iso_dates_are_not_treated_as_figures() {
        let got: Vec<_> = answer_number_tokens(
            "The missing reading was on 2024-01-02; the chart needs a valid value.",
        )
        .into_iter()
        .map(|(_, value)| value)
        .collect();
        assert!(
            got.is_empty(),
            "date components leaked into figures: {got:?}"
        );

        let got_with_data: Vec<_> = answer_number_tokens("On 2024-01, spending was $450.")
            .into_iter()
            .map(|(_, value)| value)
            .collect();
        assert_eq!(got_with_data, vec![450.0]);
    }

    #[test]
    fn close_matches() {
        assert!(close(450.0, 450.0));
        assert!(close(450.0, 450.4));
        assert!(close(1000.0, 1004.0)); // within 0.5%
        assert!(!close(450.0, 470.0));
    }

    #[test]
    fn answer_numbers_must_match_the_precision_the_answer_displays() {
        let evidence = vec![run_sql_ev(
            "SELECT 900000.4 AS total",
            &["total"],
            vec![vec![Json::from(900000.4)]],
        )];

        let mut rounded = Vec::new();
        check_numbers(
            "What is the total?",
            "The total is 900,000.",
            &evidence,
            None,
            &mut rounded,
        );
        assert!(rounded.iter().all(|check| check.ok), "{rounded:?}");

        let mut exact = Vec::new();
        check_numbers(
            "What is the total?",
            "The total is 900,000.4.",
            &evidence,
            None,
            &mut exact,
        );
        assert!(exact.iter().all(|check| check.ok), "{exact:?}");

        let mut incorrectly_rounded = Vec::new();
        check_numbers(
            "What is the total?",
            "The total is 900,001.",
            &evidence,
            None,
            &mut incorrectly_rounded,
        );
        assert!(
            incorrectly_rounded.iter().any(|check| !check.ok),
            "an incorrectly rounded figure must not be accepted: {incorrectly_rounded:?}"
        );
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
            warn(
                "a total here is computed over the text column `amount`",
                None,
            ),
            ok("every number in the answer came from the data above"),
        ];
        assert_eq!(hard_fail(&soft), None);

        // A replay mismatch excludes only its cited evidence and is actionable,
        // but it does not veto unrelated supported claims.
        let replay = vec![exclude_evidence_warning(
            "a query behind this answer gives a different result now",
            Some("SELECT sum(amount) FROM t".into()),
            VerificationFindingCode::ReplayMismatch,
            vec!["query-1".into()],
            "rerun only this query and revise dependent claims",
        )];
        assert_eq!(hard_fail(&replay), None);
        assert!(rerun_regression(&replay).is_some());
        let replay_finding = replay[0].finding.as_ref().unwrap();
        assert_eq!(replay_finding.effect, VerificationEffect::ExcludeEvidence);
        assert_eq!(replay_finding.evidence_ids, vec!["query-1"]);

        // An unsupported figure scopes repair to the claim and leaves source
        // results eligible for use; it is not an answer-wide veto.
        let unsupported = vec![repair_claim_warning(
            "the answer mentions 999 not found in any result",
            None,
            VerificationFindingCode::UnsupportedClaim,
            "999",
            vec!["query-1".into()],
            "revise or omit only this figure",
        )];
        assert_eq!(hard_fail(&unsupported), None);
        assert_eq!(rerun_regression(&unsupported), None);
        assert_eq!(
            unsupported[0].finding.as_ref().unwrap().target,
            VerificationTarget::Claim
        );

        // Only a finding explicitly authorized to block the complete answer
        // creates a hard failure (for example, a stale workspace snapshot).
        let stale = vec![warn_with_effect(
            "workspace changed while this answer was running",
            Some("revision changed".into()),
            VerificationFindingCode::WorkspaceStale,
            VerificationEffect::BlockAnswer,
            VerificationTarget::Answer,
            Some("turn-1".into()),
            vec!["query-1".into()],
            Some("rerun against the current revision".into()),
        )];
        assert!(hard_fail(&stale).is_some());

        // A same-model disagreement is a quality warning, not proof that the
        // answer is wrong; replay mismatches remain scoped to their evidence.
        let disagreed = vec![self_consistency_check("Total: $450", "Total: $600").unwrap()];
        assert_eq!(hard_fail(&disagreed), None);
        assert_eq!(
            status(
                &disagreed,
                &[run_sql_ev(
                    "SELECT 450",
                    &["total"],
                    vec![vec![Json::from(450)]]
                )],
                true
            ),
            VerificationStatus::NeedsReview
        );
    }

    #[test]
    fn unbacked_figures_request_a_tool_backed_repair_when_only_groups_exist() {
        let evidence = vec![run_sql_ev(
            "SELECT area, SUM(amount) AS total FROM transactions GROUP BY area",
            &["area", "total"],
            vec![
                vec![Json::from("north"), Json::from(100)],
                vec![Json::from("south"), Json::from(200)],
            ],
        )];
        let checks = vec![repair_claim_warning(
            "the answer mentions 93914.13 not found in any result",
            Some("check these against the evidence below".into()),
            VerificationFindingCode::UnsupportedClaim,
            "93914.13",
            vec![evidence[0].id.clone()],
            "recompute or retract this figure",
        )];
        let hint = semantic_repair_hint(&checks);
        assert!(
            hint.is_some(),
            "an unbacked number may need a derived computation or retraction"
        );
    }

    #[test]
    fn case_sensitive_label_warning_requests_a_tool_backed_repair() {
        let evidence = vec![run_sql_ev(
            "SELECT SUM(amount) FROM spending WHERE category IN ('Rent', 'housing')",
            &["sum"],
            vec![vec![Json::from(100)]],
        )];
        let checks = vec![exclude_evidence_warning(
            "a filter on `category` matches exact case",
            Some("values differ only in capitalisation".into()),
            VerificationFindingCode::SemanticExecutionMismatch,
            vec![evidence[0].id.clone()],
            "revise only this filter",
        )];

        assert!(
            semantic_repair_hint(&checks).is_some_and(|hint| hint.contains("matches exact case"))
        );
    }

    #[test]
    fn unbacked_claims_request_repair_without_changing_the_requested_shape() {
        let evidence = vec![run_sql_ev(
            "SELECT area, SUM(amount) AS total FROM transactions GROUP BY area",
            &["area", "total"],
            vec![
                vec![Json::from("north"), Json::from(100)],
                vec![Json::from("south"), Json::from(200)],
            ],
        )];
        let checks = vec![repair_claim_warning(
            "the answer mentions 93914.13 not found in any result",
            None,
            VerificationFindingCode::UnsupportedClaim,
            "93914.13",
            vec![evidence[0].id.clone()],
            "revise or omit this figure",
        )];
        assert!(semantic_repair_hint(&checks).is_some());
    }

    #[test]
    fn an_unbacked_figure_is_rechecked_even_when_a_scalar_aggregate_exists() {
        let evidence = vec![run_sql_ev(
            "SELECT SUM(amount) AS total FROM transactions",
            &["total"],
            vec![vec![Json::from(300)]],
        )];
        let checks = vec![repair_claim_warning(
            "the answer mentions 93914.13 not found in any result",
            None,
            VerificationFindingCode::UnsupportedClaim,
            "93914.13",
            vec![evidence[0].id.clone()],
            "revise or omit this figure",
        )];
        assert!(semantic_repair_hint(&checks).is_some());
    }

    #[test]
    fn pie_evidence_backs_plotted_values_and_its_deterministic_composition() {
        let mut pie = run_sql_ev("SELECT category, total FROM totals", &[], Vec::new());
        pie.tool = "make_chart".into();
        pie.columns = None;
        pie.rows = None;
        pie.row_count = None;
        pie.chart = Some(chart::ChartData {
            kind: chart::ChartKind::Pie,
            title: Some("Spending by category".into()),
            labels: vec![
                "Rent".into(),
                "Groceries".into(),
                "Dining".into(),
                "Transport".into(),
            ],
            series: vec![chart::Series {
                name: "spending".into(),
                values: vec![Some(3600.0), Some(350.0), Some(180.0), Some(60.0)],
            }],
            unit: Some("$".into()),
            x_label: None,
            y_label: None,
            payload: None,
            metadata: Some(chart::ChartMetadata {
                denominator: Some("all included category spending".into()),
                part_to_whole: true,
                ..Default::default()
            }),
        });

        let answer = "Rent was $3,600, or 85.9% of $4,190 total; the slices sum to 100%.";
        let mut pie_checks = Vec::new();
        check_numbers(
            "Show spending by category as a pie chart",
            answer,
            &[pie.clone()],
            None,
            &mut pie_checks,
        );
        assert!(pie_checks.iter().all(|check| check.ok), "{pie_checks:?}");

        let mut bar = pie.clone();
        let chart = bar.chart.as_mut().unwrap();
        chart.kind = chart::ChartKind::Bar;
        chart.metadata.as_mut().unwrap().part_to_whole = false;
        let mut bar_checks = Vec::new();
        check_numbers(
            "Show spending by category as a bar chart",
            answer,
            &[bar],
            None,
            &mut bar_checks,
        );
        assert!(bar_checks
            .iter()
            .any(|check| !check.ok && check.label.contains("not found in any result")));

        pie.error = Some("superseded chart evidence".into());
        let mut failed_checks = Vec::new();
        check_numbers(
            "Show spending by category as a pie chart",
            answer,
            &[pie],
            None,
            &mut failed_checks,
        );
        assert!(failed_checks
            .iter()
            .any(|check| !check.ok && check.label.contains("not found in any result")));
    }

    #[test]
    fn a_fully_backed_scalar_answer_does_not_trigger_a_duplicate_repair() {
        let checks = vec![ok("every number in the answer came from the data above")];
        assert!(semantic_repair_hint(&checks).is_none());
    }

    #[test]
    fn status_uses_one_typed_answer_classification() {
        let evidence = vec![run_sql_ev(
            "SELECT 1 AS total",
            &["total"],
            vec![vec![Json::from(1)]],
        )];
        let clean = vec![
            successful_replay_check("re-checked the queries behind this answer  same results"),
            ok("every number in the answer came from the data above"),
        ];
        assert_eq!(
            status(&clean, &evidence, true),
            VerificationStatus::Verified
        );
        assert_eq!(
            status(&clean, &evidence, false),
            VerificationStatus::NeedsReview,
            "a matching replay alone does not verify the user's semantic interpretation"
        );
        assert_eq!(
            status(
                &[warn("a total here is computed over a text column", None)],
                &evidence,
                true
            ),
            VerificationStatus::NeedsReview
        );
        assert_eq!(
            status(
                &[repair_claim_warning(
                    "the answer mentions 999 not found in any result",
                    None,
                    VerificationFindingCode::UnsupportedClaim,
                    "999",
                    vec![evidence[0].id.clone()],
                    "revise or omit this unsupported figure",
                )],
                &evidence,
                true
            ),
            VerificationStatus::NeedsReview
        );
        assert_eq!(
            status(&[], &[], false),
            VerificationStatus::InsufficientData
        );

        let mut failed_tool = evidence[0].clone();
        failed_tool.error = Some("query failed".into());
        assert_eq!(
            status(&[], &[failed_tool], false),
            VerificationStatus::InsufficientData
        );
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
        assert!(check.label.contains("disagrees with this answer"));
        assert!(check.detail.unwrap().contains("$600"));
        // A year in one and not the other doesn't count as a figure mismatch.
        assert!(self_consistency_check("In 2024, you spent $450.", "$450").is_none());
        // Neither answer has a comparable figure (e.g. both prose) -> nothing to compare.
        assert!(self_consistency_check("The files can't answer this.", "I'm not sure.").is_none());
        // A checker that returns no usable response is not treated as a
        // disagreement and cannot downgrade the original answer.
        assert!(self_consistency_check("Total: $450", "").is_none());
    }

    #[test]
    fn background_line_numbers_are_not_flagged() {
        let mut evidence = run_sql_ev(
            "SELECT 450 AS total",
            &["total"],
            vec![vec![Json::from(450)]],
        );
        evidence.result_summary = "1 row in 50ms".into();
        let ev = vec![evidence];

        let answer = "Background: RPE is a 1-10 scale.\nYour total was 450, peaking at 999.";
        let mut out = Vec::new();
        check_numbers("What is my total?", answer, &ev, None, &mut out);

        let warns: Vec<_> = out.iter().filter(|c| !c.ok).collect();
        assert_eq!(
            warns.len(),
            1,
            "only the body's stray 999 should warn: {out:?}"
        );
        assert!(warns[0].label.contains("999"), "{}", warns[0].label);
    }

    #[test]
    fn sql_runtime_metadata_cannot_back_a_data_claim() {
        let mut evidence = run_sql_ev(
            "SELECT period, total FROM periods",
            &["period", "total"],
            vec![
                vec![Json::from("first"), Json::from(200)],
                vec![Json::from("second"), Json::from(250)],
            ],
        );
        evidence.result_summary = "2 rows in 50ms".into();

        let mut checks = Vec::new();
        check_numbers(
            "What is the difference between these totals?",
            "The difference was $50; totals were $200 and $250.",
            &[evidence],
            None,
            &mut checks,
        );

        assert_eq!(
            checks.len(),
            1,
            "only latency-matching $50 should be unsupported: {checks:?}"
        );
        assert!(!checks[0].ok);
        assert!(checks[0].label.contains("50"));
    }

    #[test]
    fn failed_or_superseded_query_rows_cannot_back_answer_numbers() {
        let mut evidence = run_sql_ev(
            "SELECT 450 AS total",
            &["total"],
            vec![vec![Json::from(450)]],
        );
        evidence.error = Some("superseded after semantic verification".into());

        let mut checks = Vec::new();
        check_numbers(
            "What is the total?",
            "The total is 450.",
            &[evidence],
            None,
            &mut checks,
        );

        assert!(checks
            .iter()
            .any(|check| { !check.ok && check.label.contains("450 not found in any result") }));
    }

    #[test]
    fn declared_user_scenario_input_is_distinct_from_a_computed_result() {
        let evidence = vec![run_sql_ev(
            "SELECT 100 AS observed_total, 90 AS scenario_total",
            &["observed_total", "scenario_total"],
            vec![vec![Json::from(100), Json::from(90)]],
        )];
        let contract = AnalysisContract {
            assumptions: vec!["Apply the user's 10% reduction assumption".into()],
            ..Default::default()
        };
        let mut checks = Vec::new();
        check_numbers(
            "What if the total were 10% lower?",
            "Applying the user-specified 10% reduction gives a scenario total of 90.",
            &evidence,
            Some(&contract),
            &mut checks,
        );

        assert!(checks.iter().all(|check| check.ok), "{checks:?}");
        assert!(checks.iter().any(|check| {
            check.label.contains("user-supplied assumption values")
                && check
                    .detail
                    .as_deref()
                    .is_some_and(|detail| detail.contains("10%"))
        }));

        let mut unsupported_result = Vec::new();
        check_numbers(
            "What if the total were 10% lower?",
            "Applying the user-specified 10% reduction gives a scenario total of 80.",
            &evidence,
            Some(&contract),
            &mut unsupported_result,
        );
        assert!(unsupported_result
            .iter()
            .any(|check| !check.ok && check.label.contains("80")));
    }

    #[test]
    fn an_empty_aggregate_backs_the_answer_zero() {
        let ev = vec![EvidenceItem {
            id: "evidence-test".into(),
            tool: "run_sql".into(),
            sources: Vec::new(),
            args: Json::Object(Default::default()),
            note: None,
            sql: Some("SELECT SUM(amount) FROM t WHERE category = 'healthcare'".into()),
            result_summary: "1 row".into(),
            columns: Some(vec!["SUM(amount)".into()]),
            rows: Some(vec![vec![Json::Null]]),
            row_count: Some(1),
            output: None,
            chart: None,
            result_table: None,
            python_input_trace: None,
            python_queries: None,
            python_queries_complete: None,
            ms: 1,
            error: None,
            verifier_disposition: None,
        }];
        let mut out = Vec::new();
        check_numbers(
            "What did I spend on healthcare?",
            "You spent $0 on healthcare.",
            &ev,
            None,
            &mut out,
        );
        assert!(
            out.iter().all(|c| c.ok),
            "0 is backed by the empty aggregate, not a stray figure: {out:?}"
        );
    }

    #[test]
    fn no_evidence_at_all_is_not_flagged() {
        // A general-knowledge / conversational answer, no tool ran. Its
        // numbers aren't a data claim, so there's nothing to check.
        let mut out = Vec::new();
        check_numbers(
            "",
            "A common rule of thumb is saving 20% of income.",
            &[],
            None,
            &mut out,
        );
        assert!(
            out.is_empty(),
            "no evidence means no check, not a warning: {out:?}"
        );
    }

    #[test]
    fn list_files_row_counts_in_output_back_the_answer() {
        // list_files (and inspect_table) report per-table row counts only in
        // their detail text, stored in `output` -- a summary answer quoting
        // one of those counts is backed, not a stray figure.
        let ev = vec![EvidenceItem {
            id: "evidence-test".into(),
            tool: "list_files".into(),
            sources: Vec::new(),
            args: Json::Object(Default::default()),
            note: None,
            sql: None,
            result_summary: "2 files".into(),
            columns: None,
            rows: None,
            row_count: None,
            output: Some(
                "table ledger  (from ledger.csv, 30 rows)\ndocument notes.md  (Notes, 1 KB)".into(),
            ),
            chart: None,
            result_table: None,
            python_input_trace: None,
            python_queries: None,
            python_queries_complete: None,
            ms: 1,
            error: None,
            verifier_disposition: None,
        }];
        let mut out = Vec::new();
        check_numbers(
            "Which tables and files are present?",
            "This folder has a ledger table with 30 rows and one notes file.",
            &ev,
            None,
            &mut out,
        );
        assert!(
            out.iter().all(|c| c.ok),
            "30 came from list_files' own listing, not a stray figure: {out:?}"
        );
    }

    fn run_sql_ev(sql: &str, columns: &[&str], rows: Vec<Vec<Json>>) -> EvidenceItem {
        EvidenceItem {
            id: "evidence-test".into(),
            tool: "run_sql".into(),
            sources: Vec::new(),
            args: Json::Object(Default::default()),
            note: None,
            sql: Some(sql.to_string()),
            result_summary: format!("{} row(s)", rows.len()),
            columns: Some(columns.iter().map(|s| s.to_string()).collect()),
            row_count: Some(rows.len()),
            rows: Some(rows),
            output: None,
            chart: None,
            result_table: None,
            python_input_trace: None,
            python_queries: None,
            python_queries_complete: None,
            ms: 1,
            error: None,
            verifier_disposition: None,
        }
    }

    #[test]
    fn spots_a_date_group_by_that_collapsed_to_null() {
        assert!(groups_by_date_expr(
            "SELECT strftime('%Y-%m', Date) AS month, SUM(x) AS t FROM t \
             GROUP BY strftime('%Y-%m', Date) ORDER BY month"
        ));
        assert!(groups_by_date_expr(
            "SELECT date(d) AS day, count(*) FROM t GROUP BY date(d)"
        ));
        assert!(!groups_by_date_expr(
            "SELECT category, SUM(x) FROM t GROUP BY category"
        ));
        assert!(!groups_by_date_expr("SELECT SUM(x) FROM t")); // no GROUP BY at all

        assert!(has_null_group_key(&[vec![Json::Null, Json::from(15797)]]));
        assert!(!has_null_group_key(&[vec![
            Json::from("2025-09"),
            Json::from(1316)
        ]]));
    }

    #[test]
    fn check_null_group_key_flags_the_real_reported_bug() {
        // The exact reported failure: a rent ledger with non-ISO dates made
        // every strftime() call return NULL, collapsing 4 months into one
        // blank bucket holding the grand total.
        let ev = vec![run_sql_ev(
            r#"SELECT strftime('%Y-%m', Date) AS month, SUM(parse_num("Charges / Payments")) \
               AS rent_total FROM full_ledger WHERE Type = 'Charge - Rent' \
               GROUP BY strftime('%Y-%m', Date) ORDER BY month"#,
            &["month", "rent_total"],
            vec![vec![Json::Null, Json::from(15797)]],
        )];
        let mut out = Vec::new();
        check_null_group_key(&ev, &mut out);
        assert_eq!(out.len(), 1, "{out:?}");
        assert!(!out[0].ok);
        let finding = out[0].finding.as_ref().unwrap();
        assert_eq!(finding.effect, VerificationEffect::ExcludeEvidence);
        assert_eq!(finding.evidence_ids, vec![ev[0].id.clone()]);
        assert!(hard_fail(&out).is_none());
    }

    #[test]
    fn check_null_group_key_leaves_a_healthy_breakdown_alone() {
        // A real multi-month breakdown -- every key is a real month.
        let ev = vec![run_sql_ev(
            "SELECT strftime('%Y-%m', d) AS month, SUM(x) AS t FROM t \
             GROUP BY strftime('%Y-%m', d) ORDER BY month",
            &["month", "t"],
            vec![
                vec![Json::from("2025-09"), Json::from(1316)],
                vec![Json::from("2025-10"), Json::from(1321)],
            ],
        )];
        let mut out = Vec::new();
        check_null_group_key(&ev, &mut out);
        assert!(out.is_empty(), "{out:?}");
    }

    #[test]
    fn check_null_group_key_leaves_a_genuine_single_bucket_alone() {
        // A dataset that legitimately spans one real month: one row, but a
        // real, non-null key -- not the bug this check is for.
        let ev = vec![run_sql_ev(
            "SELECT strftime('%Y-%m', d) AS month, SUM(x) AS t FROM t \
             GROUP BY strftime('%Y-%m', d)",
            &["month", "t"],
            vec![vec![Json::from("2025-09"), Json::from(1316)]],
        )];
        let mut out = Vec::new();
        check_null_group_key(&ev, &mut out);
        assert!(
            out.is_empty(),
            "a real single-month result is not a bug: {out:?}"
        );
    }

    #[test]
    fn check_null_group_key_ignores_non_date_grouping() {
        // A NULL key from grouping by an ordinary column (e.g. an
        // unlabelled category) isn't what this check is for -- narrow to
        // date/time expressions only, to avoid false-positiving on
        // legitimately-NULL categories.
        let ev = vec![run_sql_ev(
            "SELECT category, SUM(x) AS t FROM t GROUP BY category",
            &["category", "t"],
            vec![vec![Json::Null, Json::from(100)]],
        )];
        let mut out = Vec::new();
        check_null_group_key(&ev, &mut out);
        assert!(out.is_empty(), "{out:?}");
    }

    /// The real shape from a live `fqah-goal-ontrack` run: five unrelated
    /// aggregates packed into one row by a subquery-per-column `SELECT`.
    fn goal_ontrack_ev() -> Vec<EvidenceItem> {
        vec![run_sql_ev(
            "SELECT (SELECT count(*) FROM books WHERE finished = 'Yes') AS books_count, \
             (SELECT sum(distance_km) FROM workouts WHERE activity = 'Running') AS run_km, \
             (SELECT count(*) FROM trips WHERE purpose = 'leisure') AS leisure_trips, \
             (SELECT avg(hours) FROM sleep) AS avg_sleep, \
             (SELECT max(amount) FROM spend WHERE category = 'dining') AS max_dining",
            &[
                "books_count",
                "run_km",
                "leisure_trips",
                "avg_sleep",
                "max_dining",
            ],
            vec![vec![
                Json::from(0),
                Json::from(4.0),
                Json::from(2),
                Json::from(6.76),
                Json::from(202.78),
            ]],
        )]
    }

    #[test]
    fn check_row_value_labels_leaves_correctly_labeled_prose_alone() {
        let ev = goal_ontrack_ev();
        let answer = "- Reading: 0 books finished\n\
             - Running: 4 km\n\
             - Travel: 2 leisure trips\n\
             - Sleep: 6.76 hours average\n\
             - Dining out: $202.78 max in a month";
        let mut out = Vec::new();
        check_row_value_labels(answer, &ev, &mut out);
        assert!(
            out.is_empty(),
            "every value sits next to its own column's words: {out:?}"
        );
    }

    #[test]
    fn check_row_value_labels_ignores_words_shared_by_columns() {
        let ev = vec![run_sql_ev(
            "SELECT 100 AS observed_2024_units, 0.9 AS retained_rate, 90 AS scenario_2024_units",
            &[
                "observed_2024_units",
                "retained_rate",
                "scenario_2024_units",
            ],
            vec![vec![Json::from(100), Json::from(0.9), Json::from(90)]],
        )];
        let answer = "The adjusted total is 100 x 0.9 = 90 units.";
        let mut out = Vec::new();
        check_row_value_labels(answer, &ev, &mut out);
        assert!(
            out.is_empty(),
            "shared unit/year words are not enough to identify a mislabeled result: {out:?}"
        );
    }

    #[test]
    fn check_row_value_labels_does_not_assign_a_formula_input_to_its_result_label() {
        let ev = vec![run_sql_ev(
            "SELECT 1200 AS period_total, 200 AS forecast",
            &["period_total", "forecast"],
            vec![vec![Json::from(1200), Json::from(200)]],
        )];
        let answer = "Forecast: 1,200 ÷ 6 = 200.";
        let mut out = Vec::new();
        check_row_value_labels(answer, &ev, &mut out);
        assert!(
            out.is_empty(),
            "the expression input is not the forecast column: {out:?}"
        );
    }

    #[test]
    fn check_row_value_labels_does_not_assign_a_summary_size_mention_to_a_total() {
        let ev = vec![run_sql_ev(
            "SELECT 524652 AS total_value, 6 AS months_used, 87442 AS average_value",
            &["total_value", "months_used", "average_value"],
            vec![vec![Json::from(524652), Json::from(6), Json::from(87442)]],
        )];
        let answer = "The total was 524,652 rides across 6 months; the average was 87,442.";
        let mut out = Vec::new();
        check_row_value_labels(answer, &ev, &mut out);
        assert!(
            out.is_empty(),
            "the later sample-size phrase must not relabel the total: {out:?}"
        );
    }

    #[test]
    fn check_row_value_labels_keeps_scenario_output_distinct_from_its_base_total() {
        let ev = vec![run_sql_ev(
            "SELECT 2049576 AS recorded_total, 1844618.4 AS scenario_total",
            &["recorded_total", "scenario_total"],
            vec![vec![Json::from(2049576), Json::from(1844618.4)]],
        )];
        let answer = "Scenario result: 1,844,618.4 rentals (approximately 1,844,618 rentals).\n\
             The checked calculation uses the 2012 recorded total of 2,049,576 rentals and applies the requested 10% reduction.";
        let mut out = Vec::new();
        check_row_value_labels(answer, &ev, &mut out);
        assert!(
            out.is_empty(),
            "the scenario figure and recorded base must retain their own labels: {out:?}"
        );
    }

    #[test]
    fn check_row_value_labels_does_not_confuse_a_scenario_total_with_its_recorded_input() {
        let ev = vec![run_sql_ev(
            "SELECT 2049576 AS recorded_2012_rentals, 1844618.4 AS scenario_total_10_percent_lower",
            &["recorded_2012_rentals", "scenario_total_10_percent_lower"],
            vec![vec![Json::from(2049576), Json::from(1844618.4)]],
        )];
        let answer = "**Scenario result:** **1,844,618.4 rentals** (approximately **1,844,618 rentals**).\n\
             The checked calculation uses the 2012 recorded total of **2,049,576 rentals** and applies the requested 10% reduction.";
        let mut out = Vec::new();
        check_row_value_labels(answer, &ev, &mut out);
        assert!(
            out.is_empty(),
            "the scenario estimate should not inherit the label of its separate base input: {out:?}"
        );
    }

    #[test]
    fn check_row_value_labels_does_not_use_a_generic_total_word_as_column_identity() {
        let ev = vec![run_sql_ev(
            "SELECT 1000 AS recorded_total_units, 900 AS units_after_adjustment",
            &["recorded_total_units", "units_after_adjustment"],
            vec![vec![Json::from(1000), Json::from(900)]],
        )];
        let answer = "The annual total would be 900 units.";
        let mut out = Vec::new();
        check_row_value_labels(answer, &ev, &mut out);
        assert!(
            out.is_empty(),
            "a generic aggregation word alone cannot identify the recorded column: {out:?}"
        );
    }

    #[test]
    fn check_row_value_labels_does_not_treat_a_year_as_a_column_label() {
        let ev = vec![run_sql_ev(
            "SELECT 5000 AS recorded_2025_units, 4500 AS units_after_reduction",
            &["recorded_2025_units", "units_after_reduction"],
            vec![vec![Json::from(5000), Json::from(4500)]],
        )];
        let answer = "Scenario result: 4,500 units for 2025.";
        let mut out = Vec::new();
        check_row_value_labels(answer, &ev, &mut out);
        assert!(
            out.is_empty(),
            "a date label and shared unit word should not imply a column swap: {out:?}"
        );
    }

    #[test]
    fn check_row_value_labels_does_not_let_a_year_steal_the_result_label() {
        let evidence = vec![run_sql_ev(
            "SELECT 1000 AS recorded_2025_widgets, 900 AS scenario_annual_total",
            &["recorded_2025_widgets", "scenario_annual_total"],
            vec![vec![Json::from(1000), Json::from(900)]],
        )];
        let answer = "The annual total for 2025: 900 widgets.";
        let mut checks = Vec::new();
        check_row_value_labels(answer, &evidence, &mut checks);
        assert!(
            checks.is_empty(),
            "the year should not make the unit look like a column label: {checks:?}"
        );
    }

    #[test]
    fn check_row_value_labels_flags_the_real_observed_swap() {
        let ev = goal_ontrack_ev();
        // The exact live failure: 202.78 (max_dining) called "Sleep".
        let answer = "You are currently on track to meet your Sleep goal, with an average of \
202.78 hours (which is significantly above the 7.5 hours per night target).";
        let mut out = Vec::new();
        check_row_value_labels(answer, &ev, &mut out);
        assert_eq!(out.len(), 1, "{out:?}");
        assert!(!out[0].ok);
        assert!(out[0].label.contains("max_dining"), "{}", out[0].label);
        assert_eq!(
            out[0].finding.as_ref().unwrap().effect,
            VerificationEffect::Repair
        );
        assert!(hard_fail(&out).is_none());
    }

    #[test]
    fn check_row_value_labels_ignores_a_normal_multi_row_breakdown() {
        // Each row already carries its own label column -- not the risky
        // shape this check targets, even though it also has 2+ columns.
        let ev = vec![run_sql_ev(
            "SELECT category, SUM(amount) AS total FROM spend GROUP BY category",
            &["category", "total"],
            vec![
                vec![Json::from("rent"), Json::from(15013.23)],
                vec![Json::from("dining"), Json::from(2233.69)],
            ],
        )];
        let answer = "Rent was 15013.23 and dining was 2233.69.";
        let mut out = Vec::new();
        check_row_value_labels(answer, &ev, &mut out);
        assert!(
            out.is_empty(),
            "multi-row results are out of scope: {out:?}"
        );
    }

    #[test]
    fn check_row_value_labels_skips_a_value_shared_by_two_columns() {
        let ev = vec![run_sql_ev(
            "SELECT (SELECT count(*) FROM a) AS a_count, (SELECT count(*) FROM b) AS b_count",
            &["a_count", "b_count"],
            vec![vec![Json::from(5), Json::from(5)]],
        )];
        let answer = "Both a and b came to 5.";
        let mut out = Vec::new();
        check_row_value_labels(answer, &ev, &mut out);
        assert!(
            out.is_empty(),
            "an ambiguous value is left alone, not guessed at: {out:?}"
        );
    }

    #[test]
    fn contract_checks_require_grounded_bindings_to_reach_sql() {
        let contract = AnalysisContract {
            interpretation: InterpretationStatus::Grounded,
            subject: Some("spend".into()),
            measures: vec![crate::engine::runtime::ContractMeasure {
                concept: "amount".into(),
                field: Some("amount".into()),
                operation: "sum".into(),
                unit: None,
            }],
            filters: vec![crate::engine::runtime::ContractFilter {
                concept: "category".into(),
                field: Some("category".into()),
                exclude: false,
                candidate_values: vec!["dining".into()],
                resolved_values: vec!["dining".into()],
                resolution: Some("observed".into()),
            }],
            group_by: vec!["category".into()],
            ..Default::default()
        };
        let evidence = vec![run_sql_ev(
            "SELECT category, SUM(amount) AS total FROM spend WHERE category = 'dining' GROUP BY category",
            &["category", "total"],
            vec![vec![Json::from("dining"), Json::from(12)]],
        )];
        let report = GroundingReport {
            source: Some("spend".into()),
            sources: vec![],
            probes: vec![],
            unresolved: vec![],
        };
        let checks = contract_checks(&contract, Some(&report), &evidence);
        assert!(checks.iter().all(|check| check.ok), "{checks:?}");
    }

    #[test]
    fn contract_checks_require_the_requested_time_bucket() {
        let contract = AnalysisContract {
            interpretation: InterpretationStatus::Grounded,
            subject: Some("spend".into()),
            measures: vec![crate::engine::runtime::ContractMeasure {
                concept: "amount".into(),
                field: Some("amount".into()),
                operation: "sum".into(),
                unit: None,
            }],
            time: Some(crate::engine::runtime::ContractTime {
                field: Some("date".into()),
                range: None,
                bucket: Some(TimeBucket::Month),
                timezone: None,
            }),
            ..Default::default()
        };
        let report = GroundingReport {
            source: Some("spend".into()),
            sources: vec![],
            probes: vec![],
            unresolved: vec![],
        };
        let good = vec![run_sql_ev(
            "SELECT strftime('%Y-%m', date), SUM(amount) FROM spend GROUP BY strftime('%Y-%m', date)",
            &["month", "total"],
            vec![vec![Json::from("2024-01"), Json::from(12)]],
        )];
        let good_checks = contract_checks(&contract, Some(&report), &good);
        assert!(good_checks.iter().all(|check| check.ok), "{good_checks:?}");

        let bad = vec![run_sql_ev(
            "SELECT date, SUM(amount) FROM spend GROUP BY date",
            &["date", "total"],
            vec![vec![Json::from("2024-01-01"), Json::from(12)]],
        )];
        let bad_checks = contract_checks(&contract, Some(&report), &bad);
        assert!(bad_checks.iter().any(|check| {
            !check.ok && check.label.contains("time bucket `month` was not used")
        }));
    }

    #[test]
    fn contract_checks_require_both_typed_comparison_windows() {
        let contract = AnalysisContract {
            interpretation: InterpretationStatus::Grounded,
            subject: Some("spend".into()),
            measures: vec![crate::engine::runtime::ContractMeasure {
                concept: "revenue".into(),
                field: Some("amount".into()),
                operation: "sum".into(),
                unit: None,
            }],
            time: Some(crate::engine::runtime::ContractTime {
                field: Some("date".into()),
                range: None,
                bucket: None,
                timezone: None,
            }),
            comparison_spec: Some(crate::engine::runtime::ContractComparison {
                kind: crate::engine::runtime::ComparisonKind::PeriodOverPeriod,
                current_range: "2024".into(),
                previous_range: "2023".into(),
            }),
            ..Default::default()
        };
        let report = GroundingReport {
            source: Some("spend".into()),
            sources: vec![],
            probes: vec![],
            unresolved: vec![],
        };
        let good = vec![run_sql_ev(
            "SELECT SUM(CASE WHEN \"date\" >= '2024-01-01' AND \"date\" < '2025-01-01' THEN \"amount\" END) AS measure_0_current, SUM(CASE WHEN \"date\" >= '2023-01-01' AND \"date\" < '2024-01-01' THEN \"amount\" END) AS measure_0_previous, ((SUM(CASE WHEN \"date\" >= '2024-01-01' AND \"date\" < '2025-01-01' THEN \"amount\" END)) - (SUM(CASE WHEN \"date\" >= '2023-01-01' AND \"date\" < '2024-01-01' THEN \"amount\" END))) AS measure_0_change, (((SUM(CASE WHEN \"date\" >= '2024-01-01' AND \"date\" < '2025-01-01' THEN \"amount\" END)) - (SUM(CASE WHEN \"date\" >= '2023-01-01' AND \"date\" < '2024-01-01' THEN \"amount\" END))) / NULLIF(ABS(SUM(CASE WHEN \"date\" >= '2023-01-01' AND \"date\" < '2024-01-01' THEN \"amount\" END)), 0)) * 100 AS measure_0_change_pct FROM spend",
            &[
                "measure_0_current",
                "measure_0_previous",
                "measure_0_change",
                "measure_0_change_pct",
            ],
            vec![vec![
                Json::from(12),
                Json::from(10),
                Json::from(2),
                Json::from(20.0),
            ]],
        )];
        let checks = contract_checks(&contract, Some(&report), &good);
        assert!(checks.iter().all(|check| check.ok), "{checks:?}");

        let arithmetic_bad = vec![run_sql_ev(
            "SELECT 12 AS measure_0_current, 10 AS measure_0_previous, 5 AS measure_0_change, 30 AS measure_0_change_pct FROM spend",
            &[
                "measure_0_current",
                "measure_0_previous",
                "measure_0_change",
                "measure_0_change_pct",
            ],
            vec![vec![
                Json::from(12),
                Json::from(10),
                Json::from(5),
                Json::from(30),
            ]],
        )];
        let arithmetic_checks = contract_checks(&contract, Some(&report), &arithmetic_bad);
        assert!(arithmetic_checks
            .iter()
            .any(|check| { !check.ok && check.label.contains("period comparison arithmetic") }));
        assert!(hard_fail(&arithmetic_checks).is_none());
        assert!(arithmetic_checks.iter().any(|check| {
            !check.ok
                && check.finding.as_ref().is_some_and(|finding| {
                    finding.effect == VerificationEffect::Repair
                        && finding.target == VerificationTarget::Claim
                })
        }));

        let bad = vec![run_sql_ev(
            "SELECT SUM(amount) AS measure_0_current FROM spend",
            &["measure_0_current"],
            vec![vec![Json::from(12)]],
        )];
        let bad_checks = contract_checks(&contract, Some(&report), &bad);
        assert!(bad_checks.iter().any(|check| {
            !check.ok
                && check
                    .label
                    .contains("requested period comparison was not used")
        }));
    }

    #[test]
    fn contract_checks_reconcile_period_comparison_derived_metrics() {
        let contract = AnalysisContract {
            interpretation: InterpretationStatus::Grounded,
            subject: Some("spend".into()),
            measures: vec![
                crate::engine::runtime::ContractMeasure {
                    concept: "revenue".into(),
                    field: Some("amount".into()),
                    operation: "sum".into(),
                    unit: None,
                },
                crate::engine::runtime::ContractMeasure {
                    concept: "orders".into(),
                    field: None,
                    operation: "count".into(),
                    unit: None,
                },
            ],
            derived_metrics: vec![ContractDerivedMetric {
                concept: "revenue per order".into(),
                kind: crate::engine::runtime::DerivedMetricKind::Ratio,
                numerator: "revenue".into(),
                denominator: "orders".into(),
                unit: None,
            }],
            time: Some(crate::engine::runtime::ContractTime {
                field: Some("date".into()),
                range: None,
                bucket: Some(TimeBucket::Month),
                timezone: None,
            }),
            comparison_spec: Some(crate::engine::runtime::ContractComparison {
                kind: crate::engine::runtime::ComparisonKind::PeriodOverPeriod,
                current_range: "2024".into(),
                previous_range: "2023".into(),
            }),
            ..Default::default()
        };
        let report = GroundingReport {
            source: Some("spend".into()),
            sources: vec![],
            probes: vec![],
            unresolved: vec![],
        };
        let evidence = vec![run_sql_ev(
            "select \
                strftime('%m', \"date\") as time_month, \
                sum(case when \"date\" >= '2024-01-01' and \"date\" < '2025-01-01' then \"amount\" end) as measure_0_current, \
                sum(case when \"date\" >= '2023-01-01' and \"date\" < '2024-01-01' then \"amount\" end) as measure_0_previous, \
                20 as measure_0_change, 25 as measure_0_change_pct, \
                sum(case when \"date\" >= '2024-01-01' and \"date\" < '2025-01-01' then 1 else 0 end) as measure_1_current, \
                sum(case when \"date\" >= '2023-01-01' and \"date\" < '2024-01-01' then 1 else 0 end) as measure_1_previous, \
                2 as measure_1_change, 25 as measure_1_change_pct, \
                1 / nullif(1, 0) as derived_0_current, \
                1 / nullif(1, 0) as derived_0_previous, \
                0 as derived_0_change, 0 as derived_0_change_pct \
             from spend group by strftime('%m', \"date\")",
            &[
                "time_month",
                "measure_0_current",
                "measure_0_previous",
                "measure_0_change",
                "measure_0_change_pct",
                "measure_1_current",
                "measure_1_previous",
                "measure_1_change",
                "measure_1_change_pct",
                "derived_0_current",
                "derived_0_previous",
                "derived_0_change",
                "derived_0_change_pct",
            ],
            vec![vec![
                Json::from("01"),
                Json::from(100),
                Json::from(80),
                Json::from(20),
                Json::from(25),
                Json::from(10),
                Json::from(8),
                Json::from(2),
                Json::from(25),
                Json::from(10),
                Json::from(10),
                Json::from(0),
                Json::from(0),
            ]],
        )];
        let checks = contract_checks(&contract, Some(&report), &evidence);
        assert!(checks.iter().all(|check| check.ok), "{checks:?}");
    }

    #[test]
    fn contract_checks_require_the_requested_ranking_and_limit() {
        let contract = AnalysisContract {
            interpretation: InterpretationStatus::Grounded,
            subject: Some("spend".into()),
            measures: vec![crate::engine::runtime::ContractMeasure {
                concept: "revenue".into(),
                field: Some("amount".into()),
                operation: "sum".into(),
                unit: None,
            }],
            group_by: vec!["category".into()],
            order_by: Some(ContractOrder {
                by: "revenue".into(),
                direction: SortDirection::Desc,
            }),
            limit: Some(3),
            ..Default::default()
        };
        let report = GroundingReport {
            source: Some("spend".into()),
            sources: vec![],
            probes: vec![],
            unresolved: vec![],
        };
        let good = vec![run_sql_ev(
            "SELECT category, SUM(amount) AS measure_0 FROM spend GROUP BY category ORDER BY measure_0 DESC LIMIT 3",
            &["category", "measure_0"],
            vec![vec![Json::from("rent"), Json::from(12)]],
        )];
        let good_checks = contract_checks(&contract, Some(&report), &good);
        assert!(good_checks.iter().all(|check| check.ok), "{good_checks:?}");

        let bad = vec![run_sql_ev(
            "SELECT category, SUM(amount) AS total FROM spend GROUP BY category",
            &["category", "total"],
            vec![vec![Json::from("rent"), Json::from(12)]],
        )];
        let bad_checks = contract_checks(&contract, Some(&report), &bad);
        assert!(bad_checks.iter().any(|check| {
            !check.ok
                && check
                    .label
                    .contains("requested order by `revenue` was not used")
        }));
        assert!(bad_checks.iter().any(|check| {
            !check.ok && check.label.contains("requested row limit `3` was not used")
        }));
    }

    #[test]
    fn contract_checks_require_the_requested_derived_ratio() {
        let contract = AnalysisContract {
            interpretation: InterpretationStatus::Grounded,
            subject: Some("spend".into()),
            measures: vec![
                crate::engine::runtime::ContractMeasure {
                    concept: "revenue".into(),
                    field: Some("amount".into()),
                    operation: "sum".into(),
                    unit: None,
                },
                crate::engine::runtime::ContractMeasure {
                    concept: "orders".into(),
                    field: None,
                    operation: "count".into(),
                    unit: None,
                },
            ],
            derived_metrics: vec![ContractDerivedMetric {
                concept: "revenue per order".into(),
                kind: crate::engine::runtime::DerivedMetricKind::Ratio,
                numerator: "revenue".into(),
                denominator: "orders".into(),
                unit: None,
            }],
            ..Default::default()
        };
        let report = GroundingReport {
            source: Some("spend".into()),
            sources: vec![],
            probes: vec![],
            unresolved: vec![],
        };
        let good = vec![run_sql_ev(
            "SELECT SUM(amount) AS measure_0, COUNT(*) AS measure_1, (SUM(amount)) / NULLIF((COUNT(*)), 0) AS derived_0 FROM spend",
            &["measure_0", "measure_1", "derived_0"],
            vec![vec![Json::from(12), Json::from(2), Json::from(6)]],
        )];
        let good_checks = contract_checks(&contract, Some(&report), &good);
        assert!(good_checks.iter().all(|check| check.ok), "{good_checks:?}");

        let arithmetic_bad = vec![run_sql_ev(
            "SELECT SUM(amount) AS measure_0, COUNT(*) AS measure_1, (SUM(amount)) / NULLIF((COUNT(*)), 0) AS derived_0 FROM spend",
            &["measure_0", "measure_1", "derived_0"],
            vec![vec![Json::from(12), Json::from(2), Json::from(7)]],
        )];
        let arithmetic_checks = contract_checks(&contract, Some(&report), &arithmetic_bad);
        assert!(arithmetic_checks
            .iter()
            .any(|check| { !check.ok && check.label.contains("derived metric arithmetic") }));
        assert!(hard_fail(&arithmetic_checks).is_none());
        assert!(arithmetic_checks.iter().any(|check| {
            !check.ok
                && check.finding.as_ref().is_some_and(|finding| {
                    finding.effect == VerificationEffect::Repair
                        && finding.target == VerificationTarget::Claim
                })
        }));

        let bad = vec![run_sql_ev(
            "SELECT SUM(amount) AS measure_0, COUNT(*) AS measure_1 FROM spend",
            &["measure_0", "measure_1"],
            vec![vec![Json::from(12), Json::from(2)]],
        )];
        let bad_checks = contract_checks(&contract, Some(&report), &bad);
        assert!(bad_checks.iter().any(|check| {
            !check.ok
                && check
                    .label
                    .contains("derived metric `revenue per order` was not used")
        }));
    }

    #[test]
    fn contract_checks_flag_ratio_operands_from_separate_populations() {
        let contract = AnalysisContract {
            interpretation: InterpretationStatus::Grounded,
            subject: Some("spend".into()),
            measures: vec![
                crate::engine::runtime::ContractMeasure {
                    concept: "revenue".into(),
                    field: Some("amount".into()),
                    operation: "sum".into(),
                    unit: None,
                },
                crate::engine::runtime::ContractMeasure {
                    concept: "orders".into(),
                    field: None,
                    operation: "count".into(),
                    unit: None,
                },
            ],
            derived_metrics: vec![ContractDerivedMetric {
                concept: "revenue per order".into(),
                kind: crate::engine::runtime::DerivedMetricKind::Ratio,
                numerator: "revenue".into(),
                denominator: "orders".into(),
                unit: None,
            }],
            ..Default::default()
        };
        let report = GroundingReport {
            source: Some("spend".into()),
            sources: vec![],
            probes: vec![],
            unresolved: vec![],
        };
        let evidence = vec![run_sql_ev(
            "SELECT (SELECT SUM(amount) FROM spend WHERE category = 'dining') AS measure_0, \
                    (SELECT COUNT(*) FROM spend WHERE category = 'rent') AS measure_1, \
                    ((SELECT SUM(amount) FROM spend WHERE category = 'dining') / \
                     NULLIF((SELECT COUNT(*) FROM spend WHERE category = 'rent'), 0)) AS derived_0",
            &["measure_0", "measure_1", "derived_0"],
            vec![vec![Json::from(12), Json::from(2), Json::from(6)]],
        )];
        let checks = contract_checks(&contract, Some(&report), &evidence);
        assert!(
            checks.iter().any(|check| {
                !check.ok
                    && check.label.contains(
                        "derived metric `revenue per order` may use incompatible populations",
                    )
            }),
            "{checks:?}"
        );
        assert!(hard_fail(&checks).is_none());
        assert_eq!(
            status(&checks, &evidence, true),
            VerificationStatus::NeedsReview
        );
    }

    #[test]
    fn contract_checks_reject_missing_operation_and_observed_value() {
        let contract = AnalysisContract {
            interpretation: InterpretationStatus::Grounded,
            subject: Some("spend".into()),
            measures: vec![crate::engine::runtime::ContractMeasure {
                concept: "amount".into(),
                field: Some("amount".into()),
                operation: "sum".into(),
                unit: None,
            }],
            filters: vec![crate::engine::runtime::ContractFilter {
                concept: "category".into(),
                field: Some("category".into()),
                exclude: false,
                candidate_values: vec!["dining".into()],
                resolved_values: vec!["dining".into()],
                resolution: Some("observed".into()),
            }],
            ..Default::default()
        };
        let evidence = vec![run_sql_ev(
            "SELECT category, amount FROM spend WHERE category = 'rent'",
            &["category", "amount"],
            vec![vec![Json::from("rent"), Json::from(12)]],
        )];
        let report = GroundingReport {
            source: Some("spend".into()),
            sources: vec![],
            probes: vec![],
            unresolved: vec![],
        };
        let checks = contract_checks(&contract, Some(&report), &evidence);
        assert!(checks.iter().any(|check| {
            !check.ok && check.label.contains("measure operation `sum` was not used")
        }));
        assert!(checks.iter().any(|check| {
            !check.ok && check.label.contains("filter value `dining` was not used")
        }));
    }

    #[test]
    fn contract_checks_do_not_accept_unresolved_meaning() {
        let contract = AnalysisContract {
            interpretation: InterpretationStatus::Ambiguous,
            unresolved: vec!["which revenue field?".into()],
            ..Default::default()
        };
        let checks = contract_checks(&contract, None, &[]);
        assert!(checks.iter().any(|check| {
            !check.ok && check.label.contains("semantic contract remains ambiguous")
        }));
    }
}
