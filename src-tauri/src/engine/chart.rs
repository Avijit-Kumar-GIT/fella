//! A deterministic bar/line chart tool: `make_chart`. Renders an inline SVG
//! from labels + one or more numeric series the model already has (e.g. from
//! a prior `run_sql`) -- no LLM-authored markup trusted directly, only
//! numbers and short strings interpolated through `esc()`. Ports the
//! horizontal-bar layout math already proven in `bench/chart.py`'s `_bars()`
//! to Rust; colors reuse the app's existing near-monochrome theme tokens
//! (`var(--text)` / `var(--text-dim)`) rather than inventing a palette.

use serde::Deserialize;
use serde_json::Value as Json;

use crate::engine::error::{EngineError, EngineResult};
use crate::engine::state::EngineState;
use crate::engine::tools::{Tool, ToolOutput};

/// A folder's worth of tables rarely needs more than this many categories in
/// one chart before it stops being readable; past this, tell the model to
/// aggregate first.
const MAX_CATEGORIES: usize = 12;
/// Two series is already two colors/dash-styles on a near-monochrome
/// palette (see `src/app.css`); a third would need a real color system this
/// isn't building yet.
const MAX_SERIES: usize = 2;
/// `ponytail:` approximate character-width heuristic, not real font
/// metrics (no font-measurement crate here) -- good enough to keep labels
/// from usually overrunning their column, upgrade to real text measurement
/// if a label regularly overflows in practice.
const MAX_LABEL_CHARS: usize = 18;

const WIDTH: f64 = 520.0;
const PAD_L: f64 = 150.0;
const PAD_R: f64 = 52.0;
const ROW: f64 = 30.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChartKind {
    Bar,
    Line,
}

pub struct Series {
    pub name: String,
    pub values: Vec<f64>,
}

pub struct ChartData {
    pub kind: ChartKind,
    pub title: Option<String>,
    pub labels: Vec<String>,
    pub series: Vec<Series>,
    pub unit: Option<String>,
}

/// Rejects shapes that can't be charted meaningfully. Doesn't reject
/// degenerate-but-valid data (a single category, all-zero values, negative
/// values) -- `render_svg` handles those gracefully instead.
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

/// XML-escape: the one load-bearing safety control every interpolated
/// string (title, labels, series names, formatted values) goes through
/// before it reaches the SVG text. This is Rust-generated markup, not
/// LLM-authored, but a model-supplied label could still echo file content
/// verbatim, so nothing skips this.
fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// Truncates a label for display only -- the full label stays in the
/// tool's `result_summary`, so nothing is lost from the model's view, only
/// from the drawn chart. `max_chars` lets a caller with less horizontal
/// room (the line chart's x-axis) truncate tighter than the default.
fn short_label(s: &str, max_chars: usize) -> String {
    let mut it = s.chars();
    let head: String = it.by_ref().take(max_chars).collect();
    if it.next().is_some() {
        format!("{head}…")
    } else {
        head
    }
}

fn fmt_value(v: f64, unit: Option<&str>) -> String {
    let body = if v.fract() == 0.0 { format!("{v:.0}") } else { format!("{v:.2}") };
    match unit {
        Some(u) if u == "$" || u == "€" || u == "£" => format!("{u}{body}"),
        Some(u) => format!("{body}{u}"),
        None => body,
    }
}

/// The lowest/highest value across every series, zero-inclusive (a chart
/// with only positive values still baselines at zero, matching how a bar
/// chart reads). `(0.0, 1.0)` for no data at all; a flat span (e.g.
/// all-zero values) still gets a visible span rather than collapsing.
fn value_range(data: &ChartData) -> (f64, f64) {
    let mut lo = 0.0f64;
    let mut hi = 0.0f64;
    let mut any = false;
    for s in &data.series {
        for &v in &s.values {
            if !any {
                lo = v;
                hi = v;
                any = true;
            } else {
                lo = lo.min(v);
                hi = hi.max(v);
            }
        }
    }
    if !any {
        (0.0, 1.0)
    } else if (hi - lo).abs() < f64::EPSILON {
        (lo.min(0.0), if hi == 0.0 { 1.0 } else { hi })
    } else {
        (lo.min(0.0), hi.max(0.0))
    }
}

/// Draws the title (if any) and a legend (if more than one series), and
/// returns the vertical space they took up so the caller knows where the
/// plot area starts.
fn draw_header(p: &mut String, data: &ChartData) -> f64 {
    let mut y = 4.0;
    if let Some(t) = &data.title {
        y += 14.0;
        p.push_str(&format!(
            "<text x=\"4\" y=\"{y:.1}\" font-size=\"12\" font-weight=\"600\" fill=\"var(--text)\">{}</text>",
            esc(t)
        ));
    }
    if data.series.len() > 1 {
        y += 14.0;
        let mut lx = 4.0;
        for (si, s) in data.series.iter().enumerate() {
            let fill = if si == 0 { "var(--border-strong)" } else { "var(--text-dim)" };
            p.push_str(&format!(
                "<rect x=\"{lx:.1}\" y=\"{:.1}\" width=\"7\" height=\"7\" rx=\"1.5\" fill=\"{fill}\"/>",
                y - 7.0
            ));
            let label = short_label(&s.name, MAX_LABEL_CHARS);
            p.push_str(&format!(
                "<text x=\"{:.1}\" y=\"{y:.1}\" fill=\"var(--text-dim)\" font-size=\"10\">{}</text>",
                lx + 10.0,
                esc(&label)
            ));
            lx += 10.0 + label.chars().count() as f64 * 5.5 + 16.0;
        }
    }
    y + 8.0
}

pub fn render_svg(data: &ChartData) -> String {
    match data.kind {
        ChartKind::Bar => render_bar(data),
        ChartKind::Line => render_line(data),
    }
}

fn render_bar(data: &ChartData) -> String {
    let (vmin, vmax) = value_range(data);
    let span = if (vmax - vmin).abs() < f64::EPSILON { 1.0 } else { vmax - vmin };
    let plot_w = WIDTH - PAD_L - PAD_R;
    let x = |v: f64| PAD_L + (v - vmin) / span * plot_w;
    let n = data.labels.len();

    let mut body = String::new();
    let top = draw_header(&mut body, data);
    let h = top + ROW * n as f64 + 10.0;

    let base = x(0.0);
    if vmin < 0.0 {
        body.push_str(&format!(
            "<line x1=\"{base:.1}\" y1=\"{:.1}\" x2=\"{base:.1}\" y2=\"{:.1}\" \
stroke=\"var(--border)\" stroke-dasharray=\"3 3\"/>",
            top - 2.0,
            h - 8.0
        ));
    }
    // Slim pill bars sitting inside a track (var(--bg-inset)), not a solid
    // block floating on nothing -- matches app.css's own rule that
    // "hierarchy comes from type and spacing before fills." One series gets
    // a full-height band; two share the row with a small gap between them.
    let ns = data.series.len().max(1);
    let band_h = if ns == 1 { 12.0 } else { 8.0 };
    let gap = 3.0;
    let group_h = band_h * ns as f64 + gap * (ns.saturating_sub(1)) as f64;

    for (i, label) in data.labels.iter().enumerate() {
        let y0 = top + i as f64 * ROW;
        let group_y = y0 + (ROW - group_h) / 2.0;
        body.push_str(&format!(
            "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"end\" fill=\"var(--text-dim)\">{}</text>",
            PAD_L - 10.0,
            y0 + ROW / 2.0 + 4.0,
            esc(&short_label(label, MAX_LABEL_CHARS))
        ));
        for si in 0..ns {
            let by = group_y + si as f64 * (band_h + gap);
            body.push_str(&format!(
                "<rect x=\"{PAD_L:.1}\" y=\"{by:.1}\" width=\"{plot_w:.1}\" height=\"{band_h:.1}\" \
rx=\"{:.1}\" fill=\"var(--bg-inset)\"/>",
                band_h / 2.0
            ));
        }
        for (si, s) in data.series.iter().enumerate() {
            let v = s.values[i];
            let x2 = x(v);
            let (bx, bw) = if x2 >= base { (base, x2 - base) } else { (x2, base - x2) };
            let by = group_y + si as f64 * (band_h + gap);
            let fill = if si == 0 { "var(--border-strong)" } else { "var(--text-dim)" };
            body.push_str(&format!(
                "<rect x=\"{bx:.1}\" y=\"{by:.1}\" width=\"{:.1}\" height=\"{band_h:.1}\" rx=\"{:.1}\" fill=\"{fill}\"/>",
                bw.max(2.0),
                band_h / 2.0
            ));
            if si == ns - 1 {
                let anchor = if x2 >= base { "start" } else { "end" };
                let tx = x2 + if x2 >= base { 6.0 } else { -6.0 };
                body.push_str(&format!(
                    "<text x=\"{tx:.1}\" y=\"{:.1}\" text-anchor=\"{anchor}\" fill=\"var(--text)\">{}</text>",
                    y0 + ROW / 2.0 + 4.0,
                    esc(&fmt_value(v, data.unit.as_deref()))
                ));
            }
        }
    }

    format!(
        "<svg viewBox=\"0 0 {WIDTH:.0} {h:.0}\" xmlns=\"http://www.w3.org/2000/svg\" \
font-family=\"ui-sans-serif,system-ui,sans-serif\" font-size=\"12\">{body}</svg>"
    )
}

fn render_line(data: &ChartData) -> String {
    let (vmin, vmax) = value_range(data);
    let span = if (vmax - vmin).abs() < f64::EPSILON { 1.0 } else { vmax - vmin };
    let n = data.labels.len();
    let plot_w = WIDTH - PAD_L - PAD_R;
    let plot_h = 120.0f64;
    let step = if n > 1 { plot_w / (n - 1) as f64 } else { 0.0 };
    // How many chars fit per x-axis slot, given the actual gap between
    // labels -- tighter than the bar chart's fixed column since these sit
    // side by side, not stacked.
    let label_chars = if step > 0.0 {
        ((step / 6.5).floor() as usize).clamp(3, MAX_LABEL_CHARS)
    } else {
        MAX_LABEL_CHARS
    };
    let mut body = String::new();
    let top = draw_header(&mut body, data);
    let h = top + plot_h + 26.0;

    let x = |i: usize| PAD_L + i as f64 * step;
    let y = |v: f64| top + plot_h - (v - vmin) / span * plot_h;

    body.push_str(&format!(
        "<line x1=\"{PAD_L:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"var(--border)\"/>",
        top + plot_h,
        WIDTH - PAD_R,
        top + plot_h
    ));
    for (si, s) in data.series.iter().enumerate() {
        let stroke = if si == 0 { "var(--text)" } else { "var(--text-dim)" };
        let dash = if si == 0 { "" } else { " stroke-dasharray=\"4 3\"" };
        let pts: Vec<String> =
            s.values.iter().enumerate().map(|(i, &v)| format!("{:.1},{:.1}", x(i), y(v))).collect();
        body.push_str(&format!(
            "<polyline points=\"{}\" fill=\"none\" stroke=\"{stroke}\" stroke-width=\"1.6\"{dash}/>",
            pts.join(" ")
        ));
        for (i, &v) in s.values.iter().enumerate() {
            body.push_str(&format!(
                "<circle cx=\"{:.1}\" cy=\"{:.1}\" r=\"2.2\" fill=\"{stroke}\"/>",
                x(i),
                y(v)
            ));
        }
    }
    for (i, label) in data.labels.iter().enumerate() {
        body.push_str(&format!(
            "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" fill=\"var(--text-dim)\" font-size=\"10\">{}</text>",
            x(i),
            top + plot_h + 16.0,
            esc(&short_label(label, label_chars))
        ));
    }

    format!(
        "<svg viewBox=\"0 0 {WIDTH:.0} {h:.0}\" xmlns=\"http://www.w3.org/2000/svg\" \
font-family=\"ui-sans-serif,system-ui,sans-serif\" font-size=\"12\">{body}</svg>"
    )
}

#[derive(Deserialize)]
struct SeriesArg {
    name: String,
    values: Vec<f64>,
}

#[derive(Deserialize)]
struct ChartArgs {
    kind: String,
    #[serde(default)]
    title: Option<String>,
    labels: Vec<String>,
    series: Vec<SeriesArg>,
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
        let kind = match parsed.kind.as_str() {
            "bar" => ChartKind::Bar,
            "line" => ChartKind::Line,
            other => {
                return Err(EngineError::msg(format!(
                    "unknown chart kind `{other}` (use \"bar\" or \"line\")"
                )))
            }
        };
        let data = ChartData {
            kind,
            title: parsed.title,
            labels: parsed.labels,
            series: parsed
                .series
                .into_iter()
                .map(|s| Series { name: s.name, values: s.values })
                .collect(),
            unit: parsed.unit,
        };
        validate(&data).map_err(EngineError::msg)?;

        let n_series = data.series.len();
        let n_labels = data.labels.len();
        let kind_word = match data.kind {
            ChartKind::Bar => "bar",
            ChartKind::Line => "line",
        };
        let svg = render_svg(&data);
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
            chart: Some(svg),
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
    fn render_is_deterministic_for_fixed_input() {
        let data = bar(&["a", "b"], vec![("s", vec![1.0, 2.0])]);
        assert_eq!(render_svg(&data), render_svg(&data));
    }

    #[test]
    fn render_escapes_a_label_containing_markup() {
        let data = bar(&["</text><script>x</script>"], vec![("s", vec![1.0])]);
        let svg = render_svg(&data);
        assert!(!svg.contains("<script"), "svg: {svg}");
        assert!(svg.contains("&lt;script&gt;"), "svg: {svg}");
    }

    #[test]
    fn render_handles_all_zero_values() {
        let data = bar(&["a", "b", "c"], vec![("s", vec![0.0, 0.0, 0.0])]);
        let svg = render_svg(&data);
        assert!(svg.starts_with("<svg"));
        assert!(svg.contains("</svg>"));
    }

    #[test]
    fn render_handles_single_category() {
        let data = bar(&["only"], vec![("s", vec![42.0])]);
        let svg = render_svg(&data);
        assert!(svg.contains("only") || svg.contains("only…"));
    }

    #[test]
    fn render_handles_negative_values() {
        let data = bar(&["a", "b"], vec![("s", vec![-5.0, 10.0])]);
        let svg = render_svg(&data);
        // a zero baseline should be drawn since the range crosses zero
        assert!(svg.contains("stroke-dasharray=\"3 3\""));
    }

    #[test]
    fn long_label_is_truncated_with_ellipsis() {
        let long = "a very long category label that goes on and on";
        let data = bar(&[long], vec![("s", vec![1.0])]);
        let svg = render_svg(&data);
        assert!(svg.contains('…'));
        assert!(!svg.contains(long));
    }

    #[test]
    fn line_chart_also_renders() {
        let data = ChartData {
            kind: ChartKind::Line,
            title: None,
            labels: vec!["Jan".into(), "Feb".into(), "Mar".into()],
            series: vec![
                Series { name: "this year".into(), values: vec![10.0, 20.0, 15.0] },
                Series { name: "last year".into(), values: vec![8.0, 18.0, 12.0] },
            ],
            unit: Some("$".into()),
        };
        let svg = render_svg(&data);
        assert!(svg.contains("<polyline"));
        assert_eq!(svg.matches("<polyline").count(), 2);
        assert!(svg.contains("stroke-dasharray=\"4 3\""));
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

    #[test]
    fn escaping_covers_all_five_xml_special_chars() {
        assert_eq!(esc("a&b<c>d\"e'f"), "a&amp;b&lt;c&gt;d&quot;e&#39;f");
    }
}
