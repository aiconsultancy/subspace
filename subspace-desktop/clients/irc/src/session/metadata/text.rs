//! Plain, bounded status text at the protocol boundary.

const LIMIT: usize = 80;

pub(super) fn clean(text: &str) -> Option<String> {
    let stripped = crate::format::strip(text);
    let line = single_line(&stripped);
    bounded(&line)
}

fn bounded(line: &str) -> Option<String> {
    let line = line.trim();
    let mut chars = line.chars();
    let mut bounded: String = chars.by_ref().take(LIMIT).collect();
    if chars.next().is_some() {
        bounded.pop();
        bounded.push('…');
    }
    (!bounded.is_empty()).then_some(bounded)
}

fn single_line(text: &str) -> String {
    text.chars().filter_map(|c| match c {
        '\r' | '\n' | '\t' | '\u{85}' | '\u{2028}' | '\u{2029}' => Some(' '),
        c if c.is_control() || matches!(c, '\u{200b}' | '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2060}'..='\u{206f}' | '\u{feff}') => None,
        c => Some(c),
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::clean;

    #[test]
    fn formatting_and_colour_arguments_are_removed() {
        assert_eq!(
            clean(
                " \u{2}bold\u{f} \u{3}04,02red\u{3} \u{4}FF0000,000000hex\u{f} \u{1d}italic\u{1f}\u{1e}\u{11}\u{16} "
            ),
            Some("bold red hex italic".into())
        );
    }

    #[test]
    fn controls_and_line_separators_cannot_escape_one_line() {
        assert_eq!(
            clean(" \0a\u{7}\u{1b}\u{7f}\u{80}b\r\nc\td\u{2028}e\u{2029}f\u{85}g\u{202e}h "),
            Some("ab  c d e f gh".into())
        );
    }

    #[test]
    fn markdown_links_mentions_and_issue_references_stay_literal() {
        let text = "**Thinking** [link](https://example.test) @person #channel TST-1638";
        assert_eq!(clean(text).as_deref(), Some(text));
    }

    #[test]
    fn limit_counts_unicode_characters_including_the_ellipsis() {
        assert_eq!(clean(&"é".repeat(80)), Some("é".repeat(80)));
        assert_eq!(
            clean(&"🦀".repeat(81)),
            Some(format!("{}…", "🦀".repeat(79)))
        );
        assert_eq!(
            clean(&format!(" {} ", "a".repeat(80))),
            Some("a".repeat(80))
        );
    }

    #[test]
    fn empty_after_cleaning_means_cleared() {
        for text in [
            "",
            " \r\n\t ",
            "\u{2}\u{3}04,02\u{f}\0\u{1b}",
            "\u{202e}\u{200b}",
        ] {
            assert_eq!(clean(text), None);
        }
    }
}
