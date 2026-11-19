//! Closed structural schema before the bounded Flint interpreter.
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;
const MAX_ROWS: usize = 5000;
const MAX_CELLS: usize = 20_000;
const TYPE_BYTES: usize = 64;
const TITLE_BYTES: usize = 160;
const SUBTITLE_BYTES: usize = 240;
const MAX_CHANNELS: usize = 8;
const FIELD_BYTES: usize = 128;
const MAX_FIELDS: usize = 32;
const VALUE_BYTES: usize = 4096;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ChartInput {
    data: Data,
    semantic_types: BTreeMap<String, String>,
    chart_spec: Spec,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Data {
    values: Vec<BTreeMap<String, Value>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Spec {
    chart_type: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    subtitle: String,
    encodings: BTreeMap<String, Encoding>,
    #[serde(default)]
    chart_properties: BTreeMap<String, Value>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Encoding {
    field: String,
}

pub(super) fn validate(value: &Value) -> Result<(), String> {
    let input: ChartInput = serde_json::from_value(value.clone()).map_err(|_| "Chart input must contain only inline data.values, semantic_types and chart_spec with field encodings.".to_string())?;
    if input.data.values.len() > MAX_ROWS
        || input.data.values.iter().map(BTreeMap::len).sum::<usize>() > MAX_CELLS
    {
        return Err("Chart input exceeds the row or cell limit.".into());
    }
    validate_spec(&input.chart_spec)?;
    validate_types(&input.semantic_types)?;
    for row in input.data.values {
        validate_row(&row)?;
    }
    Ok(())
}

fn validate_spec(spec: &Spec) -> Result<(), String> {
    if spec.chart_type.len() > TYPE_BYTES
        || spec.title.len() > TITLE_BYTES
        || spec.subtitle.len() > SUBTITLE_BYTES
        || spec.encodings.len() > MAX_CHANNELS
    {
        return Err("Chart input metadata exceeds limits.".into());
    }
    if spec.encodings.iter().any(|(channel, encoding)| {
        !["x", "y", "color", "group", "size", "opacity"].contains(&channel.as_str())
            || encoding.field.len() > FIELD_BYTES
    }) {
        return Err("Chart input contains an unsupported encoding.".into());
    }
    let allowed = [
        "cornerRadius",
        "stackMode",
        "dodge",
        "interpolate",
        "innerRadius",
        "sortSlices",
        "opacity",
        "logScale_x",
        "logScale_y",
        "includeZero_x",
        "includeZero_y",
    ];
    if spec
        .chart_properties
        .iter()
        .any(|(key, value)| !allowed.contains(&key.as_str()) || !scalar(value))
    {
        return Err("Chart input contains an unsupported property.".into());
    }
    Ok(())
}

fn validate_types(types: &BTreeMap<String, String>) -> Result<(), String> {
    if types.len() > MAX_FIELDS
        || types
            .iter()
            .any(|(key, value)| key.len() > FIELD_BYTES || value.len() > TYPE_BYTES)
    {
        return Err("Chart input semantic types exceed limits.".into());
    }
    Ok(())
}

fn validate_row(row: &BTreeMap<String, Value>) -> Result<(), String> {
    if row.len() > MAX_FIELDS
        || row
            .iter()
            .any(|(key, value)| key.len() > FIELD_BYTES || !scalar(value))
    {
        return Err("Chart input rows require bounded scalar values.".into());
    }
    Ok(())
}

fn scalar(value: &Value) -> bool {
    match value {
        Value::Null | Value::Bool(_) => true,
        Value::String(text) => text.len() <= VALUE_BYTES,
        Value::Number(number) => number.as_f64().is_some_and(f64::is_finite),
        _ => false,
    }
}
