//! Settings a board declares at run time, such as from a web board's
//! manifest. The shell shows them on the board's settings page and saves
//! them in the board's own config section.

use serde_json::Value;

/// The longest text value a field accepts.
pub const MAX_FIELD_TEXT_CHARS: usize = 2000;

/// The run-time settings of one board.
#[derive(Clone, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct BoardFields {
    /// The board's config section, such as `["webBoards", "notes"]`.
    pub section: Vec<String>,
    pub fields: Vec<FieldSpec>,
    /// Facts shown above the fields, such as the app's name and version.
    pub notes: Vec<String>,
    /// Explicit user grants, separate from settings the app may read.
    pub permissions: Vec<PermissionSpec>,
    /// The board's app saves secrets of its own in the section's
    /// `secrets`, beside the declared fields. Settings then says how many it
    /// saved, never what they are, and offers Clear.
    pub app_secrets: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct PermissionSpec {
    /// Includes the app identity and manifest fingerprint.
    pub key: String,
    pub label: String,
}

/// One setting.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct FieldSpec {
    /// The key in the board's `settings` or `secrets` object.
    pub key: String,
    pub label: String,
    pub help: Option<String>,
    pub kind: FieldKind,
}

#[derive(Clone, Debug, PartialEq)]
pub enum FieldKind {
    Text {
        default: Option<String>,
    },
    Url {
        default: Option<String>,
    },
    Number {
        default: Option<f64>,
    },
    Boolean {
        default: bool,
    },
    Select {
        options: Vec<String>,
        default: Option<String>,
    },
    /// A credential: never shown, only kept, replaced or cleared.
    Secret,
}

impl BoardFields {
    pub fn new(
        section: Vec<String>,
        fields: Vec<FieldSpec>,
        notes: Vec<String>,
        permissions: Vec<PermissionSpec>,
    ) -> Self {
        Self {
            section,
            fields,
            notes,
            permissions,
            app_secrets: false,
        }
    }

    /// The app saves secrets of its own: see [`BoardFields::app_secrets`].
    // qual:api (an app's web Space boards set it)
    pub fn with_app_secrets(mut self) -> Self {
        self.app_secrets = true;
        self
    }
}

impl PermissionSpec {
    pub fn new(key: String, label: String) -> Self {
        Self { key, label }
    }
}

impl FieldSpec {
    pub fn new(key: String, label: String, help: Option<String>, kind: FieldKind) -> Self {
        Self {
            key,
            label,
            help,
            kind,
        }
    }

    /// The saved value of `text` typed into a text, URL or number field.
    /// `None` removes the value, so the default applies.
    pub fn parse_text(&self, text: &str) -> Result<Option<Value>, String> {
        let text = text.trim();
        if text.is_empty() {
            return Ok(None);
        }
        if text.chars().count() > MAX_FIELD_TEXT_CHARS {
            return Err(format!(
                "{} is longer than {MAX_FIELD_TEXT_CHARS} characters.",
                self.label
            ));
        }
        match &self.kind {
            FieldKind::Url { .. } => web_address(text)
                .map(|url| Some(Value::String(url)))
                .ok_or_else(|| format!("{} must be an http:// or https:// address.", self.label)),
            FieldKind::Number { .. } => number(text)
                .map(Some)
                .ok_or_else(|| format!("{} must be a number.", self.label)),
            _ => Ok(Some(Value::String(text.to_owned()))),
        }
    }

    /// The text a text, URL or number field shows for `saved`, else its
    /// default.
    pub fn text(&self, saved: Option<&Value>) -> String {
        match (saved, &self.kind) {
            (Some(Value::String(text)), _) => text.clone(),
            (Some(Value::Number(number)), _) => number.to_string(),
            (_, FieldKind::Text { default } | FieldKind::Url { default }) => {
                default.clone().unwrap_or_default()
            }
            (_, FieldKind::Number { default }) => {
                default.map(|number| number.to_string()).unwrap_or_default()
            }
            _ => String::new(),
        }
    }
}

fn web_address(text: &str) -> Option<String> {
    let url = url::Url::parse(text).ok()?;
    let web = matches!(url.scheme(), "https" | "http") && url.host().is_some();
    web.then(|| url.into())
}

fn number(text: &str) -> Option<Value> {
    let number = text
        .parse::<f64>()
        .ok()
        .filter(|number| number.is_finite())?;
    const MAX_EXACT_INTEGER: f64 = 9e15;
    let whole = number.fract() == 0.0 && number.abs() < MAX_EXACT_INTEGER;
    if whole {
        // A whole number is saved without a fraction, as a person typed it.
        return Some(Value::from(number as i64));
    }
    serde_json::Number::from_f64(number).map(Value::Number)
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::{FieldKind, FieldSpec};

    fn field(kind: FieldKind) -> FieldSpec {
        FieldSpec {
            key: "k".into(),
            label: "Limit".into(),
            help: None,
            kind,
        }
    }

    #[test]
    fn typed_text_becomes_a_saved_value_of_the_field_kind() {
        let number = field(FieldKind::Number { default: None });
        assert_eq!(number.parse_text(" 5 "), Ok(Some(json!(5))));
        assert_eq!(number.parse_text("2.5"), Ok(Some(json!(2.5))));
        assert!(number.parse_text("five").is_err());
        assert!(number.parse_text("NaN").is_err());
        let url = field(FieldKind::Url { default: None });
        assert_eq!(
            url.parse_text("https://api.example.com"),
            Ok(Some(json!("https://api.example.com/")))
        );
        assert!(url.parse_text("file:///etc").is_err());
        let text = field(FieldKind::Text { default: None });
        assert_eq!(text.parse_text("  "), Ok(None));
        assert!(text.parse_text(&"x".repeat(2001)).is_err());
    }

    #[test]
    fn fields_show_the_saved_value_or_their_default() {
        let number = field(FieldKind::Number { default: Some(3.0) });
        assert_eq!(number.text(None), "3");
        assert_eq!(number.text(Some(&json!(7))), "7");
        let url = field(FieldKind::Url {
            default: Some("https://a.example".into()),
        });
        assert_eq!(url.text(None), "https://a.example");
        assert_eq!(
            url.text(Some(&Value::String("https://b".into()))),
            "https://b"
        );
    }
}
