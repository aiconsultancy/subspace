//! The Help Dock's stored placement, width and open state.
//!
//! The shell validates this schema before it hands any value to the Dock. A
//! stored value that is invalid, or written by a newer version, is reported as
//! [`Restored::Invalid`]. The caller keeps it in the state file and does not
//! save over it; see [`Restored::protects_stored_value`].
//!
//! The key is the shell's own (`app.*`), separate from board keys. Zoom is not
//! stored: the user asked for placement, size and open state only.

use serde::{Deserialize, Serialize};

/// The state file key. The value is the JSON text of [`HelpDockState`].
pub(super) const KEY: &str = "app.help-dock";
/// The only envelope version this build reads and writes.
pub(super) const VERSION: u64 = 1;
/// The width, in logical pixels, of a Help Dock that was never resized.
pub(super) const DEFAULT_WIDTH: f32 = 360.;
/// The smallest stored width. The shell clamps to the window at layout time.
pub(super) const MIN_WIDTH: f32 = 1.;
/// The largest stored width.
pub(super) const MAX_WIDTH: f32 = 100_000.;
const MAX_JSON_BYTES: usize = 1_024;

/// The edge of the window that holds the Help Dock.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum Side {
    Left,
    #[default]
    Right,
}

/// The remembered Help Dock choices: side, width and whether it is open.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct HelpDockState {
    pub(super) side: Side,
    pub(super) width: f32,
    pub(super) open: bool,
}

impl Default for HelpDockState {
    fn default() -> Self {
        Self {
            side: Side::default(),
            width: default_width(),
            open: false,
        }
    }
}

/// The version 1 JSON document. A missing field takes its default, so a
/// version 1 envelope that holds only some choices still restores.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    version: u64,
    #[serde(default)]
    side: Side,
    #[serde(default = "default_width")]
    width: f32,
    #[serde(default)]
    open: bool,
}

fn default_width() -> f32 {
    DEFAULT_WIDTH
}

/// What the state file held for [`KEY`].
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Restored {
    /// Nothing is stored. Use the defaults. Saving is allowed.
    Default,
    /// A valid version 1 value.
    Saved(HelpDockState),
    /// A value this build cannot use. Use the defaults for this run, but do
    /// not save over the stored value.
    Invalid(String),
}

impl Restored {
    /// Reads the stored text of [`KEY`].
    ///
    /// `stored` is the string value, if any. `non_string` is true when the
    /// file holds a value for the key that is not a string; the shell
    /// preferences report it as an invalid value.
    pub(super) fn from_stored(stored: Option<&str>, non_string: bool) -> Self {
        match (stored, non_string) {
            (_, true) => Self::Invalid("The stored Help Dock value is not text".into()),
            (None, false) => Self::Default,
            (Some(text), false) => match HelpDockState::decode(text) {
                Ok(state) => Self::Saved(state),
                Err(reason) => Self::Invalid(reason),
            },
        }
    }

    /// The state to use for this run.
    pub(super) fn state(&self) -> HelpDockState {
        match self {
            Self::Saved(state) => *state,
            Self::Default | Self::Invalid(_) => HelpDockState::default(),
        }
    }

    /// True when the caller must not write [`KEY`], so the stored value
    /// survives for a newer version or for repair.
    pub(super) fn protects_stored_value(&self) -> bool {
        matches!(self, Self::Invalid(_))
    }
}

impl HelpDockState {
    /// Parses and validates stored JSON text. The version is checked first,
    /// so a newer envelope with other fields is reported as unsupported
    /// rather than as malformed.
    pub(super) fn decode(text: &str) -> Result<Self, String> {
        if text.len() > MAX_JSON_BYTES {
            return Err("Help Dock value exceeds the size limit".into());
        }
        let value: serde_json::Value =
            serde_json::from_str(text).map_err(|error| error.to_string())?;
        check_version(&value)?;
        let envelope: Envelope =
            serde_json::from_value(value).map_err(|error| error.to_string())?;
        let state = Self {
            side: envelope.side,
            width: envelope.width,
            open: envelope.open,
        };
        state.validate()?;
        Ok(state)
    }

    /// The JSON text to store. It fails for a state that [`Self::validate`]
    /// rejects, so a bad width is never written.
    pub(super) fn encode(&self) -> Result<String, String> {
        self.validate()?;
        serde_json::to_string(&Envelope {
            version: VERSION,
            side: self.side,
            width: self.width,
            open: self.open,
        })
        .map_err(|error| error.to_string())
    }

    /// The width must be finite, positive and no larger than [`MAX_WIDTH`].
    pub(super) fn validate(&self) -> Result<(), String> {
        if self.width.is_finite() && (MIN_WIDTH..=MAX_WIDTH).contains(&self.width) {
            Ok(())
        } else {
            Err("Help Dock width is not a finite, positive, bounded number".into())
        }
    }
}

fn check_version(value: &serde_json::Value) -> Result<(), String> {
    match value.get("version").and_then(serde_json::Value::as_u64) {
        Some(VERSION) => Ok(()),
        Some(other) => Err(format!("Unsupported Help Dock version {other}")),
        None => Err("Missing or invalid Help Dock version".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::{DEFAULT_WIDTH, HelpDockState, KEY, MAX_WIDTH, MIN_WIDTH, Restored, Side, VERSION};

    fn state(side: Side, width: f32, open: bool) -> HelpDockState {
        HelpDockState { side, width, open }
    }

    #[test]
    fn defaults_to_a_closed_right_side_dock() {
        let default = HelpDockState::default();
        assert_eq!(default.side, Side::Right);
        assert_eq!(default.width, DEFAULT_WIDTH);
        assert!(!default.open);
        assert!(default.validate().is_ok());
        assert_eq!(Restored::from_stored(None, false), Restored::Default);
        assert_eq!(Restored::Default.state(), default);
        assert!(!Restored::Default.protects_stored_value());
        assert_eq!(KEY, "app.help-dock");
    }

    #[test]
    fn non_default_choices_survive_a_round_trip() {
        for saved in [
            state(Side::Left, 412.5, true),
            state(Side::Right, MIN_WIDTH, false),
            state(Side::Left, MAX_WIDTH, true),
        ] {
            let text = saved.encode().unwrap();
            assert_eq!(HelpDockState::decode(&text).unwrap(), saved);
            assert_eq!(
                Restored::from_stored(Some(&text), false),
                Restored::Saved(saved)
            );
        }
    }

    #[test]
    fn encoded_text_is_a_version_one_envelope_without_zoom() {
        let text = state(Side::Left, 300., true).encode().unwrap();
        let json: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"version": VERSION, "side": "left", "width": 300.0, "open": true})
        );
    }

    #[test]
    fn a_version_one_envelope_with_missing_fields_uses_the_defaults() {
        assert_eq!(
            HelpDockState::decode(r#"{"version":1}"#).unwrap(),
            HelpDockState::default()
        );
        assert_eq!(
            HelpDockState::decode(r#"{"version":1,"side":"left"}"#).unwrap(),
            state(Side::Left, DEFAULT_WIDTH, false)
        );
    }

    #[test]
    fn future_and_unreadable_versions_are_invalid_and_protected() {
        let future = r#"{"version":2,"side":"top","anchor":{"x":1},"open":true}"#;
        for text in [
            future,
            r#"{"version":0}"#,
            r#"{"version":-1}"#,
            r#"{"version":1.5}"#,
            r#"{"version":"1"}"#,
            r#"{"side":"left"}"#,
            "[]",
        ] {
            let restored = Restored::from_stored(Some(text), false);
            assert!(matches!(restored, Restored::Invalid(_)), "{text}");
            assert!(restored.protects_stored_value(), "{text}");
            assert_eq!(restored.state(), HelpDockState::default(), "{text}");
        }
        let Restored::Invalid(reason) = Restored::from_stored(Some(future), false) else {
            panic!("a future version must be invalid");
        };
        assert!(reason.contains("version 2"), "{reason}");
    }

    #[test]
    fn malformed_or_out_of_range_values_are_invalid() {
        let oversized = format!(r#"{{"version":1,"pad":"{}"}}"#, "x".repeat(2_048));
        for text in [
            "not json",
            "",
            r#"{"version":1,"side":"top"}"#,
            r#"{"version":1,"side":"Left"}"#,
            r#"{"version":1,"open":"yes"}"#,
            r#"{"version":1,"width":"360"}"#,
            r#"{"version":1,"width":null}"#,
            r#"{"version":1,"width":0}"#,
            r#"{"version":1,"width":0.5}"#,
            r#"{"version":1,"width":-1}"#,
            r#"{"version":1,"width":100001}"#,
            r#"{"version":1,"width":1e999}"#,
            r#"{"version":1,"zoom":1.5}"#,
            oversized.as_str(),
        ] {
            assert!(HelpDockState::decode(text).is_err(), "{text}");
            assert!(
                Restored::from_stored(Some(text), false).protects_stored_value(),
                "{text}"
            );
        }
    }

    #[test]
    fn a_non_string_stored_value_is_invalid_even_without_text() {
        let restored = Restored::from_stored(None, true);
        assert!(matches!(restored, Restored::Invalid(_)));
        assert!(restored.protects_stored_value());
        assert!(Restored::from_stored(Some(r#"{"version":1}"#), true).protects_stored_value());
    }

    #[test]
    fn encode_refuses_an_unusable_width() {
        for width in [0., -1., f32::NAN, f32::INFINITY, MAX_WIDTH + 1.] {
            assert!(state(Side::Right, width, true).encode().is_err(), "{width}");
        }
    }
}
