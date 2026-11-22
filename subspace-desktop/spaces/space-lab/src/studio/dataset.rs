//! Dataset JSON bounds used by recipe runs and canvas snapshots.

use super::limits::{
    MAX_DATASET_CELLS, MAX_DATASET_ROWS, MAX_ERROR_SNIPPET, MAX_FIELD_LEN, MAX_LABELS,
    MAX_RESULT_BYTES, MAX_ROW_FIELDS, MAX_SERIES, MAX_SERIES_POINTS, MAX_VALUE_LEN,
};
use crate::contracts::Dataset;
use anyhow::{Context, Result, bail};
use serde_json::Value;

pub(super) fn parse_dataset(stdout: &str) -> Result<Dataset> {
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        bail!("worker produced empty stdout; expected a Dataset JSON object");
    }
    serde_json::from_str::<Dataset>(trimmed).with_context(|| {
        format!(
            "worker stdout is not a Dataset JSON object: {}",
            snippet(trimmed)
        )
    })
}

pub(super) fn validate_dataset(dataset: &Dataset) -> Result<()> {
    if dataset.rows.len() > MAX_DATASET_ROWS {
        bail!(
            "dataset has {} rows; limit is {MAX_DATASET_ROWS}",
            dataset.rows.len()
        );
    }
    if dataset.labels.len() > MAX_LABELS {
        bail!(
            "dataset has {} labels; limit is {MAX_LABELS}",
            dataset.labels.len()
        );
    }
    if dataset.series.len() > MAX_SERIES {
        bail!(
            "dataset has {} series; limit is {MAX_SERIES}",
            dataset.series.len()
        );
    }
    check_cells(dataset)?;
    check_series(dataset)?;
    bound_bytes(&serde_json::to_value(dataset)?, MAX_RESULT_BYTES, "dataset")
}

pub(super) fn bound_bytes(value: &Value, limit: usize, label: &str) -> Result<()> {
    let bytes = serde_json::to_vec(value).with_context(|| format!("serializing {label}"))?;
    if bytes.len() > limit {
        bail!("{label} exceeds the {limit} byte limit");
    }
    Ok(())
}

pub(super) fn object_rows(rows: &[Value]) -> Result<Vec<Value>> {
    let mut values = Vec::new();
    for row in rows {
        values.push(Value::Object(checked_object(row)?));
    }
    Ok(values)
}

pub(super) fn scalar(value: &Value) -> bool {
    match value {
        Value::Null | Value::Bool(_) => true,
        Value::String(text) => text.len() <= MAX_VALUE_LEN,
        Value::Number(number) => number.as_f64().is_some_and(f64::is_finite),
        _ => false,
    }
}

fn checked_object(row: &Value) -> Result<serde_json::Map<String, Value>> {
    let object = row
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("dataset rows must be JSON objects"))?;
    if object.len() > MAX_ROW_FIELDS {
        bail!("dataset row has too many fields");
    }
    for (key, value) in object {
        if key.len() > MAX_FIELD_LEN {
            bail!("dataset field {key:?} exceeds {MAX_FIELD_LEN} bytes");
        }
        if !scalar(value) {
            bail!("dataset field {key:?} is not a bounded scalar");
        }
    }
    Ok(object.clone())
}

fn check_cells(dataset: &Dataset) -> Result<()> {
    let cells = dataset
        .rows
        .iter()
        .map(|row| match row {
            Value::Array(items) => items.len(),
            Value::Object(map) => map.len(),
            _ => 1,
        })
        .sum::<usize>();
    if cells > MAX_DATASET_CELLS {
        bail!("dataset has {cells} cells; limit is {MAX_DATASET_CELLS}");
    }
    Ok(())
}

fn check_series(dataset: &Dataset) -> Result<()> {
    for series in &dataset.series {
        if series.values.len() > MAX_SERIES_POINTS {
            bail!(
                "series {} has {} points; limit is {MAX_SERIES_POINTS}",
                series.name,
                series.values.len()
            );
        }
        if series.values.iter().any(|value| !value.is_finite()) {
            bail!("series {} contains a non-finite value", series.name);
        }
        if !dataset.labels.is_empty() && series.values.len() != dataset.labels.len() {
            bail!(
                "series {} length {} does not match {} labels",
                series.name,
                series.values.len(),
                dataset.labels.len()
            );
        }
    }
    Ok(())
}

fn snippet(text: &str) -> String {
    let mut item = text.replace('\n', " ");
    if item.len() > MAX_ERROR_SNIPPET {
        item.truncate(MAX_ERROR_SNIPPET);
        item.push('…');
    }
    item
}
