use std::fmt;

use serde::Deserialize;

/// A credential value. `Debug` and `Display` print `***`, so a secret cannot
/// reach a log, an error message or the UI by formatting. It has no
/// `Serialize`, so it cannot reach the state file either.
#[derive(Clone, PartialEq, Eq, Deserialize)]
#[serde(transparent)]
pub struct Secret(String);

impl Secret {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// The value, for the one place that sends it: an `Authorization` header.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("***")
    }
}

// qual:allow(dry, boilerplate) reason: "Credential Display deliberately writes only a fixed mask; the formatting test verifies redaction without exposing the stored value."
impl fmt::Display for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("***")
    }
}

#[cfg(test)]
mod tests {
    use super::Secret;

    #[test]
    fn formatting_hides_the_value() {
        let secret = Secret::new("ghp_value");
        assert_eq!(format!("{secret:?} {secret}"), "*** ***");
        assert_eq!(format!("{:?}", Some(secret.clone())), "Some(***)");
        assert_eq!(secret.expose(), "ghp_value");
    }
}
