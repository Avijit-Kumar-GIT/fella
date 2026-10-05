//! Chart data: converts a bounded query result into validated structured chart
//! data. Rendering happens entirely client-side
//! (`src/lib/components/Chart.svelte`, `d3-scale`/`d3-shape` for the
//! line-chart math), so real DOM/CSS owns layout and theming instead of
//! Rust estimating character widths into a hand-built SVG string. This
//! module only ever emits data (labels, numbers, short strings); it does not
//! generate markup, so there's no sanitizer boundary on the way out.
//!
//! The `make_chart` tool that wraps this for the agent loop lives in
//! `tools.rs`, not here -- it's app-calling glue (`Tool`/`ToolOutput`),
//! while everything in this file is plain data plus one pure validation
//! function, independent of the rest of the app.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::Value as Json;

/// A folder's worth of categorical data rarely needs more than this many
/// labels in one bar chart before it stops being readable; past this, tell the
/// model to aggregate first.
pub const MAX_CATEGORIES: usize = 12;
/// Maximum source rows materialized by a chart query. A long-form chart may
/// contain several group rows per x-axis point before it is pivoted.
pub const MAX_SOURCE_ROWS: usize = 10_000;
/// Line charts can carry a much longer time axis than a categorical bar chart.
/// The source may be long-form (multiple rows per date), but the final plotted
/// time axis remains bounded to keep rendering and exact-value tables usable.
pub const MAX_TIME_POINTS: usize = 1_000;
/// Keep simultaneous series limited to a legible, color-safe palette.
pub const MAX_SERIES: usize = 5;
pub const MAX_PIE_SLICES: usize = 8;
pub const MAX_SCATTER_POINTS: usize = 500;
pub const MAX_HEATMAP_AXIS: usize = 24;
pub const MAX_HISTOGRAM_BINS: usize = 40;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChartKind {
    /// Request-time choice. `from_query` resolves this to a concrete kind
    /// before the data crosses the engine/UI boundary.
    #[default]
    Auto,
    Bar,
    Line,
    Pie,
    Donut,
    Scatter,
    Histogram,
    BoxPlot,
    Area,
    StackedArea,
    Heatmap,
    Forecast,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Series {
    pub name: String,
    /// `None` is a visible gap, never an implicit zero.
    #[serde(default)]
    pub values: Vec<Option<f64>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ChartPayload {
    Scatter {
        points: Vec<ScatterPoint>,
    },
    BoxPlot {
        groups: Vec<BoxSummary>,
    },
    Heatmap {
        x_labels: Vec<String>,
        y_labels: Vec<String>,
        values: Vec<Vec<Option<f64>>>,
    },
    Forecast {
        observed: Vec<Option<f64>>,
        forecast: Vec<Option<f64>>,
        lower: Vec<Option<f64>>,
        upper: Vec<Option<f64>>,
        uncertainty_note: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScatterPoint {
    pub x: f64,
    pub y: f64,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoxSummary {
    pub label: String,
    pub low_whisker: f64,
    pub q1: f64,
    pub median: f64,
    pub q3: f64,
    pub high_whisker: f64,
    pub outliers: Vec<f64>,
    pub n: usize,
}

/// Inspectable, non-executable description of how a visualization was made.
/// The source evidence remains authoritative; this metadata is a compact guide
/// back to its fields and semantic choices.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChartMetadata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_evidence_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_label: Option<String>,
    #[serde(default)]
    pub fields: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aggregation: Option<String>,
    #[serde(default)]
    pub filters: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_range: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub denominator: Option<String>,
    #[serde(default)]
    pub part_to_whole: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub missing_treatment: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MissingTreatment {
    #[default]
    Reject,
    Exclude,
    Gap,
}

#[derive(Debug, Clone, Default)]
pub struct ChartRequest {
    pub kind: ChartKind,
    pub title: Option<String>,
    pub unit: Option<String>,
    pub x_field: Option<String>,
    pub y_field: Option<String>,
    pub value_field: Option<String>,
    pub group_field: Option<String>,
    pub label_field: Option<String>,
    pub series_fields: Vec<String>,
    pub bin_count: Option<usize>,
    pub missing_treatment: MissingTreatment,
    pub metadata: ChartMetadata,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TabularResult {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Json>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisualizationSpec {
    pub kind: ChartKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub labels: Vec<String>,
    pub series: Vec<Series>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub x_label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub y_label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<ChartPayload>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<ChartMetadata>,
}

/// Compatibility name for the existing engine/UI boundary. New code should
/// think of this as a visualization specification: typed data for a known
/// renderer, never model-generated markup.
pub type ChartData = VisualizationSpec;

/// Build chart data from a query result whose first column is the label and
/// remaining columns are numeric series. Keeping this conversion here makes
/// the shape rule testable without `EngineState`; the tool layer only runs the
/// read-only query and supplies its result.
pub fn from_query(
    kind: ChartKind,
    title: Option<String>,
    unit: Option<String>,
    columns: &[String],
    rows: &[Vec<Json>],
) -> Result<ChartData, String> {
    if columns.len() < 2 {
        return Err("a chart query needs one label column and at least one numeric column".into());
    }
    let series_count = columns.len() - 1;
    if series_count > MAX_SERIES {
        return Err(format!(
            "a chart query returned {series_count} numeric columns (max {MAX_SERIES})"
        ));
    }
    if rows.is_empty() {
        return Err("the chart query returned no rows -- nothing to chart".into());
    }

    let labels: Vec<String> = rows
        .iter()
        .enumerate()
        .map(|(row_index, row)| {
            if row.len() != columns.len() {
                return Err(format!(
                    "chart query row {} has {} values but the result has {} columns",
                    row_index + 1,
                    row.len(),
                    columns.len()
                ));
            }
            label_value(&row[0], row_index)
        })
        .collect::<Result<_, _>>()?;
    let resolved_kind = resolve_kind(kind, &columns[0], &labels);
    let max_points = max_points(resolved_kind);
    if rows.len() > max_points {
        return Err(size_error(resolved_kind, rows.len()));
    }

    let mut values = vec![Vec::with_capacity(rows.len()); series_count];
    for (row_index, row) in rows.iter().enumerate() {
        if row.len() != columns.len() {
            return Err(format!(
                "chart query row {} has {} values but the result has {} columns",
                row_index + 1,
                row.len(),
                columns.len()
            ));
        }
        for (series_index, value) in row[1..].iter().enumerate() {
            values[series_index].push(Some(number_value(
                value,
                &columns[series_index + 1],
                row_index,
            )?));
        }
    }

    let series = columns[1..]
        .iter()
        .enumerate()
        .map(|(i, name)| Series {
            name: name.clone(),
            values: values[i].clone(),
        })
        .collect();
    let data = ChartData {
        kind: resolved_kind,
        title,
        labels,
        series,
        unit,
        x_label: Some(columns[0].clone()),
        y_label: None,
        payload: None,
        metadata: None,
    };
    validate(&data)?;
    Ok(data)
}

/// Construct any supported visualization from a bounded, already-computed
/// table. This is deliberately independent of SQL so Python, forecast, and
/// scenario results use the same validated rendering boundary.
pub fn from_table(table: &TabularResult, request: ChartRequest) -> Result<ChartData, String> {
    validate_table(table)?;
    if table.rows.is_empty() {
        return Err("the result has no rows -- nothing to visualize".into());
    }

    match request.kind {
        ChartKind::Auto
        | ChartKind::Bar
        | ChartKind::Line
        | ChartKind::Area
        | ChartKind::StackedArea => build_series_chart(table, request),
        ChartKind::Pie | ChartKind::Donut => build_pie_chart(table, request),
        ChartKind::Scatter => build_scatter_chart(table, request),
        ChartKind::Histogram => build_histogram(table, request),
        ChartKind::BoxPlot => build_box_plot(table, request),
        ChartKind::Heatmap => build_heatmap(table, request),
        ChartKind::Forecast => build_forecast_chart(table, request),
    }
}

fn validate_table(table: &TabularResult) -> Result<(), String> {
    if table.columns.is_empty() {
        return Err("the chart source has no columns".into());
    }
    if table.rows.len() > MAX_SOURCE_ROWS {
        return Err(
            format!("the chart source is too large (maximum {MAX_SOURCE_ROWS} rows); aggregate before charting"),
        );
    }
    for (index, row) in table.rows.iter().enumerate() {
        if row.len() != table.columns.len() {
            return Err(format!(
                "result row {} has {} values but the table has {} columns",
                index + 1,
                row.len(),
                table.columns.len()
            ));
        }
    }
    Ok(())
}

fn column_index(
    table: &TabularResult,
    requested: Option<&str>,
    fallback: usize,
) -> Result<usize, String> {
    let Some(requested) = requested else {
        return table
            .columns
            .get(fallback)
            .map(|_| fallback)
            .ok_or_else(|| format!("the result has no column at position {}", fallback + 1));
    };
    table
        .columns
        .iter()
        .position(|column| column.eq_ignore_ascii_case(requested))
        .ok_or_else(|| {
            let available = table
                .columns
                .iter()
                .map(|column| format!("`{column}`"))
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "chart field `{requested}` is not present in the result; available result fields: {available}"
            )
        })
}

fn selected_numeric_columns(
    table: &TabularResult,
    x_index: usize,
    request: &ChartRequest,
) -> Result<Vec<usize>, String> {
    if !request.series_fields.is_empty() {
        let indices = request
            .series_fields
            .iter()
            .map(|field| column_index(table, Some(field), 0))
            .collect::<Result<Vec<_>, _>>()?;
        if indices.iter().any(|index| *index == x_index) {
            return Err("the x/category field cannot also be a numeric series".into());
        }
        return Ok(indices);
    }
    if let Some(field) = request
        .y_field
        .as_deref()
        .or(request.value_field.as_deref())
    {
        let index = column_index(table, Some(field), 0)?;
        if index == x_index {
            return Err("the x/category field cannot also be a numeric series".into());
        }
        return Ok(vec![index]);
    }
    let indices: Vec<_> = (0..table.columns.len())
        .filter(|index| *index != x_index)
        .collect();
    if indices.is_empty() {
        return Err("a chart needs at least one numeric value field".into());
    }
    Ok(indices)
}

fn apply_missing_treatment(
    treatment: MissingTreatment,
    metadata: &mut ChartMetadata,
    excluded: usize,
) -> Result<(), String> {
    if excluded > 0 {
        if treatment != MissingTreatment::Exclude {
            return Err(format!(
                "{excluded} source row(s) have missing chart values; choose an explicit missing_treatment such as `exclude` or `gap`"
            ));
        }
        metadata.missing_treatment = Some(format!(
            "excluded {excluded} row(s) with missing values; excluded count is not a plotted zero"
        ));
    } else {
        metadata.missing_treatment = Some(match treatment {
            MissingTreatment::Reject => "no missing values encountered".into(),
            MissingTreatment::Exclude => "missing rows excluded if present".into(),
            MissingTreatment::Gap => "missing values shown as gaps, not zero".into(),
        });
    }
    Ok(())
}

fn build_series_chart(
    table: &TabularResult,
    mut request: ChartRequest,
) -> Result<ChartData, String> {
    if request.group_field.is_some() {
        return build_grouped_series_chart(table, request);
    }
    let x_index = column_index(table, request.x_field.as_deref(), 0)?;
    let labels = table
        .rows
        .iter()
        .enumerate()
        .map(|(row, values)| label_value(&values[x_index], row))
        .collect::<Result<Vec<_>, _>>()?;
    let kind = resolve_kind(request.kind, &table.columns[x_index], &labels);
    let indices = selected_numeric_columns(table, x_index, &request)?;
    let max_series = if kind == ChartKind::StackedArea {
        MAX_SERIES
    } else {
        MAX_SERIES
    };
    if indices.len() > max_series {
        return Err(format!(
            "chart has {} series, above the readable limit of {max_series}",
            indices.len()
        ));
    }
    ensure_unique_labels(&labels)?;

    let mut kept = Vec::new();
    let mut excluded = 0;
    for (row_index, row) in table.rows.iter().enumerate() {
        let mut numeric = Vec::with_capacity(indices.len());
        let mut row_has_missing = false;
        for &column in &indices {
            match optional_number_value(&row[column], &table.columns[column], row_index)? {
                Some(value) => numeric.push(Some(value)),
                None => {
                    row_has_missing = true;
                    numeric.push(None);
                }
            }
        }
        if row_has_missing && request.missing_treatment == MissingTreatment::Exclude {
            excluded += 1;
            continue;
        }
        if row_has_missing && request.missing_treatment == MissingTreatment::Reject {
            return Err(format!(
                "chart row {} contains missing values (not a finite number); choose an explicit missing_treatment",
                row_index + 1
            ));
        }
        kept.push((labels[row_index].clone(), numeric));
    }
    if kept.is_empty() {
        return Err(
            "no complete chart rows remain after the selected missing-value treatment".into(),
        );
    }
    if looks_temporal(&table.columns[x_index], &labels) {
        if let Some(order) = chronological_order(&labels) {
            kept = order.into_iter().map(|index| kept[index].clone()).collect();
        }
    }
    let labels: Vec<_> = kept.iter().map(|(label, _)| label.clone()).collect();
    let series = indices
        .iter()
        .enumerate()
        .map(|(series_index, column)| Series {
            name: table.columns[*column].clone(),
            values: kept
                .iter()
                .map(|(_, values)| values[series_index])
                .collect(),
        })
        .collect();
    request.metadata.fields = std::iter::once(table.columns[x_index].clone())
        .chain(indices.iter().map(|index| table.columns[*index].clone()))
        .collect();
    apply_missing_treatment(request.missing_treatment, &mut request.metadata, excluded)?;
    let mut data = base_spec(kind, request, labels, series);
    data.x_label = Some(table.columns[x_index].clone());
    validate(&data)?;
    Ok(data)
}

/// Convert tidy/long-form observations (`x`, `group`, `value`) into a wide
/// chart. Each observed group becomes a series and absent x/group combinations
/// stay gaps; the chart layer never guesses that an absent row means zero.
fn build_grouped_series_chart(
    table: &TabularResult,
    mut request: ChartRequest,
) -> Result<ChartData, String> {
    let x_index = column_index(table, request.x_field.as_deref(), 0)?;
    let group_name = request.group_field.as_deref().unwrap_or_default();
    let group_index = column_index(table, Some(group_name), 1)?;
    let value_name = request
        .value_field
        .as_deref()
        .or(request.y_field.as_deref());
    let value_index = if let Some(name) = value_name {
        column_index(table, Some(name), 0)?
    } else {
        (0..table.columns.len())
            .find(|index| *index != x_index && *index != group_index)
            .ok_or_else(|| {
                "a grouped chart needs a numeric `value_field` (or `y_field`) in addition to its x and group fields".to_string()
            })?
    };
    if x_index == group_index || x_index == value_index || group_index == value_index {
        return Err(
            "grouped charts need distinct x_field, group_field, and numeric value_field columns"
                .into(),
        );
    }

    let mut labels = Vec::<String>::new();
    let mut groups = Vec::<String>::new();
    let mut seen_labels = HashSet::<String>::new();
    let mut seen_groups = HashSet::<String>::new();
    let mut values = HashMap::<(String, String), Option<f64>>::new();
    let mut excluded = 0usize;
    for (row_index, row) in table.rows.iter().enumerate() {
        let label = label_value(&row[x_index], row_index)?;
        let group = label_value(&row[group_index], row_index)?;
        let value =
            optional_number_value(&row[value_index], &table.columns[value_index], row_index)?;
        let value = match value {
            Some(value) => Some(value),
            None if request.missing_treatment == MissingTreatment::Exclude => {
                excluded += 1;
                continue;
            }
            None if request.missing_treatment == MissingTreatment::Gap => None,
            None => {
                return Err(format!(
                    "grouped chart row {} has a missing measurement; choose `gap` or `exclude` explicitly",
                    row_index + 1
                ));
            }
        };
        if values
            .insert((label.clone(), group.clone()), value)
            .is_some()
        {
            return Err(format!(
                "grouped chart has multiple rows for ({label}, {group}); aggregate to one value per x/group pair in SQL before charting"
            ));
        }
        if seen_labels.insert(label.clone()) {
            labels.push(label.clone());
        }
        if seen_groups.insert(group.clone()) {
            groups.push(group);
        }
    }
    if labels.is_empty() || groups.is_empty() {
        return Err(
            "no grouped chart observations remain after the selected missing-value treatment"
                .into(),
        );
    }
    ensure_unique_labels(&labels)?;
    ensure_unique_labels(&groups)?;
    if groups.len() > MAX_SERIES {
        return Err(format!(
            "grouped chart has {} series, above the readable limit of {MAX_SERIES}; filter or aggregate groups explicitly rather than silently dropping them",
            groups.len()
        ));
    }

    let kind = resolve_kind(request.kind, &table.columns[x_index], &labels);
    let max_points = max_points(kind);
    if labels.len() > max_points {
        return Err(size_error(kind, labels.len()));
    }
    if looks_temporal(&table.columns[x_index], &labels) {
        if let Some(order) = chronological_order(&labels) {
            labels = order
                .into_iter()
                .map(|index| labels[index].clone())
                .collect();
        }
    }

    let absent_pairs = labels.len() * groups.len() - values.len();
    if absent_pairs > 0 && request.missing_treatment == MissingTreatment::Reject {
        return Err(format!(
            "grouped chart has {absent_pairs} unobserved x/group combinations; choose `gap` to preserve them as gaps, or resolve whether they mean zero before charting"
        ));
    }
    let series = groups
        .iter()
        .map(|group| Series {
            name: group.clone(),
            values: labels
                .iter()
                .map(|label| {
                    values
                        .get(&(label.clone(), group.clone()))
                        .copied()
                        .flatten()
                })
                .collect(),
        })
        .collect();
    request.metadata.fields = vec![
        table.columns[x_index].clone(),
        table.columns[group_index].clone(),
        table.columns[value_index].clone(),
    ];
    apply_missing_treatment(request.missing_treatment, &mut request.metadata, excluded)?;
    if absent_pairs > 0 {
        let note =
            format!("{absent_pairs} unobserved x/group combination(s) remain gaps, not zero");
        request.metadata.missing_treatment =
            Some(match request.metadata.missing_treatment.take() {
                Some(existing) => format!("{existing}; {note}"),
                None => note,
            });
    }
    let mut data = base_spec(kind, request, labels, series);
    data.x_label = Some(table.columns[x_index].clone());
    data.y_label = Some(table.columns[value_index].clone());
    validate(&data)?;
    Ok(data)
}

fn build_pie_chart(table: &TabularResult, mut request: ChartRequest) -> Result<ChartData, String> {
    let category = column_index(table, request.x_field.as_deref(), 0)?;
    let value = column_index(
        table,
        request
            .value_field
            .as_deref()
            .or(request.y_field.as_deref()),
        if category == 0 { 1 } else { 0 },
    )?;
    if category == value {
        return Err("pie/donut charts need separate category and value fields".into());
    }
    if !request.metadata.part_to_whole
        || request
            .metadata
            .denominator
            .as_deref()
            .is_none_or(str::is_empty)
    {
        return Err("pie/donut charts require a confirmed part-to-whole interpretation and a named denominator".into());
    }
    let mut labels = Vec::new();
    let mut values = Vec::new();
    let mut excluded = 0;
    for (row_index, row) in table.rows.iter().enumerate() {
        let label = label_value(&row[category], row_index)?;
        match optional_number_value(&row[value], &table.columns[value], row_index)? {
            Some(value) => {
                labels.push(label);
                values.push(Some(value));
            }
            None if request.missing_treatment == MissingTreatment::Exclude => excluded += 1,
            None => return Err(
                "pie/donut values are missing; explicitly exclude them or use a different chart"
                    .into(),
            ),
        }
    }
    ensure_unique_labels(&labels)?;
    if labels.len() > MAX_PIE_SLICES {
        return Err(size_error(request.kind, labels.len()));
    }
    request.metadata.fields = vec![
        table.columns[category].clone(),
        table.columns[value].clone(),
    ];
    apply_missing_treatment(request.missing_treatment, &mut request.metadata, excluded)?;
    let mut data = base_spec(
        request.kind,
        request,
        labels,
        vec![Series {
            name: table.columns[value].clone(),
            values,
        }],
    );
    data.x_label = Some(table.columns[category].clone());
    data.y_label = Some(table.columns[value].clone());
    validate(&data)?;
    Ok(data)
}

fn build_scatter_chart(
    table: &TabularResult,
    mut request: ChartRequest,
) -> Result<ChartData, String> {
    if table.rows.len() > MAX_SCATTER_POINTS {
        return Err(size_error(ChartKind::Scatter, table.rows.len()));
    }
    let x = column_index(table, request.x_field.as_deref(), 0)?;
    let y = column_index(table, request.y_field.as_deref(), 1)?;
    if x == y {
        return Err("scatter plot x and y fields must differ".into());
    }
    let group = request
        .group_field
        .as_deref()
        .map(|field| column_index(table, Some(field), 0))
        .transpose()?;
    let label = request
        .label_field
        .as_deref()
        .map(|field| column_index(table, Some(field), 0))
        .transpose()?;
    let mut points = Vec::new();
    let mut excluded = 0;
    for (row_index, row) in table.rows.iter().enumerate() {
        let x_value = optional_number_value(&row[x], &table.columns[x], row_index)?;
        let y_value = optional_number_value(&row[y], &table.columns[y], row_index)?;
        let (Some(x_value), Some(y_value)) = (x_value, y_value) else {
            if request.missing_treatment == MissingTreatment::Exclude {
                excluded += 1;
                continue;
            }
            return Err(format!("scatter row {} is missing x or y; explicitly exclude incomplete pairs or use another view", row_index + 1));
        };
        points.push(ScatterPoint {
            x: x_value,
            y: y_value,
            label: label
                .map(|index| label_value(&row[index], row_index))
                .transpose()?
                .unwrap_or_else(|| (row_index + 1).to_string()),
            group: group
                .map(|index| label_value(&row[index], row_index))
                .transpose()?,
        });
    }
    if points.len() > MAX_SCATTER_POINTS {
        return Err(size_error(ChartKind::Scatter, points.len()));
    }
    request.metadata.fields = vec![table.columns[x].clone(), table.columns[y].clone()];
    if let Some(index) = group {
        request.metadata.fields.push(table.columns[index].clone());
    }
    apply_missing_treatment(request.missing_treatment, &mut request.metadata, excluded)?;
    let mut data = base_spec(ChartKind::Scatter, request, Vec::new(), Vec::new());
    data.x_label = Some(table.columns[x].clone());
    data.y_label = Some(table.columns[y].clone());
    data.payload = Some(ChartPayload::Scatter { points });
    validate(&data)?;
    Ok(data)
}

fn build_histogram(table: &TabularResult, mut request: ChartRequest) -> Result<ChartData, String> {
    // Analysts often bin in SQL/Python first so the bin edges are explicit and
    // reviewable. When the model supplies a categorical x field plus a
    // frequency y field, preserve that table as-is instead of histogramming
    // the frequency counts a second time.
    if let (Some(label_field), Some(count_field)) = (
        request.x_field.as_deref(),
        request
            .y_field
            .as_deref()
            .or(request.value_field.as_deref()),
    ) {
        let label = column_index(table, Some(label_field), 0)?;
        let count = column_index(table, Some(count_field), 0)?;
        let categorical_bins = label != count
            && table
                .rows
                .iter()
                .all(|row| matches!(row[label], Json::String(_)));
        if categorical_bins {
            return build_precomputed_histogram(table, request, label, count);
        }
    }

    let value = value_column(table, &request)?;
    let mut values = Vec::new();
    let mut excluded = 0;
    for (row_index, row) in table.rows.iter().enumerate() {
        match optional_number_value(&row[value], &table.columns[value], row_index)? {
            Some(value) => values.push(value),
            None if request.missing_treatment == MissingTreatment::Exclude => excluded += 1,
            None => return Err(format!("histogram row {} is missing its value; explicitly exclude missing observations or use another view", row_index + 1)),
        }
    }
    if values.is_empty() {
        return Err("histogram has no numeric observations".into());
    }
    let bin_count = request
        .bin_count
        .unwrap_or_else(|| automatic_bin_count(&values));
    if !(1..=MAX_HISTOGRAM_BINS).contains(&bin_count) {
        return Err(format!(
            "histogram bin_count must be between 1 and {MAX_HISTOGRAM_BINS}"
        ));
    }
    let min = values.iter().copied().fold(f64::INFINITY, f64::min);
    let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let (start, end) = if min == max {
        (min - 0.5, max + 0.5)
    } else {
        (min, max)
    };
    let width = (end - start) / bin_count as f64;
    let mut counts = vec![0.0; bin_count];
    for value in &values {
        let index =
            (((*value - start) / width).floor() as isize).clamp(0, bin_count as isize - 1) as usize;
        counts[index] += 1.0;
    }
    let labels = (0..bin_count)
        .map(|index| {
            format!(
                "{}–{}",
                format_number(start + width * index as f64),
                format_number(if index + 1 == bin_count {
                    end
                } else {
                    start + width * (index + 1) as f64
                })
            )
        })
        .collect();
    request.metadata.fields = vec![table.columns[value].clone()];
    request.metadata.aggregation = Some(format!("frequency; {bin_count} equal-width bins"));
    apply_missing_treatment(request.missing_treatment, &mut request.metadata, excluded)?;
    let mut data = base_spec(
        ChartKind::Histogram,
        request,
        labels,
        vec![Series {
            name: "Count".into(),
            values: counts.into_iter().map(Some).collect(),
        }],
    );
    data.x_label = Some(table.columns[value].clone());
    data.y_label = Some("Frequency".into());
    validate(&data)?;
    Ok(data)
}

fn build_precomputed_histogram(
    table: &TabularResult,
    mut request: ChartRequest,
    label_column: usize,
    count_column: usize,
) -> Result<ChartData, String> {
    if table.rows.len() > MAX_HISTOGRAM_BINS {
        return Err(size_error(ChartKind::Histogram, table.rows.len()));
    }
    if request
        .bin_count
        .is_some_and(|requested| requested != table.rows.len())
    {
        return Err(format!(
            "the source already contains {} frequency bins, which does not match the requested bin_count; use the source binning or provide raw observations",
            table.rows.len()
        ));
    }

    let mut labels = Vec::with_capacity(table.rows.len());
    let mut values = Vec::with_capacity(table.rows.len());
    let mut excluded = 0;
    let mut missing = 0;
    for (row_index, row) in table.rows.iter().enumerate() {
        let label = label_value(&row[label_column], row_index)?;
        match optional_number_value(&row[count_column], &table.columns[count_column], row_index)? {
            Some(value) if value >= 0.0 => {
                labels.push(label);
                values.push(Some(value));
            }
            Some(_) => {
                return Err(format!(
                    "precomputed histogram frequency in row {} cannot be negative",
                    row_index + 1
                ));
            }
            None if request.missing_treatment == MissingTreatment::Exclude => excluded += 1,
            None if request.missing_treatment == MissingTreatment::Gap => {
                missing += 1;
                labels.push(label);
                values.push(None);
            }
            None => {
                return Err(format!(
                    "precomputed histogram frequency in row {} is missing; explicitly exclude it or show a gap",
                    row_index + 1
                ));
            }
        }
    }
    if labels.is_empty() {
        return Err("no frequency bins remain after the selected missing-value treatment".into());
    }
    ensure_unique_labels(&labels)?;
    request.metadata.fields = vec![
        table.columns[label_column].clone(),
        table.columns[count_column].clone(),
    ];
    request.metadata.aggregation = Some(format!(
        "precomputed frequency bins from `{}`",
        table.columns[count_column]
    ));
    if missing > 0 {
        request.metadata.missing_treatment = Some(format!(
            "{missing} missing frequency value(s) shown as gaps, not zero"
        ));
    } else {
        apply_missing_treatment(request.missing_treatment, &mut request.metadata, excluded)?;
    }
    let mut data = base_spec(
        ChartKind::Histogram,
        request,
        labels,
        vec![Series {
            name: "Count".into(),
            values,
        }],
    );
    data.x_label = Some(table.columns[label_column].clone());
    data.y_label = Some("Frequency".into());
    validate(&data)?;
    Ok(data)
}

fn build_box_plot(table: &TabularResult, mut request: ChartRequest) -> Result<ChartData, String> {
    let value = value_column(table, &request)?;
    let group_column = request
        .group_field
        .as_deref()
        .map(|field| column_index(table, Some(field), 0))
        .transpose()?;
    let mut grouped: Vec<(String, Vec<f64>)> = Vec::new();
    let mut excluded = 0;
    for (row_index, row) in table.rows.iter().enumerate() {
        let group = group_column
            .map(|index| label_value(&row[index], row_index))
            .transpose()?
            .unwrap_or_else(|| "All values".into());
        let value_result = optional_number_value(&row[value], &table.columns[value], row_index)?;
        let Some(value_result) = value_result else {
            if request.missing_treatment == MissingTreatment::Exclude {
                excluded += 1;
                continue;
            }
            return Err(format!("box plot row {} has a missing measurement; explicitly exclude it or use another view", row_index + 1));
        };
        if let Some((_, values)) = grouped.iter_mut().find(|(label, _)| *label == group) {
            values.push(value_result);
        } else {
            grouped.push((group, vec![value_result]));
        }
    }
    if grouped.len() > MAX_CATEGORIES {
        return Err(size_error(ChartKind::BoxPlot, grouped.len()));
    }
    let summaries = grouped
        .into_iter()
        .map(|(label, mut values)| box_summary(label, &mut values))
        .collect::<Result<Vec<_>, _>>()?;
    request.metadata.fields = group_column
        .map(|index| vec![table.columns[index].clone(), table.columns[value].clone()])
        .unwrap_or_else(|| vec![table.columns[value].clone()]);
    request.metadata.aggregation =
        Some("Tukey box plot: quartiles, 1.5×IQR whiskers, and outliers".into());
    apply_missing_treatment(request.missing_treatment, &mut request.metadata, excluded)?;
    let mut data = base_spec(ChartKind::BoxPlot, request, Vec::new(), Vec::new());
    data.y_label = Some(table.columns[value].clone());
    data.payload = Some(ChartPayload::BoxPlot { groups: summaries });
    validate(&data)?;
    Ok(data)
}

fn build_heatmap(table: &TabularResult, mut request: ChartRequest) -> Result<ChartData, String> {
    let x = column_index(table, request.x_field.as_deref(), 0)?;
    let y = column_index(
        table,
        request
            .group_field
            .as_deref()
            .or(request.y_field.as_deref()),
        1,
    )?;
    let value = column_index(table, request.value_field.as_deref(), 2)?;
    if x == y || x == value || y == value {
        return Err("heatmap needs distinct x, y, and value fields".into());
    }
    let mut x_labels = Vec::new();
    let mut y_labels = Vec::new();
    let mut cells: HashMap<(String, String), Option<f64>> = HashMap::new();
    let mut excluded = 0;
    for (row_index, row) in table.rows.iter().enumerate() {
        let x_label = label_value(&row[x], row_index)?;
        let y_label = label_value(&row[y], row_index)?;
        let number = optional_number_value(&row[value], &table.columns[value], row_index)?;
        if number.is_none() {
            match request.missing_treatment {
                MissingTreatment::Reject => {
                    return Err(format!("heatmap row {} has a missing value; select `gap` or `exclude` to keep it distinct from zero", row_index + 1));
                }
                MissingTreatment::Exclude => {
                    excluded += 1;
                    continue;
                }
                MissingTreatment::Gap => {}
            }
        }
        if !x_labels.contains(&x_label) {
            x_labels.push(x_label.clone());
        }
        if !y_labels.contains(&y_label) {
            y_labels.push(y_label.clone());
        }
        if cells
            .insert((x_label.clone(), y_label.clone()), number)
            .is_some()
        {
            return Err(format!("heatmap has multiple rows for ({x_label}, {y_label}); aggregate to one value per cell before charting"));
        }
    }
    if x_labels.len() > MAX_HEATMAP_AXIS {
        return Err(size_error(ChartKind::Heatmap, x_labels.len()));
    }
    if y_labels.len() > MAX_HEATMAP_AXIS {
        return Err(size_error(ChartKind::Heatmap, y_labels.len()));
    }
    if x_labels.is_empty() || y_labels.is_empty() {
        return Err("no heatmap cells remain after the selected missing-value treatment".into());
    }
    let values = y_labels
        .iter()
        .map(|y_label| {
            x_labels
                .iter()
                .map(|x_label| {
                    cells
                        .get(&(x_label.clone(), y_label.clone()))
                        .copied()
                        .flatten()
                })
                .collect()
        })
        .collect();
    request.metadata.fields = vec![
        table.columns[x].clone(),
        table.columns[y].clone(),
        table.columns[value].clone(),
    ];
    request.metadata.missing_treatment = Some(if excluded > 0 {
        format!("excluded {excluded} row(s) with missing values; unobserved cells remain blank, not zero")
    } else if request.missing_treatment == MissingTreatment::Gap {
        "null or unobserved cells shown blank; not coerced to zero".into()
    } else {
        "unobserved cells shown blank; not coerced to zero".into()
    });
    let mut data = base_spec(ChartKind::Heatmap, request, Vec::new(), Vec::new());
    data.x_label = Some(table.columns[x].clone());
    data.y_label = Some(table.columns[y].clone());
    data.payload = Some(ChartPayload::Heatmap {
        x_labels,
        y_labels,
        values,
    });
    validate(&data)?;
    Ok(data)
}

fn build_forecast_chart(
    table: &TabularResult,
    mut request: ChartRequest,
) -> Result<ChartData, String> {
    let period = column_index(table, request.x_field.as_deref(), 0)?;
    let fields = if request.series_fields.is_empty() {
        vec![
            "observed".to_string(),
            "forecast".to_string(),
            "lower".to_string(),
            "upper".to_string(),
        ]
    } else {
        request.series_fields.clone()
    };
    if fields.len() < 2 || fields.len() > 4 {
        return Err("forecast chart needs observed and forecast fields, with optional lower and upper bounds".into());
    }
    let indices = fields
        .iter()
        .map(|field| column_index(table, Some(field), 0))
        .collect::<Result<Vec<_>, _>>()?;
    let mut labels = Vec::new();
    let mut arrays = vec![Vec::new(); fields.len()];
    for (row_index, row) in table.rows.iter().enumerate() {
        labels.push(label_value(&row[period], row_index)?);
        for (series_index, column) in indices.iter().enumerate() {
            arrays[series_index].push(optional_number_value(
                &row[*column],
                &table.columns[*column],
                row_index,
            )?);
        }
    }
    ensure_unique_labels(&labels)?;
    if let Some(order) = chronological_order(&labels) {
        labels = order.iter().map(|index| labels[*index].clone()).collect();
        arrays = arrays
            .into_iter()
            .map(|values| order.iter().map(|index| values[*index]).collect())
            .collect();
    }
    request.metadata.fields = std::iter::once(table.columns[period].clone())
        .chain(indices.iter().map(|index| table.columns[*index].clone()))
        .collect();
    request.metadata.missing_treatment = Some(
        "null values separate observed and future periods; bands are shown only where available"
            .into(),
    );
    let observed = arrays[0].clone();
    let forecast = arrays[1].clone();
    let lower = arrays
        .get(2)
        .cloned()
        .unwrap_or_else(|| vec![None; labels.len()]);
    let upper = arrays
        .get(3)
        .cloned()
        .unwrap_or_else(|| vec![None; labels.len()]);
    let note = if lower.iter().all(Option::is_none) {
        Some("No calibrated uncertainty band is available for this forecast.".into())
    } else {
        None
    };
    let mut data = base_spec(ChartKind::Forecast, request, labels, Vec::new());
    data.x_label = Some(table.columns[period].clone());
    data.payload = Some(ChartPayload::Forecast {
        observed,
        forecast,
        lower,
        upper,
        uncertainty_note: note,
    });
    validate(&data)?;
    Ok(data)
}

fn base_spec(
    kind: ChartKind,
    request: ChartRequest,
    labels: Vec<String>,
    series: Vec<Series>,
) -> ChartData {
    ChartData {
        kind,
        title: request.title,
        labels,
        series,
        unit: request.unit,
        x_label: None,
        y_label: None,
        payload: None,
        metadata: Some(request.metadata),
    }
}

fn value_column(table: &TabularResult, request: &ChartRequest) -> Result<usize, String> {
    if let Some(field) = request
        .value_field
        .as_deref()
        .or(request.y_field.as_deref())
    {
        column_index(table, Some(field), 0)
    } else {
        (0..table.columns.len())
            .find(|index| {
                table
                    .rows
                    .iter()
                    .any(|row| number_value(&row[*index], &table.columns[*index], 0).is_ok())
            })
            .ok_or_else(|| "the result has no numeric value field; select one explicitly".into())
    }
}

fn optional_number_value(value: &Json, column: &str, row: usize) -> Result<Option<f64>, String> {
    if value.is_null() {
        return Ok(None);
    }
    number_value(value, column, row).map(Some)
}

fn ensure_unique_labels(labels: &[String]) -> Result<(), String> {
    let mut seen = HashSet::new();
    if let Some(duplicate) = labels
        .iter()
        .find(|label| !seen.insert(label.to_lowercase()))
    {
        return Err(format!("chart result contains duplicate label `{duplicate}`; aggregate or add a distinguishing field first"));
    }
    Ok(())
}

fn automatic_bin_count(values: &[f64]) -> usize {
    let min = values.iter().copied().fold(f64::INFINITY, f64::min);
    let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    if min == max {
        return 1;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let iqr = quantile(&sorted, 0.75) - quantile(&sorted, 0.25);
    let width = 2.0 * iqr / (values.len() as f64).cbrt();
    let count = if width > 0.0 {
        ((max - min) / width).ceil() as usize
    } else {
        (values.len() as f64).log2().ceil() as usize + 1
    };
    count.clamp(1, MAX_HISTOGRAM_BINS)
}

fn quantile(sorted: &[f64], probability: f64) -> f64 {
    let position = (sorted.len() - 1) as f64 * probability;
    let lower = position.floor() as usize;
    let upper = position.ceil() as usize;
    let fraction = position - lower as f64;
    sorted[lower] * (1.0 - fraction) + sorted[upper] * fraction
}

fn box_summary(label: String, values: &mut [f64]) -> Result<BoxSummary, String> {
    if values.is_empty() {
        return Err(format!("box plot group `{label}` has no measurements"));
    }
    values.sort_by(f64::total_cmp);
    let q1 = quantile(values, 0.25);
    let median = quantile(values, 0.5);
    let q3 = quantile(values, 0.75);
    let iqr = q3 - q1;
    let low_fence = q1 - 1.5 * iqr;
    let high_fence = q3 + 1.5 * iqr;
    let low_whisker = values
        .iter()
        .copied()
        .find(|value| *value >= low_fence)
        .unwrap_or(values[0]);
    let high_whisker = values
        .iter()
        .rev()
        .copied()
        .find(|value| *value <= high_fence)
        .unwrap_or(*values.last().unwrap());
    let outliers = values
        .iter()
        .copied()
        .filter(|value| *value < low_fence || *value > high_fence)
        .collect();
    Ok(BoxSummary {
        label,
        low_whisker,
        q1,
        median,
        q3,
        high_whisker,
        outliers,
        n: values.len(),
    })
}

fn format_number(value: f64) -> String {
    if value.abs() >= 1_000_000.0 || (value != 0.0 && value.abs() < 0.001) {
        format!("{value:.3e}")
    } else {
        format!("{value:.3}")
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string()
    }
}

/// Resolve the model's `auto` request using only the query shape. Temporal
/// labels read naturally as a line; categorical labels read naturally as
/// bars. The inference is intentionally conservative and deterministic so a
/// provider cannot inject a renderer or change the wire format.
fn resolve_kind(requested: ChartKind, label_column: &str, labels: &[String]) -> ChartKind {
    match requested {
        ChartKind::Auto => {
            if looks_temporal(label_column, labels) {
                ChartKind::Line
            } else {
                ChartKind::Bar
            }
        }
        concrete => concrete,
    }
}

fn max_points(kind: ChartKind) -> usize {
    match kind {
        ChartKind::Line | ChartKind::Area | ChartKind::StackedArea | ChartKind::Forecast => {
            MAX_TIME_POINTS
        }
        ChartKind::Pie | ChartKind::Donut => MAX_PIE_SLICES,
        ChartKind::Scatter => MAX_SCATTER_POINTS,
        ChartKind::Histogram => MAX_HISTOGRAM_BINS,
        ChartKind::Heatmap => MAX_HEATMAP_AXIS,
        ChartKind::Auto | ChartKind::Bar | ChartKind::BoxPlot => MAX_CATEGORIES,
    }
}

fn size_error(kind: ChartKind, rows: usize) -> String {
    match kind {
        ChartKind::Line | ChartKind::Area | ChartKind::StackedArea | ChartKind::Forecast => format!(
            "the chart query returned {rows} time-series points (max {MAX_TIME_POINTS}) -- aggregate to a coarser time period or narrow the date range only if that change matches the user's requested scope; otherwise keep the requested period and grain and explain the point limit"
        ),
        ChartKind::Pie | ChartKind::Donut => format!(
            "pie/donut chart has {rows} slices (max {MAX_PIE_SLICES}); use a bar chart or aggregate explicitly"
        ),
        ChartKind::Scatter => format!(
            "the scatter plot returned {rows} points (max {MAX_SCATTER_POINTS}) -- narrow scope or sample explicitly"
        ),
        ChartKind::Histogram => format!("the histogram has {rows} bins (max {MAX_HISTOGRAM_BINS})"),
        ChartKind::Heatmap => format!(
            "the heatmap has {rows} labels on an axis (max {MAX_HEATMAP_AXIS}) -- aggregate or narrow the scope"
        ),
        ChartKind::Auto | ChartKind::Bar | ChartKind::BoxPlot => format!(
            "the chart query returned {rows} categories (max {MAX_CATEGORIES}) -- aggregate first"
        ),
    }
}

fn looks_temporal(column: &str, labels: &[String]) -> bool {
    let name = column
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|part| !part.is_empty())
        .map(|part| part.to_ascii_lowercase())
        .collect::<Vec<_>>();
    if name.iter().any(|part| {
        matches!(
            part.as_str(),
            "date"
                | "datetime"
                | "day"
                | "month"
                | "quarter"
                | "time"
                | "timestamp"
                | "week"
                | "year"
        )
    }) {
        return true;
    }

    // SQL aliases are not always descriptive, so accept common ISO periods
    // and dates when most labels have the same temporal shape.
    let temporal_labels = labels.iter().filter(|label| looks_like_date(label)).count();
    temporal_labels * 2 >= labels.len().max(1)
}

fn looks_like_date(label: &str) -> bool {
    let label = label.trim();
    let bytes = label.as_bytes();
    let digits = |mut range: std::ops::Range<usize>| {
        range.end <= bytes.len() && range.all(|i| bytes[i].is_ascii_digit())
    };

    // YYYY, YYYY-MM, YYYY-MM-DD, and their slash-separated equivalents.
    if digits(0..4) && bytes.get(4).is_some_and(|b| *b == b'-' || *b == b'/') {
        if label.len() == 7 {
            return digits(5..7);
        }
        if label.len() >= 10
            && bytes.get(7).is_some_and(|b| *b == b'-' || *b == b'/')
            && digits(5..7)
            && digits(8..10)
        {
            return true;
        }
    }

    // A year-only grouping is also a time axis in analytics.
    label.len() == 4 && digits(0..4)
}

fn chronological_order(labels: &[String]) -> Option<Vec<usize>> {
    let mut indexed = labels
        .iter()
        .enumerate()
        .map(|(index, label)| Some((index, temporal_sort_key(label)?)))
        .collect::<Option<Vec<_>>>()?;
    indexed.sort_by_key(|(_, key)| *key);
    Some(indexed.into_iter().map(|(index, _)| index).collect())
}

fn temporal_sort_key(label: &str) -> Option<(i32, u32, u32)> {
    let normalized = label.trim().to_ascii_lowercase();
    let tokens = normalized
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    if tokens.is_empty() {
        return None;
    }

    let parse_year = |token: &str| {
        (token.len() == 4 && token.bytes().all(|byte| byte.is_ascii_digit()))
            .then(|| token.parse::<i32>().ok())
            .flatten()
    };
    let month_number = |token: &str| -> Option<u32> {
        let numeric = token
            .parse::<u32>()
            .ok()
            .filter(|month| (1..=12).contains(month));
        numeric.or_else(|| match token {
            "jan" | "january" => Some(1),
            "feb" | "february" => Some(2),
            "mar" | "march" => Some(3),
            "apr" | "april" => Some(4),
            "may" => Some(5),
            "jun" | "june" => Some(6),
            "jul" | "july" => Some(7),
            "aug" | "august" => Some(8),
            "sep" | "sept" | "september" => Some(9),
            "oct" | "october" => Some(10),
            "nov" | "november" => Some(11),
            "dec" | "december" => Some(12),
            _ => None,
        })
    };
    let quarter_number = |token: &str| {
        token
            .strip_prefix('q')
            .and_then(|quarter| quarter.parse::<u32>().ok())
            .filter(|quarter| (1..=4).contains(quarter))
            .map(|quarter| (quarter - 1) * 3 + 1)
    };

    if tokens.len() == 1 {
        if let Some(year) = parse_year(tokens[0]) {
            return Some((year, 0, 0));
        }
        if let Some(month) = month_number(tokens[0]) {
            return Some((0, month, 0));
        }
        if let Some(month) = quarter_number(tokens[0]) {
            return Some((0, month, 0));
        }
        return None;
    }

    if let Some(year) = parse_year(tokens[0]) {
        if let Some(month) = month_number(tokens[1]).or_else(|| quarter_number(tokens[1])) {
            let day = tokens
                .get(2)
                .and_then(|day| day.parse::<u32>().ok())
                .unwrap_or(0);
            return Some((year, month, day));
        }
    }
    if let Some(month) = month_number(tokens[0]).or_else(|| quarter_number(tokens[0])) {
        if let Some(year) = tokens.get(1).and_then(|token| parse_year(token)) {
            return Some((year, month, 0));
        }
    }
    None
}

fn label_value(value: &Json, row: usize) -> Result<String, String> {
    let label = match value {
        Json::String(s) => s.clone(),
        Json::Number(n) => n.to_string(),
        Json::Bool(b) => b.to_string(),
        Json::Null => return Err(format!("chart query row {} has an empty label", row + 1)),
        Json::Array(_) | Json::Object(_) => {
            return Err(format!(
                "chart query row {} has a non-scalar label",
                row + 1
            ))
        }
    };
    if label.trim().is_empty() {
        return Err(format!("chart query row {} has an empty label", row + 1));
    }
    Ok(label)
}

fn number_value(value: &Json, column: &str, row: usize) -> Result<f64, String> {
    let number = match value {
        Json::Number(n) => n.as_f64(),
        // DuckDB can serialize decimal values as strings. Accept only strings
        // that are unambiguously numeric; arbitrary text must fail loudly.
        Json::String(s) => s.trim().parse::<f64>().ok(),
        Json::Null | Json::Bool(_) | Json::Array(_) | Json::Object(_) => None,
    };
    match number.filter(|n| n.is_finite()) {
        Some(n) => Ok(n),
        None => Err(format!(
            "chart query row {} column \"{column}\" is not a finite number",
            row + 1
        )),
    }
}

/// Rejects shapes that can't be charted meaningfully. Doesn't reject
/// degenerate-but-valid data (a single category, negative values) -- the
/// frontend renders those fine.
pub fn validate(data: &ChartData) -> Result<(), String> {
    let specialized = matches!(
        data.kind,
        ChartKind::Scatter | ChartKind::BoxPlot | ChartKind::Heatmap | ChartKind::Forecast
    );
    if !specialized && data.labels.is_empty() {
        return Err("labels is empty -- nothing to chart".into());
    }
    if !specialized && data.series.is_empty() {
        return Err("series is empty -- nothing to chart".into());
    }
    if data.labels.len() > max_points(data.kind) {
        return Err(size_error(data.kind, data.labels.len()));
    }
    if data.series.len() > MAX_SERIES {
        return Err(format!(
            "{} series is too many (max {MAX_SERIES})",
            data.series.len()
        ));
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
        if let Some(bad) = s.values.iter().flatten().find(|v| !v.is_finite()) {
            return Err(format!(
                "series \"{}\" has a non-finite value ({bad})",
                s.name
            ));
        }
    }
    if matches!(data.kind, ChartKind::Pie | ChartKind::Donut) {
        if data.series.len() != 1 {
            return Err(
                "pie/donut charts require one value series representing parts of the same whole"
                    .into(),
            );
        }
        if !data.metadata.as_ref().is_some_and(|metadata| {
            metadata.part_to_whole
                && metadata
                    .denominator
                    .as_deref()
                    .is_some_and(|value| !value.trim().is_empty())
        }) {
            return Err("pie/donut charts require an explicit part-to-whole denominator".into());
        }
        if data.labels.len() > MAX_PIE_SLICES {
            return Err(format!("pie/donut charts support at most {MAX_PIE_SLICES} slices; use a bar chart or aggregate explicitly"));
        }
        if data.series[0].values.iter().any(Option::is_none) {
            return Err("pie/donut values contain missing slices; resolve or explicitly exclude missing values first".into());
        }
        if data.series[0]
            .values
            .iter()
            .flatten()
            .any(|value| *value < 0.0)
        {
            return Err("pie/donut values must be non-negative parts of one whole; use bars for signed values".into());
        }
        if data.series[0].values.iter().flatten().sum::<f64>() <= 0.0 {
            return Err("pie/donut values must have a positive total".into());
        }
    }
    if data.kind == ChartKind::StackedArea
        && data
            .series
            .iter()
            .any(|series| series.values.iter().flatten().any(|value| *value < 0.0))
    {
        return Err("stacked-area charts require non-negative values; use a line or unstacked area chart for signed measures".into());
    }
    if let Some(payload) = &data.payload {
        validate_payload(data, payload)?;
    } else if specialized {
        return Err(format!(
            "{} chart data is missing its typed payload",
            chart_kind_label(data.kind)
        ));
    }
    if matches!(
        data.kind,
        ChartKind::Bar | ChartKind::Line | ChartKind::Area | ChartKind::StackedArea
    ) && !meaningfully_varies(data)
    {
        return Err(
            "these values are all within a couple percent of each other -- a chart won't \
show anything a sentence wouldn't. Answer in prose or a small table instead of charting \
flat data."
                .into(),
        );
    }
    Ok(())
}

pub fn chart_kind_label(kind: ChartKind) -> &'static str {
    match kind {
        ChartKind::Bar => "bar",
        ChartKind::Line => "line",
        ChartKind::Pie => "pie",
        ChartKind::Donut => "donut",
        ChartKind::Histogram => "histogram",
        ChartKind::Scatter => "scatter",
        ChartKind::BoxPlot => "box plot",
        ChartKind::Area => "area",
        ChartKind::StackedArea => "stacked area",
        ChartKind::Heatmap => "heatmap",
        ChartKind::Forecast => "forecast",
        ChartKind::Auto => "automatic",
    }
}

pub fn visualization_count(data: &ChartData) -> usize {
    match data.payload.as_ref() {
        Some(ChartPayload::Scatter { points }) => points.len(),
        Some(ChartPayload::BoxPlot { groups }) => groups.len(),
        Some(ChartPayload::Heatmap {
            x_labels, y_labels, ..
        }) => x_labels.len() * y_labels.len(),
        Some(ChartPayload::Forecast { forecast, .. }) => {
            forecast.iter().filter(|value| value.is_some()).count()
        }
        None => data.labels.len(),
    }
}

fn validate_payload(data: &ChartData, payload: &ChartPayload) -> Result<(), String> {
    match (data.kind, payload) {
        (ChartKind::Scatter, ChartPayload::Scatter { points }) => {
            if points.is_empty() {
                return Err("scatter plot has no complete x/y pairs".into());
            }
            if points.len() > MAX_SCATTER_POINTS {
                return Err(size_error(ChartKind::Scatter, points.len()));
            }
            if points
                .iter()
                .any(|point| !point.x.is_finite() || !point.y.is_finite())
            {
                return Err("scatter plot contains a non-finite coordinate".into());
            }
        }
        (ChartKind::BoxPlot, ChartPayload::BoxPlot { groups }) => {
            if groups.is_empty() || groups.len() > MAX_CATEGORIES {
                return Err(size_error(ChartKind::BoxPlot, groups.len()));
            }
            for group in groups {
                if group.n == 0
                    || ![
                        group.low_whisker,
                        group.q1,
                        group.median,
                        group.q3,
                        group.high_whisker,
                    ]
                    .iter()
                    .all(|value| value.is_finite())
                    || group.low_whisker > group.q1
                    || group.q1 > group.median
                    || group.median > group.q3
                    || group.q3 > group.high_whisker
                    || group.outliers.iter().any(|value| !value.is_finite())
                {
                    return Err(format!("box plot summary for `{}` is invalid", group.label));
                }
            }
        }
        (
            ChartKind::Heatmap,
            ChartPayload::Heatmap {
                x_labels,
                y_labels,
                values,
            },
        ) => {
            if x_labels.is_empty()
                || y_labels.is_empty()
                || x_labels.len() > MAX_HEATMAP_AXIS
                || y_labels.len() > MAX_HEATMAP_AXIS
                || values.len() != y_labels.len()
                || values.iter().any(|row| row.len() != x_labels.len())
            {
                return Err(
                    "heatmap dimensions do not match its labels or exceed the axis limit".into(),
                );
            }
            if values
                .iter()
                .flatten()
                .flatten()
                .any(|value| !value.is_finite())
            {
                return Err("heatmap contains a non-finite value".into());
            }
            if values.iter().flatten().flatten().next().is_none() {
                return Err("heatmap has no observed values".into());
            }
        }
        (
            ChartKind::Forecast,
            ChartPayload::Forecast {
                observed,
                forecast,
                lower,
                upper,
                ..
            },
        ) => {
            let n = data.labels.len();
            if n == 0
                || [observed, forecast, lower, upper]
                    .iter()
                    .any(|series| series.len() != n)
            {
                return Err("forecast chart arrays must align with the period labels".into());
            }
            if [observed, forecast, lower, upper]
                .iter()
                .flat_map(|series| series.iter().flatten())
                .any(|value| !value.is_finite())
            {
                return Err("forecast chart contains a non-finite value".into());
            }
            if forecast.iter().all(Option::is_none) || observed.iter().all(Option::is_none) {
                return Err(
                    "forecast chart needs both observed history and projected values".into(),
                );
            }
            for index in 0..n {
                match (lower[index], upper[index], forecast[index]) {
                    (Some(low), Some(high), Some(point)) if low <= point && point <= high => {}
                    (None, None, _) => {}
                    (Some(_), Some(_), None) => return Err("uncertainty bounds exist without a forecast point".into()),
                    _ => return Err("forecast lower and upper bounds must be paired and contain the point estimate".into()),
                }
            }
        }
        _ => return Err("chart type does not match its typed data payload".into()),
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
        let values: Vec<f64> = s.values.iter().flatten().copied().collect();
        if values.len() < 2 {
            return false;
        }
        let lo = values.iter().copied().fold(f64::INFINITY, f64::min);
        let hi = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let scale = values.iter().fold(0.0f64, |m, v| m.max(v.abs())).max(1.0);
        (hi - lo) / scale >= MIN_VARIATION
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_metadata_lists_serialize_as_empty_arrays_for_the_ui_contract() {
        let value = serde_json::to_value(ChartMetadata::default()).unwrap();

        assert_eq!(value["fields"], Json::Array(Vec::new()));
        assert_eq!(value["filters"], Json::Array(Vec::new()));
    }

    #[test]
    fn missing_chart_field_error_names_the_available_result_fields() {
        let table = TabularResult {
            columns: vec!["category".into(), "total_spending".into()],
            rows: vec![vec![Json::from("Rent"), Json::from(1200.0)]],
        };

        let error = column_index(&table, Some("measure_0"), 0).unwrap_err();

        assert!(error.contains("measure_0"));
        assert!(error.contains("`category`"));
        assert!(error.contains("`total_spending`"));
    }

    fn bar(labels: &[&str], series: Vec<(&str, Vec<f64>)>) -> ChartData {
        ChartData {
            kind: ChartKind::Bar,
            title: Some("Test".into()),
            labels: labels.iter().map(|s| s.to_string()).collect(),
            series: series
                .into_iter()
                .map(|(name, values)| Series {
                    name: name.into(),
                    values: values.into_iter().map(Some).collect(),
                })
                .collect(),
            unit: None,
            x_label: None,
            y_label: None,
            payload: None,
            metadata: None,
        }
    }

    #[test]
    fn from_query_uses_the_first_column_as_labels_and_the_rest_as_series() {
        let data = from_query(
            ChartKind::Bar,
            Some("Spending".into()),
            Some("$".into()),
            &["category".into(), "amount".into()],
            &[
                vec![Json::from("Rent"), Json::from(1250.0)],
                vec![Json::from("Groceries"), Json::from(412.5)],
            ],
        )
        .unwrap();

        assert_eq!(data.labels, vec!["Rent", "Groceries"]);
        assert_eq!(data.series[0].name, "amount");
        assert_eq!(data.series[0].values, vec![Some(1250.0), Some(412.5)]);
    }

    #[test]
    fn from_query_allows_more_than_category_limit_for_time_series() {
        let rows: Vec<Vec<Json>> = (1..=13)
            .map(|day| {
                vec![
                    Json::from(format!("2024-01-{day:02}")),
                    Json::from(day as f64),
                ]
            })
            .collect();
        let data = from_query(
            ChartKind::Auto,
            None,
            None,
            &["date".into(), "value".into()],
            &rows,
        )
        .unwrap();

        assert_eq!(data.kind, ChartKind::Line);
        assert_eq!(data.labels.len(), 13);
        assert_eq!(data.series[0].values.len(), 13);
    }

    #[test]
    fn from_query_keeps_a_large_time_series_bounded() {
        let rows: Vec<Vec<Json>> = (0..=MAX_TIME_POINTS)
            .map(|point| vec![Json::from(point), Json::from(point as f64)])
            .collect();
        let error = from_query(
            ChartKind::Line,
            None,
            None,
            &["date".into(), "value".into()],
            &rows,
        )
        .unwrap_err();

        assert!(error.contains("max 1000"), "{error}");
        assert!(error.contains("coarser time period"), "{error}");
    }

    #[test]
    fn from_table_pivots_long_form_series_and_keeps_absent_pairs_as_gaps() {
        let chart = from_table(
            &TabularResult {
                columns: vec!["month".into(), "channel".into(), "visits".into()],
                rows: vec![
                    vec![Json::from("2025-01"), Json::from("Direct"), Json::from(10)],
                    vec![Json::from("2025-01"), Json::from("Search"), Json::from(5)],
                    vec![Json::from("2025-02"), Json::from("Direct"), Json::from(12)],
                ],
            },
            ChartRequest {
                kind: ChartKind::Line,
                x_field: Some("month".into()),
                group_field: Some("channel".into()),
                value_field: Some("visits".into()),
                missing_treatment: MissingTreatment::Gap,
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(chart.labels, vec!["2025-01", "2025-02"]);
        assert_eq!(chart.series.len(), 2);
        assert_eq!(chart.series[0].name, "Direct");
        assert_eq!(chart.series[0].values, vec![Some(10.0), Some(12.0)]);
        assert_eq!(chart.series[1].name, "Search");
        assert_eq!(chart.series[1].values, vec![Some(5.0), None]);
        assert!(chart
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.missing_treatment.as_deref())
            .is_some_and(|note| note.contains("gaps, not zero")));
    }

    #[test]
    fn from_table_rejects_duplicate_long_form_pairs_instead_of_summing_silently() {
        let error = from_table(
            &TabularResult {
                columns: vec!["date".into(), "group".into(), "value".into()],
                rows: vec![
                    vec![Json::from("2025-01-01"), Json::from("A"), Json::from(10)],
                    vec![Json::from("2025-01-01"), Json::from("A"), Json::from(12)],
                ],
            },
            ChartRequest {
                kind: ChartKind::Line,
                x_field: Some("date".into()),
                group_field: Some("group".into()),
                value_field: Some("value".into()),
                missing_treatment: MissingTreatment::Gap,
                ..Default::default()
            },
        )
        .unwrap_err();

        assert!(error.contains("multiple rows"), "{error}");
        assert!(
            error.contains("aggregate to one value per x/group pair"),
            "{error}"
        );
    }

    #[test]
    fn from_query_rejects_a_non_numeric_series_value() {
        let result = from_query(
            ChartKind::Bar,
            None,
            None,
            &["category".into(), "amount".into()],
            &[vec![Json::from("Rent"), Json::from("unknown")]],
        );

        assert!(result.unwrap_err().contains("amount"));
    }

    #[test]
    fn serializes_to_the_wire_shape_the_frontend_expects() {
        let data = ChartData {
            kind: ChartKind::Bar,
            title: Some("Spending".into()),
            labels: vec!["Groceries".into()],
            series: vec![Series {
                name: "amount".into(),
                values: vec![Some(412.5)],
            }],
            unit: Some("$".into()),
            x_label: None,
            y_label: None,
            payload: None,
            metadata: None,
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
        let v = serde_json::to_value(ChartData {
            title: None,
            unit: None,
            ..data
        })
        .unwrap();
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
            series: vec![Series {
                name: "s".into(),
                values: vec![Some(1.0); labels.len()],
            }],
            unit: None,
            x_label: None,
            y_label: None,
            payload: None,
            metadata: None,
        };
        assert!(validate(&data).is_err());
    }

    #[test]
    fn validate_rejects_too_many_series() {
        let data = ChartData {
            kind: ChartKind::Bar,
            title: None,
            labels: vec!["a".into()],
            series: (0..=MAX_SERIES)
                .map(|index| Series {
                    name: format!("series-{index}"),
                    values: vec![Some(1.0)],
                })
                .collect(),
            unit: None,
            x_label: None,
            y_label: None,
            payload: None,
            metadata: None,
        };
        assert!(validate(&data).is_err());
    }

    #[test]
    fn validate_rejects_empty_input() {
        let empty_labels = ChartData {
            kind: ChartKind::Bar,
            title: None,
            labels: vec![],
            series: vec![Series {
                name: "s".into(),
                values: vec![],
            }],
            unit: None,
            x_label: None,
            y_label: None,
            payload: None,
            metadata: None,
        };
        assert!(validate(&empty_labels).is_err());

        let empty_series = ChartData {
            kind: ChartKind::Bar,
            title: None,
            labels: vec!["a".into()],
            series: vec![],
            unit: None,
            x_label: None,
            y_label: None,
            payload: None,
            metadata: None,
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
        let mut values = vec![Some(1316.0); 11];
        values.push(Some(1321.0));
        let labels: Vec<String> = (0..12).map(|i| format!("m{i}")).collect();
        let data = ChartData {
            kind: ChartKind::Bar,
            title: None,
            labels,
            series: vec![Series {
                name: "rent".into(),
                values,
            }],
            unit: Some("$".into()),
            x_label: None,
            y_label: None,
            payload: None,
            metadata: None,
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
            vec![
                ("flat", vec![10.0, 10.0, 10.0]),
                ("varies", vec![5.0, 50.0, 8.0]),
            ],
        );
        assert!(validate(&data).is_ok());
    }

    fn table(columns: &[&str], rows: &[&[Json]]) -> TabularResult {
        TabularResult {
            columns: columns.iter().map(|column| (*column).into()).collect(),
            rows: rows.iter().map(|row| row.to_vec()).collect(),
        }
    }

    #[test]
    fn pie_requires_part_to_whole_and_a_named_denominator() {
        let input = table(
            &["category", "amount"],
            &[
                &[Json::from("Rent"), Json::from(1200)],
                &[Json::from("Food"), Json::from(300)],
            ],
        );
        let missing_whole = ChartRequest {
            kind: ChartKind::Pie,
            ..Default::default()
        };
        assert!(from_table(&input, missing_whole)
            .unwrap_err()
            .contains("part-to-whole"));

        let valid = ChartRequest {
            kind: ChartKind::Donut,
            metadata: ChartMetadata {
                part_to_whole: true,
                denominator: Some("total household spending".into()),
                ..Default::default()
            },
            ..Default::default()
        };
        let data = from_table(&input, valid).unwrap();
        assert_eq!(data.kind, ChartKind::Donut);
        assert_eq!(data.series[0].values, vec![Some(1200.0), Some(300.0)]);
        assert_eq!(
            data.metadata.unwrap().denominator.as_deref(),
            Some("total household spending")
        );
    }

    #[test]
    fn pie_rejects_negative_and_overly_many_slices_instead_of_relabeling_them() {
        let signed = table(
            &["category", "amount"],
            &[
                &[Json::from("credits"), Json::from(25)],
                &[Json::from("debits"), Json::from(-10)],
            ],
        );
        let request = || ChartRequest {
            kind: ChartKind::Pie,
            metadata: ChartMetadata {
                part_to_whole: true,
                denominator: Some("net balance".into()),
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(from_table(&signed, request())
            .unwrap_err()
            .contains("non-negative"));

        let many: Vec<Vec<Json>> = (0..=MAX_PIE_SLICES)
            .map(|index| vec![Json::from(format!("item-{index}")), Json::from(index + 1)])
            .collect();
        let input = TabularResult {
            columns: vec!["category".into(), "amount".into()],
            rows: many,
        };
        assert!(from_table(&input, request()).unwrap_err().contains("max 8"));
    }

    #[test]
    fn scatter_uses_paired_numeric_fields_and_explicitly_excludes_incomplete_pairs() {
        let input = table(
            &["units", "revenue", "region"],
            &[
                &[Json::from(2), Json::from(20), Json::from("East")],
                &[Json::from(3), Json::Null, Json::from("West")],
                &[Json::from(5), Json::from(50), Json::from("East")],
            ],
        );
        let request = ChartRequest {
            kind: ChartKind::Scatter,
            group_field: Some("region".into()),
            label_field: Some("units".into()),
            missing_treatment: MissingTreatment::Exclude,
            ..Default::default()
        };
        let data = from_table(&input, request).unwrap();
        let Some(ChartPayload::Scatter { points }) = data.payload else {
            panic!("expected typed scatter points")
        };
        assert_eq!(points.len(), 2);
        assert_eq!((points[1].x, points[1].y), (5.0, 50.0));
        assert_eq!(points[0].group.as_deref(), Some("East"));
        assert!(data
            .metadata
            .unwrap()
            .missing_treatment
            .unwrap()
            .contains("excluded 1"));
    }

    #[test]
    fn histogram_preserves_every_observation_and_reports_its_bin_count() {
        let input = table(
            &["reading"],
            &[
                &[Json::from(1)],
                &[Json::from(2)],
                &[Json::from(3)],
                &[Json::from(4)],
                &[Json::from(5)],
            ],
        );
        let data = from_table(
            &input,
            ChartRequest {
                kind: ChartKind::Histogram,
                bin_count: Some(2),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(data.labels.len(), 2);
        assert_eq!(data.series[0].values.iter().flatten().sum::<f64>(), 5.0);
        assert_eq!(
            data.metadata.unwrap().aggregation.as_deref(),
            Some("frequency; 2 equal-width bins")
        );
    }

    #[test]
    fn histogram_preserves_precomputed_bin_labels_and_frequencies() {
        let input = table(
            &["bin", "count"],
            &[
                &[Json::from("10–25"), Json::from(4)],
                &[Json::from("25–40"), Json::from(2)],
            ],
        );
        let data = from_table(
            &input,
            ChartRequest {
                kind: ChartKind::Histogram,
                x_field: Some("bin".into()),
                y_field: Some("count".into()),
                bin_count: Some(2),
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(data.kind, ChartKind::Histogram);
        assert_eq!(data.labels, vec!["10–25", "25–40"]);
        assert_eq!(data.series[0].name, "Count");
        assert_eq!(data.series[0].values, vec![Some(4.0), Some(2.0)]);
        assert_eq!(data.x_label.as_deref(), Some("bin"));
        assert_eq!(data.y_label.as_deref(), Some("Frequency"));
        assert!(data
            .metadata
            .unwrap()
            .aggregation
            .unwrap()
            .contains("precomputed"));
    }

    #[test]
    fn box_plot_reports_interpolated_quartiles_whiskers_and_outliers() {
        let input = table(
            &["team", "latency"],
            &[
                &[Json::from("A"), Json::from(1)],
                &[Json::from("A"), Json::from(2)],
                &[Json::from("A"), Json::from(3)],
                &[Json::from("A"), Json::from(4)],
                &[Json::from("A"), Json::from(5)],
                &[Json::from("A"), Json::from(40)],
            ],
        );
        let data = from_table(
            &input,
            ChartRequest {
                kind: ChartKind::BoxPlot,
                group_field: Some("team".into()),
                ..Default::default()
            },
        )
        .unwrap();
        let Some(ChartPayload::BoxPlot { groups }) = data.payload else {
            panic!("expected box plot summary")
        };
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].n, 6);
        assert_eq!(groups[0].median, 3.5);
        assert_eq!(groups[0].low_whisker, 1.0);
        assert_eq!(groups[0].high_whisker, 5.0);
        assert_eq!(groups[0].outliers, vec![40.0]);
    }

    #[test]
    fn heatmap_keeps_unobserved_cells_blank_and_rejects_duplicate_cells() {
        let input = table(
            &["month", "region", "revenue"],
            &[
                &[Json::from("Jan"), Json::from("East"), Json::from(10)],
                &[Json::from("Feb"), Json::from("East"), Json::from(20)],
                &[Json::from("Jan"), Json::from("West"), Json::Null],
            ],
        );
        let data = from_table(
            &input,
            ChartRequest {
                kind: ChartKind::Heatmap,
                missing_treatment: MissingTreatment::Gap,
                ..Default::default()
            },
        )
        .unwrap();
        let Some(ChartPayload::Heatmap { values, .. }) = data.payload else {
            panic!("expected heatmap matrix")
        };
        assert_eq!(values, vec![vec![Some(10.0), Some(20.0)], vec![None, None]]);

        let duplicate = table(
            &["month", "region", "revenue"],
            &[
                &[Json::from("Jan"), Json::from("East"), Json::from(10)],
                &[Json::from("Jan"), Json::from("East"), Json::from(12)],
            ],
        );
        assert!(from_table(
            &duplicate,
            ChartRequest {
                kind: ChartKind::Heatmap,
                ..Default::default()
            }
        )
        .unwrap_err()
        .contains("multiple rows"));
    }

    #[test]
    fn forecast_keeps_observed_projected_and_uncertainty_series_distinct() {
        let input = table(
            &["period", "observed", "forecast", "lower", "upper"],
            &[
                &[
                    Json::from("Jan"),
                    Json::from(10),
                    Json::Null,
                    Json::Null,
                    Json::Null,
                ],
                &[
                    Json::from("Feb"),
                    Json::from(12),
                    Json::Null,
                    Json::Null,
                    Json::Null,
                ],
                &[
                    Json::from("Mar"),
                    Json::Null,
                    Json::from(15),
                    Json::from(13),
                    Json::from(17),
                ],
            ],
        );
        let data = from_table(
            &input,
            ChartRequest {
                kind: ChartKind::Forecast,
                ..Default::default()
            },
        )
        .unwrap();
        let Some(ChartPayload::Forecast {
            observed,
            forecast,
            lower,
            upper,
            uncertainty_note,
        }) = data.payload
        else {
            panic!("expected forecast payload")
        };
        assert_eq!(observed, vec![Some(10.0), Some(12.0), None]);
        assert_eq!(forecast, vec![None, None, Some(15.0)]);
        assert_eq!(lower, vec![None, None, Some(13.0)]);
        assert_eq!(upper, vec![None, None, Some(17.0)]);
        assert!(uncertainty_note.is_none());
    }

    #[test]
    fn nullable_series_values_are_explicit_gaps_and_not_zeroes() {
        let input = table(
            &["month", "value"],
            &[
                &[Json::from("Jan"), Json::from(10)],
                &[Json::from("Feb"), Json::Null],
                &[Json::from("Mar"), Json::from(30)],
            ],
        );
        let gap = from_table(
            &input,
            ChartRequest {
                kind: ChartKind::Line,
                missing_treatment: MissingTreatment::Gap,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(gap.series[0].values, vec![Some(10.0), None, Some(30.0)]);
        assert!(from_table(
            &input,
            ChartRequest {
                kind: ChartKind::Line,
                ..Default::default()
            }
        )
        .is_err());
    }

    #[test]
    fn area_and_stacked_area_keep_the_requested_series_shape() {
        let input = table(
            &["month", "base", "added"],
            &[
                &[Json::from("Jan"), Json::from(10), Json::from(1)],
                &[Json::from("Feb"), Json::from(20), Json::from(4)],
                &[Json::from("Mar"), Json::from(15), Json::from(2)],
            ],
        );
        let area_chart = from_table(
            &input,
            ChartRequest {
                kind: ChartKind::Area,
                series_fields: vec!["base".into()],
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(area_chart.kind, ChartKind::Area);
        assert_eq!(
            area_chart.series[0].values,
            vec![Some(10.0), Some(20.0), Some(15.0)]
        );

        let stacked = from_table(
            &input,
            ChartRequest {
                kind: ChartKind::StackedArea,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(stacked.kind, ChartKind::StackedArea);
        assert_eq!(stacked.series.len(), 2);
        assert!(validate(&stacked).is_ok());
    }

    #[test]
    fn explicit_y_field_excludes_numeric_helper_columns_from_series_charts() {
        let input = table(
            &["month", "sort_order", "revenue"],
            &[
                &[Json::from("Jan"), Json::from(1), Json::from(20)],
                &[Json::from("Feb"), Json::from(2), Json::from(30)],
            ],
        );
        let data = from_table(
            &input,
            ChartRequest {
                kind: ChartKind::Area,
                x_field: Some("month".into()),
                y_field: Some("revenue".into()),
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(data.series.len(), 1);
        assert_eq!(data.series[0].name, "revenue");
        assert_eq!(data.series[0].values, vec![Some(20.0), Some(30.0)]);
    }

    #[test]
    fn named_months_on_a_month_axis_are_sorted_chronologically() {
        let input = table(
            &["month", "revenue"],
            &[
                &[Json::from("Feb"), Json::from(30)],
                &[Json::from("Jan"), Json::from(20)],
                &[Json::from("Mar"), Json::from(25)],
            ],
        );
        let data = from_table(
            &input,
            ChartRequest {
                kind: ChartKind::Area,
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(data.labels, vec!["Jan", "Feb", "Mar"]);
        assert_eq!(
            data.series[0].values,
            vec![Some(20.0), Some(30.0), Some(25.0)]
        );
    }

    #[test]
    fn heatmap_exclude_removes_missing_rows_and_records_the_exclusion() {
        let input = table(
            &["month", "region", "value"],
            &[
                &[Json::from("Jan"), Json::from("East"), Json::from(10)],
                &[Json::from("Feb"), Json::from("East"), Json::Null],
                &[Json::from("Jan"), Json::from("West"), Json::from(5)],
            ],
        );
        let chart = from_table(
            &input,
            ChartRequest {
                kind: ChartKind::Heatmap,
                missing_treatment: MissingTreatment::Exclude,
                ..Default::default()
            },
        )
        .unwrap();
        let Some(ChartPayload::Heatmap {
            x_labels, values, ..
        }) = chart.payload
        else {
            panic!("expected heatmap matrix")
        };
        assert_eq!(x_labels, vec!["Jan"]);
        assert_eq!(values, vec![vec![Some(10.0)], vec![Some(5.0)]]);
        assert!(chart
            .metadata
            .unwrap()
            .missing_treatment
            .unwrap()
            .contains("excluded 1"));
    }
}
