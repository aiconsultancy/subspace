//! Bare issue identifiers, such as `SUB-827`, in plain text.

use std::{collections::HashSet, ops::Range};

const MIN_PROJECT_KEY_LENGTH: usize = 2;
const MAX_PROJECT_KEY_LENGTH: usize = 10;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssueReference {
    identifier: String,
    range: Range<usize>,
}

impl IssueReference {
    pub fn identifier(&self) -> &str {
        &self.identifier
    }
    pub fn range(&self) -> Range<usize> {
        self.range.clone()
    }
}

/// The project keys to recognise. Holds keys only, never issue content.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReferenceRecognizer {
    keys: HashSet<String>,
}

impl Default for ReferenceRecognizer {
    fn default() -> Self {
        Self::new([], ["NET"])
    }
}

impl ReferenceRecognizer {
    /// Loaded keys take precedence over the fallback. Products pass no fallback;
    /// `Default` keeps `NET` only as a test convenience.
    pub fn new<'a>(
        loaded: impl IntoIterator<Item = &'a str>,
        fallback: impl IntoIterator<Item = &'a str>,
    ) -> Self {
        let keys = valid_keys(loaded);
        Self {
            keys: if keys.is_empty() {
                valid_keys(fallback)
            } else {
                keys
            },
        }
    }

    pub fn find(&self, text: &str) -> Vec<IssueReference> {
        let protected = protected_ranges(text);
        text.match_indices('-')
            .filter_map(|(dash, _)| candidate(text, dash))
            .filter(|range| {
                !protected
                    .iter()
                    .any(|p| range.start < p.end && p.start < range.end)
            })
            .filter(|range| self.recognises(&text[range.clone()]))
            .map(|range| IssueReference {
                identifier: text[range.clone()].into(),
                range,
            })
            .collect()
    }

    /// Checks a complete identifier against this recogniser's selected keys.
    pub fn recognises(&self, identifier: &str) -> bool {
        valid_identifier(identifier)
            && self
                .keys
                .contains(identifier.split('-').next().unwrap_or_default())
    }
}

fn valid_keys<'a>(keys: impl IntoIterator<Item = &'a str>) -> HashSet<String> {
    keys.into_iter()
        .filter(|key| valid_key(key))
        .map(str::to_owned)
        .collect()
}

fn valid_key(key: &str) -> bool {
    // Subspace project key rule: ^[A-Z][A-Z0-9]{1,9}$.
    (MIN_PROJECT_KEY_LENGTH..=MAX_PROJECT_KEY_LENGTH).contains(&key.len())
        && key.as_bytes().first().is_some_and(u8::is_ascii_uppercase)
        && key
            .bytes()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
}

/// The identifier grammar shared by plain references.
pub(super) fn valid_identifier(identifier: &str) -> bool {
    let Some((key, number)) = identifier.split_once('-') else {
        return false;
    };
    valid_key(key)
        && number.as_bytes().first().is_some_and(|digit| *digit != b'0')
        && number.bytes().all(|digit| digit.is_ascii_digit())
        // Common technical names stay plain even if a project shares their prefix.
        && !matches!(identifier, "UTF-8" | "ISO-8601")
}

fn candidate(text: &str, dash: usize) -> Option<Range<usize>> {
    let start = text[..dash]
        .char_indices()
        .rev()
        .find(|(_, c)| !c.is_alphanumeric() && *c != '_')
        .map_or(0, |(i, c)| i + c.len_utf8());
    let number_start = dash + 1;
    let digits = text[number_start..]
        .bytes()
        .take_while(u8::is_ascii_digit)
        .count();
    let end = number_start + digits;
    let invalid_end = text[end..]
        .chars()
        .next()
        .is_some_and(|c| c.is_alphanumeric() || c == '_');
    let invalid_start = text[..start]
        .chars()
        .next_back()
        .is_some_and(|c| c == '-' || c.is_alphanumeric());
    let date_suffix = text[end..]
        .strip_prefix('-')
        .is_some_and(|suffix| suffix.as_bytes().first().is_some_and(u8::is_ascii_digit));
    if invalid_start || invalid_end || date_suffix {
        return None;
    }
    Some(start..end)
}

fn protected_ranges(text: &str) -> Vec<Range<usize>> {
    let mut offset = 0;
    let mut ranges: Vec<_> = text
        .split_inclusive(char::is_whitespace)
        .filter_map(|part| {
            let start = offset;
            offset += part.len();
            (part.contains("://")
                || part.contains("www.")
                || part.contains("mailto:")
                || part.contains("/issues/"))
            .then_some(start..offset)
        })
        .collect();
    ranges.extend(code_ranges(text));
    ranges
}

fn code_ranges(text: &str) -> Vec<Range<usize>> {
    let runs = backtick_runs(text);
    let mut ranges = Vec::new();
    let mut index = 0;
    while let Some(open) = runs.get(index) {
        if let Some(offset) = runs[index + 1..]
            .iter()
            .position(|run| run.len() == open.len())
        {
            let close = index + 1 + offset;
            ranges.push(open.start..runs[close].end);
            index = close + 1;
        } else {
            index += 1;
        }
    }
    ranges
}

fn backtick_runs(text: &str) -> Vec<Range<usize>> {
    let mut runs = Vec::new();
    let mut ticks = text.match_indices('`').peekable();
    while let Some((start, _)) = ticks.next() {
        let mut end = start + 1;
        while ticks.peek().is_some_and(|(i, _)| *i == end) {
            ticks.next();
            end += 1;
        }
        runs.push(start..end);
    }
    runs
}
