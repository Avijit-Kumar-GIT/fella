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
use crate::engine::runtime::{AnalysisContract, InterpretationStatus};
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
        probes: Vec::new(),
        unresolved: contract.unresolved.clone(),
    };

    if contract.interpretation == InterpretationStatus::Unsupported {
        return GroundingResult { contract, report };
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
