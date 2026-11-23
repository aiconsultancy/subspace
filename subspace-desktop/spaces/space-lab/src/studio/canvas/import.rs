//! Static snapshot presets from a Flint chart assembly.

use super::super::dataset::{object_rows, validate_dataset};
use super::super::limits::{MAX_CANVAS_ROWS, SNAPSHOT_RECIPE};
use super::super::store::{draft_id, now_rfc3339};
use crate::contracts::{Dataset, PlotSeries, Preset};
use anyhow::{Result, bail};
use serde_json::Value;

pub(super) fn preset_from_canvas(value: &Value, title: &str) -> Result<Preset> {
    let dataset = dataset_from_canvas(value, title)?;
    Ok(Preset {
        id: draft_id("canvas"),
        title: title.to_owned(),
        environment: String::new(),
        recipe: SNAPSHOT_RECIPE.into(),
        parameters: crate::contracts::Parameters::default(),
        controls: Vec::new(),
        view: view_for_chart_type(chart_type_from_canvas(value)),
        snapshot: Some(dataset),
        revision: 0,
        updated_at: now_rfc3339(),
    })
}

fn dataset_from_canvas(value: &Value, title: &str) -> Result<Dataset> {
    let values = canvas_rows(value)?;
    let rows = object_rows(values)?;
    let (labels, series) = series_from_rows(value, &rows)?;
    let dataset = Dataset {
        columns: columns_of(&rows),
        rows,
        labels,
        series,
        summary: format!(
            "Static imported native chart canvas snapshot titled {title:?}. No refresh recipe. Original query is not inferred."
        ),
        provenance: "static-canvas-snapshot; no-refresh-recipe; original-query-not-inferred".into(),
    };
    validate_dataset(&dataset)?;
    Ok(dataset)
}

fn canvas_rows(value: &Value) -> Result<&Vec<Value>> {
    let values = value
        .pointer("/data/values")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            anyhow::anyhow!("canvas is not a Flint ChartAssemblyInput with data.values")
        })?;
    if value
        .get("semantic_types")
        .and_then(Value::as_object)
        .is_none()
    {
        bail!("canvas is missing semantic_types");
    }
    if value.get("chart_spec").and_then(Value::as_object).is_none() {
        bail!("canvas is missing chart_spec");
    }
    if values.len() > MAX_CANVAS_ROWS {
        bail!("imported canvas exceeds the {MAX_CANVAS_ROWS} row limit");
    }
    Ok(values)
}

fn series_from_rows(value: &Value, rows: &[Value]) -> Result<(Vec<String>, Vec<PlotSeries>)> {
    let x_field = encoding_field(value, "x").or_else(|| encoding_field(value, "color"));
    let y_field = encoding_field(value, "y").or_else(|| encoding_field(value, "size"));
    let (Some(category), Some(measure)) = (x_field, y_field) else {
        return Ok((Vec::new(), Vec::new()));
    };
    collect_series(rows, category, measure)
}

fn collect_series(
    rows: &[Value],
    category: &str,
    measure: &str,
) -> Result<(Vec<String>, Vec<PlotSeries>)> {
    let mut labels = Vec::new();
    let mut series_values = Vec::new();
    for row in rows {
        push_measure(row, category, measure, &mut labels, &mut series_values)?;
    }
    let series = if series_values.is_empty() {
        Vec::new()
    } else {
        vec![PlotSeries {
            name: measure.to_owned(),
            values: series_values,
        }]
    };
    Ok((labels, series))
}

fn push_measure(
    row: &Value,
    category: &str,
    measure: &str,
    labels: &mut Vec<String>,
    series_values: &mut Vec<f64>,
) -> Result<()> {
    let Some(object) = row.as_object() else {
        return Ok(());
    };
    if let Some(label) = object.get(category).and_then(value_as_label) {
        labels.push(label);
    }
    let Some(number) = object.get(measure).and_then(Value::as_f64) else {
        return Ok(());
    };
    if number.is_finite() {
        series_values.push(number);
        Ok(())
    } else {
        bail!("imported canvas contains a non-finite measure")
    }
}

fn encoding_field<'a>(value: &'a Value, encoding: &str) -> Option<&'a str> {
    value
        .pointer(&format!("/chart_spec/encodings/{encoding}/field"))
        .and_then(Value::as_str)
}

fn columns_of(rows: &[Value]) -> Vec<String> {
    rows.first()
        .and_then(Value::as_object)
        .map(|object| object.keys().cloned().collect())
        .unwrap_or_default()
}

fn chart_type_from_canvas(value: &Value) -> &str {
    value
        .pointer("/chart_spec/chartType")
        .and_then(Value::as_str)
        .unwrap_or("Bar Chart")
}

fn view_for_chart_type(chart_type: &str) -> String {
    match chart_type {
        "Line Chart" => "line",
        "Area Chart" => "area",
        _ => "bar",
    }
    .to_owned()
}

fn value_as_label(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(flag) => Some(flag.to_string()),
        _ => None,
    }
}
