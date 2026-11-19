//! The density jig's tokens (REQ-10): the controls with their limits, and the [`Density`] global that holds the
//! values the user changed. Rows, panels and the toolbar read the values through
//! [`crate::Tokens::current`].

use std::collections::BTreeMap;

use gpui_kit::Global;
use serde_json::{Map, Value, json};

/// The state file key for the overrides.
pub const STATE_KEY: &str = "board.jig";
const FORMAT_SCALE: f64 = 10_000.0;
pub const HEADING_FONT_DEFAULT: f32 = 15.;
/// Board tab labels start at the heading size, but have their own setting.
pub const TAB_FONT_DEFAULT: f32 = 15.;
pub const BODY_FONT_DEFAULT: f32 = 13.;
pub const MONO_FONT_DEFAULT: f32 = 13.;
/// The interface base (`rem`) at the default body text size. Control tokens
/// are pixels at this base, so they scale with the body text like every other
/// rem-based size.
pub const REFERENCE_REM: f32 = 14.;

/// True while density changes are a live preview and must not be persisted.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DensityPreview(pub bool);

impl Global for DensityPreview {}

/// One slider of the jig.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Control {
    /// The CSS custom property name, such as `--row-h`.
    pub token: &'static str,
    pub label: &'static str,
    pub min: f32,
    pub max: f32,
    pub step: f32,
    pub unit: &'static str,
    /// The stylesheet value (`src/styles.css`).
    pub default: f32,
    /// A choice token's stored values and their labels; the value is the
    /// index. Empty for a number or a switch.
    pub choices: &'static [(&'static str, &'static str)],
}

pub struct ControlGroup {
    pub name: &'static str,
    pub controls: &'static [Control],
}

const fn control(
    token: &'static str,
    label: &'static str,
    (min, max, step): (f32, f32, f32),
    unit: &'static str,
    default: f32,
) -> Control {
    Control {
        token,
        label,
        min,
        max,
        step,
        unit,
        default,
        choices: &[],
    }
}

/// A token with named values, such as `flat` or `glass`, stored by name and
/// held as the index of the value; `default` is an index too.
const fn choice(
    token: &'static str,
    label: &'static str,
    choices: &'static [(&'static str, &'static str)],
    (max, default): (f32, f32),
) -> Control {
    Control {
        token,
        label,
        min: 0.,
        max,
        step: 1.,
        unit: "",
        default,
        choices,
    }
}

/// The independent Settings font sizes, also available in the theme designer.
pub const FONT_SIZES: &[Control] = &[
    control(
        "--heading-font",
        "Headings",
        (10., 24., 0.5),
        "px",
        HEADING_FONT_DEFAULT,
    ),
    control(
        "--row-font",
        "Body text",
        (10., 18., 0.5),
        "px",
        BODY_FONT_DEFAULT,
    ),
    control(
        "--mono-font",
        "Code and logs",
        (10., 20., 0.5),
        "px",
        MONO_FONT_DEFAULT,
    ),
    // Last, so the first three keep lining up with the three font families.
    control(
        "--tab-font",
        "Tabs",
        (10., 24., 0.5),
        "px",
        TAB_FONT_DEFAULT,
    ),
];

/// The index in [`FONT_SIZES`] of the tab size, which has no font family of
/// its own: tabs use the headings font.
pub const TAB_FONT_SIZE: usize = 3;

mod groups;
pub use groups::{CONTROL_GROUPS, GROUPS};

pub fn controls() -> impl Iterator<Item = &'static Control> {
    GROUPS.iter().flat_map(|group| group.controls)
}

pub fn control_of(token: &str) -> Option<&'static Control> {
    controls().find(|control| control.token == token)
}

/// A value as the jig writes it: at most 4 decimals, then the unit, such as
/// `13.5px` or `0.6fr`, or a choice token's value name, such as `flat`.
pub fn format(control: &Control, value: f32) -> String {
    // A float cast saturates, so a negative value is the first choice.
    let index = value.round() as usize;
    if let Some((name, _)) = control.choices.get(index) {
        return (*name).to_owned();
    }
    let rounded = (f64::from(value) * FORMAT_SCALE).round() / FORMAT_SCALE;
    format!("{rounded}{}", control.unit)
}

/// Parses a stored value: a number with the control's unit, or one of a
/// choice token's value names, nothing else.
pub fn parse(control: &Control, text: &str) -> Option<f32> {
    if !control.choices.is_empty() {
        let index = control.choices.iter().position(|(name, _)| *name == text)?;
        return Some(index as f32);
    }
    let number = text.strip_suffix(control.unit)?;
    let digits = number.strip_prefix('-').unwrap_or(number);
    let (whole, fraction) = digits.split_once('.').unwrap_or((digits, "0"));
    let plain = |part: &str| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit());
    (plain(whole) && plain(fraction)).then(|| number.parse().ok())?
}

/// A switch token's stored values.
pub const SWITCH_OFF: f32 = 0.;
pub const SWITCH_ON: f32 = 1.;

impl Control {
    /// A unitless token without named values is an off/on switch stored as
    /// 0 or 1.
    pub fn is_switch(&self) -> bool {
        self.unit.is_empty() && self.choices.is_empty()
    }

    /// A token with named values, such as `--toolbar-button-rest`.
    pub fn is_choice(&self) -> bool {
        !self.choices.is_empty()
    }
}

/// The jig values that differ from the stylesheet.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Density {
    overrides: BTreeMap<&'static str, f32>,
}

impl Global for Density {}

impl Density {
    /// The value of `token`: the override, else the default; 0 for an
    /// unknown token.
    pub fn get(&self, token: &str) -> f32 {
        match control_of(token) {
            Some(control) => self
                .overrides
                .get(control.token)
                .copied()
                .unwrap_or(control.default),
            None => 0.0,
        }
    }

    pub fn is_changed(&self, token: &str) -> bool {
        self.overrides.contains_key(token)
    }

    pub fn is_empty(&self) -> bool {
        self.overrides.is_empty()
    }

    /// Sets `token`, clamped to its limits. The default value removes the
    /// override.
    pub fn set(&mut self, token: &str, value: f32) {
        let Some(control) = control_of(token) else {
            return;
        };
        let value = value.clamp(control.min, control.max);
        if (value - control.default).abs() < f32::EPSILON {
            self.overrides.remove(control.token);
        } else {
            self.overrides.insert(control.token, value);
        }
    }

    pub fn remove(&mut self, token: &str) {
        self.overrides.remove(token);
    }

    pub fn reset(&mut self) {
        self.overrides.clear();
    }

    /// The overrides as stored: `{"--row-h": "30px"}`.
    pub fn to_json(&self) -> String {
        let map: Map<String, Value> = self
            .overrides
            .iter()
            .filter_map(|(token, value)| {
                let control = control_of(token)?;
                Some((token.to_string(), Value::from(format(control, *value))))
            })
            .collect();
        Value::Object(map).to_string()
    }

    /// Reads stored overrides. Only known tokens with a number in the
    /// control's unit are kept, so a stored value cannot inject anything else.
    pub fn from_json(text: &str) -> Self {
        let mut density = Self::default();
        if let Ok(Value::Object(map)) = serde_json::from_str::<Value>(text) {
            for (token, value) in map {
                let control = control_of(&token);
                let value = value.as_str().zip(control).and_then(|(v, c)| parse(c, v));
                if let Some(value) = value {
                    density.set(&token, value);
                }
            }
        }
        density
    }

    /// The jig's "Copy" text: the changed tokens and every token's value.
    pub fn snapshot(&self) -> String {
        let changed: Map<String, Value> = self
            .overrides
            .keys()
            .filter_map(|token| {
                let control = control_of(token)?;
                Some((token.to_string(), format(control, self.get(token)).into()))
            })
            .collect();
        let tokens: Map<String, Value> = controls()
            .map(|c| (c.token.to_string(), format(c, self.get(c.token)).into()))
            .collect();
        serde_json::to_string_pretty(&json!({
            "subspace-board-tokens": 1,
            "changed": changed,
            "tokens": tokens,
        }))
        .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::{Density, control_of, controls, format, parse};

    #[test]
    fn values_clamp_and_the_default_removes_the_override() {
        let mut density = Density::default();
        density.set("--row-h", 100.0);
        assert_eq!(density.get("--row-h"), 48.0);
        assert!(density.is_changed("--row-h"));
        density.set("--row-h", 28.0);
        assert!(density.is_empty());
        assert!(!density.is_changed("--row-h"));
        density.set("--unknown", 3.0);
        assert!(density.is_empty());
        assert!(!density.is_changed("--unknown"));
        assert_eq!(controls().count(), 41);
    }

    #[test]
    fn choice_tokens_store_their_value_names() {
        let rest = control_of("--toolbar-button-rest").unwrap();
        assert!(rest.is_choice() && !rest.is_switch());
        assert!(control_of("--liquid-glass").unwrap().is_switch());
        assert_eq!(format(rest, 0.), "flat");
        assert_eq!(format(rest, 1.), "glass");
        assert_eq!(parse(rest, "glass"), Some(1.));
        assert_eq!(parse(rest, "1"), None);
        assert_eq!(parse(rest, "outline"), None);
        let density = Density::from_json(r#"{"--toolbar-button-hover":"glass"}"#);
        assert_eq!(density.get("--toolbar-button-hover"), 1.);
        assert_eq!(density.get("--toolbar-button-rest"), 0.);
        assert_eq!(density.to_json(), r#"{"--toolbar-button-hover":"glass"}"#);
    }

    #[test]
    fn overrides_survive_json_and_bad_values_are_dropped() {
        let mut density = Density::default();
        density.set("--row-font", 13.5);
        density.set("--done-w", 0.75);
        let text = density.to_json();
        assert_eq!(text, r#"{"--done-w":"0.75fr","--row-font":"13.5px"}"#);
        assert_eq!(Density::from_json(&text), density);

        let hostile =
            r#"{"--row-h":"30px; color: red","--pad-x":"8em","--x":"1px","--indent":"30px"}"#;
        let loaded = Density::from_json(hostile);
        assert_eq!(loaded.get("--row-h"), 28.0);
        assert_eq!(loaded.get("--pad-x"), 12.0);
        assert_eq!(loaded.get("--indent"), 30.0);
        assert_eq!(Density::from_json("not json"), Density::default());
    }

    #[test]
    fn the_snapshot_lists_changed_and_all_tokens() {
        let mut density = Density::default();
        density.set("--panel-w", 720.0);
        let snapshot: serde_json::Value = serde_json::from_str(&density.snapshot()).unwrap();
        assert_eq!(snapshot["changed"]["--panel-w"], "720px");
        assert_eq!(snapshot["tokens"]["--motion"], "140ms");
        assert_eq!(snapshot["subspace-board-tokens"], 1);
    }
}
