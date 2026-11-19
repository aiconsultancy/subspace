//! The chart model: what a chart shows, with no opinion on how it is drawn.
//!
//! A board builds one directly; [`crate::chartjs::to_model`] builds one from a
//! Chart.js configuration. The gallery file stores it, so every type is serde
//! serializable and its JSON names are the field names.

use serde::{Deserialize, Serialize};

/// The kinds a [`ChartModel`] can be.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum ChartKind {
    #[default]
    Bar,
    Line,
    Area,
    Pie,
    Doughnut,
    Scatter,
}

impl ChartKind {
    /// The word a label or a screen reader uses for this kind.
    pub fn name(self) -> &'static str {
        match self {
            Self::Bar => "bar chart",
            Self::Line => "line chart",
            Self::Area => "area chart",
            Self::Pie => "pie chart",
            Self::Doughnut => "doughnut chart",
            Self::Scatter => "scatter plot",
        }
    }

    /// Whether the kind draws slices around a centre rather than on axes.
    pub fn is_round(self) -> bool {
        matches!(self, Self::Pie | Self::Doughnut)
    }
}

/// Which way the value axis runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum Orientation {
    /// Categories across the bottom, values up the side.
    #[default]
    Vertical,
    /// Categories down the side, values across the bottom.
    Horizontal,
}

/// How a value reads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum ValueFormat {
    #[default]
    Plain,
    /// An amount in the currency with this ISO code, such as `USD`.
    Currency { code: String },
    /// A share, already scaled to 0-100.
    Percent,
}

/// What the positions on a point chart's x axis mean.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum AxisScale {
    /// Plain numbers.
    #[default]
    Number,
    /// Milliseconds since the Unix epoch.
    Timestamp,
}

/// One `(x, y)` position of a point series.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

/// One series: a line, an area, a set of bars, a cloud of points, or one
/// slice of a pie.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Series {
    /// The name in the legend and the tooltip.
    pub label: String,
    /// One value per entry of [`ChartModel::labels`]; `None` where the series
    /// has no value for that category. Empty for a point series.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub values: Vec<Option<f64>>,
    /// The positions of a point series: scatter marks, and lines or areas
    /// whose x axis carries numbers rather than categories. Empty otherwise.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub points: Vec<Point>,
    /// The series' own colour as `0xRRGGBB`. `None` takes the theme palette.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<u32>,
}

impl Series {
    /// A category series with no fixed colour.
    pub fn new(label: impl Into<String>, values: Vec<Option<f64>>) -> Self {
        Self {
            label: label.into(),
            values,
            ..Self::default()
        }
    }

    /// The same series drawn in `color`, given as `0xRRGGBB`.
    pub fn color(mut self, color: u32) -> Self {
        self.color = Some(color);
        self
    }

    /// The largest value, ignoring the gaps.
    pub fn max(&self) -> f64 {
        let values = self.values.iter().filter_map(|value| *value);
        let points = self.points.iter().map(|point| point.y);
        values.chain(points).fold(f64::NEG_INFINITY, f64::max)
    }

    /// The smallest value, ignoring the gaps.
    pub fn min(&self) -> f64 {
        let values = self.values.iter().filter_map(|value| *value);
        let points = self.points.iter().map(|point| point.y);
        values.chain(points).fold(f64::INFINITY, f64::min)
    }
}

/// A chart, ready to draw.
///
/// Category charts hold their categories in [`Self::labels`] and one value per
/// category in each series. Point charts leave `labels` empty and carry
/// `(x, y)` positions in [`Series::points`].
///
/// Pie and doughnut charts are one series per slice, each with a single value;
/// `labels` mirrors the slice names.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ChartModel {
    pub kind: ChartKind,
    pub orientation: Orientation,
    /// Whether the series stack on one another rather than sit side by side.
    pub stacked: bool,
    pub title: String,
    pub subtitle: String,
    /// The categories, in the order they are drawn.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<String>,
    /// The name of the axis the categories sit on.
    pub category_axis_title: String,
    /// The name of the axis the values sit on.
    pub value_axis_title: String,
    pub series: Vec<Series>,
    /// What the x positions of a point chart mean.
    pub x_scale: AxisScale,
    /// Whether to list the series beside the chart.
    pub legend: bool,
    /// How to write a value.
    pub format: ValueFormat,
}

impl ChartModel {
    /// Whether the series carry `(x, y)` points rather than one value per
    /// category.
    pub fn is_point_chart(&self) -> bool {
        self.series.iter().any(|series| !series.points.is_empty())
    }

    /// How many values the chart holds, across every series.
    pub fn value_count(&self) -> usize {
        self.series
            .iter()
            .map(|series| series.values.len() + series.points.len())
            .sum()
    }

    /// Whether there is nothing to draw.
    pub fn is_empty(&self) -> bool {
        self.value_count() == 0
    }

    /// A one-line description for an accessible label, such as
    /// `Daily cost, stacked bar chart, 3 series, 30 categories`.
    // qual:api - the widget's accessible label; its text is tested here.
    pub fn description(&self) -> String {
        let title = match self.title.is_empty() {
            true => String::new(),
            false => format!("{}, ", self.title),
        };
        let stacked = match self.stacked {
            true => "stacked ",
            false => "",
        };
        format!(
            "{title}{stacked}{}, {} series, {} values",
            self.kind.name(),
            self.series.len(),
            self.value_count()
        )
    }
}

#[cfg(test)]
mod tests {
    use crate::model::{AxisScale, ChartKind, ChartModel, Orientation, Point, Series, ValueFormat};

    #[test]
    fn a_series_reports_its_extent_over_values_and_points() {
        let values = Series::new("web", vec![Some(3.0), None, Some(-1.5)]);
        assert_eq!(values.max(), 3.0);
        assert_eq!(values.min(), -1.5);

        let points = Series {
            points: vec![Point { x: 1.0, y: 9.0 }, Point { x: 2.0, y: 4.0 }],
            ..Series::new("api", Vec::new())
        };
        assert_eq!(points.max(), 9.0);
        assert_eq!(points.min(), 4.0);
    }

    #[test]
    fn a_point_chart_is_the_one_whose_series_carry_points() {
        let categories = ChartModel {
            labels: vec!["Mon".into()],
            series: vec![Series::new("web", vec![Some(1.0)])],
            ..ChartModel::default()
        };
        assert!(!categories.is_point_chart());
        assert_eq!(categories.value_count(), 1);

        let points = ChartModel {
            kind: ChartKind::Scatter,
            series: vec![Series {
                points: vec![Point { x: 1.0, y: 2.0 }],
                ..Series::new("api", Vec::new())
            }],
            ..ChartModel::default()
        };
        assert!(points.is_point_chart());
        assert!(!points.is_empty());
    }

    #[test]
    fn the_description_names_the_title_the_kind_and_the_size() {
        let model = ChartModel {
            kind: ChartKind::Bar,
            stacked: true,
            title: "Daily cost".into(),
            labels: vec!["Mon".into(), "Tue".into()],
            series: vec![Series::new("web", vec![Some(1.0), Some(2.0)])],
            ..ChartModel::default()
        };
        assert_eq!(
            model.description(),
            "Daily cost, stacked bar chart, 1 series, 2 values"
        );
        assert_eq!(
            ChartModel::default().description(),
            "bar chart, 0 series, 0 values"
        );
    }

    #[test]
    fn the_model_round_trips_through_json_for_the_gallery_file() {
        let model = ChartModel {
            kind: ChartKind::Doughnut,
            orientation: Orientation::Horizontal,
            stacked: true,
            title: "Top services".into(),
            subtitle: "USD, amortized".into(),
            labels: vec!["AKS".into()],
            category_axis_title: "service".into(),
            value_axis_title: "cost".into(),
            series: vec![Series::new("AKS", vec![Some(12.5)]).color(0x36a2eb)],
            x_scale: AxisScale::Timestamp,
            legend: true,
            format: ValueFormat::Currency { code: "USD".into() },
        };
        let json = serde_json::to_string(&model).expect("the model serializes");
        let back: ChartModel = serde_json::from_str(&json).expect("the model parses");

        assert_eq!(back, model);
        assert!(json.contains("\"doughnut\""), "{json}");
        assert!(json.contains("\"xScale\":\"timestamp\""), "{json}");
    }
}
