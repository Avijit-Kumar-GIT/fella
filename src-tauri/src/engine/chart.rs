//! A deterministic chart data tool: `make_chart`. Validates labels + one or
//! more numeric series the model already has (e.g. from a prior `run_sql`)
//! and passes them through as structured data -- rendering happens entirely
//! client-side (`src/lib/components/Chart.svelte`, `d3-scale`/`d3-shape` for
//! the line-chart math), so real DOM/CSS owns layout and theming instead of
//! Rust estimating character widths into a hand-built SVG string. This tool
//! only ever emits data (labels, numbers, short strings); it does not
//! generate markup, so there's no sanitizer boundary on the way out.

use serde::{Deserialize, Serialize};
use serde_json::Value as Json;

use crate::engine::error::{EngineError, EngineResult};
use crate::engine::state::EngineState;
use crate::engine::tools::{Tool, ToolOutput};

/// A folder's worth of tables rarely needs more than this many categories in
/// one chart before it stops being readable; past this, tell the model to
/// aggregate first.
const MAX_CATEGORIES: usize = 12;
/// Two series is already two colors on a near-monochrome palette (see
/// `src/app.css`); a third would need a real color system this isn't
/// building yet.
const MAX_SERIES: usize = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChartKind {
    Bar,
    Line,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Series {
    pub name: String,
    pub values: Vec<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChartData {
    pub kind: ChartKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub labels: Vec<String>,
    pub series: Vec<Series>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
}

/// Rejects shapes that can't be charted meaningfully. Doesn't reject
/// degenerate-but-valid data (a single category, negative values) -- the
/// frontend renders those fine.
pub fn validate(data: &ChartData) -> Result<(), String> {
    if data.labels.is_empty() {
        return Err("labels is empty -- nothing to chart".into());
    }
    if data.series.is_empty() {
        return Err("series is empty -- nothing to chart".into());
    }
    if data.labels.len() > MAX_CATEGORIES {
        return Err(format!(
            "{} categories is too many to chart clearly (max {MAX_CATEGORIES}) -- \
aggregate to the top few first",
            data.labels.len()
        ));
    }
    if data.series.len() > MAX_SERIES {
        return Err(format!("{} series is too many (max {MAX_SERIES})", data.series.len()));
    }
    for s in &data.series {
        if s.values.len() != data.labels.len() {
            return Err(format!(
                "series \"{}\" has {} value(s) but there are {} label(s)",
                s.name,
                s.values.len(),
                data.labels.len()
            ));
        }
        if let Some(bad) = s.values.iter().find(|v| !v.is_finite()) {
            return Err(format!("series \"{}\" has a non-finite value ({bad})", s.name));
        }
    }
    if !meaningfully_varies(data) {
        return Err(
            "these values are all within a couple percent of each other -- a chart won't \
show anything a sentence wouldn't. Answer in prose or a small table instead of charting \
flat data."
                .into(),
        );
    }
    Ok(())
}

/// Below this, a series' values are close enough to flat that a chart adds
/// nothing over a sentence -- e.g. rent at $1316/mo except one $1321 month
/// is a >99%-flat line, not a trend worth drawing. Relative to the largest
/// magnitude in the series (not the mean), since a mean near zero would
/// blow the ratio up on otherwise-flat data centered near zero.
/// `ponytail:` a fixed heuristic threshold, not learned from real charting
/// mistakes yet -- retune from `agent_eval` feedback if it over/under-fires.
const MIN_VARIATION: f64 = 0.02;

/// True when at least one series has real spread across its values. A
/// single category is always "fine" here -- there's nothing to compare, so
/// flatness isn't the concern (`chart_rule`'s prompt guidance handles a
/// single-figure chart separately, before this ever runs).
fn meaningfully_varies(data: &ChartData) -> bool {
    if data.labels.len() <= 1 {
        return true;
    }
    data.series.iter().any(|s| {
        let lo = s.values.iter().cloned().fold(f64::INFINITY, f64::min);
        let hi = s.values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let scale = s.values.iter().fold(0.0f64, |m, v| m.max(v.abs())).max(1.0);
        (hi - lo) / scale >= MIN_VARIATION
    })
}

#[derive(Deserialize)]
struct ChartArgs {
    kind: ChartKind,
    #[serde(default)]
    title: Option<String>,
    labels: Vec<String>,
    series: Vec<Series>,
    #[serde(default)]
    unit: Option<String>,
}

pub struct MakeChart;

#[async_trait::async_trait]
impl Tool for MakeChart {
    fn name(&self) -> &'static str {
        "make_chart"
    }
    fn description(&self) -> &'static str {
        "Draw a bar or line chart from labels + one or more numeric series you already have \
(e.g. from a prior run_sql). It renders itself in the answer; don't describe it in prose."
    }
    fn parameters(&self) -> Json {
        serde_json::json!({
            "type": "object",
            "properties": {
                "kind": { "type": "string", "enum": ["bar", "line"] },
                "title": { "type": "string", "description": "short chart title, e.g. \"Spending by category\"" },
                "labels": {
                    "type": "array", "items": { "type": "string" },
                    "description": "x-axis / category labels, in order"
                },
                "series": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "name": { "type": "string" },
                            "values": { "type": "array", "items": { "type": "number" } }
                        },
                        "required": ["name", "values"],
                        "additionalProperties": false
                    },
                    "description": "one or more named numeric series, each with one value per label"
                },
                "unit": { "type": "string", "description": "optional short suffix/prefix for values, e.g. \"$\" or \"%\"" }
            },
            "required": ["kind", "labels", "series"],
            "additionalProperties": false
        })
    }
    async fn run(&self, _engine: &EngineState, args: &Json) -> EngineResult<ToolOutput> {
        let parsed: ChartArgs = serde_json::from_value(args.clone())
            .map_err(|e| EngineError::msg(format!("invalid make_chart arguments: {e}")))?;
        let data = ChartData {
            kind: parsed.kind,
            title: parsed.title,
            labels: parsed.labels,
            series: parsed.series,
            unit: parsed.unit,
        };
        validate(&data).map_err(EngineError::msg)?;

        let n_series = data.series.len();
        let n_labels = data.labels.len();
        let kind_word = match data.kind {
            ChartKind::Bar => "bar",
            ChartKind::Line => "line",
        };
        Ok(ToolOutput {
            summary: format!(
                "{kind_word} chart, {n_labels} categor{}, {n_series} series",
                if n_labels == 1 { "y" } else { "ies" }
            ),
            llm_text: "Chart drawn — it renders below this message; don't restate the numbers \
in prose."
                .to_string(),
            sql: None,
            columns: None,
            rows: None,
            row_count: None,
            output: None,
            chart: Some(data),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bar(labels: &[&str], series: Vec<(&str, Vec<f64>)>) -> ChartData {
        ChartData {
            kind: ChartKind::Bar,
            title: Some("Test".into()),
            labels: labels.iter().map(|s| s.to_string()).collect(),
            series: series
                .into_iter()
                .map(|(name, values)| Series { name: name.into(), values })
                .collect(),
            unit: None,
        }
    }

    #[test]
    fn serializes_to_the_wire_shape_the_frontend_expects() {
        let data = ChartData {
            kind: ChartKind::Bar,
            title: Some("Spending".into()),
            labels: vec!["Groceries".into()],
            series: vec![Series { name: "amount".into(), values: vec![412.5] }],
            unit: Some("$".into()),
        };
        let v = serde_json::to_value(&data).unwrap();
        assert_eq!(v["kind"], "bar");
        assert_eq!(v["title"], "Spending");
        assert_eq!(v["labels"], serde_json::json!(["Groceries"]));
        assert_eq!(v["series"][0]["name"], "amount");
        assert_eq!(v["series"][0]["values"], serde_json::json!([412.5]));
        assert_eq!(v["unit"], "$");
    }

    #[test]
    fn omits_absent_title_and_unit_rather_than_nulling_them() {
        let data = bar(&["a"], vec![("s", vec![1.0])]);
        let v = serde_json::to_value(ChartData { title: None, unit: None, ..data }).unwrap();
        assert!(v.get("title").is_none());
        assert!(v.get("unit").is_none());
    }

    #[test]
    fn validate_rejects_mismatched_series_length() {
        let data = bar(&["a", "b"], vec![("s", vec![1.0])]);
        assert!(validate(&data).is_err());
    }

    #[test]
    fn validate_rejects_too_many_categories() {
        let labels: Vec<String> = (0..MAX_CATEGORIES + 1).map(|i| i.to_string()).collect();
        let data = ChartData {
            kind: ChartKind::Bar,
            title: None,
            labels: labels.clone(),
            series: vec![Series { name: "s".into(), values: vec![1.0; labels.len()] }],
            unit: None,
        };
        assert!(validate(&data).is_err());
    }

    #[test]
    fn validate_rejects_too_many_series() {
        let data = bar(&["a"], vec![("s1", vec![1.0]), ("s2", vec![1.0]), ("s3", vec![1.0])]);
        assert!(validate(&data).is_err());
    }

    #[test]
    fn validate_rejects_empty_input() {
        let empty_labels = ChartData {
            kind: ChartKind::Bar,
            title: None,
            labels: vec![],
            series: vec![Series { name: "s".into(), values: vec![] }],
            unit: None,
        };
        assert!(validate(&empty_labels).is_err());

        let empty_series = ChartData {
            kind: ChartKind::Bar,
            title: None,
            labels: vec!["a".into()],
            series: vec![],
            unit: None,
        };
        assert!(validate(&empty_series).is_err());
    }

    #[test]
    fn validate_rejects_non_finite_value() {
        let data = bar(&["a"], vec![("s", vec![f64::NAN])]);
        assert!(validate(&data).is_err());
        let data = bar(&["a"], vec![("s", vec![f64::INFINITY])]);
        assert!(validate(&data).is_err());
    }

    #[test]
    fn validate_rejects_nearly_flat_data() {
        // The real bug report: 11 months at $1316, one at $1321 -- under
        // 0.4% spread, not a trend worth drawing.
        let mut values = vec![1316.0; 11];
        values.push(1321.0);
        let labels: Vec<String> = (0..12).map(|i| format!("m{i}")).collect();
        let data = ChartData {
            kind: ChartKind::Bar,
            title: None,
            labels,
            series: vec![Series { name: "rent".into(), values }],
            unit: Some("$".into()),
        };
        assert!(validate(&data).is_err());
    }

    #[test]
    fn validate_allows_a_single_category_even_though_flat() {
        // Nothing to compare against, so flatness isn't the concern here
        // (the prompt tells the model to skip charting a single figure).
        let data = bar(&["only"], vec![("s", vec![42.0])]);
        assert!(validate(&data).is_ok());
    }

    #[test]
    fn validate_allows_data_with_real_spread() {
        let data = bar(&["a", "b", "c"], vec![("s", vec![100.0, 250.0, 90.0])]);
        assert!(validate(&data).is_ok());
    }

    #[test]
    fn validate_allows_one_varying_series_even_if_another_is_flat() {
        let data = bar(
            &["a", "b", "c"],
            vec![("flat", vec![10.0, 10.0, 10.0]), ("varies", vec![5.0, 50.0, 8.0])],
        );
        assert!(validate(&data).is_ok());
    }
}
