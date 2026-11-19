//! Plain issue identifiers recognised against the responder's project keys.
//! With no scope nothing is recognised. URLs remain ordinary links.

use subspace_framework::contracts::issues::IssueScope;
use subspace_framework::contracts::issues::recognition::ReferenceRecognizer;

/// Project keys only, never issue content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IssueKeys {
    references: ReferenceRecognizer,
}

impl Default for IssueKeys {
    fn default() -> Self {
        Self {
            references: ReferenceRecognizer::new([], []),
        }
    }
}

impl IssueKeys {
    pub fn new(scope: &IssueScope) -> Self {
        Self {
            references: ReferenceRecognizer::new(scope.project_keys.iter().map(String::as_str), []),
        }
    }

    /// The length of the issue identifier `word` starts with, which may be
    /// followed by more of the word, such as `SUB-827/`.
    pub fn leading(&self, word: &str) -> Option<usize> {
        self.references
            .find(word)
            .first()
            .map(|found| found.range())
            .filter(|range| range.start == 0)
            .map(|range| range.end)
    }
}
