//! Versioned Subspace font, density, and extra-colour metadata.

use std::collections::BTreeMap;

use gpui_kit::Hsla;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use subspace_dashboard::density;

use subspace_dashboard::typography::{
    GROUP_HEADING_SIZE_DEFAULT, GROUP_HEADING_SIZE_MAX, GROUP_HEADING_SIZE_MIN,
};

pub(super) const METADATA_KEY: &str = "subspace";
const METADATA_VERSION: u64 = 1;
const FONT_MAX: usize = 128;
const TONE_KEYS: &[&str] = &[
    "accent",
    "red",
    "green",
    "yellow",
    "blue",
    "cyan",
    "magenta",
    "orange",
    "glass",
    "glass-rim",
    "glass-shadow",
    "button-hover",
    "button-on",
    "media-stage",
];

/// Independent font roles. An empty string means automatic.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FontChoices {
    #[serde(default)]
    pub heading: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub mono: String,
}

fn group_heading_size_default() -> f32 {
    GROUP_HEADING_SIZE_DEFAULT
}

fn group_heading_bold_default() -> bool {
    true
}

/// Font controls dedicated to collapsible row-group titles.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct GroupHeadingChoices {
    pub font: String,
    pub size: f32,
    pub bold: bool,
    pub italic: bool,
}

impl Default for GroupHeadingChoices {
    fn default() -> Self {
        Self {
            font: String::new(),
            size: group_heading_size_default(),
            bold: group_heading_bold_default(),
            italic: false,
        }
    }
}

/// Subspace metadata stored beside a GPUI theme variant.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThemeMetadata {
    pub fonts: FontChoices,
    pub group_heading: GroupHeadingChoices,
    pub density: Value,
    pub tones: BTreeMap<String, String>,
}

impl Default for ThemeMetadata {
    fn default() -> Self {
        Self {
            fonts: FontChoices::default(),
            group_heading: GroupHeadingChoices::default(),
            density: json!({}),
            tones: BTreeMap::new(),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SubspaceBlock {
    version: u64,
    #[serde(default)]
    fonts: FontChoices,
    #[serde(default, rename = "groupHeading")]
    group_heading: GroupHeadingChoices,
    #[serde(default)]
    density: Map<String, Value>,
    #[serde(default)]
    tones: BTreeMap<String, String>,
}

pub(super) fn parse_block(block: &Value) -> Result<ThemeMetadata, String> {
    let parsed: SubspaceBlock =
        serde_json::from_value(block.clone()).map_err(|error| error.to_string())?;
    if parsed.version != METADATA_VERSION {
        return Err("subspace.version must be 1".into());
    }
    validate_fonts(&parsed.fonts)?;
    validate_group_heading(&parsed.group_heading)?;
    validate_density(&parsed.density)?;
    validate_tones(&parsed.tones)?;
    Ok(ThemeMetadata {
        fonts: parsed.fonts,
        group_heading: parsed.group_heading,
        density: Value::Object(parsed.density),
        tones: parsed.tones,
    })
}

pub(super) fn metadata_block(metadata: &ThemeMetadata) -> Result<Value, String> {
    let value = json!({
        "version": METADATA_VERSION,
        "fonts": metadata.fonts,
        "groupHeading": metadata.group_heading,
        "density": metadata.density,
        "tones": metadata.tones,
    });
    parse_block(&value)?;
    Ok(value)
}

fn validate_group_heading(group: &GroupHeadingChoices) -> Result<(), String> {
    validate_font("group heading", &group.font)?;
    if group.size.is_finite()
        && (GROUP_HEADING_SIZE_MIN..=GROUP_HEADING_SIZE_MAX).contains(&group.size)
    {
        Ok(())
    } else {
        Err(format!(
            "group heading size must be between {GROUP_HEADING_SIZE_MIN} and {GROUP_HEADING_SIZE_MAX}"
        ))
    }
}

pub(super) fn is_tone_key(key: &str) -> bool {
    TONE_KEYS.contains(&key)
}

pub(super) fn validate_solid_color(key: &str, value: &str) -> Result<(), String> {
    if is_solid_color(value) {
        Ok(())
    } else {
        Err(invalid_color(key, value))
    }
}

pub(super) fn invalid_color(key: &str, value: impl std::fmt::Display) -> String {
    format!("invalid color {key}: {value}")
}

fn is_solid_color(value: &str) -> bool {
    gpui_kit::component::try_parse_color(value)
        .ok()
        .is_some_and(channels_finite)
}

fn channels_finite(color: Hsla) -> bool {
    color.h.is_finite() && color.s.is_finite() && color.l.is_finite() && color.a.is_finite()
}

pub(in crate::theme) fn validate_fonts(fonts: &FontChoices) -> Result<(), String> {
    validate_font("heading", &fonts.heading)?;
    validate_font("body", &fonts.body)?;
    validate_font("mono", &fonts.mono)
}

fn validate_font(role: &str, value: &str) -> Result<(), String> {
    if value.chars().any(char::is_control) {
        return Err(format!("font {role} contains a control character"));
    }
    if value.chars().count() > FONT_MAX {
        return Err(format!("font {role} is too long"));
    }
    Ok(())
}

fn validate_density(density: &Map<String, Value>) -> Result<(), String> {
    for (token, value) in density {
        let control =
            density::control_of(token).ok_or_else(|| format!("unknown density token: {token}"))?;
        let text = value
            .as_str()
            .ok_or_else(|| format!("density value is not a string: {token}"))?;
        let number = density::parse(control, text)
            .ok_or_else(|| format!("invalid density value: {token}"))?;
        if !number.is_finite() || number < control.min || number > control.max {
            return Err(format!("density value is out of range: {token}"));
        }
    }
    Ok(())
}

fn validate_tones(tones: &BTreeMap<String, String>) -> Result<(), String> {
    for (key, value) in tones {
        if !is_tone_key(key) {
            return Err(format!("unknown color key: {key}"));
        }
        validate_solid_color(key, value)?;
    }
    Ok(())
}
