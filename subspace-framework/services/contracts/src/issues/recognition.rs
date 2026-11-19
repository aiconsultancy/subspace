//! Recognising plain issue identifiers in text against the project keys in
//! [`IssueScope::project_keys`](super::IssueScope). Pure text matching: no
//! reads and no issue content. URLs are ordinary links, never issue references.

mod reference;

pub use reference::{IssueReference, ReferenceRecognizer};
