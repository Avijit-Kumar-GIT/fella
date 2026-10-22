//! Bounded, deterministic context for model-directed analytical presentation.
//!
//! Profiles are visual-design hints over saved rows, not analytical evidence.
//! They never replace the canonical result or authorize a computation.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value as Json};

use super::artifacts::BoundResult;
use super::error::{EngineError, EngineResult};

const PROFILE_RESULTS: usize = 16;
const PROFILE_ROWS: usize = 1_024;
const REPRESENTATIVE_ROWS: usize = 8;
const PROFILE_FIELDS: usize = 32;
const PROFILE_EXAMPLES: usize = 6;
const MAX_CELL_CHARS: usize = 120;
const MAX_PLAN_BYTES: usize = 8 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PresentationDesignPlan {
    /// Free-form task label; this is intentionally not a closed visualization enum.
    pub analytical_intent: String,
    /// The central point the view should make easy to see, without inventing results.
    pub visual_thesis: String,
    /// Free-form primary form, not a required component or chart type.
    pub primary_form: String,
    #[serde(default)]
    pub supporting_elements: Vec<String>,
    #[serde(default)]
    pub interaction: Option<String>,
    pub why_this_fits: String,
    #[serde(default)]
    pub avoid: Vec<String>,
}

impl PresentationDesignPlan {
    pub fn validate(&self) -> EngineResult<()> {
        for (name, value, max) in [
            ("analytical_intent", &self.analytical_intent, 160),
            ("visual_thesis", &self.visual_thesis, 400),
            ("primary_form", &self.primary_form, 200),
            ("why_this_fits", &self.why_this_fits, 400),
        ] {
            if value.trim().is_empty()
                || value.chars().count() > max
                || value.chars().any(char::is_control)
            {
                return Err(EngineError::msg(format!(
                    "presentation design plan has an invalid {name}"
                )));
            }
        }
        if self.supporting_elements.len() > 4
            || self.avoid.len() > 5
            || self
                .supporting_elements
                .iter()
                .chain(self.avoid.iter())
                .any(|value| {
                    value.trim().is_empty()
                        || value.chars().count() > 180
                        || value.chars().any(char::is_control)
                })
            || self.interaction.as_ref().is_some_and(|value| {
                value.chars().count() > 180 || value.chars().any(char::is_control)
            })
        {
            return Err(EngineError::msg(
                "presentation design plan has invalid supporting guidance",
            ));
        }
        Ok(())
    }
}

/// Parse a bounded JSON-only plan. A JSON code fence is tolerated; prose is not.
pub fn parse_design_plan(response: &str) -> EngineResult<PresentationDesignPlan> {
    let response = response.trim();
    if response.len() > MAX_PLAN_BYTES {
        return Err(EngineError::msg(
            "presentation design plan exceeds its budget",
        ));
    }
    let json = if response.starts_with("```") {
        let (_, body) = response
            .split_once('\n')
            .ok_or_else(|| EngineError::msg("presentation design plan is not valid JSON"))?;
        body.strip_suffix("```").unwrap_or(body).trim()
    } else {
        response
    };
    let plan: PresentationDesignPlan = serde_json::from_str(json)
        .map_err(|_| EngineError::msg("presentation design plan is not valid JSON"))?;
    plan.validate()?;
    Ok(plan)
}

/// Validate a renderer-supplied plan before reusing it for a bounded repair pass.
/// It is only prompt context; it does not grant data or execution authority.
pub fn validate_design_plan_value(value: Json) -> EngineResult<PresentationDesignPlan> {
    let plan: PresentationDesignPlan = serde_json::from_value(value)
        .map_err(|_| EngineError::msg("presentation design plan has an invalid shape"))?;
    plan.validate()?;
    Ok(plan)
}

/// Build compact profiles and representative rows from already-authorized saved results.
pub fn profile_results(results: &[BoundResult]) -> Json {
    json!({
        "results": results.iter().take(PROFILE_RESULTS).map(profile_result).collect::<Vec<_>>(),
        "omitted_result_count": results.len().saturating_sub(PROFILE_RESULTS),
        "max_results_profiled": PROFILE_RESULTS,
        "max_rows_sampled_per_result": PROFILE_ROWS,
        "max_fields_profiled_per_result": PROFILE_FIELDS
    })
}

fn profile_result(result: &BoundResult) -> Json {
    let profile_indices = sample_indices(result.rows.len(), PROFILE_ROWS);
    let selected_fields = result.columns.len().min(PROFILE_FIELDS);
    let fields = (0..selected_fields)
        .map(|column| profile_field(result, column, &profile_indices))
        .collect::<Vec<_>>();
    let representative_rows = sample_indices(result.rows.len(), REPRESENTATIVE_ROWS)
        .iter()
        .map(|row_index| {
            let values = (0..selected_fields)
                .map(|column| {
                    let role = fields[column]["inferred_role"]
                        .as_str()
                        .unwrap_or("unknown");
                    let omission_reason = if identifier_name(&result.columns[column]) {
                        Some("identifier")
                    } else if matches!(role, "identifier" | "high_cardinality_label" | "long_text")
                    {
                        Some(role)
                    } else {
                        None
                    };
                    let value = if let Some(reason) = omission_reason {
                        format!("[omitted: {reason}]")
                    } else {
                        result
                            .exact_rows
                            .get(*row_index)
                            .and_then(|row| row.get(column))
                            .cloned()
                            .or_else(|| {
                                result
                                    .rows
                                    .get(*row_index)
                                    .and_then(|row| row.get(column))
                                    .map(Json::to_string)
                            })
                            .unwrap_or_else(|| "null".to_owned())
                    };
                    (
                        result.columns[column].clone(),
                        Json::String(truncate(&value, MAX_CELL_CHARS)),
                    )
                })
                .collect::<serde_json::Map<_, _>>();
            json!({ "saved_row_index": row_index, "values": values })
        })
        .collect::<Vec<_>>();
    json!({
        "result_id": result.id,
        "reported_row_count": result.row_count,
        "stored_row_count": result.rows.len(),
        "complete": result.complete,
        "profiled_saved_rows": profile_indices.len(),
        "columns_omitted": result.columns.len().saturating_sub(selected_fields),
        "fields": fields,
        "representative_rows": representative_rows,
        "profile_is_design_guidance_not_analysis": true
    })
}

fn profile_field(result: &BoundResult, column: usize, indices: &[usize]) -> Json {
    let name = &result.columns[column];
    let mut non_null = 0usize;
    let mut numeric = Vec::new();
    let mut strings = 0usize;
    let mut booleans = 0usize;
    let mut temporal = 0usize;
    let mut total_string_chars = 0usize;
    let mut distinct = BTreeSet::new();
    let mut frequencies = BTreeMap::<String, usize>::new();
    let mut ordered_values = Vec::new();

    for row_index in indices {
        let Some(value) = result.rows.get(*row_index).and_then(|row| row.get(column)) else {
            continue;
        };
        if value.is_null() {
            continue;
        }
        non_null += 1;
        let exact = result
            .exact_rows
            .get(*row_index)
            .and_then(|row| row.get(column))
            .cloned()
            .unwrap_or_else(|| value.to_string());
        distinct.insert(exact.clone());
        *frequencies.entry(exact.clone()).or_default() += 1;
        if let Some(number) = value.as_f64().filter(|number| number.is_finite()) {
            numeric.push(number);
        }
        if let Some(text) = value.as_str() {
            strings += 1;
            total_string_chars += text.chars().count();
            if looks_temporal(text) || (temporal_name(name) && !text.trim().is_empty()) {
                temporal += 1;
                ordered_values.push(text.to_owned());
            }
        }
        if value.is_boolean() {
            booleans += 1;
        }
    }

    let distinct_count = distinct.len();
    let unique_ratio = if non_null == 0 {
        0.0
    } else {
        distinct_count as f64 / non_null as f64
    };
    let role = if non_null == 0 {
        "empty"
    } else if temporal * 4 >= non_null * 3 {
        "time"
    } else if identifier_name(name) && unique_ratio >= 0.65 {
        "identifier"
    } else if !numeric.is_empty() && numeric.len() * 4 >= non_null * 3 {
        "measure"
    } else if booleans > 0 && booleans * 4 >= non_null * 3 {
        "boolean"
    } else if strings > 0 && total_string_chars / strings >= 80 {
        "long_text"
    } else if unique_ratio >= 0.8 && non_null >= 8 {
        "high_cardinality_label"
    } else {
        "category_or_label"
    };
    let observed_type = if non_null == 0 {
        "empty"
    } else if numeric.len() == non_null {
        "number"
    } else if strings == non_null {
        "string"
    } else if booleans == non_null {
        "boolean"
    } else {
        "mixed"
    };

    let numeric_sample_order = numeric_order(&numeric);
    numeric.sort_by(f64::total_cmp);
    let numeric_shape = if numeric.is_empty() {
        None
    } else {
        let sign = if numeric[0] < 0.0 && numeric[numeric.len() - 1] > 0.0 {
            "crosses_zero"
        } else if numeric[0] < 0.0 {
            "negative"
        } else if numeric[numeric.len() - 1] > 0.0 {
            "non_negative"
        } else {
            "all_zero"
        };
        let mut sorted = numeric.clone();
        sorted.sort_by(f64::total_cmp);
        let quantile = |numerator: usize| {
            let index = (sorted.len().saturating_sub(1) * numerator) / 4;
            sorted[index]
        };
        Some(json!({
            "sign_pattern": sign,
            "sampled_distinct_values": distinct_count,
            "sampled_order": numeric_sample_order,
            "approx_min": sorted.first(),
            "approx_q1": quantile(1),
            "approx_median": quantile(2),
            "approx_q3": quantile(3),
            "approx_max": sorted.last(),
            "sampled_zero_count": sorted.iter().filter(|value| **value == 0.0).count(),
            "hints_are_approximate": true
        }))
    };
    let temporal_order = temporal_order(&ordered_values);
    let top_categories = frequencies
        .iter()
        .map(|(value, count)| (value, count))
        .collect::<Vec<_>>();
    let mut top_categories = top_categories;
    top_categories.sort_by(|left, right| right.1.cmp(left.1).then_with(|| left.0.cmp(right.0)));
    let examples = if !identifier_name(name) && (role == "category_or_label" || role == "boolean") {
        top_categories
            .iter()
            .take(PROFILE_EXAMPLES)
            .map(|(value, count)| {
                json!({
                    "value": truncate(value, MAX_CELL_CHARS),
                    "sample_frequency": count
                })
            })
            .collect::<Vec<_>>()
    } else if !identifier_name(name) && role == "time" && distinct_count <= PROFILE_EXAMPLES {
        distinct
            .iter()
            .take(PROFILE_EXAMPLES)
            .map(|value| Json::String(truncate(value, MAX_CELL_CHARS)))
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    let null_samples = indices.len().saturating_sub(non_null);

    json!({
        "field": name,
        "inferred_role": role,
        "observed_type": observed_type,
        "nulls_in_profile_sample": null_samples,
        "non_nulls_in_profile_sample": non_null,
        "sampled_distinct_values": distinct_count,
        "distinct_count_is_sampled": indices.len() < result.rows.len(),
        "numeric_shape": numeric_shape,
        "temporal_sample_order": (role == "time").then_some(temporal_order),
        "examples_or_common_values": examples
    })
}

fn sample_indices(len: usize, cap: usize) -> Vec<usize> {
    if len <= cap {
        return (0..len).collect();
    }
    if cap <= 1 {
        return vec![0];
    }
    (0..cap)
        .map(|index| index * (len - 1) / (cap - 1))
        .collect()
}

fn numeric_order(values: &[f64]) -> &'static str {
    if values.len() < 2 {
        "not_enough_observations"
    } else if values.windows(2).all(|pair| pair[0] <= pair[1]) {
        "ascending"
    } else if values.windows(2).all(|pair| pair[0] >= pair[1]) {
        "descending"
    } else {
        "not_monotonic"
    }
}

fn identifier_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.ends_with("_id")
        || lower.ends_with("_key")
        || lower.ends_with("_uuid")
        || matches!(lower.as_str(), "id" | "key" | "uuid")
        || lower
            .split(|character: char| !character.is_ascii_alphanumeric())
            .any(|part| matches!(part, "id" | "key" | "uuid"))
}

fn temporal_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    [
        "date", "time", "quarter", "month", "year", "period", "week", "day",
    ]
    .iter()
    .any(|hint| lower.contains(hint))
}

fn looks_temporal(value: &str) -> bool {
    let value = value.trim();
    let bytes = value.as_bytes();
    let iso_day = bytes.len() >= 10
        && bytes[0..4].iter().all(u8::is_ascii_digit)
        && bytes[4] == b'-'
        && bytes[5..7].iter().all(u8::is_ascii_digit)
        && bytes[7] == b'-'
        && bytes[8..10].iter().all(u8::is_ascii_digit);
    let iso_month = bytes.len() == 7
        && bytes[0..4].iter().all(u8::is_ascii_digit)
        && bytes[4] == b'-'
        && bytes[5..7].iter().all(u8::is_ascii_digit);
    let quarter = {
        let upper = value.to_ascii_uppercase();
        upper.contains("Q1") || upper.contains("Q2") || upper.contains("Q3") || upper.contains("Q4")
    };
    iso_day || iso_month || quarter
}

fn temporal_order(values: &[String]) -> &'static str {
    if values.len() < 2 {
        return "not_enough_observations";
    }
    let Some(keys) = values
        .iter()
        .map(|value| temporal_key(value))
        .collect::<Option<Vec<_>>>()
    else {
        return "unknown_format";
    };
    if keys.windows(2).all(|pair| pair[0] <= pair[1]) {
        "ascending"
    } else if keys.windows(2).all(|pair| pair[0] >= pair[1]) {
        "descending"
    } else {
        "not_monotonic"
    }
}

fn temporal_key(value: &str) -> Option<i64> {
    let value = value.trim().to_ascii_uppercase();
    let bytes = value.as_bytes();
    let parse_number = |slice: &[u8]| std::str::from_utf8(slice).ok()?.parse::<i64>().ok();

    if bytes.len() >= 10
        && bytes[0..4].iter().all(u8::is_ascii_digit)
        && bytes[4] == b'-'
        && bytes[5..7].iter().all(u8::is_ascii_digit)
        && bytes[7] == b'-'
        && bytes[8..10].iter().all(u8::is_ascii_digit)
    {
        let year = parse_number(&bytes[0..4])?;
        let month = parse_number(&bytes[5..7])?;
        let day = parse_number(&bytes[8..10])?;
        return Some(year * 372 + month * 31 + day);
    }
    if bytes.len() == 7
        && bytes[0..4].iter().all(u8::is_ascii_digit)
        && bytes[4] == b'-'
        && bytes[5..7].iter().all(u8::is_ascii_digit)
    {
        let year = parse_number(&bytes[0..4])?;
        let month = parse_number(&bytes[5..7])?;
        return Some(year * 12 + month);
    }

    let quarter = bytes.windows(2).find_map(|pair| {
        (pair[0] == b'Q' && matches!(pair[1], b'1'..=b'4')).then(|| i64::from(pair[1] - b'0'))
    });
    if let Some(quarter) = quarter {
        let year = value
            .split(|character: char| !character.is_ascii_digit())
            .find(|part| part.len() == 4)
            .and_then(|part| part.parse::<i64>().ok())?;
        return Some(year * 4 + quarter);
    }

    let year = value
        .split(|character: char| !character.is_ascii_digit())
        .find(|part| part.len() == 4)
        .and_then(|part| part.parse::<i64>().ok())?;
    Some(year)
}

fn truncate(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        value.to_owned()
    } else {
        format!(
            "{}…",
            value
                .chars()
                .take(max_chars.saturating_sub(1))
                .collect::<String>()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn result(
        columns: &[&str],
        rows: Vec<Vec<Json>>,
        row_count: usize,
        complete: bool,
    ) -> BoundResult {
        let exact_rows = rows
            .iter()
            .map(|row| {
                row.iter()
                    .map(|value| {
                        value
                            .as_str()
                            .map(str::to_owned)
                            .unwrap_or_else(|| value.to_string())
                    })
                    .collect()
            })
            .collect();
        BoundResult {
            id: "result-1".into(),
            columns: columns.iter().map(|column| (*column).into()).collect(),
            rows,
            exact_rows,
            row_count,
            complete,
        }
    }

    #[test]
    fn profiles_time_measure_and_category_structure_without_calling_it_analysis() {
        let rows = vec![
            vec![json!("2025-Q1"), json!(12.0), json!("North")],
            vec![json!("2025-Q2"), json!(-2.0), json!("North")],
            vec![json!("2025-Q3"), json!(18.0), json!("South")],
            vec![json!("2025-Q4"), json!(23.0), json!("West")],
        ];
        let bound = result(&["quarter", "net_revenue", "region"], rows, 7, false);
        let profile = profile_results(&[bound]);
        let result = &profile["results"][0];
        assert_eq!(result["complete"], false);
        assert_eq!(result["reported_row_count"], 7);
        assert_eq!(result["stored_row_count"], 4);
        assert_eq!(result["fields"][0]["inferred_role"], "time");
        assert_eq!(result["fields"][0]["temporal_sample_order"], "ascending");
        assert_eq!(result["fields"][1]["inferred_role"], "measure");
        assert_eq!(
            result["fields"][1]["numeric_shape"]["sign_pattern"],
            "crosses_zero"
        );
        assert_eq!(result["fields"][1]["numeric_shape"]["approx_median"], 12.0);
        assert_eq!(result["fields"][2]["inferred_role"], "category_or_label");
        assert_eq!(result["profile_is_design_guidance_not_analysis"], true);
    }

    #[test]
    fn profiles_include_nulls_and_evenly_spaced_rows_instead_of_only_the_head() {
        let rows = (0..20)
            .map(|index| {
                vec![
                    json!(format!("2025-{:02}", index + 1)),
                    if index == 9 { Json::Null } else { json!(index) },
                ]
            })
            .collect();
        let bound = result(&["date", "value"], rows, 20, true);
        let profile = profile_results(&[bound]);
        assert_eq!(
            profile["results"][0]["representative_rows"]
                .as_array()
                .unwrap()
                .len(),
            8
        );
        assert_eq!(
            profile["results"][0]["representative_rows"][0]["saved_row_index"],
            0
        );
        assert_eq!(
            profile["results"][0]["representative_rows"][7]["saved_row_index"],
            19
        );
        assert_eq!(
            profile["results"][0]["fields"][1]["nulls_in_profile_sample"],
            1
        );
    }

    #[test]
    fn temporal_order_understands_quarters_written_in_either_order() {
        let rows = vec![
            vec![json!("Q3 2025"), json!(10)],
            vec![json!("Q4 2025"), json!(12)],
            vec![json!("Q1 2026"), json!(14)],
        ];
        let bound = result(&["quarter", "revenue"], rows, 3, true);
        let profile = profile_results(&[bound]);
        assert_eq!(
            profile["results"][0]["fields"][0]["temporal_sample_order"],
            "ascending"
        );
    }

    #[test]
    fn recognizes_high_cardinality_identifiers_without_confusing_them_with_measures() {
        let rows = (0..12)
            .map(|index| vec![json!(index), json!(format!("customer-{index}"))])
            .collect();
        let bound = result(&["revenue", "customer_id"], rows, 12, true);
        let profile = profile_results(&[bound]);
        assert_eq!(
            profile["results"][0]["fields"][0]["inferred_role"],
            "measure"
        );
        assert_eq!(
            profile["results"][0]["fields"][1]["inferred_role"],
            "identifier"
        );
        assert_eq!(
            profile["results"][0]["fields"][1]["examples_or_common_values"],
            json!([])
        );
        assert_eq!(
            profile["results"][0]["representative_rows"][0]["values"]["customer_id"],
            "[omitted: identifier]"
        );
    }

    #[test]
    fn empty_fields_and_low_cardinality_identifiers_are_not_mislabeled_or_exposed() {
        let rows = vec![
            vec![Json::Null, json!("A")],
            vec![Json::Null, json!("A")],
            vec![Json::Null, json!("B")],
            vec![Json::Null, json!("B")],
        ];
        let bound = result(&["empty_value", "customer_id"], rows, 4, true);
        let profile = profile_results(&[bound]);
        assert_eq!(profile["results"][0]["fields"][0]["inferred_role"], "empty");
        assert_eq!(
            profile["results"][0]["fields"][1]["examples_or_common_values"],
            json!([])
        );
        assert_eq!(
            profile["results"][0]["representative_rows"][0]["values"]["customer_id"],
            "[omitted: identifier]"
        );
    }

    #[test]
    fn result_profile_work_is_capped_and_discloses_omitted_tables() {
        let bound = result(&["value"], vec![vec![json!(1)]], 1, true);
        let results = vec![bound; PROFILE_RESULTS + 2];
        let profile = profile_results(&results);
        assert_eq!(
            profile["results"].as_array().unwrap().len(),
            PROFILE_RESULTS
        );
        assert_eq!(profile["omitted_result_count"], 2);
        assert_eq!(profile["max_rows_sampled_per_result"], PROFILE_ROWS);

        let rows = (0..PROFILE_ROWS + 20)
            .map(|value| vec![json!(value)])
            .collect();
        let bound = result(&["value"], rows, PROFILE_ROWS + 20, true);
        let profile = profile_results(&[bound]);
        assert_eq!(profile["results"][0]["profiled_saved_rows"], PROFILE_ROWS);
        assert_eq!(
            profile["results"][0]["representative_rows"][7]["saved_row_index"],
            PROFILE_ROWS + 19
        );
    }

    #[test]
    fn design_plan_remains_open_ended_but_is_bounded_and_well_formed() {
        let response = r#"```json
        {"analytical_intent":"two-period comparison","visual_thesis":"Make the change between the periods obvious.","primary_form":"a paired slope view with annotated values","supporting_elements":["a compact exact-value table"],"interaction":null,"why_this_fits":"There are two ordered periods, so a directional comparison is more direct than a generic category chart.","avoid":["a generic bar chart"]}
        ```"#;
        let plan = parse_design_plan(response).unwrap();
        assert!(plan.primary_form.contains("paired slope"));
        assert_eq!(plan.supporting_elements.len(), 1);
        assert!(parse_design_plan("not json").is_err());

        let mut invalid = plan;
        invalid.visual_thesis = " ".into();
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn plan_parser_rejects_unbounded_or_unknown_plan_fields() {
        assert!(parse_design_plan(&"x".repeat(MAX_PLAN_BYTES + 1)).is_err());
        assert!(parse_design_plan(
            r#"{"analytical_intent":"x","visual_thesis":"x","primary_form":"x","why_this_fits":"x","extra":"not allowed"}"#
        )
        .is_err());
    }
}
