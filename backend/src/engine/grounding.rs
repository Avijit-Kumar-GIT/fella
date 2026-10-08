//! Deterministic grounding for a model-proposed analysis contract.
//!
//! The model can name concepts, fields, and filter values, but it cannot make
//! those names authoritative. This module resolves exact normalized names or
//! conservative human-label candidates against the current catalog, and uses
//! bounded read-only probes for requested filter values. Anything else remains
//! explicitly unresolved for the model or user to handle.

use serde::{Deserialize, Serialize};
use serde_json::Value as Json;

use crate::engine::analytics::data::{quote_ident, quote_str};
use crate::engine::catalog::{
    field_name_matches, source_has_any_field, source_name_exact_matches, source_name_matches,
    source_scope, Catalog, ColumnInfo, SourceInfo, SourceScope,
};
use crate::engine::runtime::{
    AnalysisContract, ContextReference, ContractFilter, ContractJoin, InterpretationStatus,
    JoinKind,
};
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

pub fn ground(engine: &EngineState, contract: AnalysisContract) -> GroundingResult {
    ground_with_context(engine, contract, &[])
}

/// Ground a model-proposed contract with the user's selected Context starting
/// points. A single selected source is a deterministic scope hint only when
/// the contract did not name a source; it never grants access or turns a
/// column label into evidence.
pub fn ground_with_context(
    engine: &EngineState,
    mut contract: AnalysisContract,
    context_refs: &[ContextReference],
) -> GroundingResult {
    let context_source = context_source_hint(engine, context_refs);
    let context_applied = apply_context_source_hint(&mut contract, context_source.as_deref());
    let mut result = ground_unhinted(engine, contract);
    if context_applied {
        let source = context_source.expect("context source exists when context is applied");
        result.report.probes.insert(
            0,
            GroundingProbe {
                kind: "context_source".into(),
                target: source.clone(),
                outcome: ProbeOutcome::Resolved,
                detail: "selected Context source used as the starting scope".into(),
            },
        );
    }
    result
}

fn apply_context_source_hint(
    contract: &mut AnalysisContract,
    context_source: Option<&str>,
) -> bool {
    if contract.source.is_some() {
        return false;
    }
    let Some(source) = context_source
        .map(str::trim)
        .filter(|source| !source.is_empty())
    else {
        return false;
    };
    contract.source = Some(source.to_string());
    true
}

fn ground_unhinted(engine: &EngineState, mut contract: AnalysisContract) -> GroundingResult {
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
    contract.source = source.view.clone().or_else(|| Some(source.name.clone()));
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
        prepare_filter_candidates(filter);
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
            Err(reason) => {
                let requested_scope = source_scope(&requested, "", None);
                let selected_scope =
                    source_scope(&source.name, &source.path, source.view.as_deref());
                if requested_scope != SourceScope::Unknown && requested_scope == selected_scope {
                    filter.resolution = Some("source_scope".into());
                    report.probes.push(GroundingProbe {
                        kind: "source_scope".into(),
                        target: requested,
                        outcome: ProbeOutcome::Resolved,
                        detail: format!(
                            "resolved against the mounted source scope ({}) rather than a row field",
                            selected_scope.label()
                        ),
                    });
                } else {
                    add_unresolved(&mut report, reason);
                }
            }
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
        contract.interpretation = if contract.clarification.is_some() {
            // A clarification is an explicit user-authority decision. Even if
            // the physical source and fields are already known, grounding must
            // not promote the contract to executable while that choice is
            // pending.
            InterpretationStatus::Ambiguous
        } else if contract.unresolved.is_empty() {
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
    let declared_joins = contract.joins.clone();

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
        match resolve_join_field_with_keys(&scope, &declared_joins, &requested) {
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
        prepare_filter_candidates(filter);
        let requested = filter
            .field
            .as_deref()
            .unwrap_or(&filter.concept)
            .to_string();
        match resolve_join_field_with_keys(&scope, &declared_joins, &requested) {
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
            match resolve_join_field_with_keys(&scope, &declared_joins, &requested) {
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
        match resolve_join_field_with_keys(&scope, &declared_joins, &requested) {
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
                [(index, _)] => {
                    order.by = format!("derived_{index}");
                    report.probes.push(GroundingProbe {
                        kind: "order_by".into(),
                        target: requested,
                        outcome: ProbeOutcome::Resolved,
                        detail: "resolved to a derived metric".into(),
                    });
                }
                [_first, ..] => add_unresolved(
                    &mut report,
                    format!("order_by {requested:?} matched more than one derived metric"),
                ),
                [] => match resolve_join_field_with_keys(&scope, &declared_joins, &requested) {
                    Ok((source, column)) => {
                        order.by = qualified_field(source, column);
                        report
                            .probes
                            .push(join_binding_probe("order_by", &requested, source, column));
                    }
                    Err(reason) => add_unresolved(&mut report, reason),
                },
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
    let exact: Vec<&SourceInfo> = sources
        .iter()
        .copied()
        .filter(|source| source_name_exact_matches(&source.name, source.view.as_deref(), requested))
        .collect();
    if exact.len() == 1 {
        return exact.into_iter().next();
    }
    if !exact.is_empty() {
        return None;
    }
    let fuzzy: Vec<&SourceInfo> = sources
        .iter()
        .copied()
        .filter(|source| source_name_matches(&source.name, source.view.as_deref(), requested))
        .collect();
    (fuzzy.len() == 1).then(|| fuzzy[0])
}

fn context_source_hint(engine: &EngineState, refs: &[ContextReference]) -> Option<String> {
    let catalog = engine.catalog();
    let mut matches = Vec::<String>::new();
    for source in catalog
        .sources
        .iter()
        .filter(|source| source.view.is_some())
    {
        let view = source.view.as_deref().unwrap_or(&source.name);
        let matched = refs.iter().any(|reference| {
            let key = reference.key.as_str();
            let label = reference.label.as_str();
            match reference.kind.as_str() {
                "source" => {
                    key.eq_ignore_ascii_case(&source.path)
                        || key.eq_ignore_ascii_case(&source.name)
                        || key.eq_ignore_ascii_case(view)
                        || label.eq_ignore_ascii_case(&source.name)
                        || label.eq_ignore_ascii_case(view)
                }
                "column" => key.starts_with(&format!("{}:", source.path)),
                _ => false,
            }
        });
        if matched && !matches.iter().any(|candidate| candidate == view) {
            matches.push(view.to_string());
        }
    }
    (matches.len() == 1).then(|| matches.remove(0))
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
    let scoped_sources: Vec<&SourceInfo> = sources
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
        .collect();
    let normalized_field = normalize(field);
    let exact_candidates: Vec<(&SourceInfo, &ColumnInfo)> = scoped_sources
        .iter()
        .copied()
        .flat_map(|source| {
            source
                .columns
                .as_deref()
                .unwrap_or_default()
                .iter()
                .filter(|column| normalize(&column.name) == normalized_field)
                .map(move |column| (source, column))
        })
        .collect();
    let candidates: Vec<(&SourceInfo, &ColumnInfo)> = if exact_candidates.is_empty() {
        scoped_sources
            .into_iter()
            .flat_map(|source| {
                source
                    .columns
                    .as_deref()
                    .unwrap_or_default()
                    .iter()
                    .filter(move |column| field_name_matches(&column.name, field))
                    .map(move |column| (source, column))
            })
            .collect()
    } else {
        exact_candidates
    };
    match candidates.as_slice() {
        [(source, column)] => Ok((*source, *column)),
        [] => Err(format!("could not resolve joined field {requested:?}")),
        _ => Err(format!("joined field {requested:?} is ambiguous")),
    }
}

fn resolve_join_field_with_keys<'a>(
    sources: &[&'a SourceInfo],
    joins: &[ContractJoin],
    requested: &str,
) -> Result<(&'a SourceInfo, &'a ColumnInfo), String> {
    match resolve_join_field(sources, requested) {
        Ok(resolved) => Ok(resolved),
        Err(reason) if reason.contains("ambiguous") && !requested.contains('.') => {
            let wanted = normalize(requested);
            for join in joins {
                for (source_name, field) in [
                    (&join.left_source, &join.left_field),
                    (&join.right_source, &join.right_field),
                ] {
                    if normalize(field) != wanted {
                        continue;
                    }
                    let Some(source) = find_source(sources, source_name) else {
                        continue;
                    };
                    if let Ok(column) = resolve_field(source, field) {
                        // A bare shared key is safe to bind only because this
                        // contract declared the relationship explicitly.
                        return Ok((source, column));
                    }
                }
            }
            Err(reason)
        }
        Err(reason) => Err(reason),
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

    if let Some(source_hint) = contract
        .source
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        let exact_matches: Vec<SourceInfo> = queryable
            .iter()
            .filter(|source| {
                source_name_exact_matches(&source.name, source.view.as_deref(), source_hint)
            })
            .cloned()
            .collect();
        if !exact_matches.is_empty() {
            return match exact_matches.len() {
                1 => Ok(exact_matches.into_iter().next()),
                _ => Err(format!(
                    "source {source_hint:?} matched more than one table"
                )),
            };
        }

        let fuzzy_matches: Vec<SourceInfo> = queryable
            .into_iter()
            .filter(|source| source_name_matches(&source.name, source.view.as_deref(), source_hint))
            .collect();
        return match fuzzy_matches.len() {
            0 => Err(format!("could not find queryable source {source_hint:?}")),
            1 => Ok(fuzzy_matches.into_iter().next()),
            _ => Err(format!(
                "source {source_hint:?} matched more than one table"
            )),
        };
    }

    let requested = requested_fields(contract);

    if let Some(subject) = contract.subject.as_deref() {
        let subject = subject.trim();
        let exact_matches: Vec<SourceInfo> = queryable
            .iter()
            .filter(|source| {
                source_name_exact_matches(&source.name, source.view.as_deref(), subject)
            })
            .cloned()
            .collect();
        if !exact_matches.is_empty() {
            return match exact_matches.len() {
                1 => Ok(exact_matches.into_iter().next()),
                _ => Err(format!("source {subject:?} matched more than one table")),
            };
        }

        // A descriptive subject is not a source authority. Prefer the source
        // whose physical fields support the contract; this handles phrases
        // such as "reading list" -> books.rating and prevents "rent" from
        // winning merely because a table happens to be named rent_ledger.
        let field_matches: Vec<SourceInfo> = queryable
            .iter()
            .filter(|source| source_has_any_field(source, &requested))
            .cloned()
            .collect();
        if !field_matches.is_empty() {
            return match field_matches.len() {
                1 => Ok(field_matches.into_iter().next()),
                _ => Err(
                    "the requested fields occur in more than one table; specify the source".into(),
                ),
            };
        }

        let fuzzy_matches: Vec<SourceInfo> = queryable
            .into_iter()
            .filter(|source| source_name_matches(&source.name, source.view.as_deref(), subject))
            .collect();
        return match fuzzy_matches.len() {
            0 => Err(format!("could not find queryable source {subject:?}")),
            1 => Ok(fuzzy_matches.into_iter().next()),
            _ => Err(format!("source {subject:?} matched more than one table")),
        };
    }

    let matches: Vec<SourceInfo> = queryable
        .into_iter()
        .filter(|source| source_has_any_field(source, &requested))
        .collect();
    match matches.len() {
        1 => Ok(matches.into_iter().next()),
        0 => Ok(None),
        _ => Err("the requested fields occur in more than one table; specify the source".into()),
    }
}

fn requested_fields(contract: &AnalysisContract) -> Vec<String> {
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
    requested
}

fn resolve_field<'a>(source: &'a SourceInfo, requested: &str) -> Result<&'a ColumnInfo, String> {
    // Accept either the bare physical column name or a source-qualified name
    // emitted by a model. The source is already selected here, so strip a
    // prefix only when it actually identifies this source; dotted column
    // labels from other formats remain eligible for ordinary matching.
    let requested = requested
        .rsplit_once('.')
        .filter(|(hint, _)| {
            source.name.eq_ignore_ascii_case(hint)
                || source
                    .view
                    .as_deref()
                    .is_some_and(|view| view.eq_ignore_ascii_case(hint))
        })
        .map_or(requested, |(_, field)| field);
    let columns = source.columns.as_deref().unwrap_or_default();
    let normalized_requested = normalize(requested);
    let exact_matches: Vec<&ColumnInfo> = columns
        .iter()
        .filter(|column| normalize(&column.name) == normalized_requested)
        .collect();
    let matches: Vec<&ColumnInfo> = if exact_matches.is_empty() {
        columns
            .iter()
            .filter(|column| field_name_matches(&column.name, requested))
            .collect()
    } else {
        exact_matches
    };
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

/// Values emitted in `resolved_values` are still model input until a bounded
/// probe observes them. Treating them as candidate values lets a provider
/// repair a contract without repeating every hypothesis, while preserving the
/// trust boundary: only `probe_value` can put a value back into the resolved
/// output.
fn prepare_filter_candidates(filter: &mut ContractFilter) {
    let proposed = std::mem::take(&mut filter.resolved_values);
    filter.resolution = None;
    for value in proposed {
        if !filter.candidate_values.contains(&value) {
            filter.candidate_values.push(value);
        }
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
    use crate::engine::runtime::ContractMeasure;

    #[test]
    fn field_matching_ignores_case_and_separators() {
        assert_eq!(normalize("Order Amount"), "orderamount");
        assert_eq!(normalize("order_amount"), "orderamount");
    }

    #[test]
    fn context_source_applies_without_replacing_the_semantic_subject() {
        let mut contract = AnalysisContract {
            subject: Some("monthly rental total".into()),
            ..Default::default()
        };
        assert!(apply_context_source_hint(
            &mut contract,
            Some("selected_metrics_table")
        ));
        assert_eq!(contract.subject.as_deref(), Some("monthly rental total"));
        assert_eq!(contract.source.as_deref(), Some("selected_metrics_table"));

        assert!(!apply_context_source_hint(
            &mut contract,
            Some("another_source")
        ));
        assert_eq!(contract.source.as_deref(), Some("selected_metrics_table"));
    }

    #[test]
    fn non_text_probe_values_keep_their_json_value() {
        assert_eq!(value_string(&Json::from(12)), Some("12".into()));
        assert_eq!(value_string(&Json::from(true)), Some("true".into()));
        assert_eq!(value_string(&Json::Null), None);
    }

    #[test]
    fn source_matching_does_not_choose_the_first_fuzzy_candidate() {
        let first = SourceInfo {
            name: "Bank export current.csv".into(),
            path: "/tmp/current.csv".into(),
            kind: crate::engine::catalog::SourceKind::Csv,
            view: Some("bank_export_current".into()),
            row_count: None,
            columns: None,
            size_bytes: 0,
            mtime: 0,
            synopsis: None,
            note: None,
        };
        let second = SourceInfo {
            name: "Bank export archived.csv".into(),
            path: "/tmp/archived.csv".into(),
            kind: crate::engine::catalog::SourceKind::Csv,
            view: Some("bank_export_archived".into()),
            row_count: None,
            columns: None,
            size_bytes: 0,
            mtime: 0,
            synopsis: None,
            note: None,
        };
        let sources = vec![&first, &second];
        assert!(find_source(&sources, "bank export").is_none());
        assert_eq!(
            find_source(&sources, "bank_export_current").unwrap().name,
            first.name
        );
    }

    #[test]
    fn explicit_source_disambiguates_shared_fields() {
        let source = |name: &str, view: &str| SourceInfo {
            name: format!("{name}.csv"),
            path: format!("/tmp/{name}.csv"),
            kind: crate::engine::catalog::SourceKind::Csv,
            view: Some(view.into()),
            row_count: Some(1),
            columns: Some(vec![ColumnInfo::bare("rides", "BIGINT")]),
            size_bytes: 1,
            mtime: 1,
            synopsis: None,
            note: None,
        };
        let catalog = Catalog {
            sources: vec![
                source("daily", "daily"),
                source("monthly_usage_from_hourly", "monthly_usage_from_hourly"),
            ],
            ..Default::default()
        };
        let mut contract = AnalysisContract {
            measures: vec![ContractMeasure {
                concept: "rentals".into(),
                field: Some("rides".into()),
                operation: "sum".into(),
                unit: Some("rentals".into()),
            }],
            ..Default::default()
        };

        assert!(select_source(&catalog, &contract)
            .unwrap_err()
            .contains("occur in more than one table"));

        contract.source = Some("monthly_usage_from_hourly".into());
        let selected = select_source(&catalog, &contract)
            .unwrap()
            .expect("explicit source should select one of the matching tables");
        assert_eq!(selected.view.as_deref(), Some("monthly_usage_from_hourly"));
        assert_eq!(
            resolve_field(&selected, "monthly_usage_from_hourly.rides")
                .unwrap()
                .name,
            "rides"
        );
    }
}
