//! Deterministic grounding for a model-proposed analysis contract.
//!
//! The model can name concepts, fields, and filter values, but it cannot make
//! those names authoritative. This module resolves only exact or
//! case-insensitive matches against the current catalog and uses bounded
//! read-only probes for requested filter values. Anything else remains
//! explicitly unresolved for the model or user to handle.

use serde::{Deserialize, Serialize};
use serde_json::Value as Json;

use crate::engine::analytics::data::{quote_ident, quote_str};
use crate::engine::catalog::{Catalog, ColumnInfo, SourceInfo};
use crate::engine::runtime::{AnalysisContract, ContractJoin, InterpretationStatus, JoinKind};
use crate::engine::state::EngineState;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProbeOutcome {
    Resolved,
    NotObserved,
    Ambiguous,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroundingProbe {
    pub kind: String,
    pub target: String,
    pub outcome: ProbeOutcome,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroundingReport {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sources: Vec<String>,
    #[serde(default)]
    pub probes: Vec<GroundingProbe>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unresolved: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroundingResult {
    pub contract: AnalysisContract,
    pub report: GroundingReport,
}

pub fn ground(engine: &EngineState, mut contract: AnalysisContract) -> GroundingResult {
    let catalog = engine.catalog();
    let mut report = GroundingReport {
        source: None,
        sources: Vec::new(),
        probes: Vec::new(),
        unresolved: contract.unresolved.clone(),
    };

    if contract.interpretation == InterpretationStatus::Unsupported {
        return GroundingResult { contract, report };
    }

    if !contract.joins.is_empty() {
        return ground_join(engine, contract, catalog, report);
    }

    let source = match select_source(&catalog, &contract) {
        Ok(source) => source,
        Err(reason) => {
            add_unresolved(&mut report, reason);
            return finish(contract, report);
        }
    };
    let Some(source) = source else {
        add_unresolved(
            &mut report,
            "the contract did not identify a queryable source".into(),
        );
        return finish(contract, report);
    };
    let Some(view) = source.view.clone() else {
        add_unresolved(
            &mut report,
            format!("{} is not a queryable table", source.name),
        );
        return finish(contract, report);
    };
    report.source = Some(view.clone());
    report.sources = vec![view.clone()];

    for measure in &mut contract.measures {
        if measure.field.is_none()
            && matches!(
                measure.operation.trim().to_ascii_lowercase().as_str(),
                "count" | "number"
            )
        {
            report.probes.push(GroundingProbe {
                kind: "measure".into(),
                target: measure.concept.clone(),
                outcome: ProbeOutcome::Resolved,
                detail: "resolved as a row count; no physical field required".into(),
            });
            continue;
        }
        let requested = measure
            .field
            .as_deref()
            .unwrap_or(measure.concept.as_str())
            .to_string();
        match resolve_field(&source, &requested) {
            Ok(column) => {
                measure.field = Some(column.name.clone());
                report
                    .probes
                    .push(binding_probe("measure", &requested, column));
            }
            Err(reason) => add_unresolved(&mut report, reason),
        }
    }

    for filter in &mut contract.filters {
        // `resolved_values` is an output of grounding, not an authority the
        // model can grant itself. A provider may return it in the contract
        // payload, but only a bounded probe against this workspace revision
        // can promote a candidate into an observed value.
        let proposed_resolved_values = std::mem::take(&mut filter.resolved_values);
        filter.resolution = None;
        if filter.candidate_values.is_empty() && !proposed_resolved_values.is_empty() {
            add_unresolved(
                &mut report,
                format!(
                    "filter {:?} supplied a resolved value without a candidate to verify",
                    filter.concept
                ),
            );
        }
        let requested = filter
            .field
            .as_deref()
            .unwrap_or(filter.concept.as_str())
            .to_string();
        match resolve_field(&source, &requested) {
            Ok(column) => {
                filter.field = Some(column.name.clone());
                report
                    .probes
                    .push(binding_probe("filter", &requested, column));
                let mut observed = false;
                let mut not_observed = false;
                for candidate in filter.candidate_values.clone() {
                    match probe_value(engine, &view, &column.name, &candidate) {
                        Ok(Some(observed_value)) => {
                            observed = true;
                            if !filter.resolved_values.contains(&observed_value) {
                                filter.resolved_values.push(observed_value.clone());
                            }
                            report.probes.push(GroundingProbe {
                                kind: "filter_value".into(),
                                target: format!("{}={candidate}", column.name),
                                outcome: ProbeOutcome::Resolved,
                                detail: format!("matched observed value {observed_value}"),
                            });
                        }
                        Ok(None) => {
                            not_observed = true;
                            report.probes.push(GroundingProbe {
                                kind: "filter_value".into(),
                                target: format!("{}={candidate}", column.name),
                                outcome: ProbeOutcome::NotObserved,
                                detail: "no matching value was observed in this snapshot".into(),
                            });
                        }
                        Err(error) => {
                            add_unresolved(
                                &mut report,
                                format!("could not probe filter value {candidate:?}: {error}"),
                            );
                            report.probes.push(GroundingProbe {
                                kind: "filter_value".into(),
                                target: format!("{}={candidate}", column.name),
                                outcome: ProbeOutcome::Unavailable,
                                detail: error,
                            });
                        }
                    }
                }
                filter.resolution = if observed {
                    Some("observed".into())
                } else if not_observed {
                    Some("not_observed".into())
                } else {
                    None
                };
            }
            Err(reason) => add_unresolved(&mut report, reason),
        }
    }

    if let Some(time) = &mut contract.time {
        if let Some(requested) = time.field.clone() {
            match resolve_field(&source, &requested) {
                Ok(column) => {
                    time.field = Some(column.name.clone());
                    report
                        .probes
                        .push(binding_probe("time", &requested, column));
                }
                Err(reason) => add_unresolved(&mut report, reason),
            }
        }
    }

    if contract.comparison_spec.is_some() {
        ground_comparison_ranges(engine, &view, &contract, &mut report);
    }

    for group in &mut contract.group_by {
        let requested = group.clone();
        match resolve_field(&source, &requested) {
            Ok(column) => {
                *group = column.name.clone();
                report
                    .probes
                    .push(binding_probe("group_by", &requested, column));
            }
            Err(reason) => add_unresolved(&mut report, reason),
        }
    }

    if let Some(order) = &mut contract.order_by {
        let requested = order.by.clone();
        let measure_matches: Vec<_> = contract
            .measures
            .iter()
            .filter(|measure| {
                normalize(&measure.concept) == normalize(&requested)
                    || measure
                        .field
                        .as_deref()
                        .is_some_and(|field| normalize(field) == normalize(&requested))
            })
            .collect();
        match measure_matches.as_slice() {
            [measure] => {
                let target = measure
                    .field
                    .clone()
                    .unwrap_or_else(|| measure.concept.clone());
                order.by = target.clone();
                report.probes.push(GroundingProbe {
                    kind: "order_by".into(),
                    target: requested,
                    outcome: ProbeOutcome::Resolved,
                    detail: format!("resolved to measure {target}"),
                });
            }
            [_first, ..] => add_unresolved(
                &mut report,
                format!("order_by {requested:?} matched more than one measure"),
            ),
            [] => {
                let derived_matches: Vec<_> = contract
                    .derived_metrics
                    .iter()
                    .enumerate()
                    .filter(|(index, derived)| {
                        normalize(&derived.concept) == normalize(&requested)
                            || normalize(&format!("derived_{index}")) == normalize(&requested)
                    })
                    .collect();
                match derived_matches.as_slice() {
                    [(index, derived)] => {
                        order.by = format!("derived_{index}");
                        report.probes.push(GroundingProbe {
                            kind: "order_by".into(),
                            target: requested,
                            outcome: ProbeOutcome::Resolved,
                            detail: format!("resolved to derived metric {}", derived.concept),
                        });
                    }
                    [_first, ..] => add_unresolved(
                        &mut report,
                        format!("order_by {requested:?} matched more than one derived metric"),
                    ),
                    [] => match resolve_field(&source, &requested) {
                        Ok(column) => {
                            order.by = column.name.clone();
                            report
                                .probes
                                .push(binding_probe("order_by", &requested, column));
                        }
                        Err(reason) => add_unresolved(&mut report, reason),
                    },
                }
            }
        }
    }

    for derived in &contract.derived_metrics {
        for (kind, requested) in [
            ("derived_numerator", derived.numerator.as_str()),
            ("derived_denominator", derived.denominator.as_str()),
        ] {
            let matches: Vec<_> = contract
                .measures
                .iter()
                .enumerate()
                .filter(|(_, measure)| {
                    normalize(&measure.concept) == normalize(requested)
                        || measure
                            .field
                            .as_deref()
                            .is_some_and(|field| normalize(field) == normalize(requested))
                })
                .collect();
            match matches.len() {
                1 => {
                    let (_, measure) = matches[0];
                    report.probes.push(GroundingProbe {
                        kind: kind.into(),
                        target: requested.into(),
                        outcome: ProbeOutcome::Resolved,
                        detail: format!("resolved to measure {}", measure.concept),
                    });
                }
                0 => add_unresolved(
                    &mut report,
                    format!(
                        "derived metric {:?} {kind} {:?} did not match a declared measure",
                        derived.concept, requested
                    ),
                ),
                _ => add_unresolved(
                    &mut report,
                    format!(
                        "derived metric {:?} {kind} {:?} matched more than one measure",
                        derived.concept, requested
                    ),
                ),
            }
        }
    }

    finish(contract, report)
}

fn finish(mut contract: AnalysisContract, mut report: GroundingReport) -> GroundingResult {
    contract.unresolved = report.unresolved.clone();
    if contract.interpretation != InterpretationStatus::Unsupported {
        contract.interpretation = if contract.unresolved.is_empty() {
            InterpretationStatus::Grounded
        } else {
            InterpretationStatus::Ambiguous
        };
    }
    report.unresolved = contract.unresolved.clone();
    GroundingResult { contract, report }
}

fn ground_join(
    engine: &EngineState,
    mut contract: AnalysisContract,
    catalog: Catalog,
    mut report: GroundingReport,
) -> GroundingResult {
    if contract.comparison_spec.is_some() {
        add_unresolved(
            &mut report,
            "typed period comparisons are currently limited to one queryable source".into(),
        );
    }
    let queryable: Vec<&SourceInfo> = catalog
        .sources
        .iter()
        .filter(|source| source.view.is_some())
        .collect();
    let mut scope = Vec::new();
    for join in &mut contract.joins {
        let left_source = join.left_source.clone();
        let right_source = join.right_source.clone();
        let left = match find_source(&queryable, &left_source) {
            Some(source) => source,
            None => {
                add_unresolved(
                    &mut report,
                    format!("could not find join source {:?}", left_source),
                );
                continue;
            }
        };
        let right = match find_source(&queryable, &right_source) {
            Some(source) => source,
            None => {
                add_unresolved(
                    &mut report,
                    format!("could not find join source {:?}", right_source),
                );
                continue;
            }
        };
        if !scope
            .iter()
            .any(|candidate: &&SourceInfo| std::ptr::eq(*candidate, left))
        {
            scope.push(left);
        }
        if !scope
            .iter()
            .any(|candidate: &&SourceInfo| std::ptr::eq(*candidate, right))
        {
            scope.push(right);
        }
        ground_join_edge(engine, join, left, right, &mut report);
    }
    if scope.is_empty() {
        return finish(contract, report);
    }
    report.sources = scope
        .iter()
        .filter_map(|source| source.view.clone())
        .collect();
    let primary = contract
        .subject
        .as_deref()
        .and_then(|subject| find_source(&scope, subject))
        .or_else(|| scope.first().copied());
    let Some(primary) = primary else {
        add_unresolved(
            &mut report,
            "the join contract did not identify a primary source".into(),
        );
        return finish(contract, report);
    };
    report.source = primary.view.clone();
    if contract.subject.is_none() {
        contract.subject = primary.view.clone();
    }

    for measure in &mut contract.measures {
        if measure.field.is_none()
            && matches!(
                measure.operation.trim().to_ascii_lowercase().as_str(),
                "count" | "number"
            )
        {
            report.probes.push(GroundingProbe {
                kind: "measure".into(),
                target: measure.concept.clone(),
                outcome: ProbeOutcome::Resolved,
                detail: "resolved as a row count; no physical field required".into(),
            });
            continue;
        }
        let requested = measure
            .field
            .as_deref()
            .unwrap_or(&measure.concept)
            .to_string();
        match resolve_join_field(&scope, &requested) {
            Ok((source, column)) => {
                measure.field = Some(qualified_field(source, column));
                report
                    .probes
                    .push(join_binding_probe("measure", &requested, source, column));
            }
            Err(reason) => add_unresolved(&mut report, reason),
        }
    }

    for filter in &mut contract.filters {
        let proposed_resolved_values = std::mem::take(&mut filter.resolved_values);
        filter.resolution = None;
        if filter.candidate_values.is_empty() && !proposed_resolved_values.is_empty() {
            add_unresolved(
                &mut report,
                format!(
                    "filter {:?} supplied a resolved value without a candidate to verify",
                    filter.concept
                ),
            );
        }
        let requested = filter
            .field
            .as_deref()
            .unwrap_or(&filter.concept)
            .to_string();
        match resolve_join_field(&scope, &requested) {
            Ok((source, column)) => {
                filter.field = Some(qualified_field(source, column));
                report
                    .probes
                    .push(join_binding_probe("filter", &requested, source, column));
                let view = source.view.as_deref().unwrap_or(&source.name);
                let mut observed = false;
                let mut not_observed = false;
                for candidate in filter.candidate_values.clone() {
                    match probe_value(engine, view, &column.name, &candidate) {
                        Ok(Some(observed_value)) => {
                            observed = true;
                            if !filter.resolved_values.contains(&observed_value) {
                                filter.resolved_values.push(observed_value.clone());
                            }
                            report.probes.push(GroundingProbe {
                                kind: "filter_value".into(),
                                target: format!("{}.{}={candidate}", view, column.name),
                                outcome: ProbeOutcome::Resolved,
                                detail: format!("matched observed value {observed_value}"),
                            });
                        }
                        Ok(None) => {
                            not_observed = true;
                            report.probes.push(GroundingProbe {
                                kind: "filter_value".into(),
                                target: format!("{}.{}={candidate}", view, column.name),
                                outcome: ProbeOutcome::NotObserved,
                                detail: "no matching value was observed in this snapshot".into(),
                            });
                        }
                        Err(error) => {
                            add_unresolved(
                                &mut report,
                                format!("could not probe filter value {candidate:?}: {error}"),
                            );
                            report.probes.push(GroundingProbe {
                                kind: "filter_value".into(),
                                target: format!("{}.{}={candidate}", view, column.name),
                                outcome: ProbeOutcome::Unavailable,
                                detail: error,
                            });
                        }
                    }
                }
                filter.resolution = if observed {
                    Some("observed".into())
                } else if not_observed {
                    Some("not_observed".into())
                } else {
                    None
                };
            }
            Err(reason) => add_unresolved(&mut report, reason),
        }
    }

    if let Some(time) = &mut contract.time {
        if let Some(requested) = time.field.clone() {
            match resolve_join_field(&scope, &requested) {
                Ok((source, column)) => {
                    time.field = Some(qualified_field(source, column));
                    report
                        .probes
                        .push(join_binding_probe("time", &requested, source, column));
                }
                Err(reason) => add_unresolved(&mut report, reason),
            }
        }
    }

    for group in &mut contract.group_by {
        let requested = group.clone();
        match resolve_join_field(&scope, &requested) {
            Ok((source, column)) => {
                *group = qualified_field(source, column);
                report
                    .probes
                    .push(join_binding_probe("group_by", &requested, source, column));
            }
            Err(reason) => add_unresolved(&mut report, reason),
        }
    }

    for derived in &contract.derived_metrics {
        for (kind, requested) in [
            ("derived_numerator", derived.numerator.as_str()),
            ("derived_denominator", derived.denominator.as_str()),
        ] {
            let matches: Vec<_> = contract
                .measures
                .iter()
                .filter(|measure| {
                    normalize(&measure.concept) == normalize(requested)
                        || measure
                            .field
                            .as_deref()
                            .is_some_and(|field| normalize(field) == normalize(requested))
                })
                .collect();
            if matches.len() == 1 {
                report.probes.push(GroundingProbe {
                    kind: kind.into(),
                    target: requested.into(),
                    outcome: ProbeOutcome::Resolved,
                    detail: format!("resolved to measure {}", matches[0].concept),
                });
            } else if matches.is_empty() {
                add_unresolved(
                    &mut report,
                    format!(
                        "derived metric {:?} {kind} {:?} did not match a declared measure",
                        derived.concept, requested
                    ),
                );
            } else {
                add_unresolved(
                    &mut report,
                    format!(
                        "derived metric {:?} {kind} {:?} matched more than one measure",
                        derived.concept, requested
                    ),
                );
            }
        }
    }

    for order in contract.order_by.iter_mut() {
        let requested = order.by.clone();
        let measure_matches: Vec<_> = contract
            .measures
            .iter()
            .filter(|measure| {
                normalize(&measure.concept) == normalize(&requested)
                    || measure
                        .field
                        .as_deref()
                        .is_some_and(|field| normalize(field) == normalize(&requested))
            })
            .collect();
        if measure_matches.len() == 1 {
            let target = measure_matches[0]
                .field
                .clone()
                .unwrap_or_else(|| measure_matches[0].concept.clone());
            order.by = target;
            report.probes.push(GroundingProbe {
                kind: "order_by".into(),
                target: requested,
                outcome: ProbeOutcome::Resolved,
                detail: "resolved to a declared measure".into(),
            });
        } else if measure_matches.is_empty() {
            match resolve_join_field(&scope, &requested) {
                Ok((source, column)) => {
                    order.by = qualified_field(source, column);
                    report
                        .probes
                        .push(join_binding_probe("order_by", &requested, source, column));
                }
                Err(reason) => add_unresolved(&mut report, reason),
            }
        } else {
            add_unresolved(
                &mut report,
                format!("order_by {requested:?} matched more than one measure"),
            );
        }
    }

    finish(contract, report)
}

fn ground_comparison_ranges(
    engine: &EngineState,
    view: &str,
    contract: &AnalysisContract,
    report: &mut GroundingReport,
) {
    let Some(comparison) = contract.comparison_spec.as_ref() else {
        return;
    };
    let Some(field) = contract
        .time
        .as_ref()
        .and_then(|time| time.field.as_deref())
    else {
        add_unresolved(
            report,
            "typed period comparison has no grounded time field".into(),
        );
        return;
    };
    for (label, range) in [
        ("current", comparison.current_range.as_str()),
        ("previous", comparison.previous_range.as_str()),
    ] {
        match probe_time_range(engine, view, field, range) {
            Ok(rows) if rows > 0 => report.probes.push(GroundingProbe {
                kind: "comparison_range".into(),
                target: format!("{label}:{range}"),
                outcome: ProbeOutcome::Resolved,
                detail: format!("observed {rows} row(s) in this snapshot"),
            }),
            Ok(_) => {
                let detail = format!("no rows were observed for the {label} comparison range");
                add_unresolved(
                    report,
                    format!("{label} comparison range {range:?} was not observed"),
                );
                report.probes.push(GroundingProbe {
                    kind: "comparison_range".into(),
                    target: format!("{label}:{range}"),
                    outcome: ProbeOutcome::NotObserved,
                    detail,
                });
            }
            Err(error) => {
                let detail = format!("could not probe the {label} comparison range: {error}");
                add_unresolved(report, detail.clone());
                report.probes.push(GroundingProbe {
                    kind: "comparison_range".into(),
                    target: format!("{label}:{range}"),
                    outcome: ProbeOutcome::Unavailable,
                    detail,
                });
            }
        }
    }
}

fn probe_time_range(
    engine: &EngineState,
    view: &str,
    field: &str,
    range: &str,
) -> Result<i64, String> {
    let predicate = crate::engine::planner::time_predicate(field, range)?;
    let sql = format!(
        "SELECT COUNT(*) AS rows FROM {} WHERE {predicate}",
        quote_ident(view)
    );
    let result = engine.run_sql(&sql).map_err(|error| error.to_string())?;
    result
        .rows
        .first()
        .and_then(|row| row.first())
        .and_then(value_count)
        .ok_or_else(|| "time range probe returned no count".into())
}

fn find_source<'a>(sources: &[&'a SourceInfo], requested: &str) -> Option<&'a SourceInfo> {
    sources.iter().copied().find(|source| {
        source.name.eq_ignore_ascii_case(requested)
            || source
                .view
                .as_deref()
                .is_some_and(|view| view.eq_ignore_ascii_case(requested))
    })
}

fn ground_join_edge(
    engine: &EngineState,
    join: &mut ContractJoin,
    left: &SourceInfo,
    right: &SourceInfo,
    report: &mut GroundingReport,
) {
    let left_field = join.left_field.clone();
    let right_field = join.right_field.clone();
    let left_column = match resolve_field(left, &left_field) {
        Ok(column) => {
            report
                .probes
                .push(join_binding_probe("join_left", &left_field, left, column));
            column
        }
        Err(reason) => {
            add_unresolved(report, reason);
            return;
        }
    };
    let right_column = match resolve_field(right, &right_field) {
        Ok(column) => {
            report.probes.push(join_binding_probe(
                "join_right",
                &right_field,
                right,
                column,
            ));
            column
        }
        Err(reason) => {
            add_unresolved(report, reason);
            return;
        }
    };
    let Some(left_view) = left.view.as_deref() else {
        add_unresolved(report, format!("{} is not queryable", left.name));
        return;
    };
    let Some(right_view) = right.view.as_deref() else {
        add_unresolved(report, format!("{} is not queryable", right.name));
        return;
    };
    join.left_source = left_view.to_string();
    join.left_field = left_column.name.clone();
    join.right_source = right_view.to_string();
    join.right_field = right_column.name.clone();
    let join_kind = match join.kind {
        JoinKind::Inner => "JOIN",
        JoinKind::Left => "LEFT JOIN",
    };
    let sql = format!(
        "SELECT (SELECT COUNT(*) FROM {left_view_sql}) AS left_rows, (SELECT COUNT(*) FROM {right_view_sql}) AS right_rows, (SELECT COUNT(*) FROM {left_view_sql} AS l {join_kind} {right_view_sql} AS r ON l.{left_field_sql} = r.{right_field_sql}) AS matched_rows",
        left_view_sql = quote_ident(left_view),
        right_view_sql = quote_ident(right_view),
        left_field_sql = quote_ident(&left_column.name),
        right_field_sql = quote_ident(&right_column.name),
    );
    match engine.run_sql(&sql) {
        Ok(result) => {
            let detail = result
                .rows
                .first()
                .map(|row| {
                    let values = row.iter().filter_map(value_count).collect::<Vec<_>>();
                    if values.len() == 3 {
                        format!(
                            "left rows={}, right rows={}, matched rows={}",
                            values[0], values[1], values[2]
                        )
                    } else {
                        "cardinality probe returned an unexpected shape".into()
                    }
                })
                .unwrap_or_else(|| "cardinality probe returned no row".into());
            if detail.starts_with("cardinality probe returned") {
                add_unresolved(report, detail.clone());
                report.probes.push(GroundingProbe {
                    kind: "join_cardinality".into(),
                    target: format!(
                        "{left_view}.{} ↔ {right_view}.{}",
                        left_column.name, right_column.name
                    ),
                    outcome: ProbeOutcome::Unavailable,
                    detail,
                });
            } else {
                report.probes.push(GroundingProbe {
                    kind: "join_cardinality".into(),
                    target: format!(
                        "{left_view}.{} ↔ {right_view}.{}",
                        left_column.name, right_column.name
                    ),
                    outcome: ProbeOutcome::Resolved,
                    detail,
                });
            }
        }
        Err(error) => {
            let detail = format!("join cardinality probe failed: {error}");
            add_unresolved(report, detail.clone());
            report.probes.push(GroundingProbe {
                kind: "join_cardinality".into(),
                target: format!(
                    "{left_view}.{} ↔ {right_view}.{}",
                    left_column.name, right_column.name
                ),
                outcome: ProbeOutcome::Unavailable,
                detail,
            });
        }
    }
}

fn resolve_join_field<'a>(
    sources: &[&'a SourceInfo],
    requested: &str,
) -> Result<(&'a SourceInfo, &'a ColumnInfo), String> {
    let (source_hint, field) = requested
        .rsplit_once('.')
        .map_or((None, requested), |(source, field)| (Some(source), field));
    let candidates: Vec<(&SourceInfo, &ColumnInfo)> = sources
        .iter()
        .copied()
        .filter(|source| {
            source_hint.is_none_or(|hint| {
                source.name.eq_ignore_ascii_case(hint)
                    || source
                        .view
                        .as_deref()
                        .is_some_and(|view| view.eq_ignore_ascii_case(hint))
            })
        })
        .flat_map(|source| {
            source
                .columns
                .as_deref()
                .unwrap_or_default()
                .iter()
                .filter(move |column| normalize(&column.name) == normalize(field))
                .map(move |column| (source, column))
        })
        .collect();
    match candidates.as_slice() {
        [(source, column)] => Ok((*source, *column)),
        [] => Err(format!("could not resolve joined field {requested:?}")),
        _ => Err(format!("joined field {requested:?} is ambiguous")),
    }
}

fn qualified_field(source: &SourceInfo, column: &ColumnInfo) -> String {
    format!(
        "{}.{}",
        source.view.as_deref().unwrap_or(&source.name),
        column.name
    )
}

fn join_binding_probe(
    kind: &str,
    requested: &str,
    source: &SourceInfo,
    column: &ColumnInfo,
) -> GroundingProbe {
    GroundingProbe {
        kind: kind.into(),
        target: requested.into(),
        outcome: ProbeOutcome::Resolved,
        detail: format!(
            "resolved to {}.{}",
            source.view.as_deref().unwrap_or(&source.name),
            column.name
        ),
    }
}

fn value_count(value: &Json) -> Option<i64> {
    value
        .as_i64()
        .or_else(|| value.as_u64().and_then(|value| i64::try_from(value).ok()))
        .or_else(|| value.as_str().and_then(|value| value.parse().ok()))
}

fn select_source(
    catalog: &Catalog,
    contract: &AnalysisContract,
) -> Result<Option<SourceInfo>, String> {
    let queryable: Vec<SourceInfo> = catalog
        .sources
        .iter()
        .filter(|source| source.view.is_some())
        .cloned()
        .collect();

    if let Some(subject) = contract.subject.as_deref() {
        let subject = subject.trim();
        let matches: Vec<SourceInfo> = queryable
            .into_iter()
            .filter(|source| {
                source
                    .view
                    .as_deref()
                    .is_some_and(|view| view.eq_ignore_ascii_case(subject))
                    || source.name.eq_ignore_ascii_case(subject)
            })
            .collect();
        return match matches.len() {
            0 => Err(format!("could not find queryable source {subject:?}")),
            1 => Ok(matches.into_iter().next()),
            _ => Err(format!("source {subject:?} matched more than one table")),
        };
    }

    let mut requested: Vec<String> = contract
        .measures
        .iter()
        .map(|measure| {
            measure
                .field
                .as_deref()
                .unwrap_or(&measure.concept)
                .to_string()
        })
        .chain(contract.filters.iter().map(|filter| {
            filter
                .field
                .as_deref()
                .unwrap_or(&filter.concept)
                .to_string()
        }))
        .chain(contract.time.iter().filter_map(|time| time.field.clone()))
        .chain(contract.group_by.iter().cloned())
        .collect();
    if let Some(order) = &contract.order_by {
        let is_measure_reference = contract.measures.iter().any(|measure| {
            normalize(&measure.concept) == normalize(&order.by)
                || measure
                    .field
                    .as_deref()
                    .is_some_and(|field| normalize(field) == normalize(&order.by))
        });
        if !is_measure_reference {
            let is_derived_reference =
                contract
                    .derived_metrics
                    .iter()
                    .enumerate()
                    .any(|(index, derived)| {
                        normalize(&derived.concept) == normalize(&order.by)
                            || normalize(&format!("derived_{index}")) == normalize(&order.by)
                    });
            if !is_derived_reference {
                requested.push(order.by.clone());
            }
        }
    }
    for derived in &contract.derived_metrics {
        for reference in [&derived.numerator, &derived.denominator] {
            let is_measure_reference = contract.measures.iter().any(|measure| {
                normalize(&measure.concept) == normalize(reference)
                    || measure
                        .field
                        .as_deref()
                        .is_some_and(|field| normalize(field) == normalize(reference))
            });
            if !is_measure_reference {
                requested.push(reference.clone());
            }
        }
    }
    let matches: Vec<SourceInfo> = queryable
        .into_iter()
        .filter(|source| {
            requested.iter().any(|name| {
                source
                    .columns
                    .as_deref()
                    .unwrap_or_default()
                    .iter()
                    .any(|column| normalize(&column.name) == normalize(name))
            })
        })
        .collect();
    match matches.len() {
        1 => Ok(matches.into_iter().next()),
        0 => Ok(None),
        _ => Err("the requested fields occur in more than one table; specify the source".into()),
    }
}

fn resolve_field<'a>(source: &'a SourceInfo, requested: &str) -> Result<&'a ColumnInfo, String> {
    let columns = source.columns.as_deref().unwrap_or_default();
    let matches: Vec<&ColumnInfo> = columns
        .iter()
        .filter(|column| normalize(&column.name) == normalize(requested))
        .collect();
    match matches.len() {
        1 => Ok(matches[0]),
        0 => Err(format!(
            "could not resolve field {requested:?} in {}",
            source.view.as_deref().unwrap_or(&source.name)
        )),
        _ => Err(format!(
            "field {requested:?} is ambiguous in {}",
            source.name
        )),
    }
}

fn binding_probe(kind: &str, requested: &str, column: &ColumnInfo) -> GroundingProbe {
    GroundingProbe {
        kind: kind.into(),
        target: requested.into(),
        outcome: ProbeOutcome::Resolved,
        detail: format!("resolved to {}", column.name),
    }
}

fn probe_value(
    engine: &EngineState,
    view: &str,
    field: &str,
    candidate: &str,
) -> Result<Option<String>, String> {
    let view_sql = quote_ident(view);
    let field_sql = quote_ident(field);
    let sql = format!(
        "SELECT DISTINCT {field_sql} AS value FROM {view_sql} WHERE lower(CAST({field_sql} AS TEXT)) = lower({}) LIMIT 1",
        quote_str(candidate)
    );
    let result = engine.run_sql(&sql).map_err(|error| error.to_string())?;
    Ok(result
        .rows
        .first()
        .and_then(|row| row.first())
        .and_then(value_string))
}

fn value_string(value: &Json) -> Option<String> {
    match value {
        Json::Null => None,
        Json::String(value) => Some(value.clone()),
        other => Some(other.to_string()),
    }
}

fn add_unresolved(report: &mut GroundingReport, reason: String) {
    if !report.unresolved.contains(&reason) {
        report.unresolved.push(reason);
    }
}

fn normalize(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(|character| character.to_lowercase())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn field_matching_ignores_case_and_separators() {
        assert_eq!(normalize("Order Amount"), "orderamount");
        assert_eq!(normalize("order_amount"), "orderamount");
    }

    #[test]
    fn non_text_probe_values_keep_their_json_value() {
        assert_eq!(value_string(&Json::from(12)), Some("12".into()));
        assert_eq!(value_string(&Json::from(true)), Some("true".into()));
        assert_eq!(value_string(&Json::Null), None);
    }
}
