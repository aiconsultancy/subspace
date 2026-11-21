use super::{ComponentKind, InputValue, limits};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use subspace_framework::charts::{ChartKind, ChartModel};

/// A scoped successful evaluation, not a freshened failure or permission grant.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct ResultUpdate {
    pub scope: String,
    pub evaluated_at_ms: u64,
    pub value: ResultValue,
}

impl ResultUpdate {
    pub fn new(scope: impl Into<String>, evaluated_at_ms: u64, value: ResultValue) -> Self {
        Self {
            scope: scope.into(),
            evaluated_at_ms,
            value,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ResultValue {
    Text { text: String },
    Chart { chart: ChartModel },
    Table { table: TableData },
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct TableData {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<InputValue>>,
}

impl TableData {
    pub fn new(columns: Vec<String>, rows: Vec<Vec<InputValue>>) -> Self {
        Self { columns, rows }
    }
}

pub(super) fn validate(result: &ResultUpdate, target: &ComponentKind) -> Result<()> {
    ensure!(
        !result.scope.trim().is_empty() && result.scope.len() <= 1024,
        "result scope must be 1 to 1024 bytes"
    );
    match (&result.value, target) {
        (ResultValue::Text { text }, ComponentKind::Text { .. }) => {
            ensure!(text.len() <= limits::TEXT_BYTES, "text exceeds 16 KiB")
        }
        (ResultValue::Chart { chart }, ComponentKind::Chart) => validate_chart(chart)?,
        (ResultValue::Table { table }, ComponentKind::Table) => validate_table(table)?,
        _ => anyhow::bail!("result type does not match its component"),
    }
    Ok(())
}

fn validate_table(table: &TableData) -> Result<()> {
    ensure!(
        !table.columns.is_empty() && table.columns.len() <= limits::TABLE_CELLS,
        "table must declare bounded columns"
    );
    ensure!(
        table.rows.len() <= limits::TABLE_ROWS
            && table.rows.len().saturating_mul(table.columns.len()) <= limits::TABLE_CELLS,
        "table exceeds 500 rows or 8000 cells"
    );
    ensure!(
        table
            .columns
            .iter()
            .all(|label| !label.is_empty() && label.len() <= 1024),
        "table column labels must be 1 to 1024 bytes"
    );
    ensure!(
        table
            .rows
            .iter()
            .all(|row| row.len() == table.columns.len()),
        "table rows must match column count"
    );
    ensure!(
        table.rows.iter().flatten().all(|cell| match cell {
            InputValue::Number(number) => number.is_finite(),
            InputValue::String(text) => text.len() <= limits::TEXT_BYTES,
        }),
        "table cells must be finite numbers or bounded text"
    );
    Ok(())
}

fn validate_chart(chart: &ChartModel) -> Result<()> {
    ensure!(
        chart.series.len() <= limits::CHART_SERIES && chart.labels.len() <= limits::CHART_POINTS,
        "chart exceeds 32 series or 500 labels"
    );
    ensure!(
        chart.labels.iter().all(|label| label.len() <= 1024),
        "chart label exceeds 1024 bytes"
    );
    for series in &chart.series {
        ensure!(
            series.values.len() <= limits::CHART_POINTS
                && series.points.len() <= limits::CHART_POINTS,
            "chart series exceeds 500 points"
        );
        ensure!(
            series
                .values
                .iter()
                .flatten()
                .all(|value| value.is_finite())
                && series
                    .points
                    .iter()
                    .all(|point| point.x.is_finite() && point.y.is_finite()),
            "chart values must be finite"
        );
        ensure!(
            series.values.is_empty() || series.points.is_empty(),
            "chart series cannot mix categories and points"
        );
    }
    validate_chart_shape(chart)
}

fn validate_chart_shape(chart: &ChartModel) -> Result<()> {
    if matches!(chart.kind, ChartKind::Pie | ChartKind::Doughnut) {
        ensure!(
            chart.labels.len() == chart.series.len()
                && chart
                    .series
                    .iter()
                    .all(|series| series.values.len() == 1 && series.points.is_empty()),
            "round charts require one value per labelled slice"
        );
    } else if chart.series.iter().any(|series| !series.points.is_empty())
        || chart.kind == ChartKind::Scatter
    {
        ensure!(
            chart.labels.is_empty() && chart.series.iter().all(|series| series.values.is_empty()),
            "point charts cannot contain categories"
        );
    } else {
        ensure!(
            chart
                .series
                .iter()
                .all(|series| series.values.len() == chart.labels.len()),
            "category series must match labels"
        );
    }
    Ok(())
}
