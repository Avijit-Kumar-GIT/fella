//! Deterministic compilation for the small, common analytical core.
//!
//! This compiler is deliberately conservative. It handles one grounded table,
//! read-only aggregates, filters, date ranges, typed time buckets, grouping,
//! guarded ratios, and conservative connected joins. Unsupported comparison
//! semantics remain on the model-driven fallback until they have a typed
//! representation and dedicated checks.

use crate::engine::analytics::data::{quote_ident, quote_str};
use crate::engine::catalog::{
    field_name_matches, source_has_any_field, source_name_exact_matches, source_name_matches,
    Catalog, ColumnInfo, SourceInfo,
};
use crate::engine::runtime::{
    AnalysisContract, ComparisonKind, ContractMeasure, DerivedMetricKind, InterpretationStatus,
    JoinKind, SortDirection, TimeBucket,
};

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
    if contract.comparison_spec.is_some() {
        return compile_period_comparison(catalog, contract, source_hint);
    }
    if !contract.joins.is_empty() {
        return compile_join(catalog, contract, source_hint);
    }
    let source = select_source(catalog, contract, source_hint)?;
    let view = source
        .view
        .as_deref()
        .ok_or_else(|| format!("{} is not queryable", source.name))?;

    let mut select = Vec::new();
    let mut steps = vec![format!("read from {view}")];
    for (index, measure) in contract.measures.iter().enumerate() {
        let expression = measure_expression(source, measure)?;
        let alias = format!("measure_{index}");
        select.push(format!("{expression} AS {}", quote_ident(&alias)));
        steps.push(format!("{} {}", measure.operation, measure.concept));
    }
    if select.is_empty() {
        return Err("the grounded contract has no measure to compute".into());
    }
    for (index, derived) in contract.derived_metrics.iter().enumerate() {
        let numerator = derived_expression(source, contract, &derived.numerator)?;
        let denominator = derived_expression(source, contract, &derived.denominator)?;
        let expression = match derived.kind {
            crate::engine::runtime::DerivedMetricKind::Ratio => {
                let ratio = format!("({numerator}) / NULLIF(({denominator}), 0)");
                if is_percentage_unit(derived.unit.as_deref()) {
                    format!("({ratio}) * 100.0")
                } else {
                    ratio
                }
            }
            crate::engine::runtime::DerivedMetricKind::Difference => {
                format!("({numerator}) - ({denominator})")
            }
        };
        let alias = format!("derived_{index}");
        select.push(format!("{expression} AS {}", quote_ident(&alias)));
        steps.push(format!(
            "{} {}",
            match derived.kind {
                DerivedMetricKind::Ratio => "ratio",
                DerivedMetricKind::Difference => "difference",
            },
            derived.concept
        ));
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
        let operator = if filter.exclude { "NOT IN" } else { "IN" };
        predicates.push(format!(
            "{} {operator} ({values})",
            quote_ident(&column.name)
        ));
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

fn compile_period_comparison(
    catalog: &Catalog,
    contract: &AnalysisContract,
    source_hint: Option<&str>,
) -> Result<CompiledPlan, String> {
    if !contract.joins.is_empty() {
        return Err("period comparison compilation with joins is not implemented yet".into());
    }
    let comparison = contract
        .comparison_spec
        .as_ref()
        .ok_or_else(|| "comparison spec is missing".to_string())?;
    if comparison.kind != ComparisonKind::PeriodOverPeriod {
        return Err("comparison kind is not supported by the compiler".into());
    }
    let source = select_source(catalog, contract, source_hint)?;
    let view = source
        .view
        .as_deref()
        .ok_or_else(|| format!("{} is not queryable", source.name))?;
    let time = contract
        .time
        .as_ref()
        .ok_or_else(|| "period comparison has no time field".to_string())?;
    if let Some(range) = time.range.as_deref() {
        if normalize(range) != normalize(&comparison.current_range) {
            return Err("comparison current_range conflicts with the contract time range".into());
        }
    }
    let time_field = time
        .field
        .as_deref()
        .ok_or_else(|| "period comparison has no grounded time field".to_string())?;
    let time_column = resolve_column(source, time_field)?;
    let current_predicate = time_predicate(&time_column.name, &comparison.current_range)?;
    let previous_predicate = time_predicate(&time_column.name, &comparison.previous_range)?;

    let mut select = Vec::new();
    let mut group = Vec::new();
    let mut steps = vec![format!(
        "compare {} with {}",
        comparison.current_range, comparison.previous_range
    )];
    let bucket_field = time.bucket.map(|_| time_field);
    if let Some(bucket) = time.bucket {
        let expression = comparison_time_bucket_expression(&time_column.name, bucket)?;
        let alias = format!("time_{}", bucket_name(bucket));
        select.push(format!("{expression} AS {}", quote_ident(&alias)));
        group.push(expression);
        steps.push(format!(
            "align {} buckets by {}",
            bucket_name(bucket),
            time_column.name
        ));
    }
    for field in &contract.group_by {
        if bucket_field.is_some_and(|bucket_field| normalize(bucket_field) == normalize(field)) {
            continue;
        }
        let column = resolve_column(source, field)?;
        let expression = quote_ident(&column.name);
        group.push(expression.clone());
        select.push(expression);
        steps.push(format!("group by {}", column.name));
    }

    for (index, measure) in contract.measures.iter().enumerate() {
        let current = comparison_measure_expression(source, measure, &current_predicate)?;
        let previous = comparison_measure_expression(source, measure, &previous_predicate)?;
        let current_alias = quote_ident(&format!("measure_{index}_current"));
        let previous_alias = quote_ident(&format!("measure_{index}_previous"));
        let change_alias = quote_ident(&format!("measure_{index}_change"));
        let percent_alias = quote_ident(&format!("measure_{index}_change_pct"));
        select.push(format!("{current} AS {current_alias}"));
        select.push(format!("{previous} AS {previous_alias}"));
        select.push(format!("(({current}) - ({previous})) AS {change_alias}"));
        select.push(format!(
            "((({current}) - ({previous})) / NULLIF(ABS({previous}), 0)) * 100 AS {percent_alias}"
        ));
        steps.push(format!("compare {} {}", measure.operation, measure.concept));
    }
    if contract.measures.is_empty() {
        return Err("the grounded comparison has no measure to compute".into());
    }
    for (index, derived) in contract.derived_metrics.iter().enumerate() {
        let current = comparison_derived_expression(source, contract, derived, &current_predicate)?;
        let previous =
            comparison_derived_expression(source, contract, derived, &previous_predicate)?;
        let current_alias = quote_ident(&format!("derived_{index}_current"));
        let previous_alias = quote_ident(&format!("derived_{index}_previous"));
        let change_alias = quote_ident(&format!("derived_{index}_change"));
        let percent_alias = quote_ident(&format!("derived_{index}_change_pct"));
        select.push(format!("{current} AS {current_alias}"));
        select.push(format!("{previous} AS {previous_alias}"));
        select.push(format!("(({current}) - ({previous})) AS {change_alias}"));
        select.push(format!(
            "((({current}) - ({previous})) / NULLIF(ABS({previous}), 0)) * 100 AS {percent_alias}"
        ));
        steps.push(format!(
            "compare {} {}",
            match derived.kind {
                DerivedMetricKind::Ratio => "ratio",
                DerivedMetricKind::Difference => "difference",
            },
            derived.concept
        ));
    }

    let mut predicates = Vec::new();
    for filter in &contract.filters {
        let field = filter
            .field
            .as_deref()
            .ok_or_else(|| format!("filter {:?} has no grounded field", filter.concept))?;
        let column = resolve_column(source, field)?;
        if filter.resolved_values.is_empty() {
            return Err(format!(
                "filter {:?} has no observed value to compile",
                filter.concept
            ));
        }
        let values = filter
            .resolved_values
            .iter()
            .map(|value| quote_str(value))
            .collect::<Vec<_>>()
            .join(", ");
        let operator = if filter.exclude { "NOT IN" } else { "IN" };
        predicates.push(format!(
            "{} {operator} ({values})",
            quote_ident(&column.name)
        ));
        steps.push(format!("filter {}", filter.concept));
    }

    let order_clause = contract
        .order_by
        .as_ref()
        .map(|order| {
            let expression = comparison_order_expression(source, contract, order)?;
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

fn comparison_measure_expression(
    source: &SourceInfo,
    measure: &ContractMeasure,
    predicate: &str,
) -> Result<String, String> {
    let operation = normalize_operation(&measure.operation)?;
    if operation == "COUNT" && measure.field.is_none() {
        return Ok(format!("SUM(CASE WHEN {predicate} THEN 1 ELSE 0 END)"));
    }
    let field = measure
        .field
        .as_deref()
        .ok_or_else(|| format!("measure {:?} has no grounded field", measure.concept))?;
    let column = resolve_column(source, field)?;
    let value = value_expression(column);
    Ok(match operation {
        "COUNT" => format!("COUNT(CASE WHEN {predicate} THEN {value} END)"),
        operation => format!("{operation}(CASE WHEN {predicate} THEN {value} END)"),
    })
}

fn comparison_derived_expression(
    source: &SourceInfo,
    contract: &AnalysisContract,
    derived: &crate::engine::runtime::ContractDerivedMetric,
    predicate: &str,
) -> Result<String, String> {
    let numerator = comparison_operand_expression(source, contract, &derived.numerator, predicate)?;
    let denominator =
        comparison_operand_expression(source, contract, &derived.denominator, predicate)?;
    match derived.kind {
        DerivedMetricKind::Ratio => {
            let ratio = format!("({numerator}) / NULLIF(({denominator}), 0)");
            if is_percentage_unit(derived.unit.as_deref()) {
                Ok(format!("({ratio}) * 100.0"))
            } else {
                Ok(ratio)
            }
        }
        DerivedMetricKind::Difference => Ok(format!("({numerator}) - ({denominator})")),
    }
}

fn comparison_operand_expression(
    source: &SourceInfo,
    contract: &AnalysisContract,
    requested: &str,
    predicate: &str,
) -> Result<String, String> {
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
    match matches.as_slice() {
        [measure] => comparison_measure_expression(source, measure, predicate),
        [] => Err(format!(
            "derived metric operand {:?} is not a declared measure",
            requested
        )),
        _ => Err(format!(
            "derived metric operand {:?} is ambiguous",
            requested
        )),
    }
}

fn comparison_order_expression(
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
        return Ok(quote_ident(&format!("measure_{index}_current")));
    }
    if measure_matches.len() > 1 {
        return Err(format!("order_by {:?} is ambiguous", order.by));
    }
    let derived_matches: Vec<_> = contract
        .derived_metrics
        .iter()
        .enumerate()
        .filter(|(index, derived)| {
            normalize(&derived.concept) == requested
                || normalize(&format!("derived_{index}")) == requested
        })
        .collect();
    if let [(index, _)] = derived_matches.as_slice() {
        return Ok(quote_ident(&format!("derived_{index}_current")));
    }
    if derived_matches.len() > 1 {
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
        "order_by {:?} must reference a comparison measure, derived metric, or grouping field",
        order.by
    ))
}

fn compile_join(
    catalog: &Catalog,
    contract: &AnalysisContract,
    source_hint: Option<&str>,
) -> Result<CompiledPlan, String> {
    let queryable: Vec<&SourceInfo> = catalog
        .sources
        .iter()
        .filter(|source| source.view.is_some())
        .collect();
    let primary_name = source_hint
        .or(contract.subject.as_deref())
        .or_else(|| contract.joins.first().map(|join| join.left_source.as_str()))
        .ok_or_else(|| "the join contract did not identify a primary source".to_string())?;
    let primary = find_join_source(&queryable, primary_name)
        .ok_or_else(|| format!("could not find source {primary_name:?}"))?;
    let mut sources = vec![primary];
    let mut aliases = vec!["t0".to_string()];
    let mut joins_sql = Vec::new();
    let mut steps = vec![format!(
        "read from {}",
        primary.view.as_deref().unwrap_or(&primary.name)
    )];

    for join in &contract.joins {
        let left_index = join_source_index(&sources, &join.left_source).ok_or_else(|| {
            format!(
                "join source {:?} is not connected to the primary source",
                join.left_source
            )
        })?;
        let right = find_join_source(&queryable, &join.right_source)
            .ok_or_else(|| format!("could not find join source {:?}", join.right_source))?;
        if join_source_index(&sources, &join.right_source).is_some() {
            return Err(format!(
                "join source {:?} appears more than once in the connected plan",
                join.right_source
            ));
        }
        let right_index = sources.len();
        sources.push(right);
        aliases.push(format!("t{right_index}"));
        // Join keys are resolved against their declared sides.  Keeping the
        // source qualifier here matters when both tables expose a conventional
        // `id` column; resolving the bare field against the whole scope would
        // incorrectly reject an otherwise unambiguous join as ambiguous.
        let left_reference = format!("{}.{}", join.left_source, join.left_field);
        let right_reference = format!("{}.{}", join.right_source, join.right_field);
        let left_expression = join_field_expression(&sources, &aliases, &left_reference)?;
        let right_expression = join_field_expression(&sources, &aliases, &right_reference)?;
        let keyword = match join.kind {
            JoinKind::Inner => "JOIN",
            JoinKind::Left => "LEFT JOIN",
        };
        let right_view = right
            .view
            .as_deref()
            .ok_or_else(|| format!("{} is not queryable", right.name))?;
        joins_sql.push(format!(
            "{keyword} {} AS {} ON {left_expression} = {right_expression}",
            quote_ident(right_view),
            quote_ident(&aliases[right_index])
        ));
        steps.push(format!(
            "{} {}.{} to {}.{}",
            match join.kind {
                JoinKind::Inner => "join",
                JoinKind::Left => "left join",
            },
            sources[left_index]
                .view
                .as_deref()
                .unwrap_or(&sources[left_index].name),
            join.left_field,
            right_view,
            join.right_field
        ));
    }

    let mut select = Vec::new();
    for (index, measure) in contract.measures.iter().enumerate() {
        let expression = join_measure_expression(&sources, &aliases, measure)?;
        select.push(format!(
            "{expression} AS {}",
            quote_ident(&format!("measure_{index}"))
        ));
        steps.push(format!("{} {}", measure.operation, measure.concept));
    }
    if select.is_empty() {
        return Err("the grounded contract has no measure to compute".into());
    }
    for (index, derived) in contract.derived_metrics.iter().enumerate() {
        let numerator = join_derived_expression(&sources, &aliases, contract, &derived.numerator)?;
        let denominator =
            join_derived_expression(&sources, &aliases, contract, &derived.denominator)?;
        let expression = match derived.kind {
            DerivedMetricKind::Ratio => {
                let ratio = format!("({numerator}) / NULLIF(({denominator}), 0)");
                if is_percentage_unit(derived.unit.as_deref()) {
                    format!("({ratio}) * 100.0")
                } else {
                    ratio
                }
            }
            DerivedMetricKind::Difference => format!("({numerator}) - ({denominator})"),
        };
        select.push(format!(
            "{expression} AS {}",
            quote_ident(&format!("derived_{index}"))
        ));
        steps.push(format!(
            "{} {}",
            match derived.kind {
                DerivedMetricKind::Ratio => "ratio",
                DerivedMetricKind::Difference => "difference",
            },
            derived.concept
        ));
    }

    let mut predicates = Vec::new();
    for filter in &contract.filters {
        let field = filter
            .field
            .as_deref()
            .ok_or_else(|| format!("filter {:?} has no grounded field", filter.concept))?;
        let expression = join_field_expression(&sources, &aliases, field)?;
        if filter.resolved_values.is_empty() {
            return Err(format!(
                "filter {:?} has no observed value to compile",
                filter.concept
            ));
        }
        let values = filter
            .resolved_values
            .iter()
            .map(|value| quote_str(value))
            .collect::<Vec<_>>()
            .join(", ");
        let operator = if filter.exclude { "NOT IN" } else { "IN" };
        predicates.push(format!("{expression} {operator} ({values})"));
        steps.push(format!("filter {}", filter.concept));
    }
    if let Some(time) = &contract.time {
        if let Some(range) = time.range.as_deref() {
            let field = time
                .field
                .as_deref()
                .ok_or_else(|| "time range has no grounded field".to_string())?;
            let expression = join_field_expression(&sources, &aliases, field)?;
            predicates.push(time_predicate_expression(&expression, range)?);
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
        let expression = join_field_expression(&sources, &aliases, field)?;
        let bucket_expression = time_bucket_expression_sql(&expression, bucket);
        let alias = format!("time_{}", bucket_name(bucket));
        select.insert(0, format!("{bucket_expression} AS {}", quote_ident(&alias)));
        group.push(bucket_expression);
        steps.push(format!("bucket {} by {field}", bucket_name(bucket)));
    }
    for field in &contract.group_by {
        if bucket_field
            .as_deref()
            .is_some_and(|bucket_field| normalize(bucket_field) == normalize(field))
        {
            continue;
        }
        let expression = join_field_expression(&sources, &aliases, field)?;
        group.push(expression.clone());
        select.insert(0, expression);
        steps.push(format!("group by {field}"));
    }

    let order_clause = contract
        .order_by
        .as_ref()
        .map(|order| {
            let expression = join_order_expression(&sources, &aliases, contract, order)?;
            let direction = match order.direction {
                SortDirection::Asc => "ASC",
                SortDirection::Desc => "DESC",
            };
            steps.push(format!("order by {} {direction}", order.by));
            Ok::<String, String>(format!("ORDER BY {expression} {direction}"))
        })
        .transpose()?;

    let primary_view = primary
        .view
        .as_deref()
        .ok_or_else(|| format!("{} is not queryable", primary.name))?;
    let mut sql = format!(
        "SELECT {} FROM {} AS {} {}",
        select.join(", "),
        quote_ident(primary_view),
        quote_ident(&aliases[0]),
        joins_sql.join(" ")
    );
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
        source: primary_view.to_string(),
        steps,
    })
}

fn find_join_source<'a>(sources: &[&'a SourceInfo], requested: &str) -> Option<&'a SourceInfo> {
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

fn join_source_index(sources: &[&SourceInfo], requested: &str) -> Option<usize> {
    let exact: Vec<usize> = sources
        .iter()
        .enumerate()
        .filter_map(|(index, source)| {
            source_name_exact_matches(&source.name, source.view.as_deref(), requested)
                .then_some(index)
        })
        .collect();
    if exact.len() == 1 {
        return exact.into_iter().next();
    }
    if !exact.is_empty() {
        return None;
    }
    let fuzzy: Vec<usize> = sources
        .iter()
        .enumerate()
        .filter_map(|(index, source)| {
            source_name_matches(&source.name, source.view.as_deref(), requested).then_some(index)
        })
        .collect();
    (fuzzy.len() == 1).then(|| fuzzy[0])
}

fn resolve_join_column<'a>(
    sources: &[&'a SourceInfo],
    field: &str,
) -> Result<(usize, &'a ColumnInfo), String> {
    let (source_hint, column_name) = field
        .rsplit_once('.')
        .map_or((None, field), |(source, column)| (Some(source), column));
    let candidates: Vec<(usize, &ColumnInfo)> = sources
        .iter()
        .enumerate()
        .filter(|(_, source)| {
            source_hint.is_none_or(|hint| {
                source.name.eq_ignore_ascii_case(hint)
                    || source
                        .view
                        .as_deref()
                        .is_some_and(|view| view.eq_ignore_ascii_case(hint))
            })
        })
        .flat_map(|(index, source)| {
            source
                .columns
                .as_deref()
                .unwrap_or_default()
                .iter()
                .filter(move |column| normalize(&column.name) == normalize(column_name))
                .map(move |column| (index, column))
        })
        .collect();
    match candidates.as_slice() {
        [(index, column)] => Ok((*index, *column)),
        [] => Err(format!("joined field {field:?} is not in the plan")),
        _ => Err(format!("joined field {field:?} is ambiguous")),
    }
}

fn join_field_expression(
    sources: &[&SourceInfo],
    aliases: &[String],
    field: &str,
) -> Result<String, String> {
    let (index, column) = resolve_join_column(sources, field)?;
    Ok(format!(
        "{}.{}",
        quote_ident(&aliases[index]),
        quote_ident(&column.name)
    ))
}

fn join_value_expression(
    sources: &[&SourceInfo],
    aliases: &[String],
    field: &str,
) -> Result<String, String> {
    let (index, column) = resolve_join_column(sources, field)?;
    let expression = format!(
        "{}.{}",
        quote_ident(&aliases[index]),
        quote_ident(&column.name)
    );
    if column
        .note
        .as_deref()
        .is_some_and(|note| note.contains("parse_num"))
    {
        Ok(format!("parse_num({expression})"))
    } else {
        Ok(expression)
    }
}

fn join_measure_expression(
    sources: &[&SourceInfo],
    aliases: &[String],
    measure: &ContractMeasure,
) -> Result<String, String> {
    let operation = normalize_operation(&measure.operation)?;
    if operation == "COUNT" && measure.field.is_none() {
        return Ok("COUNT(*)".into());
    }
    let field = measure
        .field
        .as_deref()
        .ok_or_else(|| format!("measure {:?} has no grounded field", measure.concept))?;
    Ok(format!(
        "{operation}({})",
        join_value_expression(sources, aliases, field)?
    ))
}

fn join_derived_expression(
    sources: &[&SourceInfo],
    aliases: &[String],
    contract: &AnalysisContract,
    requested: &str,
) -> Result<String, String> {
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
    match matches.as_slice() {
        [measure] => join_measure_expression(sources, aliases, measure),
        [] => Err(format!(
            "derived metric operand {:?} is not a declared measure",
            requested
        )),
        _ => Err(format!(
            "derived metric operand {:?} is ambiguous",
            requested
        )),
    }
}

fn join_order_expression(
    sources: &[&SourceInfo],
    aliases: &[String],
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
    let derived_matches: Vec<_> = contract
        .derived_metrics
        .iter()
        .enumerate()
        .filter(|(index, derived)| {
            normalize(&derived.concept) == requested
                || normalize(&format!("derived_{index}")) == requested
        })
        .collect();
    if let [(index, _)] = derived_matches.as_slice() {
        return Ok(quote_ident(&format!("derived_{index}")));
    }
    if derived_matches.len() > 1 {
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
        return join_field_expression(sources, aliases, &order.by);
    }
    Err(format!(
        "order_by {:?} must reference a grounded measure, derived metric, grouping field, or time bucket",
        order.by
    ))
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
        let exact_matches: Vec<&SourceInfo> = queryable
            .iter()
            .copied()
            .filter(|source| {
                source_name_exact_matches(&source.name, source.view.as_deref(), requested)
            })
            .collect();
        if exact_matches.len() == 1 {
            return Ok(exact_matches[0]);
        }
        if exact_matches.len() > 1 {
            return Err(format!("source {requested:?} is ambiguous"));
        }

        let requested_fields: Vec<String> = contract
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
        let field_matches: Vec<&SourceInfo> = queryable
            .iter()
            .copied()
            .filter(|source| source_has_any_field(source, &requested_fields))
            .collect();
        if !field_matches.is_empty() {
            return match field_matches.as_slice() {
                [source] => Ok(source),
                _ => Err("grounded fields occur in more than one source".into()),
            };
        }

        let fuzzy_matches: Vec<&SourceInfo> = queryable
            .into_iter()
            .filter(|source| source_name_matches(&source.name, source.view.as_deref(), requested))
            .collect();
        return match fuzzy_matches.as_slice() {
            [source] => Ok(source),
            [] => Err(format!("could not find source {requested:?}")),
            _ => Err(format!("source {requested:?} is ambiguous")),
        };
    }
    let requested_fields: Vec<String> = contract
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
    let matches: Vec<&SourceInfo> = queryable
        .into_iter()
        .filter(|source| source_has_any_field(source, &requested_fields))
        .collect();
    match matches.as_slice() {
        [source] => Ok(source),
        [] => Err("grounded fields did not identify a source".into()),
        _ => Err("grounded fields occur in more than one source".into()),
    }
}

fn resolve_column<'a>(source: &'a SourceInfo, field: &str) -> Result<&'a ColumnInfo, String> {
    let columns = source.columns.as_deref().unwrap_or_default();
    let requested = normalize(field);
    let exact: Vec<&ColumnInfo> = columns
        .iter()
        .filter(|column| normalize(&column.name) == requested)
        .collect();
    let matches: Vec<&ColumnInfo> = if exact.is_empty() {
        columns
            .iter()
            .filter(|column| field_name_matches(&column.name, field))
            .collect()
    } else {
        exact
    };
    match matches.as_slice() {
        [column] => Ok(column),
        [] => Err(format!("field {field:?} is not in {}", source.name)),
        _ => Err(format!("field {field:?} is ambiguous in {}", source.name)),
    }
}

fn measure_expression(
    source: &SourceInfo,
    measure: &crate::engine::runtime::ContractMeasure,
) -> Result<String, String> {
    let operation = normalize_operation(&measure.operation)?;
    if operation == "COUNT" && measure.field.is_none() {
        return Ok("COUNT(*)".to_string());
    }
    let field = measure
        .field
        .as_deref()
        .ok_or_else(|| format!("measure {:?} has no grounded field", measure.concept))?;
    let column = resolve_column(source, field)?;
    let value = value_expression(column);
    Ok(format!("{operation}({value})"))
}

fn derived_expression(
    source: &SourceInfo,
    contract: &AnalysisContract,
    requested: &str,
) -> Result<String, String> {
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
    match matches.as_slice() {
        [measure] => measure_expression(source, measure),
        [] => Err(format!(
            "derived metric operand {:?} is not a declared measure",
            requested
        )),
        _ => Err(format!(
            "derived metric operand {:?} is ambiguous",
            requested
        )),
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

    let derived_matches: Vec<_> = contract
        .derived_metrics
        .iter()
        .enumerate()
        .filter(|(index, derived)| {
            normalize(&derived.concept) == requested
                || normalize(&format!("derived_{index}")) == requested
        })
        .collect();
    if let [(index, _)] = derived_matches.as_slice() {
        return Ok(quote_ident(&format!("derived_{index}")));
    }
    if derived_matches.len() > 1 {
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

fn is_percentage_unit(unit: Option<&str>) -> bool {
    unit.is_some_and(|unit| {
        let unit = unit.to_ascii_lowercase();
        unit.contains('%') || unit.contains("percent") || unit.contains("percentage")
    })
}

pub(crate) fn time_predicate(field: &str, range: &str) -> Result<String, String> {
    time_predicate_expression(&quote_ident(field), range)
}

fn time_predicate_expression(field_sql: &str, range: &str) -> Result<String, String> {
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
            field_sql,
            quote_str(&format!("{year:04}-01-01")),
            field_sql,
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
            field_sql,
            quote_str(&format!("{year:04}-{month:02}-01")),
            field_sql,
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
        field_sql,
        quote_str(start.trim()),
        field_sql,
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
    time_bucket_expression_sql(&quote_ident(field), bucket)
}

fn time_bucket_expression_sql(field_sql: &str, bucket: TimeBucket) -> String {
    let format = match bucket {
        TimeBucket::Year => "%Y",
        TimeBucket::Month => "%Y-%m",
        TimeBucket::Week => "%Y-%W",
        TimeBucket::Day => "%Y-%m-%d",
    };
    if cfg!(feature = "duckdb") {
        format!("strftime({}, {})", field_sql, quote_str(format))
    } else {
        format!("strftime({}, {})", quote_str(format), field_sql)
    }
}

/// A period comparison needs a bucket key shared by both windows. A regular
/// month bucket (`%Y-%m`) would keep 2024-01 and 2023-01 separate; comparison
/// buckets deliberately align on month/week/day within the period.
fn comparison_time_bucket_expression(field: &str, bucket: TimeBucket) -> Result<String, String> {
    let format = match bucket {
        TimeBucket::Year => {
            return Err(
                "period comparisons cannot align a year bucket; omit the bucket or use month, week, or day".into(),
            )
        }
        TimeBucket::Month => "%m",
        TimeBucket::Week => "%W",
        TimeBucket::Day => "%m-%d",
    };
    let field_sql = quote_ident(field);
    Ok(if cfg!(feature = "duckdb") {
        format!("strftime({}, {})", field_sql, quote_str(format))
    } else {
        format!("strftime({}, {})", quote_str(format), field_sql)
    })
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
                        common_value_counts: None,
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
                        common_value_counts: None,
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
                        common_value_counts: None,
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

    fn join_catalog() -> Catalog {
        let mut catalog = catalog();
        let orders = &mut catalog.sources[0];
        orders.name = "orders.csv".into();
        orders.view = Some("orders".into());
        orders.columns = Some(vec![
            ColumnInfo::bare("customer_id", "INTEGER"),
            ColumnInfo::bare("amount", "REAL"),
        ]);
        catalog.sources.push(SourceInfo {
            name: "customers.csv".into(),
            path: "/tmp/ws/customers.csv".into(),
            kind: SourceKind::Csv,
            view: Some("customers".into()),
            row_count: Some(2),
            columns: Some(vec![
                ColumnInfo::bare("id", "INTEGER"),
                ColumnInfo::bare("segment", "TEXT"),
            ]),
            size_bytes: 1,
            mtime: 1,
            synopsis: None,
            note: None,
        });
        catalog
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
                exclude: false,
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

    #[test]
    fn compiles_a_guarded_ratio_and_can_rank_by_it() {
        let mut contract = contract();
        contract
            .measures
            .push(crate::engine::runtime::ContractMeasure {
                concept: "orders".into(),
                field: None,
                operation: "count".into(),
                unit: None,
            });
        contract.group_by = vec!["category".into()];
        contract.derived_metrics = vec![crate::engine::runtime::ContractDerivedMetric {
            concept: "revenue per order".into(),
            kind: crate::engine::runtime::DerivedMetricKind::Ratio,
            numerator: "revenue".into(),
            denominator: "orders".into(),
            unit: None,
        }];
        contract.order_by = Some(crate::engine::runtime::ContractOrder {
            by: "revenue per order".into(),
            direction: SortDirection::Desc,
        });
        contract.limit = Some(3);

        let plan = compile(&catalog(), &contract, Some("sales")).unwrap();

        assert!(plan
            .sql
            .contains("(SUM(\"amount\")) / NULLIF((COUNT(*)), 0) AS \"derived_0\""));
        assert!(plan.sql.contains("ORDER BY \"derived_0\" DESC"));
        assert!(plan.sql.ends_with("LIMIT 3"));
        assert!(plan
            .steps
            .iter()
            .any(|step| step == "ratio revenue per order"));
    }

    #[test]
    fn compiles_signed_exclusions_and_declared_derived_units() {
        let mut contract = contract();
        contract.filters[0].exclude = true;
        contract.filters[0].candidate_values = vec!["Income".into()];
        contract.filters[0].resolved_values = vec!["Income".into()];
        contract
            .measures
            .push(crate::engine::runtime::ContractMeasure {
                concept: "refunds".into(),
                field: Some("amount".into()),
                operation: "sum".into(),
                unit: None,
            });
        contract.derived_metrics = vec![
            crate::engine::runtime::ContractDerivedMetric {
                concept: "refund percentage".into(),
                kind: crate::engine::runtime::DerivedMetricKind::Ratio,
                numerator: "refunds".into(),
                denominator: "revenue".into(),
                unit: Some("percent".into()),
            },
            crate::engine::runtime::ContractDerivedMetric {
                concept: "net difference".into(),
                kind: crate::engine::runtime::DerivedMetricKind::Difference,
                numerator: "revenue".into(),
                denominator: "refunds".into(),
                unit: None,
            },
        ];

        let plan = compile(&catalog(), &contract, Some("sales")).unwrap();

        assert!(plan.sql.contains("\"category\" NOT IN ('Income')"));
        assert!(plan.sql.contains("* 100.0 AS \"derived_0\""));
        assert!(plan.sql.contains(") - (SUM(\"amount\")) AS \"derived_1\""));
        assert!(plan
            .steps
            .iter()
            .any(|step| step == "difference net difference"));
    }

    #[test]
    fn compiles_a_typed_period_comparison_with_safe_change_columns() {
        let mut contract = contract();
        contract.group_by = vec!["category".into()];
        contract.comparison_spec = Some(crate::engine::runtime::ContractComparison {
            kind: ComparisonKind::PeriodOverPeriod,
            current_range: "2024".into(),
            previous_range: "2023".into(),
        });
        contract.limit = Some(12);

        let plan = compile(&catalog(), &contract, Some("sales")).unwrap();

        assert!(plan.sql.contains("SUM(CASE WHEN \"month\" >= '2024-01-01'"));
        assert!(plan.sql.contains("SUM(CASE WHEN \"month\" >= '2023-01-01'"));
        assert!(plan.sql.contains("AS \"measure_0_current\""));
        assert!(plan.sql.contains("AS \"measure_0_previous\""));
        assert!(plan.sql.contains("AS \"measure_0_change\""));
        assert!(plan.sql.contains("AS \"measure_0_change_pct\""));
        assert!(plan.sql.contains("GROUP BY \"category\""));
        assert!(plan.sql.ends_with("LIMIT 12"));
    }

    #[test]
    fn compiles_a_period_comparison_with_aligned_month_buckets() {
        let mut contract = contract();
        contract.filters.clear();
        contract.group_by.clear();
        contract.time.as_mut().unwrap().bucket = Some(TimeBucket::Month);
        contract.comparison_spec = Some(crate::engine::runtime::ContractComparison {
            kind: ComparisonKind::PeriodOverPeriod,
            current_range: "2024".into(),
            previous_range: "2023".into(),
        });
        contract.order_by = Some(crate::engine::runtime::ContractOrder {
            by: "time_month".into(),
            direction: SortDirection::Asc,
        });

        let plan = compile(&catalog(), &contract, Some("sales")).unwrap();

        assert!(plan
            .sql
            .contains("strftime('%m', \"month\") AS \"time_month\""));
        assert!(plan.sql.contains("GROUP BY strftime('%m', \"month\")"));
        assert!(plan.sql.contains("AS \"measure_0_current\""));
        assert!(plan.sql.contains("AS \"measure_0_previous\""));
        assert!(plan.sql.contains("ORDER BY \"time_month\" ASC"));
        assert!(plan
            .steps
            .iter()
            .any(|step| step == "align month buckets by month"));

        contract.time.as_mut().unwrap().bucket = Some(TimeBucket::Year);
        assert!(compile(&catalog(), &contract, Some("sales"))
            .unwrap_err()
            .contains("cannot align a year bucket"));
    }

    #[test]
    fn compiles_a_period_comparison_with_a_guarded_derived_metric() {
        let mut contract = contract();
        contract.filters.clear();
        contract.group_by.clear();
        contract
            .measures
            .push(crate::engine::runtime::ContractMeasure {
                concept: "orders".into(),
                field: None,
                operation: "count".into(),
                unit: None,
            });
        contract.derived_metrics = vec![crate::engine::runtime::ContractDerivedMetric {
            concept: "revenue per order".into(),
            kind: DerivedMetricKind::Ratio,
            numerator: "revenue".into(),
            denominator: "orders".into(),
            unit: None,
        }];
        contract.comparison_spec = Some(crate::engine::runtime::ContractComparison {
            kind: ComparisonKind::PeriodOverPeriod,
            current_range: "2024".into(),
            previous_range: "2023".into(),
        });
        contract.time.as_mut().unwrap().range = None;
        contract.order_by = Some(crate::engine::runtime::ContractOrder {
            by: "revenue per order".into(),
            direction: SortDirection::Desc,
        });

        let plan = compile(&catalog(), &contract, Some("sales")).unwrap();

        assert!(plan.sql.contains("AS \"derived_0_current\""));
        assert!(plan.sql.contains("AS \"derived_0_previous\""));
        assert!(plan.sql.contains("AS \"derived_0_change_pct\""));
        assert!(plan.sql.contains("NULLIF((SUM(CASE WHEN"));
        assert!(plan.sql.contains("ORDER BY \"derived_0_current\" DESC"));
        assert!(plan
            .steps
            .iter()
            .any(|step| step == "compare ratio revenue per order"));
    }

    #[test]
    fn compiles_a_grounded_left_join_with_qualified_fields() {
        let contract = AnalysisContract {
            interpretation: InterpretationStatus::Grounded,
            subject: Some("orders".into()),
            measures: vec![crate::engine::runtime::ContractMeasure {
                concept: "revenue".into(),
                field: Some("orders.amount".into()),
                operation: "sum".into(),
                unit: None,
            }],
            group_by: vec!["customers.segment".into()],
            joins: vec![crate::engine::runtime::ContractJoin {
                left_source: "orders".into(),
                left_field: "customer_id".into(),
                right_source: "customers".into(),
                right_field: "id".into(),
                kind: JoinKind::Left,
            }],
            ..Default::default()
        };

        let plan = compile(&join_catalog(), &contract, Some("orders")).unwrap();

        assert!(plan.sql.contains("FROM \"orders\" AS \"t0\""));
        assert!(plan.sql.contains(
            "LEFT JOIN \"customers\" AS \"t1\" ON \"t0\".\"customer_id\" = \"t1\".\"id\""
        ));
        assert!(plan.sql.contains("SUM(\"t0\".\"amount\")"));
        assert!(plan.sql.contains("\"t1\".\"segment\""));
        assert!(plan.sql.contains("GROUP BY \"t1\".\"segment\""));
    }
}
