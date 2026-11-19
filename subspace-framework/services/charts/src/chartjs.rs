//! Chart.js configurations turned into a [`ChartModel`].
//!
//! Flint's Chart.js backend writes plain data: a `type`, `data.labels`,
//! `data.datasets[]` and the `options` that say which way the chart runs and
//! whether it stacks. This module reads that and nothing else; a style the
//! model has no field for is dropped rather than guessed at.

use serde_json::Value;

use crate::model::{AxisScale, ChartKind, ChartModel, Orientation, Point, Series};

/// The Chart.js types the renderer draws.
pub const SUPPORTED_TYPES: &str = "bar, line, pie, doughnut, scatter";
/// The most series one chart may carry.
pub const MAX_SERIES: usize = 50;
/// The most values one chart may carry, across every series.
pub const MAX_VALUES: usize = 5_000;

/// A colour's parts: `rgb()` takes three channels, `rgba()` four, and a hex
/// colour three or six digits, each a value from zero to 255.
const RGB_PARTS: usize = 3;
const RGBA_PARTS: usize = 4;
const SHORT_HEX: usize = 3;
const LONG_HEX: usize = 6;
const HEX_RADIX: u32 = 16;
const RED_SHIFT: u32 = 16;
const GREEN_SHIFT: u32 = 8;
const MAX_CHANNEL: f64 = 255.0;

/// The smallest x treated as a millisecond timestamp: 2001-09-09.
const EPOCH_MS_FLOOR: f64 = 1_000_000_000_000.0;
/// The largest x treated as a millisecond timestamp: 2100-01-01.
const EPOCH_MS_CEILING: f64 = 4_102_444_800_000.0;

/// Convert a Chart.js configuration (REQ-43).
///
/// The configuration carries no currency, so [`ChartModel::format`] stays
/// [`crate::ValueFormat::Plain`]; the caller sets it. Flint keeps the chart's
/// title outside the Chart.js options, so the title is empty unless the
/// configuration has a `plugins.title`.
pub fn to_model(config: &Value) -> Result<ChartModel, String> {
    let chart_type = config
        .get("type")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("the chart has no type. Supported types: {SUPPORTED_TYPES}"))?;
    let datasets = config
        .pointer("/data/datasets")
        .and_then(Value::as_array)
        .ok_or("the chart has no data.datasets")?;
    let kind = kind_of(chart_type, datasets)?;
    let labels = labels_of(config);
    let series = series_of(kind, &labels, datasets)?;
    let options = config.get("options").unwrap_or(&Value::Null).clone();
    Ok(assemble(kind, labels, series, &options))
}

/// The model for a chart whose parts are already read.
fn assemble(
    kind: ChartKind,
    labels: Vec<String>,
    series: Vec<Series>,
    options: &Value,
) -> ChartModel {
    let orientation = orientation_of(options);
    let (title, subtitle) = headings_of(options);
    let (category_axis_title, value_axis_title) = axis_titles_of(options, orientation);
    ChartModel {
        kind,
        orientation,
        stacked: stacked_of(options),
        title,
        subtitle,
        category_axis_title,
        value_axis_title,
        x_scale: x_scale_of(&series),
        legend: legend_of(options, &series),
        labels,
        series,
        format: crate::ValueFormat::Plain,
    }
}

/// The kind a Chart.js `type` means; a `line` that fills is an area.
fn kind_of(chart_type: &str, datasets: &[Value]) -> Result<ChartKind, String> {
    match chart_type {
        "bar" => Ok(ChartKind::Bar),
        "line" if datasets.iter().any(is_filled) => Ok(ChartKind::Area),
        "line" => Ok(ChartKind::Line),
        "pie" => Ok(ChartKind::Pie),
        "doughnut" => Ok(ChartKind::Doughnut),
        "scatter" => Ok(ChartKind::Scatter),
        other => Err(format!(
            "unsupported chart type `{other}`. Supported types: {SUPPORTED_TYPES}"
        )),
    }
}

/// Whether a `line` dataset fills the space under it: `fill` is `true` or a
/// Chart.js fill target such as `"origin"` or `"stack"`.
fn is_filled(dataset: &Value) -> bool {
    match dataset.get("fill") {
        Some(Value::Bool(filled)) => *filled,
        Some(Value::String(target)) => !target.is_empty(),
        _ => false,
    }
}

/// The category labels, as text.
fn labels_of(config: &Value) -> Vec<String> {
    config
        .pointer("/data/labels")
        .and_then(Value::as_array)
        .map(|labels| labels.iter().map(text_of).collect())
        .unwrap_or_default()
}

/// A JSON scalar as label text. A number keeps its shortest form; anything
/// else that is not a string has no label.
fn text_of(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Number(number) => number.to_string(),
        Value::Bool(flag) => flag.to_string(),
        _ => String::new(),
    }
}

/// The series of a chart, within the limits.
fn series_of(
    kind: ChartKind,
    labels: &[String],
    datasets: &[Value],
) -> Result<Vec<Series>, String> {
    let series = match kind.is_round() {
        true => slice_series(labels, datasets),
        false => datasets.iter().enumerate().map(axis_series).collect(),
    };
    check_limits(&series)?;
    Ok(series)
}

/// Pie and doughnut charts hold their slices in one dataset: each slice
/// becomes a series with a single value and its own colour.
fn slice_series(labels: &[String], datasets: &[Value]) -> Vec<Series> {
    let Some(dataset) = datasets.first() else {
        return Vec::new();
    };
    let values = values_of(dataset);
    let colors = colors_of(dataset);
    values
        .iter()
        .enumerate()
        .map(|(index, value)| Series {
            label: slice_label(labels, index),
            values: vec![*value],
            points: Vec::new(),
            color: colors.get(index).copied().flatten(),
        })
        .collect()
}

/// A slice's name: its label, else its place in the dataset.
fn slice_label(labels: &[String], index: usize) -> String {
    match labels.get(index) {
        Some(label) if !label.is_empty() => label.clone(),
        _ => format!("Slice {}", index + 1),
    }
}

/// One dataset of a chart drawn on axes.
fn axis_series((index, dataset): (usize, &Value)) -> Series {
    let points = points_of(dataset);
    Series {
        label: dataset_label(dataset, index),
        values: match points.is_empty() {
            true => values_of(dataset),
            false => Vec::new(),
        },
        points,
        color: colors_of(dataset).first().copied().flatten(),
    }
}

/// A dataset's name: its label, else its place among the datasets.
fn dataset_label(dataset: &Value, index: usize) -> String {
    match dataset.get("label").and_then(Value::as_str) {
        Some(label) if !label.is_empty() => label.to_owned(),
        _ => format!("Series {}", index + 1),
    }
}

/// A dataset's plain values; an entry that is not a number is a gap.
fn values_of(dataset: &Value) -> Vec<Option<f64>> {
    let Some(data) = dataset.get("data").and_then(Value::as_array) else {
        return Vec::new();
    };
    data.iter()
        .filter(|entry| !entry.is_object())
        .map(|entry| entry.as_f64().filter(|value| value.is_finite()))
        .collect()
}

/// A dataset's `{ x, y }` positions. Entries that are not points are skipped,
/// so a dataset of plain numbers has none.
fn points_of(dataset: &Value) -> Vec<Point> {
    let Some(data) = dataset.get("data").and_then(Value::as_array) else {
        return Vec::new();
    };
    data.iter().filter_map(point_of).collect()
}

/// One `{ x, y }` entry, when both coordinates are finite numbers.
fn point_of(entry: &Value) -> Option<Point> {
    let x = entry.get("x")?.as_f64()?;
    let y = entry.get("y")?.as_f64()?;
    (x.is_finite() && y.is_finite()).then_some(Point { x, y })
}

/// The colours a dataset names, in order. The border colour wins over the
/// background, because Flint draws the border solid and the background
/// translucent, and the widget chooses its own opacity.
fn colors_of(dataset: &Value) -> Vec<Option<u32>> {
    for key in ["borderColor", "backgroundColor"] {
        let colors = match dataset.get(key) {
            Some(Value::String(text)) => vec![parse_color(text)],
            Some(Value::Array(entries)) => entries.iter().map(color_of).collect(),
            _ => Vec::new(),
        };
        if colors.iter().any(Option::is_some) {
            return colors;
        }
    }
    Vec::new()
}

fn color_of(entry: &Value) -> Option<u32> {
    parse_color(entry.as_str()?)
}

/// A CSS colour as `0xRRGGBB`: `#rgb`, `#rrggbb`, `rgb(...)` and `rgba(...)`.
/// Any other form, including a named colour or a gradient, has no colour and
/// the widget uses the theme palette instead. Alpha is dropped.
pub fn parse_color(text: &str) -> Option<u32> {
    let text = text.trim();
    match text.strip_prefix('#') {
        Some(digits) => hex_color(digits),
        None => functional_color(text),
    }
}

fn hex_color(digits: &str) -> Option<u32> {
    let expanded = match digits.len() {
        SHORT_HEX => digits.chars().flat_map(|digit| [digit, digit]).collect(),
        LONG_HEX => digits.to_owned(),
        _ => return None,
    };
    u32::from_str_radix(&expanded, HEX_RADIX).ok()
}

fn functional_color(text: &str) -> Option<u32> {
    let lower = text.to_ascii_lowercase();
    let rest = lower.strip_prefix("rgba").or(lower.strip_prefix("rgb"))?;
    let body = rest.trim_start().strip_prefix('(')?.strip_suffix(')')?;
    let channels: Vec<&str> = body.split(',').collect();
    if !(RGB_PARTS..=RGBA_PARTS).contains(&channels.len()) {
        return None;
    }
    let red = channel(channels[0])?;
    let green = channel(channels[1])?;
    let blue = channel(channels[2])?;
    Some(red << RED_SHIFT | green << GREEN_SHIFT | blue)
}

fn channel(text: &str) -> Option<u32> {
    let value: f64 = text.trim().parse().ok()?;
    (value.is_finite() && (0.0..=MAX_CHANNEL).contains(&value)).then(|| value.round() as u32)
}

/// Refuse a chart too large to draw or to hold in the gallery file.
fn check_limits(series: &[Series]) -> Result<(), String> {
    if series.len() > MAX_SERIES {
        return Err(format!(
            "the chart has {} series; at most {MAX_SERIES} are supported",
            series.len()
        ));
    }
    let values: usize = series
        .iter()
        .map(|one| one.values.len() + one.points.len())
        .sum();
    match values > MAX_VALUES {
        true => Err(format!(
            "the chart has {values} values; at most {MAX_VALUES} are supported"
        )),
        false => Ok(()),
    }
}

/// `options.indexAxis == "y"` puts the categories down the side.
fn orientation_of(options: &Value) -> Orientation {
    match options.get("indexAxis").and_then(Value::as_str) {
        Some("y") => Orientation::Horizontal,
        _ => Orientation::Vertical,
    }
}

/// Either scale saying `stacked` stacks the chart.
fn stacked_of(options: &Value) -> bool {
    ["/scales/x/stacked", "/scales/y/stacked"]
        .iter()
        .any(|path| options.pointer(path).and_then(Value::as_bool) == Some(true))
}

/// The chart's title and subtitle, from the Chart.js title plugins. A title
/// given as several lines keeps the first as the title and joins the rest
/// into the subtitle.
fn headings_of(options: &Value) -> (String, String) {
    let mut lines = plugin_text(options, "title").into_iter();
    let title = lines.next().unwrap_or_default();
    let rest: Vec<String> = lines.collect();
    let subtitle = match rest.is_empty() {
        true => plugin_text(options, "subtitle").join(" "),
        false => rest.join(" "),
    };
    (title, subtitle)
}

/// A title plugin's text, which Chart.js allows as one string or as lines.
/// A plugin that is switched off contributes nothing.
fn plugin_text(options: &Value, plugin: &str) -> Vec<String> {
    let Some(node) = options.pointer(&format!("/plugins/{plugin}")) else {
        return Vec::new();
    };
    if node.get("display").and_then(Value::as_bool) == Some(false) {
        return Vec::new();
    }
    match node.get("text") {
        Some(Value::String(text)) => vec![text.clone()],
        Some(Value::Array(lines)) => lines.iter().map(text_of).collect(),
        _ => Vec::new(),
    }
}

/// The names of the two axes, with the category axis first. `indexAxis`
/// decides which scale carries the categories.
fn axis_titles_of(options: &Value, orientation: Orientation) -> (String, String) {
    let title = |axis: &str| {
        options
            .pointer(&format!("/scales/{axis}/title/text"))
            .map(text_of)
            .unwrap_or_default()
    };
    match orientation {
        Orientation::Vertical => (title("x"), title("y")),
        Orientation::Horizontal => (title("y"), title("x")),
    }
}

/// Whether to show the legend. Chart.js states it; without a statement, a
/// chart with more than one series names them.
fn legend_of(options: &Value, series: &[Series]) -> bool {
    match options
        .pointer("/plugins/legend/display")
        .and_then(Value::as_bool)
    {
        Some(display) => display,
        None => series.len() > 1,
    }
}

/// What the x positions mean. Flint writes a date field as milliseconds since
/// the epoch, which this reads back when every position falls in a plausible
/// window; anything else is a plain number.
fn x_scale_of(series: &[Series]) -> AxisScale {
    let mut positions = series.iter().flat_map(|one| one.points.iter()).peekable();
    if positions.peek().is_none() {
        return AxisScale::Number;
    }
    let epoch = positions.all(|point| {
        (EPOCH_MS_FLOOR..=EPOCH_MS_CEILING).contains(&point.x) && point.x.fract() == 0.0
    });
    match epoch {
        true => AxisScale::Timestamp,
        false => AxisScale::Number,
    }
}

#[cfg(test)]
mod tests {
    use crate::chartjs::{MAX_SERIES, MAX_VALUES, parse_color, to_model};
    use crate::model::{AxisScale, ChartKind, Orientation};
    use serde_json::{Value, json};

    fn bar(stacked: bool, index_axis: &str) -> Value {
        json!({
            "type": "bar",
            "data": {
                "labels": ["2026-10-01", "2026-10-02"],
                "datasets": [
                    {
                        "label": "web",
                        "data": [120.5, 131.0],
                        "backgroundColor": "rgba(54, 162, 235, 0.6)",
                        "borderColor": "#36a2eb"
                    },
                    {
                        "label": "api",
                        "data": [40.25, null],
                        "backgroundColor": "rgba(255, 99, 132, 0.6)",
                        "borderColor": "#ff6384"
                    }
                ]
            },
            "options": {
                "indexAxis": index_axis,
                "scales": {
                    "x": { "stacked": stacked, "title": { "display": true, "text": "date" } },
                    "y": { "stacked": stacked, "title": { "display": true, "text": "cost" } }
                },
                "plugins": { "legend": { "display": true, "position": "right" } }
            }
        })
    }

    #[test]
    fn a_stacked_bar_chart_keeps_its_series_labels_values_and_colours() {
        let model = to_model(&bar(true, "x")).expect("the bar chart converts");

        assert_eq!(model.kind, ChartKind::Bar);
        assert_eq!(model.orientation, Orientation::Vertical);
        assert!(model.stacked);
        assert!(model.legend);
        assert_eq!(model.labels, ["2026-10-01", "2026-10-02"]);
        assert_eq!(model.category_axis_title, "date");
        assert_eq!(model.value_axis_title, "cost");
        assert_eq!(model.series.len(), 2);
        assert_eq!(model.series[0].label, "web");
        assert_eq!(model.series[0].values, [Some(120.5), Some(131.0)]);
        assert_eq!(model.series[0].color, Some(0x36a2eb));
        assert_eq!(model.series[1].values, [Some(40.25), None]);
    }

    #[test]
    fn a_horizontal_bar_chart_reads_the_category_axis_from_index_axis() {
        let model = to_model(&bar(false, "y")).expect("the bar chart converts");

        assert_eq!(model.orientation, Orientation::Horizontal);
        assert!(!model.stacked);
        assert_eq!(model.category_axis_title, "cost");
        assert_eq!(model.value_axis_title, "date");
    }

    #[test]
    fn a_line_chart_of_timestamps_keeps_its_points_and_reads_the_scale() {
        let config = json!({
            "type": "line",
            "data": {
                "labels": [],
                "datasets": [{
                    "label": "web",
                    "data": [{ "x": 1_790_812_800_000_i64, "y": 47 }, { "x": 1_790_899_200_000_i64, "y": 158 }],
                    "borderColor": "#36a2eb",
                    "backgroundColor": "transparent",
                    "fill": false
                }]
            },
            "options": { "scales": { "x": { "type": "linear", "title": { "text": "date" } } } }
        });
        let model = to_model(&config).expect("the line chart converts");

        assert_eq!(model.kind, ChartKind::Line);
        assert!(model.is_point_chart());
        assert_eq!(model.x_scale, AxisScale::Timestamp);
        assert_eq!(model.series[0].points.len(), 2);
        assert_eq!(model.series[0].points[1].y, 158.0);
        assert!(model.series[0].values.is_empty());
        assert!(!model.legend, "one series names itself in the title");
    }

    #[test]
    fn a_filled_line_chart_is_an_area_chart() {
        let mut config = json!({
            "type": "line",
            "data": { "datasets": [{ "label": "web", "data": [1, 2], "fill": "stack" }] },
            "options": { "scales": { "y": { "stacked": true } } }
        });
        assert_eq!(
            to_model(&config).expect("the area chart converts").kind,
            ChartKind::Area
        );

        config["data"]["datasets"][0]["fill"] = Value::Bool(true);
        assert_eq!(
            to_model(&config).expect("the area chart converts").kind,
            ChartKind::Area
        );

        config["data"]["datasets"][0]["fill"] = Value::Bool(false);
        assert_eq!(
            to_model(&config).expect("the line chart converts").kind,
            ChartKind::Line
        );
    }

    #[test]
    fn a_pie_or_doughnut_chart_makes_one_series_per_slice() {
        let config = json!({
            "type": "doughnut",
            "data": {
                "labels": ["AKS", "Storage", ""],
                "datasets": [{
                    "data": [251.5, 78.75, 1.0],
                    "backgroundColor": ["rgba(54, 162, 235, 0.6)", "rgba(255, 99, 132, 0.6)", "oklch(0.7 0.1 20)"],
                    "borderColor": ["#36a2eb", "#ff6384", "oklch(0.7 0.1 20)"]
                }]
            },
            "options": { "cutout": "55%", "plugins": { "legend": { "display": true } } }
        });
        let model = to_model(&config).expect("the doughnut converts");

        assert_eq!(model.kind, ChartKind::Doughnut);
        assert_eq!(model.series.len(), 3);
        assert_eq!(model.series[0].label, "AKS");
        assert_eq!(model.series[0].values, [Some(251.5)]);
        assert_eq!(model.series[0].color, Some(0x36a2eb));
        assert_eq!(
            model.series[2].label, "Slice 3",
            "an empty label is numbered"
        );
        assert_eq!(
            model.series[2].color, None,
            "an unreadable colour is dropped"
        );

        let pie =
            json!({ "type": "pie", "data": { "labels": ["a"], "datasets": [{ "data": [1] }] } });
        assert_eq!(
            to_model(&pie).expect("the pie converts").kind,
            ChartKind::Pie
        );
    }

    #[test]
    fn a_scatter_plot_keeps_one_point_cloud_per_series() {
        let config = json!({
            "type": "scatter",
            "data": { "datasets": [
                { "label": "web", "data": [{ "x": 1, "y": 2 }, { "x": 3.5, "y": 4 }], "backgroundColor": "rgba(54, 162, 235, 1)" },
                { "data": [{ "x": 9, "y": 9 }] }
            ] },
            "options": { "scales": { "x": { "title": { "text": "cost" } } } }
        });
        let model = to_model(&config).expect("the scatter plot converts");

        assert_eq!(model.kind, ChartKind::Scatter);
        assert_eq!(model.x_scale, AxisScale::Number);
        assert_eq!(model.series[0].points.len(), 2);
        assert_eq!(model.series[0].color, Some(0x36a2eb));
        assert_eq!(model.series[1].label, "Series 2");
        assert!(model.legend, "two series need naming");
    }

    #[test]
    fn the_title_plugin_gives_the_title_and_the_subtitle() {
        let titled = |title: Value| {
            json!({
                "type": "bar",
                "data": { "labels": ["a"], "datasets": [{ "data": [1] }] },
                "options": { "plugins": { "title": { "display": true, "text": title },
                                          "subtitle": { "display": true, "text": "USD, amortized" } } }
            })
        };
        let one = to_model(&titled(json!("Daily cost"))).expect("converts");
        assert_eq!(one.title, "Daily cost");
        assert_eq!(one.subtitle, "USD, amortized");

        let lines = to_model(&titled(json!(["Daily cost", "by work stream"]))).expect("converts");
        assert_eq!(lines.title, "Daily cost");
        assert_eq!(lines.subtitle, "by work stream");

        let plain = json!({ "type": "bar", "data": { "datasets": [] } });
        let plain = to_model(&plain).expect("converts");
        assert_eq!(plain.title, "");
    }

    #[test]
    fn an_unsupported_type_lists_the_supported_ones() {
        let config = json!({ "type": "radar", "data": { "datasets": [] } });
        let error = to_model(&config).expect_err("radar is not supported");

        assert!(error.contains("radar"), "{error}");
        assert!(
            error.contains("bar, line, pie, doughnut, scatter"),
            "{error}"
        );

        let missing = json!({ "data": { "datasets": [] } });
        assert!(to_model(&missing).expect_err("no type").contains("no type"));

        let no_data = json!({ "type": "bar" });
        assert!(
            to_model(&no_data)
                .expect_err("no datasets")
                .contains("data.datasets")
        );
    }

    #[test]
    fn colours_parse_from_the_four_accepted_forms_and_nothing_else() {
        assert_eq!(parse_color("#36a2eb"), Some(0x36a2eb));
        assert_eq!(parse_color("  #FFF "), Some(0xffffff));
        assert_eq!(parse_color("#1a2"), Some(0x11aa22));
        assert_eq!(parse_color("rgb(54, 162, 235)"), Some(0x36a2eb));
        assert_eq!(parse_color("rgba(54,162,235,0.6)"), Some(0x36a2eb));
        assert_eq!(parse_color("RGBA(255, 99, 132, 1)"), Some(0xff6384));

        assert_eq!(parse_color("transparent"), None);
        assert_eq!(parse_color("#36a2eb80"), None, "8 digits is another form");
        assert_eq!(parse_color("rgb(54, 162)"), None);
        assert_eq!(parse_color("rgb(54, 162, 300)"), None);
        assert_eq!(parse_color("rgb(10%, 20%, 30%)"), None);
        assert_eq!(parse_color("oklch(0.7 0.1 20)"), None);
    }

    #[test]
    fn a_chart_past_the_limits_says_which_limit_it_passed() {
        let datasets: Vec<Value> = (0..=MAX_SERIES)
            .map(|index| json!({ "label": format!("s{index}"), "data": [1] }))
            .collect();
        let many_series = json!({ "type": "bar", "data": { "datasets": datasets } });
        let error = to_model(&many_series).expect_err("too many series");
        assert!(error.contains("51 series"), "{error}");
        assert!(error.contains("at most 50"), "{error}");

        let values: Vec<f64> = (0..=MAX_VALUES).map(|index| index as f64).collect();
        let many_values = json!({ "type": "bar", "data": { "datasets": [{ "data": values }] } });
        let error = to_model(&many_values).expect_err("too many values");
        assert!(error.contains("5001 values"), "{error}");
        assert!(error.contains("at most 5000"), "{error}");
    }
}
