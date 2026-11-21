//! Flint chart assembly from a preset dataset.

use super::super::controls::{validate_title, validate_view};
use super::super::dataset::{object_rows, validate_dataset};
use super::super::limits::{
    MAX_CANVAS_CELLS, MAX_CANVAS_ROWS, MAX_SUBTITLE_LEN, MAX_TITLE_LEN, MAX_VALUE_LEN,
    SNAPSHOT_RECIPE,
};
use crate::contracts::{Dataset, PlotSeries, Preset};
use anyhow::{Result, bail};
use serde_json::{Value, json};

pub(super) fn canvas_source(preset: &Preset, data: &Dataset) -> Result<Value> {
    validate_title(&preset.title)?;
    validate_view(&preset.view)?;
    validate_dataset(data)?;
    let values = bounded_values(preset, data)?;
    assembly(preset, data, values)
}

fn bounded_values(preset: &Preset, data: &Dataset) -> Result<Vec<Value>> {
    let values = canvas_values(preset, data)?;
    if values.len() > MAX_CANVAS_ROWS {
        bail!("canvas export exceeds the {MAX_CANVAS_ROWS} row limit");
    }
    if cell_count(&values) > MAX_CANVAS_CELLS {
        bail!("canvas export exceeds the {MAX_CANVAS_CELLS} cell limit");
    }
    Ok(values)
}

fn assembly(preset: &Preset, data: &Dataset, values: Vec<Value>) -> Result<Value> {
    let group_field = preset.parameters.group_by.as_str();
    let (semantic_types, encodings) = encodings(preset, data, group_field);
    Ok(json!({
        "data": { "values": values },
        "semantic_types": semantic_types,
        "chart_spec": {
            "chartType": chart_type_for_view(&preset.view)?,
            "title": truncate(&preset.title, MAX_TITLE_LEN),
            "subtitle": subtitle(preset, data),
            "encodings": encodings,
        },
    }))
}

fn encodings(preset: &Preset, data: &Dataset, group_field: &str) -> (Value, Value) {
    let category_type = if group_field == "date" {
        "Date"
    } else {
        "Category"
    };
    // Flint's Price formatting is USD-oriented. Other currencies retain
    // numeric ticks and explicitly name their units in the subtitle.
    let mut semantic_types = json!({ "amount": amount_type(preset, data) });
    semantic_types[group_field] = json!(category_type);
    let mut encodings = json!({
        "x": { "field": group_field },
        "y": { "field": "amount" },
    });
    if data.series.len() > 1 {
        semantic_types["series"] = json!("Category");
        encodings["color"] = json!({ "field": "series" });
    }
    (semantic_types, encodings)
}

fn amount_type(preset: &Preset, data: &Dataset) -> &'static str {
    if preset.parameters.currency == "USD" && data.columns.iter().any(|column| column == "currency")
    {
        "Price"
    } else {
        "Quantity"
    }
}

fn subtitle(preset: &Preset, data: &Dataset) -> String {
    if preset.recipe == SNAPSHOT_RECIPE {
        truncate(
            "Static imported native chart snapshot. No refresh recipe. Original query is not inferred.",
            MAX_SUBTITLE_LEN,
        )
    } else {
        truncate(
            &format!(
                "{} to {}. {}",
                preset.parameters.start, preset.parameters.end, data.summary
            ),
            MAX_SUBTITLE_LEN,
        )
    }
}

fn canvas_values(preset: &Preset, data: &Dataset) -> Result<Vec<Value>> {
    let group_field = preset.parameters.group_by.as_str();
    if !data.series.is_empty() && !data.labels.is_empty() {
        series_rows(group_field, data)
    } else {
        object_rows(&data.rows)
    }
}

fn series_rows(group_field: &str, data: &Dataset) -> Result<Vec<Value>> {
    let mut values = Vec::new();
    for series in &data.series {
        for (label, value) in data.labels.iter().zip(&series.values) {
            values.push(series_row(
                group_field,
                data.series.len(),
                series,
                label,
                *value,
            )?);
        }
    }
    Ok(values)
}

fn series_row(
    group_field: &str,
    series_count: usize,
    series: &PlotSeries,
    label: &str,
    value: f64,
) -> Result<Value> {
    if label.len() > MAX_VALUE_LEN {
        bail!("label exceeds {MAX_VALUE_LEN} bytes");
    }
    let mut row = serde_json::Map::new();
    row.insert(group_field.to_owned(), json!(label));
    if series_count > 1 {
        row.insert("series".into(), json!(series.name));
    }
    row.insert("amount".into(), json!(value));
    Ok(Value::Object(row))
}

fn cell_count(values: &[Value]) -> usize {
    values
        .iter()
        .map(|row| row.as_object().map(serde_json::Map::len).unwrap_or(0))
        .sum()
}

fn chart_type_for_view(view: &str) -> Result<&'static str> {
    match view {
        "bar" => Ok("Bar Chart"),
        "line" => Ok("Line Chart"),
        "area" => Ok("Area Chart"),
        other => bail!("unsupported view {other:?}"),
    }
}

fn truncate(text: &str, limit: usize) -> String {
    if text.len() <= limit {
        return text.to_owned();
    }
    let mut end = limit;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_owned()
}
