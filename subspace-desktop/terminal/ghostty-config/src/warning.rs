//! The warning type returned alongside a resolved configuration or theme.

use std::path::PathBuf;

/// A problem found while loading a configuration or theme file.
///
/// `line` is 1-indexed to match the file as a human reads it. A warning that
/// is not tied to a specific line (for example a missing theme) uses line 0.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Warning {
    pub file: PathBuf,
    pub line: usize,
    pub reason: String,
}

impl Warning {
    pub fn new(file: impl Into<PathBuf>, line: usize, reason: impl Into<String>) -> Self {
        Self {
            file: file.into(),
            line,
            reason: reason.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Warning;
    use std::path::PathBuf;

    #[test]
    fn new_stores_the_given_fields() {
        let warning = Warning::new("/tmp/config", 7, "bad value");
        assert_eq!(warning.file, PathBuf::from("/tmp/config"));
        assert_eq!(warning.line, 7);
        assert_eq!(warning.reason, "bad value");
    }
}
