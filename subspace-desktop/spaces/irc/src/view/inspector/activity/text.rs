//! Untrusted text as the turn view draws it: whole, with its newlines and
//! tabs, and every other control character, bidirectional formatting
//! character, line or paragraph separator and byte order mark shown as
//! `⟨U+XXXX⟩`, so nothing a tool printed can hide, reorder or break up
//! what is drawn beside it. The set is the Fleet Console's (`HIDDEN` in
//! `web-spaces/fleet-console/src/activity-view.js`), so both clients show
//! the same text the same way.
//!
//! This duplicates the helper of the crew detail (#113) on purpose, until
//! one of the two lands and the other moves onto it.

/// The explicit bidirectional formatting characters: marks, embeddings,
/// overrides and isolates.
const BIDI: [char; 12] = [
    '\u{061C}', '\u{200E}', '\u{200F}', '\u{202A}', '\u{202B}', '\u{202C}', '\u{202D}', '\u{202E}',
    '\u{2066}', '\u{2067}', '\u{2068}', '\u{2069}',
];

/// Characters drawn as their code point although they are not controls:
/// the line and paragraph separators, which break a line where none shows,
/// and the byte order mark, which draws nothing.
const INVISIBLE: [char; 3] = ['\u{2028}', '\u{2029}', '\u{FEFF}'];

/// What a marker draws besides its hex digits: `⟨U+⟩`.
const MARKER: &str = "⟨U+⟩";
/// The bits one hex digit shows.
const HEX_BITS: u32 = 4;
/// The fewest hex digits a marker shows, as `{:04X}` writes them.
const MIN_DIGITS: u32 = 4;

/// Whether `ch` is drawn as its code point: a control, bidirectional
/// formatting or [`INVISIBLE`] character, other than a newline or a tab.
fn marked(ch: char) -> bool {
    !matches!(ch, '\n' | '\t') && (ch.is_control() || BIDI.contains(&ch) || INVISIBLE.contains(&ch))
}

/// The UTF-8 bytes `ch` draws as: its own, or those of its `⟨U+XXXX⟩`
/// marker, those of `⟨U+⟩` plus one per hex digit (at least four).
pub(super) fn unit_len(ch: char) -> usize {
    if !marked(ch) {
        return ch.len_utf8();
    }
    let digits = (u32::BITS - u32::from(ch).leading_zeros())
        .div_ceil(HEX_BITS)
        .max(MIN_DIGITS);
    MARKER.len() + digits as usize
}

/// The UTF-8 bytes `text` draws as ([`visible`]), counted without
/// writing it: what drawing it mounts.
pub(super) fn visible_len(text: &str) -> usize {
    text.chars().map(unit_len).sum()
}

/// Appends `ch` to `shown` as it draws.
pub(super) fn push_visible(shown: &mut String, ch: char) {
    if marked(ch) {
        shown.push_str(&format!("⟨U+{:04X}⟩", u32::from(ch)));
    } else {
        shown.push(ch);
    }
}

/// `text` with each control, bidirectional formatting or [`INVISIBLE`]
/// character, other than a newline or a tab, shown as its code point.
pub(super) fn visible(text: &str) -> String {
    let mut shown = String::with_capacity(text.len());
    for ch in text.chars() {
        push_visible(&mut shown, ch);
    }
    shown
}

#[cfg(test)]
mod tests {
    use super::{visible, visible_len};

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

    /// The separators and the byte order mark show, as in the Fleet
    /// Console; joiners, soft hyphens and word joiners stay as they are,
    /// as there.
    #[test]
    fn separators_and_the_byte_order_mark_show_their_code_point() {
        assert_eq!(visible("a\u{2028}b"), "a⟨U+2028⟩b");
        assert_eq!(visible("a\u{2029}b"), "a⟨U+2029⟩b");
        assert_eq!(visible("\u{FEFF}a"), "⟨U+FEFF⟩a");
        assert_eq!(
            visible("a\u{200B}\u{200D}\u{2060}\u{00AD}b"),
            "a\u{200B}\u{200D}\u{2060}\u{00AD}b"
        );
    }

    /// Round 4 review (P1): what is charged is what is drawn, the markers'
    /// bytes too, so a text of controls counts twelve bytes per byte.
    #[test]
    fn the_visible_length_counts_each_marker_in_full() {
        for text in [
            "plain",
            "\u{0}\u{1b}\u{85}",
            "a\u{202E}b\u{2028}\u{FEFF}",
            "ネコ🐈\r\n\t",
            "\u{E0001}\u{10FFFF}",
        ] {
            assert_eq!(visible_len(text), visible(text).len(), "{text:?}");
        }
        assert_eq!(visible_len(&"\u{0}".repeat(1000)), 12_000);
    }
}
