use anyhow::{Context, Result};

use crate::contracts::Preset;
use crate::studio::Studio;

use super::schema::Input;

pub(super) fn required<'a>(value: Option<&'a str>, name: &str) -> Result<&'a str> {
    value
        .filter(|value| !value.is_empty())
        .with_context(|| format!("{name} is required"))
}

pub(super) fn preset(studio: &Studio, input: &Input) -> Result<Preset> {
    if let Some(preset) = &input.preset {
        return Ok(preset.clone());
    }
    let id = required(input.id.as_deref(), "preset or id")?;
    studio
        .presets()?
        .into_iter()
        .find(|preset| preset.id == id)
        .context("Saved preset not found")
}

#[cfg(test)]
mod tests {
    use super::required;

    #[test]
    fn required_rejects_missing_and_empty_values() {
        assert!(required(None, "id").is_err());
        assert!(required(Some(""), "id").is_err());
        assert_eq!(required(Some("env"), "environment").unwrap(), "env");
    }
}
