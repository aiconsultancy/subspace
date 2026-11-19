//! Validated GPUI theme JSON with versioned Subspace metadata.

mod metadata;

use std::collections::BTreeSet;

use gpui_kit::component::{ThemeConfig, ThemeConfigColors, ThemeSet};
use serde_json::{Value, json};

use metadata::{
    METADATA_KEY, invalid_color, is_tone_key, metadata_block, parse_block, validate_solid_color,
};

pub(in crate::theme) use metadata::validate_fonts;
pub use metadata::{FontChoices, GroupHeadingChoices, ThemeMetadata};

/// GPUI Kit's corner radii, with what they shape: `radius` for inputs,
/// selects, menus and other Kit controls, `radius.lg` for Kit dialogs,
/// notifications and other large surfaces.
pub const KIT_RADII: [(&str, &str); 2] = [
    ("radius", "Fields and menus"),
    ("radius.lg", "Large surfaces"),
];

/// A GPUI `ThemeSet` document that preserves unknown JSON fields.
#[derive(Debug, Clone, PartialEq)]
pub struct ThemeDocument {
    value: Value,
}

impl ThemeDocument {
    pub fn parse(text: &str) -> Result<Self, String> {
        let value = parse_json(text)?;
        validate_document(&value)?;
        Ok(Self { value })
    }

    pub fn variant(&self, name: &str) -> Result<Self, String> {
        let selected = find_theme(&self.value, name)?.clone();
        let mut value = self.value.clone();
        value["themes"] = json!([selected]);
        Ok(Self { value })
    }

    pub fn names(&self) -> Vec<String> {
        theme_values(&self.value)
            .unwrap_or(&[])
            .iter()
            .filter_map(theme_name)
            .map(str::to_owned)
            .collect()
    }

    pub fn config(&self) -> Result<ThemeConfig, String> {
        let theme = first_theme(&self.value)?;
        serde_json::from_value(theme.clone()).map_err(|error| error.to_string())
    }

    pub fn to_json(&self) -> Result<String, String> {
        serde_json::to_string_pretty(&self.value).map_err(|error| error.to_string())
    }

    pub fn named(&self, name: &str) -> Result<Self, String> {
        validate_variant_name(name)?;
        let mut value = self.value.clone();
        value["name"] = json!(name);
        first_theme_mut(&mut value)?["name"] = json!(name);
        validate_names(&value)?;
        Ok(Self { value })
    }

    pub fn color(&self, key: &str) -> Option<String> {
        color_from_theme(&self.value, key).or_else(|| {
            self.metadata()
                .ok()
                .and_then(|metadata| metadata.tones.get(key).cloned())
        })
    }

    pub fn set_color(&mut self, key: &str, value: &str) -> Result<(), String> {
        if is_tone_key(key) {
            return set_tone(self, key, value);
        }
        validate_solid_color(key, value)?;
        if !is_gpui_color_key(key) {
            return Err(format!("unknown color key: {key}"));
        }
        let colors = first_theme_mut(&mut self.value)?
            .as_object_mut()
            .ok_or_else(|| "theme variant is not an object".to_string())?
            .entry("colors")
            .or_insert_with(|| json!({}));
        let map = colors
            .as_object_mut()
            .ok_or_else(|| "theme colors are not an object".to_string())?;
        map.insert(key.to_owned(), json!(value));
        Ok(())
    }

    /// A GPUI Kit corner radius in whole pixels, such as `radius`.
    pub fn radius(&self, key: &str) -> Option<f32> {
        let value = first_theme(&self.value).ok()?.get(key)?.as_u64()?;
        u16::try_from(value).ok().map(f32::from)
    }

    /// Set one of [`KIT_RADII`] in whole pixels.
    pub fn set_radius(&mut self, key: &str, value: u16) -> Result<(), String> {
        if !KIT_RADII.iter().any(|(name, _)| *name == key) {
            return Err(format!("unknown radius key: {key}"));
        }
        first_theme_mut(&mut self.value)?[key] = json!(value);
        Ok(())
    }

    pub fn metadata(&self) -> Result<ThemeMetadata, String> {
        match metadata_slot(&self.value) {
            None => Ok(ThemeMetadata::default()),
            Some(block) => parse_block(block),
        }
    }

    pub fn set_metadata(&mut self, metadata: ThemeMetadata) -> Result<(), String> {
        let block = metadata_block(&metadata)?;
        first_theme_mut(&mut self.value)?[METADATA_KEY] = block;
        Ok(())
    }
}

fn parse_json(text: &str) -> Result<Value, String> {
    serde_json::from_str(text).map_err(|error| error.to_string())
}

fn validate_document(value: &Value) -> Result<(), String> {
    if !value.is_object() {
        return Err("theme JSON is not an object".into());
    }
    let set: ThemeSet = serde_json::from_value(value.clone()).map_err(|error| error.to_string())?;
    validate_set(&set)?;
    validate_names(value)?;
    validate_colors(value)?;
    validate_all_metadata(value)
}

fn validate_set(set: &ThemeSet) -> Result<(), String> {
    if set.themes.is_empty() {
        return Err("theme has no variants".into());
    }
    Ok(())
}

fn validate_names(value: &Value) -> Result<(), String> {
    let mut seen = BTreeSet::new();
    for name in theme_values(value)?.iter().filter_map(theme_name) {
        validate_variant_name(name)?;
        if !seen.insert(name) {
            return Err(format!("duplicate theme name: {name}"));
        }
    }
    if seen.is_empty() {
        return Err("theme has no variants".into());
    }
    Ok(())
}

fn validate_variant_name(name: &str) -> Result<(), String> {
    if name.trim().is_empty() {
        return Err("blank theme name".into());
    }
    Ok(())
}

fn validate_colors(value: &Value) -> Result<(), String> {
    for theme in theme_values(value)? {
        if let Some(colors) = theme.get("colors") {
            validate_color_map(colors)?;
        }
    }
    Ok(())
}

fn validate_color_map(colors: &Value) -> Result<(), String> {
    let map = colors
        .as_object()
        .ok_or_else(|| "theme colors are not an object".to_string())?;
    for (key, value) in map {
        if !is_gpui_color_key(key) {
            continue;
        }
        let Some(text) = value.as_str() else {
            return Err(invalid_color(key, value));
        };
        validate_color_value(key, text)?;
    }
    Ok(())
}

fn validate_color_value(key: &str, value: &str) -> Result<(), String> {
    if gpui_kit::component::try_parse_background(value).is_ok() {
        Ok(())
    } else {
        Err(invalid_color(key, value))
    }
}

fn is_gpui_color_key(key: &str) -> bool {
    let Ok(parsed) = serde_json::from_value::<ThemeConfigColors>(json!({ key: "#000000" })) else {
        return false;
    };
    serde_json::to_value(parsed)
        .ok()
        .and_then(|value| value.get(key)?.as_str().map(|text| text == "#000000"))
        .unwrap_or(false)
}

fn validate_all_metadata(value: &Value) -> Result<(), String> {
    if let Some(block) = value.get(METADATA_KEY) {
        parse_block(block)?;
    }
    for theme in theme_values(value)? {
        if let Some(block) = theme.get(METADATA_KEY) {
            parse_block(block)?;
        }
    }
    Ok(())
}

fn metadata_slot(value: &Value) -> Option<&Value> {
    first_theme(value)
        .ok()
        .and_then(|theme| theme.get(METADATA_KEY))
        .or_else(|| value.get(METADATA_KEY))
}

fn set_tone(document: &mut ThemeDocument, key: &str, value: &str) -> Result<(), String> {
    validate_solid_color(key, value)?;
    let mut metadata = document.metadata()?;
    metadata.tones.insert(key.to_owned(), value.to_owned());
    document.set_metadata(metadata)
}

fn color_from_theme(value: &Value, key: &str) -> Option<String> {
    first_theme(value)
        .ok()?
        .get("colors")?
        .get(key)?
        .as_str()
        .map(str::to_owned)
}

fn find_theme<'a>(value: &'a Value, name: &str) -> Result<&'a Value, String> {
    theme_values(value)?
        .iter()
        .find(|theme| theme_name(theme) == Some(name))
        .ok_or_else(|| format!("unknown theme variant: {name}"))
}

fn first_theme(value: &Value) -> Result<&Value, String> {
    theme_values(value)?
        .first()
        .ok_or_else(|| "theme has no variants".to_string())
}

fn first_theme_mut(value: &mut Value) -> Result<&mut Value, String> {
    value
        .get_mut("themes")
        .and_then(Value::as_array_mut)
        .and_then(|themes| themes.first_mut())
        .ok_or_else(|| "theme has no variants".to_string())
}

fn theme_values(value: &Value) -> Result<&[Value], String> {
    value
        .get("themes")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .ok_or_else(|| "theme has no variants".to_string())
}

fn theme_name(theme: &Value) -> Option<&str> {
    theme.get("name")?.as_str()
}

#[cfg(test)]
mod tests {
    use super::{FontChoices, GroupHeadingChoices, ThemeDocument, ThemeMetadata};
    use serde_json::{Value, json};

    fn parse_value(value: Value) -> Result<ThemeDocument, String> {
        ThemeDocument::parse(&value.to_string())
    }

    fn theme(name: &str, colors: Value, subspace: Option<Value>) -> Value {
        let mut theme = json!({
            "name": name,
            "mode": "dark",
            "colors": colors,
        });
        if let Some(block) = subspace {
            theme["subspace"] = block;
        }
        theme
    }

    fn minimal() -> Value {
        json!({
            "$schema": "keep",
            "name": "T",
            "author": "A",
            "themes": [theme("T Dark", json!({"background": "#101115", "foreground": "#abb2bf"}), None)]
        })
    }

    fn two_variants() -> Value {
        json!({
            "name": "Shell",
            "themes": [
                theme(
                    "Shell Light",
                    json!({ "background": "#f5f5f5" }),
                    Some(json!({
                        "version": 1,
                        "fonts": { "heading": "Inter", "body": "", "mono": "" },
                        "density": {},
                        "tones": { "orange": "#945200", "glass": "#ffffffb8" }
                    }))
                ),
                theme(
                    "Shell Dark",
                    json!({ "background": "#292929" }),
                    Some(json!({
                        "version": 1,
                        "fonts": { "heading": "Inter", "body": "", "mono": "" },
                        "density": {},
                        "tones": { "orange": "#ffb15b", "glass": "#ffffff1f" }
                    }))
                )
            ]
        })
    }

    #[test]
    fn parse_round_trip_keeps_unknown_fields() {
        let document = parse_value(minimal()).unwrap();
        assert_eq!(document.names(), ["T Dark"]);
        let json = document.to_json().unwrap();
        let value: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["$schema"], "keep");
        assert_eq!(value["author"], "A");
        assert_eq!(document.config().unwrap().name.as_ref(), "T Dark");
    }

    #[test]
    fn variant_keeps_that_mode_metadata() {
        let document = parse_value(two_variants()).unwrap();
        let dark = document.variant("Shell Dark").unwrap();
        assert_eq!(dark.names(), ["Shell Dark"]);
        assert_eq!(dark.metadata().unwrap().tones["orange"], "#ffb15b");
        assert_eq!(
            document.variant("Missing").unwrap_err(),
            "unknown theme variant: Missing"
        );
    }

    #[test]
    fn named_renames_the_set_and_first_variant() {
        let document = parse_value(minimal()).unwrap().named("My Theme").unwrap();
        assert_eq!(document.names(), ["My Theme"]);
        let value: Value = serde_json::from_str(&document.to_json().unwrap()).unwrap();
        assert_eq!(value["name"], "My Theme");
        assert!(parse_value(minimal()).unwrap().named("  ").is_err());
    }

    #[test]
    fn color_and_metadata_defaults_and_writes() {
        let mut document = parse_value(minimal()).unwrap();
        assert_eq!(document.color("background").as_deref(), Some("#101115"));
        assert_eq!(document.metadata().unwrap(), ThemeMetadata::default());
        document.set_color("foreground", "#d1d5dc").unwrap();
        document.set_color("orange", "#d19a66").unwrap();
        assert!(
            document
                .set_color("orange", "linear-gradient(90deg, #000000, #ffffff)")
                .is_err()
        );
        assert!(
            document
                .set_color("background", "linear-gradient(90deg, #000000, #ffffff)")
                .is_err()
        );
        assert_eq!(document.color("foreground").as_deref(), Some("#d1d5dc"));
        assert_eq!(document.color("orange").as_deref(), Some("#d19a66"));
        let mut metadata = document.metadata().unwrap();
        metadata.fonts = FontChoices {
            heading: "Inter".into(),
            ..FontChoices::default()
        };
        metadata.density = json!({ "--row-h": "30px" });
        metadata.group_heading = GroupHeadingChoices {
            font: "Menlo".into(),
            size: 14.5,
            bold: false,
            italic: true,
        };
        document.set_metadata(metadata.clone()).unwrap();
        assert_eq!(document.metadata().unwrap(), metadata);
        let value: Value = serde_json::from_str(&document.to_json().unwrap()).unwrap();
        assert_eq!(value["themes"][0]["subspace"]["version"], 1);
        assert!(value.get("subspace").is_none());
        assert_eq!(value["themes"][0]["subspace"]["groupHeading"]["size"], 14.5);
    }

    #[test]
    fn old_metadata_gets_safe_group_heading_defaults() {
        let document = parse_value(two_variants()).unwrap();
        assert_eq!(
            document.metadata().unwrap().group_heading,
            GroupHeadingChoices::default()
        );
    }

    #[test]
    fn parse_rejects_invalid_names_colors_and_metadata() {
        let color = json!({ "background": "#101115" });
        assert!(parse_value(json!({"name":"T","themes":[]})).is_err());
        assert!(
            parse_value(json!({
                "name": "T",
                "themes": [
                    theme("A", color.clone(), None),
                    theme("A", color.clone(), None)
                ]
            }))
            .is_err()
        );
        assert!(
            parse_value(json!({
                "name": "T",
                "themes": [theme("A", color.clone(), Some(json!({
                    "version": 1,
                    "groupHeading": { "size": 0, "bold": true, "italic": false }
                })))]
            }))
            .is_err()
        );
        assert!(
            parse_value(json!({
                "name": "T",
                "themes": [theme("A", json!({ "background": "not-a-color" }), None)]
            }))
            .is_err()
        );
        assert!(
            parse_value(json!({
                "name": "T",
                "themes": [theme("A", color.clone(), Some(json!({ "version": 2 })))]
            }))
            .is_err()
        );
        assert!(
            parse_value(json!({
                "name": "T",
                "themes": [theme("A", color.clone(), Some(json!({
                    "version": 1,
                    "density": { "--nope": "1px" }
                })))]
            }))
            .is_err()
        );
        assert!(
            parse_value(json!({
                "name": "T",
                "themes": [theme("A", color.clone(), Some(json!({
                    "version": 1,
                    "density": { "--row-h": "100px" }
                })))]
            }))
            .is_err()
        );
        assert!(
            parse_value(json!({
                "name": "T",
                "themes": [theme("A", color.clone(), Some(json!({
                    "version": 1,
                    "fonts": { "heading": "bad\n" },
                    "density": {}
                })))]
            }))
            .is_err()
        );
        assert!(
            parse_value(json!({
                "name": "T",
                "themes": [theme("A", color.clone(), Some(json!({
                    "version": 1,
                    "tones": { "nope": "#ffffff" }
                })))]
            }))
            .is_err()
        );
        assert!(
            parse_value(json!({
                "name": "T",
                "themes": [theme("A", color, Some(json!({
                    "version": 1,
                    "tones": { "orange": "linear-gradient(90deg, #000000, #ffffff)" }
                })))]
            }))
            .is_err()
        );
    }

    #[test]
    fn bundled_themes_parse() {
        let one_dark = ThemeDocument::parse(super::super::THEME_JSON).unwrap();
        assert_eq!(one_dark.names()[0], super::super::THEME_NAME);
    }

    #[test]
    fn top_level_subspace_is_used_when_the_variant_has_none() {
        let document = parse_value(json!({
            "name": "T",
            "subspace": {
                "version": 1,
                "fonts": { "heading": "Menlo", "body": "", "mono": "" },
                "density": {},
                "tones": {}
            },
            "themes": [theme("T Dark", json!({ "background": "#101115" }), None)]
        }))
        .unwrap();
        assert_eq!(document.metadata().unwrap().fonts.heading, "Menlo");
    }
}
