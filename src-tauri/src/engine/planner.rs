//! Deterministic compilation for the small, common analytical core.
//!
//! This compiler is deliberately conservative. It handles one grounded table,
//! read-only aggregates, filters, date ranges, typed time buckets, and
//! grouping. Ratios, joins, and unsupported comparison semantics remain on the
//! model-driven fallback until they have a typed representation and dedicated
//! checks.

use crate::engine::analytics::data::{quote_ident, quote_str};
use crate::engine::catalog::{Catalog, ColumnInfo, SourceInfo};
use crate::engine::runtime::{AnalysisContract, InterpretationStatus, SortDirection, TimeBucket};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledPlan {
    pub sql: String,
    pub source: String,
    pub steps: Vec<String>,
}

pub fn compile(
    catalog: &Catalog,
    contract: &AnalysisContract,
    source_hint: Option<&str>,
) -> Result<CompiledPlan, String> {
    if contract.interpretation != InterpretationStatus::Grounded {
        return Err("only a grounded contract can be compiled".into());
    }
    if contract.comparison.is_some() {
        return Err("comparison compilation is not implemented yet".into());
    }
    let source = select_source(catalog, contract, source_hint)?;
    let view = source
        .view
        .as_deref()
        .ok_or_else(|| format!("{} is not queryable", source.name))?;

    let mut select = Vec::new();
    let mut steps = vec![format!("read from {view}")];
    for (index, measure) in contract.measures.iter().enumerate() {
        let operation = normalize_operation(&measure.operation)?;
        let expression = if operation == "count" && measure.field.is_none() {
            "COUNT(*)".to_string()
        } else {
            let field = measure
                .field
                .as_deref()
                .ok_or_else(|| format!("measure {:?} has no grounded field", measure.concept))?;
            let column = resolve_column(source, field)?;
            let value = value_expression(column);
            format!("{operation}({value})")
        };
        let alias = format!("measure_{index}");
        select.push(format!("{expression} AS {}", quote_ident(&alias)));
        steps.push(format!("{operation} {}", measure.concept));
    }
    if select.is_empty() {
        return Err("the grounded contract has no measure to compute".into());
    }

    let mut predicates = Vec::new();
    for filter in &contract.filters {
        let field = filter
            .field
            .as_deref()
            .ok_or_else(|| format!("filter {:?} has no grounded field", filter.concept))?;
        let column = resolve_column(source, field)?;
        let values = if filter.resolved_values.is_empty() {
            if filter.candidate_values.is_empty() {
                return Err(format!("filter {:?} has no value", filter.concept));
            }
            return Err(format!(
                "filter {:?} has no observed value to compile",
                filter.concept
            ));
        } else {
            &filter.resolved_values
        };
        let values = values
            .iter()
            .map(|value| quote_str(value))
            .collect::<Vec<_>>()
            .join(", ");
        predicates.push(format!("{} IN ({values})", quote_ident(&column.name)));
        steps.push(format!("filter {}", filter.concept));
    }

    if let Some(time) = &contract.time {
        if let Some(range) = time.range.as_deref() {
            let field = time
                .field
                .as_deref()
                .ok_or_else(|| "time range has no grounded field".to_string())?;
            let column = resolve_column(source, field)?;
            predicates.push(time_predicate(&column.name, range)?);
            steps.push(format!("filter time {range}"));
        }
    }

    let mut group = Vec::new();
    let bucket_field = contract
        .time
        .as_ref()
        .and_then(|time| time.bucket.map(|_| time.field.clone()))
        .flatten();
    if let Some(bucket) = contract.time.as_ref().and_then(|time| time.bucket) {
        let field = contract
            .time
            .as_ref()
            .and_then(|time| time.field.as_deref())
            .ok_or_else(|| "time bucket has no grounded field".to_string())?;
        let column = resolve_column(source, field)?;
        let expression = time_bucket_expression(&column.name, bucket);
        let alias = format!("time_{}", bucket_name(bucket));
        select.insert(0, format!("{expression} AS {}", quote_ident(&alias)));
        group.push(expression);
        steps.push(format!("bucket {} by {}", bucket_name(bucket), column.name));
    }
    for field in &contract.group_by {
        if bucket_field
            .as_deref()
            .is_some_and(|bucket_field| normalize(bucket_field) == normalize(field))
        {
            continue;
        }
        let column = resolve_column(source, field)?;
        group.push(quote_ident(&column.name));
        select.insert(0, quote_ident(&column.name));
        steps.push(format!("group by {}", column.name));
    }

    let order_clause = contract
        .order_by
        .as_ref()
        .map(|order| {
            let expression = order_expression(source, contract, order)?;
            let direction = match order.direction {
                SortDirection::Asc => "ASC",
                SortDirection::Desc => "DESC",
            };
            steps.push(format!("order by {} {direction}", order.by));
            Ok::<String, String>(format!("ORDER BY {expression} {direction}"))
        })
        .transpose()?;

    let mut sql = format!("SELECT {} FROM {}", select.join(", "), quote_ident(view));
    if !predicates.is_empty() {
        sql.push_str(" WHERE ");
        sql.push_str(&predicates.join(" AND "));
    }
    if !group.is_empty() {
        sql.push_str(" GROUP BY ");
        sql.push_str(&group.join(", "));
    }
    if let Some(order_clause) = order_clause {
        sql.push(' ');
        sql.push_str(&order_clause);
    }
    sql.push_str(&format!(" LIMIT {}", contract.limit.unwrap_or(1000)));

    Ok(CompiledPlan {
        sql,
        source: view.to_string(),
        steps,
    })
}

fn select_source<'a>(
    catalog: &'a Catalog,
    contract: &AnalysisContract,
    source_hint: Option<&str>,
) -> Result<&'a SourceInfo, String> {
    let queryable: Vec<&SourceInfo> = catalog
        .sources
        .iter()
        .filter(|source| source.view.is_some())
        .collect();
    let requested_source = source_hint.or(contract.subject.as_deref());
    if let Some(requested) = requested_source {
        let matches: Vec<&SourceInfo> = queryable
            .into_iter()
            .filter(|source| {
                source.name.eq_ignore_ascii_case(requested)
                    || source
                        .view
                        .as_deref()
                        .is_some_and(|view| view.eq_ignore_ascii_case(requested))
            })
            .collect();
        return match matches.as_slice() {
            [source] => Ok(source),
            [] => Err(format!("could not find source {requested:?}")),
            _ => Err(format!("source {requested:?} is ambiguous")),
        };
    }
    let requested_fields: Vec<&str> = contract
        .measures
        .iter()
        .filter_map(|measure| measure.field.as_deref())
        .chain(
            contract
                .filters
                .iter()
                .filter_map(|filter| filter.field.as_deref()),
        )
        .chain(
            contract
                .time
                .iter()
                .filter_map(|time| time.field.as_deref()),
        )
        .chain(contract.group_by.iter().map(String::as_str))
        .collect();
    let matches: Vec<&SourceInfo> = queryable
        .into_iter()
        .filter(|source| {
            requested_fields.iter().any(|field| {
                source
                    .columns
                    .as_deref()
                    .unwrap_or_default()
                    .iter()
                    .any(|column| normalize(&column.name) == normalize(field))
            })
        })
        .collect();
    match matches.as_slice() {
        [source] => Ok(source),
        [] => Err("grounded fields did not identify a source".into()),
        _ => Err("grounded fields occur in more than one source".into()),
    }
}

fn resolve_column<'a>(source: &'a SourceInfo, field: &str) -> Result<&'a ColumnInfo, String> {
    let matches: Vec<&ColumnInfo> = source
        .columns
        .as_deref()
        .unwrap_or_default()
        .iter()
        .filter(|column| normalize(&column.name) == normalize(field))
        .collect();
    match matches.as_slice() {
        [column] => Ok(column),
        [] => Err(format!("field {field:?} is not in {}", source.name)),
        _ => Err(format!("field {field:?} is ambiguous in {}", source.name)),
    }
}

fn order_expression(
    source: &SourceInfo,
    contract: &AnalysisContract,
    order: &crate::engine::runtime::ContractOrder,
) -> Result<String, String> {
    let requested = normalize(&order.by);
    let measure_matches: Vec<_> = contract
        .measures
        .iter()
        .enumerate()
        .filter(|(_, measure)| {
            normalize(&measure.concept) == requested
                || measure
                    .field
                    .as_deref()
                    .is_some_and(|field| normalize(field) == requested)
        })
        .collect();
    if let [(index, _)] = measure_matches.as_slice() {
        return Ok(quote_ident(&format!("measure_{index}")));
    }
    if measure_matches.len() > 1 {
        return Err(format!("order_by {:?} is ambiguous", order.by));
    }

    if let Some(bucket) = contract.time.as_ref().and_then(|time| time.bucket) {
        let alias = format!("time_{}", bucket_name(bucket));
        if normalize(&alias) == requested {
            return Ok(quote_ident(&alias));
        }
    }

    if contract
        .group_by
        .iter()
        .any(|field| normalize(field) == requested)
    {
        let column = resolve_column(source, &order.by)?;
        return Ok(quote_ident(&column.name));
    }

    Err(format!(
        "order_by {:?} must reference a grounded measure, grouping field, or time bucket",
        order.by
    ))
}

fn value_expression(column: &ColumnInfo) -> String {
    if column
        .note
        .as_deref()
        .is_some_and(|note| note.contains("parse_num"))
    {
        format!("parse_num({})", quote_ident(&column.name))
    } else {
        quote_ident(&column.name)
    }
}

fn normalize_operation(operation: &str) -> Result<&'static str, String> {
    match operation.trim().to_ascii_lowercase().as_str() {
        "sum" | "total" => Ok("SUM"),
        "avg" | "average" | "mean" => Ok("AVG"),
        "count" | "number" => Ok("COUNT"),
        "min" | "minimum" => Ok("MIN"),
        "max" | "maximum" => Ok("MAX"),
        other => Err(format!(
            "operation {other:?} is not supported by the compiler"
        )),
    }
}

fn time_predicate(field: &str, range: &str) -> Result<String, String> {
    let range = range.trim();
    if range.len() == 4 && range.chars().all(|character| character.is_ascii_digit()) {
        let year: i32 = range
            .parse()
            .map_err(|_| "invalid year range".to_string())?;
        if !(1900..=2100).contains(&year) {
            return Err(format!("unsupported year range {range:?}"));
        }
        return Ok(format!(
            "{} >= {} AND {} < {}",
            quote_ident(field),
            quote_str(&format!("{year:04}-01-01")),
            quote_ident(field),
            quote_str(&format!("{:04}-01-01", year + 1))
        ));
    }
    if range.len() == 7
        && range.as_bytes()[4] == b'-'
        && range[..4]
            .chars()
            .all(|character| character.is_ascii_digit())
        && range[5..]
            .chars()
            .all(|character| character.is_ascii_digit())
    {
        let year: i32 = range[..4]
            .parse()
            .map_err(|_| "invalid month range".to_string())?;
        let month: u32 = range[5..]
            .parse()
            .map_err(|_| "invalid month range".to_string())?;
        if !(1900..=2100).contains(&year) || !(1..=12).contains(&month) {
            return Err(format!("unsupported month range {range:?}"));
        }
        let (next_year, next_month) = if month == 12 {
            (year + 1, 1)
        } else {
            (year, month + 1)
        };
        return Ok(format!(
            "{} >= {} AND {} < {}",
            quote_ident(field),
            quote_str(&format!("{year:04}-{month:02}-01")),
            quote_ident(field),
            quote_str(&format!("{next_year:04}-{next_month:02}-01"))
        ));
    }
    let (start, end) = range
        .split_once("..")
        .or_else(|| range.split_once(" to "))
        .ok_or_else(|| format!("unsupported time range {range:?}"))?;
    if !is_iso_date(start.trim()) || !is_iso_date(end.trim()) {
        return Err(format!("unsupported time range {range:?}"));
    }
    Ok(format!(
        "{} >= {} AND {} <= {}",
        quote_ident(field),
        quote_str(start.trim()),
        quote_ident(field),
        quote_str(end.trim())
    ))
}

fn bucket_name(bucket: TimeBucket) -> &'static str {
    match bucket {
        TimeBucket::Year => "year",
        TimeBucket::Month => "month",
        TimeBucket::Week => "week",
        TimeBucket::Day => "day",
    }
}

fn time_bucket_expression(field: &str, bucket: TimeBucket) -> String {
    let format = match bucket {
        TimeBucket::Year => "%Y",
        TimeBucket::Month => "%Y-%m",
        TimeBucket::Week => "%Y-%W",
        TimeBucket::Day => "%Y-%m-%d",
    };
    if cfg!(feature = "duckdb") {
        format!("strftime({}, {})", quote_ident(field), quote_str(format))
    } else {
        format!("strftime({}, {})", quote_str(format), quote_ident(field))
    }
}

fn is_iso_date(value: &str) -> bool {
    value.len() == 10
        && value.as_bytes()[4] == b'-'
        && value.as_bytes()[7] == b'-'
        && value
            .chars()
            .enumerate()
            .all(|(index, character)| matches!(index, 4 | 7) || character.is_ascii_digit())
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
    use crate::engine::catalog::{Catalog, ColumnInfo, SourceKind};

    fn catalog() -> Catalog {
        Catalog {
            workspace: Some("/tmp/ws".into()),
            revision: Some("r1".into()),
            indexed_at_ms: None,
            sources: vec![SourceInfo {
                name: "sales.csv".into(),
                path: "/tmp/ws/sales.csv".into(),
                kind: SourceKind::Csv,
                view: Some("sales".into()),
                row_count: Some(3),
                columns: Some(vec![
                    ColumnInfo {
                        name: "month".into(),
                        type_: "TEXT".into(),
                        null_fraction: None,
                        distinct: None,
                        min: None,
                        max: None,
                        example: None,
                        common_values: None,
                        note: None,
                    },
                    ColumnInfo {
                        name: "amount".into(),
                        type_: "REAL".into(),
                        null_fraction: None,
                        distinct: None,
                        min: None,
                        max: None,
                        example: None,
                        common_values: None,
                        note: None,
                    },
                    ColumnInfo {
                        name: "category".into(),
                        type_: "TEXT".into(),
                        null_fraction: None,
                        distinct: None,
                        min: None,
                        max: None,
                        example: None,
                        common_values: None,
                        note: None,
                    },
                ]),
                size_bytes: 1,
                mtime: 1,
                synopsis: None,
                note: None,
            }],
            skipped: Vec::new(),
        }
    }

    fn contract() -> AnalysisContract {
        AnalysisContract {
            interpretation: InterpretationStatus::Grounded,
            subject: Some("sales".into()),
            measures: vec![crate::engine::runtime::ContractMeasure {
                concept: "revenue".into(),
                field: Some("amount".into()),
                operation: "sum".into(),
                unit: None,
            }],
            filters: vec![crate::engine::runtime::ContractFilter {
                concept: "category".into(),
                field: Some("category".into()),
                candidate_values: vec!["Dining".into()],
                resolved_values: vec!["Dining".into()],
                resolution: Some("observed".into()),
            }],
            time: Some(crate::engine::runtime::ContractTime {
                field: Some("month".into()),
                range: Some("2024".into()),
                bucket: None,
                timezone: None,
            }),
            group_by: vec!["month".into()],
            ..Default::default()
        }
    }

    #[test]
    fn compiles_a_grounded_aggregate_with_filters_and_time() {
        let plan = compile(&catalog(), &contract(), Some("sales")).unwrap();
        assert_eq!(plan.source, "sales");
        assert!(plan.sql.contains("SUM(\"amount\")"));
        assert!(plan.sql.contains("\"category\" IN ('Dining')"));
        assert!(plan.sql.contains("GROUP BY \"month\""));
        assert!(plan.sql.contains("2024-01-01"));
    }

    #[test]
    fn refuses_unresolved_or_unsupported_contracts() {
        let mut unresolved = contract();
        unresolved.interpretation = InterpretationStatus::Ambiguous;
        assert!(compile(&catalog(), &unresolved, Some("sales")).is_err());
        let mut comparison = contract();
        comparison.comparison = Some("year over year".into());
        assert!(compile(&catalog(), &comparison, Some("sales")).is_err());
    }

    #[test]
    fn compiles_a_typed_month_bucket_without_grouping_by_raw_dates() {
        let mut contract = contract();
        contract.time.as_mut().unwrap().bucket = Some(TimeBucket::Month);
        let plan = compile(&catalog(), &contract, Some("sales")).unwrap();

        assert!(plan
            .sql
            .contains("strftime('%Y-%m', \"month\") AS \"time_month\""));
        assert!(plan.sql.contains("GROUP BY strftime('%Y-%m', \"month\")"));
        assert!(!plan.sql.contains("GROUP BY \"month\""));
        assert!(plan
            .steps
            .iter()
            .any(|step| step == "bucket month by month"));
    }

    #[test]
    fn compiles_a_grounded_top_n_ordered_by_measure() {
        let mut contract = contract();
        contract.group_by = vec!["category".into()];
        contract.order_by = Some(crate::engine::runtime::ContractOrder {
            by: "revenue".into(),
            direction: SortDirection::Desc,
        });
        contract.limit = Some(3);

        let plan = compile(&catalog(), &contract, Some("sales")).unwrap();

        assert!(plan.sql.contains("ORDER BY \"measure_0\" DESC"));
        assert!(plan.sql.ends_with("LIMIT 3"));
        assert!(plan
            .steps
            .iter()
            .any(|step| step == "order by revenue DESC"));
    }
}
