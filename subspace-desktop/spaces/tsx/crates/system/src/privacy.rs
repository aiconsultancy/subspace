//! Removes personal data from strings that leave the crate.

/// Values that must never appear in a reading: the host name and the
/// signed-in user's name.
#[derive(Clone, Debug, Default)]
pub(crate) struct Secrets {
    values: Vec<String>,
}

/// Shorter values match too much ordinary text to remove safely.
const MIN_SECRET_LEN: usize = 3;

/// The longest text a reading keeps.
const MAX_TEXT_CHARS: usize = 100;

/// This machine's host name, its first label and the user's name.
fn personal_values() -> Vec<String> {
    let mut values = Vec::new();
    if let Ok(host) = sysinfo::System::host_name() {
        // `name.local` and `name.example.com` also hold `name`.
        if let Some((short, _)) = host.split_once('.') {
            values.push(short.to_owned());
        }
        values.push(host);
    }
    for key in ["USER", "USERNAME", "LOGNAME"] {
        if let Ok(user) = std::env::var(key) {
            values.push(user);
        }
    }
    values
}

impl Secrets {
    /// The current machine's host name and user name.
    pub(crate) fn current() -> Self {
        Self::new(personal_values())
    }

    pub(crate) fn new(values: impl IntoIterator<Item = String>) -> Self {
        let mut values: Vec<String> = values
            .into_iter()
            .map(|value| value.trim().to_lowercase())
            .filter(|value| value.chars().count() >= MIN_SECRET_LEN)
            .collect();
        values.sort();
        values.dedup();
        Self { values }
    }

    /// `text` without control characters, or `None` when it holds a secret
    /// or a path.
    pub(crate) fn scrub(&self, text: &str) -> Option<String> {
        let cleaned: String = text
            .chars()
            .filter(|c| !c.is_control())
            .take(MAX_TEXT_CHARS)
            .collect::<String>()
            .trim()
            .to_owned();
        let lower = cleaned.to_lowercase();
        let leaks = self
            .values
            .iter()
            .any(|secret| lower.contains(secret.as_str()));
        let path = cleaned.contains('/') || cleaned.contains('\\');
        (!cleaned.is_empty() && !leaks && !path).then_some(cleaned)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secrets_and_paths_are_removed() {
        let secrets = Secrets::new(["alice".into(), "build-host.example.com".into(), "x".into()]);
        assert_eq!(secrets.scrub("Alice's Mac"), None);
        assert_eq!(secrets.scrub("Darwin build-host.example.com 25.0"), None);
        assert_eq!(secrets.scrub("/Users/bob/app"), None);
        assert_eq!(secrets.scrub("C:\\Users\\bob"), None);
        assert_eq!(secrets.scrub("x86_64"), Some("x86_64".into()));
        assert_eq!(secrets.scrub(" macOS\n15 "), Some("macOS15".into()));
        assert_eq!(secrets.scrub(""), None);
    }
}
