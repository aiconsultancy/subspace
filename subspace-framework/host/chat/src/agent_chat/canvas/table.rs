//! The chart Data table: every value of a chart model as reader-formatted
//! rows, so the table says what the tooltip would.
use crate::chart::{format_value, format_x_full};
use subspace_charts::{ChartModel, Series};

/// One table row: series, category or x, value or y.
pub(super) type Row = [String; 3];

pub(super) const HEADER: [&str; 3] = ["Series", "Category / x", "Value / y"];

pub(super) fn rows(model: &ChartModel) -> Vec<Row> {
    model
        .series
        .iter()
        .flat_map(|series| series_rows(model, series))
        .collect()
}

fn series_rows(model: &ChartModel, series: &Series) -> Vec<Row> {
    let categories = series.values.iter().enumerate().map(|(index, value)| {
        let label = model.labels.get(index).cloned().unwrap_or_default();
        let value = match value {
            Some(value) => format_value(*value, &model.format),
            None => "—".to_owned(),
        };
        [series.label.clone(), label, value]
    });
    let points = series.points.iter().map(|point| {
        [
            series.label.clone(),
            format_x_full(point.x, model.x_scale),
            format_value(point.y, &model.format),
        ]
    });
    categories.chain(points).collect()
}

#[cfg(test)]
mod tests {
    use super::rows;
    use subspace_charts::{AxisScale, ChartModel, Point, Series, ValueFormat};

    #[test]
    fn rows_read_like_the_chart_tooltip() {
        let categories = Series {
            label: "Cost".into(),
            values: vec![Some(1234.5), None],
            ..Series::default()
        };
        let points = Series {
            label: "PRD".into(),
            points: vec![Point {
                x: 1_790_812_800_000.,
                y: 296.98497,
            }],
            ..Series::default()
        };
        let model = ChartModel {
            format: ValueFormat::Currency { code: "USD".into() },
            x_scale: AxisScale::Timestamp,
            labels: vec!["web".into(), "data".into()],
            series: vec![categories, points],
            ..ChartModel::default()
        };

        let rows = rows(&model);

        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0], ["Cost", "web", "$1,234.5"]);
        assert_eq!(rows[1], ["Cost", "data", "—"]);
        assert_eq!(rows[2], ["PRD", "2026-10-01", "$296.98"]);
    }
}
