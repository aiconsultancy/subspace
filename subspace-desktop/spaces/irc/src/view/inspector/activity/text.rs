//! Untrusted text as the turn view draws it: whole, with its newlines and
//! tabs, and every other control or bidirectional formatting character
//! shown as `⟨U+XXXX⟩`, so nothing a tool printed can hide or reorder
//! what is drawn beside it.
//!
//! This duplicates the helper of the crew detail (#113) on purpose, until
//! one of the two lands and the other moves onto it.

/// The explicit bidirectional formatting characters: marks, embeddings,
/// overrides and isolates.
const BIDI: [char; 12] = [
    '\u{061C}', '\u{200E}', '\u{200F}', '\u{202A}', '\u{202B}', '\u{202C}', '\u{202D}', '\u{202E}',
    '\u{2066}', '\u{2067}', '\u{2068}', '\u{2069}',
];

/// `text` with each control or bidirectional formatting character, other
/// than a newline or a tab, shown as its code point.
pub(super) fn visible(text: &str) -> String {
    let mut shown = String::with_capacity(text.len());
    for ch in text.chars() {
        if matches!(ch, '\n' | '\t') || !(ch.is_control() || BIDI.contains(&ch)) {
            shown.push(ch);
        } else {
            shown.push_str(&format!("⟨U+{:04X}⟩", u32::from(ch)));
        }
    }
    shown
}

#[cfg(test)]
mod tests {
    use super::visible;

    #[test]
    fn newlines_and_tabs_stay_and_other_controls_show_their_code_point() {
        assert_eq!(visible("a\n\tb"), "a\n\tb");
        assert_eq!(visible("ok\r\n"), "ok⟨U+000D⟩\n");
        assert_eq!(visible("\u{1b}[31mred"), "⟨U+001B⟩[31mred");
        assert_eq!(visible("abc\u{202E}fed"), "abc⟨U+202E⟩fed");
        assert_eq!(visible("\u{2066}x\u{2069}"), "⟨U+2066⟩x⟨U+2069⟩");
        assert_eq!(visible("\u{0}\u{85}"), "⟨U+0000⟩⟨U+0085⟩");
        assert_eq!(visible("emoji 👩‍🚀 stays"), "emoji 👩‍🚀 stays");
    }
}
