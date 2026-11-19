//! The API counts trimmed single-line summaries in UTF-16 code units.
use super::SUMMARY_LIMIT;

pub fn summary_length(text: &str) -> usize {
    text.trim().encode_utf16().count()
}

pub fn valid_summary(text: &str) -> bool {
    let text = text.trim();
    !text.is_empty()
        && summary_length(text) <= SUMMARY_LIMIT
        && !text.chars().any(char::is_control)
        && !text.contains(['\u{2028}', '\u{2029}'])
}
