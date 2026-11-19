//! Registration requirements for the shell-owned private service editor.
//! These identify existing shared drafts, not board-owned config values.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub enum ConnectionToken {
    // Stable positions in the shell's shared service editor, including control IDs.
    GitHub = 0,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnectionField {
    Token(ConnectionToken),
    /// The shared account picker, token and their existing help text.
    GitHubCredentials,
    Repositories,
    Note(&'static str),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ConnectionSettings {
    fields: &'static [ConnectionField],
}

impl ConnectionSettings {
    pub const fn new(fields: &'static [ConnectionField]) -> Self {
        Self { fields }
    }

    pub const fn fields(&self) -> &'static [ConnectionField] {
        self.fields
    }
}

#[cfg(test)]
mod tests {
    use super::{ConnectionField, ConnectionSettings, ConnectionToken};

    #[test]
    fn fields_are_explicit_and_default_to_no_requirements() {
        assert!(ConnectionSettings::default().fields().is_empty());
        let settings = ConnectionSettings::new(&[ConnectionField::Token(ConnectionToken::GitHub)]);
        assert_eq!(
            settings.fields(),
            &[ConnectionField::Token(ConnectionToken::GitHub)]
        );
    }
}
