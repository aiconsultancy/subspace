//! Exact known-value readback filtering, not a classifier for transformed secrets.

pub(crate) fn text(source: &str, values: &[&str]) -> String {
    let mut result = String::new();
    let mut remainder = source;
    while !remainder.is_empty() {
        let matched = values
            .iter()
            .filter(|value| !value.is_empty() && remainder.starts_with(**value))
            .max_by_key(|value| value.len());
        if let Some(value) = matched {
            result.push_str("[redacted]");
            remainder = &remainder[value.len()..];
        } else if let Some(character) = remainder.chars().next() {
            result.push(character);
            remainder = &remainder[character.len_utf8()..];
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::text;

    #[test]
    fn values_are_removed_without_reprocessing_the_replacement_marker() {
        let source = "α test-only-credential and test-only-credential tail";
        let result = text(source, &["test-only-credential", "redacted", "e", ""]);
        assert_eq!(result, "α [redacted] and [redacted] tail");
        assert!(text("safe", &[""]).contains("safe"));
    }

    #[test]
    fn overlapping_values_choose_the_longest_complete_match() {
        assert_eq!(text("ababa", &["aba", "ababa", "bab"]), "[redacted]");
        assert_eq!(
            text("not a private value", &["dummy-value"]),
            "not a private value"
        );
        assert_eq!(text("test-λ-test", &["λ"]), "test-[redacted]-test");
    }
}
