use std::{fmt, sync::LazyLock};

use regex::Regex;

use super::home::Home;

const MASK: &str = "[redacted]";
const MAX_FIELD_CHARS: usize = 4_096;
const MAX_SHORT_ENVIRONMENT_CHARS: usize = 3;
const PATTERNS: [&str; 5] = [
    r#"(?i)\b[a-z][a-z0-9+.-]*://[^\s<>\"'`]+|\bwww\.[^\s<>\"'`]+|//[^\s<>\"'`]*@[^\s<>\"'`]+"#,
    r#"\b[A-Za-z_][A-Za-z0-9_]*\s*=\s*(?:\"(?:\\.|[^\"])*\"|'(?:\\.|[^'])*'|[^\s,;]+)"#,
    r##"(?i)\b[a-z0-9_-]*(?:token|secret|password|passwd|pwd|pat|api[_-]?key|apikey|private[_-]?key|signature)["']?\s*[:=]\s*(?:"(?:\\.|[^"])*"|'(?:\\.|[^'])*')"##,
    r"(?s)-----BEGIN [A-Z ]*PRIVATE KEY-----.*?(?:-----END [A-Z ]*PRIVATE KEY-----|$)",
    r#"(?:/(?:[^\s:\"'`;]+):)+/[^\s\"'`;]+|(?:[A-Za-z]:[\\/][^;\r\n\"'`]+;)+[A-Za-z]:[\\/][^;\r\n\"'`]+"#,
];

static RULES: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    PATTERNS
        .iter()
        .map(|pattern| Regex::new(pattern).expect("constant diagnostics pattern"))
        .collect()
});

/// Private inputs used only to remove values, never serialized or printed.
/// The shell supplies environment values without collecting them on copy.
/// Text removes literal values, while HOME becomes `~`. Registered identities
/// and the named executable, shell and Azure CLI paths remain explicit
/// exceptions, so common environment values cannot corrupt versions or paths.
#[derive(Clone, Default)]
pub struct RedactionContext {
    home: Option<Home>,
    environment_values: Vec<String>,
}

impl fmt::Debug for RedactionContext {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("RedactionContext { private inputs omitted }")
    }
}

impl RedactionContext {
    /// Empty values are ignored; longer values are removed before their parts.
    pub fn new(home: Option<String>, environment_values: impl IntoIterator<Item = String>) -> Self {
        let home = home.and_then(Home::new);
        let mut environment_values: Vec<_> = environment_values
            .into_iter()
            .filter(|value| {
                !value.is_empty() && !home.as_ref().is_some_and(|home| home.matches(value))
            })
            .map(|value| match &home {
                Some(home) => home.replace(&value),
                None => value,
            })
            .collect();
        normalize_values(&mut environment_values);
        Self {
            home,
            environment_values,
        }
    }

    pub(super) fn text(&self, text: &str) -> String {
        let text = self
            .environment_values
            .iter()
            // Common flags and nesting levels are not secret identities. Do not
            // erase HTTP codes or counts just because the environment has `1`.
            .filter(|value| {
                !(value.len() <= MAX_SHORT_ENVIRONMENT_CHARS
                    && value.bytes().all(|byte| byte.is_ascii_digit()))
            })
            .fold(self.home_text(text), |text, value| {
                text.replace(value, MASK)
            });
        bounded(&scrub(&text))
    }

    pub(super) fn error_text(&self, text: &str, private_values: &[String]) -> String {
        let text = private_values
            .iter()
            .map(|value| self.home_text(value))
            .filter(|value| value != "~")
            .fold(self.home_text(text), |text, value| {
                mask_identity(&text, &value)
            });
        self.text(&text)
    }

    /// Registered identities are not arbitrary environment text. In particular,
    /// an environment value of `1` must not erase version numbers.
    pub(super) fn public_field(&self, text: &str) -> String {
        bounded(&scrub(&self.home_text(text)))
    }

    /// Named paths permit short environment values and absolute path values,
    /// but opaque values of four or more characters are still private. This
    /// preserves `/bin/zsh` and version segments when the environment has `1`.
    pub(super) fn path_field(&self, text: &str) -> String {
        let text = self.home_text(text);
        if path_list(&text) {
            return MASK.into();
        }
        let text = self
            .environment_values
            .iter()
            .filter(|value| {
                value.chars().count() > MAX_SHORT_ENVIRONMENT_CHARS && !absolute_path(value)
            })
            .fold(text, |text, value| text.replace(value, MASK));
        bounded(&scrub(&text))
    }

    /// `Source::describe()` names the shell path explicitly. Other descriptions
    /// are untrusted prose, including inherited-PATH failure reasons.
    pub(super) fn source(&self, text: &str) -> String {
        match text.strip_prefix("login shell ") {
            Some(path) if absolute_path(path) => {
                format!("login shell {}", self.path_field(path))
            }
            _ => self.text(text),
        }
    }

    fn home_text(&self, text: &str) -> String {
        self.home
            .as_ref()
            .map_or_else(|| text.into(), |home| home.replace(text))
    }
}

fn absolute_path(value: &str) -> bool {
    value.starts_with('/')
        || value.starts_with("~/")
        || value.starts_with("~\\")
        || windows_path(value)
}

fn path_list(text: &str) -> bool {
    text.contains(';') || (text.contains(':') && !windows_path(text))
}

fn windows_path(value: &str) -> bool {
    value
        .as_bytes()
        .first()
        .is_some_and(u8::is_ascii_alphabetic)
        && value.as_bytes().get(1) == Some(&b':')
        && matches!(value.as_bytes().get(2), Some(b'/' | b'\\'))
}

fn scrub(text: &str) -> String {
    let text = RULES.iter().fold(text.to_owned(), |text, pattern| {
        pattern.replace_all(&text, MASK).into_owned()
    });
    crate::log::redact(&text)
}

pub(super) fn normalize_values(values: &mut Vec<String>) {
    values.retain(|value| !value.is_empty());
    values.sort_by(|left, right| right.len().cmp(&left.len()).then_with(|| left.cmp(right)));
    values.dedup();
}

fn mask_identity(text: &str, value: &str) -> String {
    let mut result = String::new();
    let mut cursor = 0;
    for (start, found) in text.match_indices(value) {
        let end = start + found.len();
        let left = text[..start].chars().next_back();
        let right = text[end..].chars().next();
        if left.is_some_and(identity_character) || right.is_some_and(identity_character) {
            continue;
        }
        result.push_str(&text[cursor..start]);
        result.push_str(MASK);
        cursor = end;
    }
    result.push_str(&text[cursor..]);
    result
}

fn identity_character(value: char) -> bool {
    value.is_alphanumeric() || "_-".contains(value)
}

fn bounded(text: &str) -> String {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.chars().count() <= MAX_FIELD_CHARS {
        return text;
    }
    let prefix: String = text.chars().take(MAX_FIELD_CHARS).collect();
    format!("{prefix} [truncated]")
}

#[cfg(test)]
mod tests {
    use super::RedactionContext;

    #[test]
    fn removes_urls_assignments_and_planted_values_without_losing_friendly_text() {
        let context = RedactionContext::new(None, ["test-only-environment-value".into()]);
        let input = concat!(
            "Azure CLI was not found. Try again after installing it. ",
            "https://test-user:test-proxy-password@proxy.invalid:8080/path ",
            "HTTP_PROXY = 'test spaced proxy value' CUSTOM_ENV=\"another test value\" ",
            "test-only-environment-value"
        );
        let result = context.text(input);
        assert!(result.starts_with("Azure CLI was not found. Try again after installing it."));
        for forbidden in [
            "proxy.invalid",
            "test-proxy-password",
            "test spaced proxy value",
            "another test value",
            "test-only-environment-value",
        ] {
            assert!(!result.contains(forbidden), "{result}");
        }
        assert!(!format!("{context:?}").contains("test-only-environment-value"));
    }
}
