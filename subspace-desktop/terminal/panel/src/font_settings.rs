//! Font overrides, kept in the config file's `terminal` object.

pub(super) mod editor;

use gpui_kit::App;
use serde_json::{Map, Value};
use subspace_terminal::style::FontChoice;

const FAMILY: &str = "fontFamily";
const SIZE: &str = "fontSize";
const SIZE_RANGE: std::ops::RangeInclusive<f32> = 8.0..=48.0;
const SIZE_ERROR: &str = "Enter a font size from 8 to 48 pt, or leave it blank for Automatic.";

pub(super) fn saved(cx: &App) -> FontChoice {
    subspace_framework::dashboard::services(cx)
        .map(|services| read(&services.config.terminal))
        .unwrap_or_default()
}

fn read(values: &Map<String, Value>) -> FontChoice {
    FontChoice {
        family: values
            .get(FAMILY)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned),
        size: values
            .get(SIZE)
            .and_then(Value::as_f64)
            .map(|value| value as f32)
            .filter(|value| SIZE_RANGE.contains(value)),
    }
}

fn parse_size(text: &str) -> Result<Option<f32>, String> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(None);
    }
    text.parse::<f32>()
        .ok()
        .filter(|value| SIZE_RANGE.contains(value))
        .map(Some)
        .ok_or_else(|| SIZE_ERROR.to_owned())
}
