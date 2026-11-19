//! IRC presentation rules: stable nick tones, mIRC colours mapped to theme
//! tones (never raw colours), and formatted text as styled runs with links.

use std::ops::Range;

use subspace_client_irc::LineKind;
use subspace_client_irc::format::{self, Span};
use subspace_framework::dashboard::Tone;

use crate::markup::{Mentions, Ref};
use crate::state::Entry;

mod marks;

pub(crate) use marks::{with_focus, with_refs, with_unavailable};

/// Nick tones, picked by a stable hash so a nick keeps its colour.
const NICK_TONES: [Tone; 7] = [
    Tone::Red,
    Tone::Orange,
    Tone::Yellow,
    Tone::Green,
    Tone::Cyan,
    Tone::Blue,
    Tone::Magenta,
];
const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;
/// Trailing characters that end a sentence rather than a URL.
const URL_TRAILING: &[char] = &['.', ',', ';', ':', '!', '?', ')', ']', '\'', '"', '>'];

/// The tone of `nick`, the same for every case of it.
pub(crate) fn nick_tone(nick: &str) -> Tone {
    let hash = nick.to_lowercase().bytes().fold(FNV_OFFSET, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(FNV_PRIME)
    });
    let index = usize::try_from(hash % NICK_TONES.len() as u64).unwrap_or_default();
    NICK_TONES[index]
}

/// The theme tone nearest an mIRC colour. White and black follow the text
/// colour, so they stay readable in light and dark themes; the extended
/// colours 16–98 keep the default text colour.
pub(crate) fn mirc_tone(code: u8) -> Option<Tone> {
    Some(match code {
        0 => Tone::Strong,
        2 | 12 => Tone::Blue,
        3 | 9 => Tone::Green,
        4 => Tone::Red,
        5 | 7 => Tone::Orange,
        6 | 13 => Tone::Magenta,
        8 => Tone::Yellow,
        10 | 11 => Tone::Cyan,
        14 | 15 => Tone::Muted,
        _ => return None,
    })
}

/// The style of one run of text.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct RunStyle {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikethrough: bool,
    pub monospace: bool,
    pub tone: Option<Tone>,
    pub link: bool,
    /// A `#channel`, member or issue reference the board opens.
    pub reference: bool,
    /// An issue reference its tracker reports as unavailable.
    pub unavailable: bool,
    /// The link or reference that has keyboard focus.
    pub focused: bool,
}

/// Formatted text: the plain text, styled byte ranges, link targets and
/// references.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Styled {
    pub text: String,
    pub runs: Vec<(Range<usize>, RunStyle)>,
    pub links: Vec<(Range<usize>, String)>,
    pub refs: Vec<(Range<usize>, Ref)>,
}

/// `text` with its mIRC codes as runs, and `http(s)` URLs as links.
pub(crate) fn styled(text: &str) -> Styled {
    let mut styled = Styled::default();
    for span in format::parse(text) {
        let start = styled.text.len();
        styled.text.push_str(&span.text);
        let style = run_style(&span);
        if style != RunStyle::default() && !span.text.is_empty() {
            styled.runs.push((start..styled.text.len(), style));
        }
    }
    styled.links = links(&styled.text);
    styled
}

fn run_style(span: &Span) -> RunStyle {
    RunStyle {
        bold: span.bold,
        italic: span.italic,
        underline: span.underline,
        strikethrough: span.strikethrough,
        monospace: span.monospace,
        tone: span.fg.and_then(mirc_tone),
        link: false,
        reference: false,
        unavailable: false,
        focused: false,
    }
}

/// Every `http://` or `https://` URL in `text`, without trailing punctuation.
pub(crate) fn links(text: &str) -> Vec<(Range<usize>, String)> {
    let mut found = Vec::new();
    let mut offset = 0;
    for word in text.split_inclusive(char::is_whitespace) {
        let trimmed = word.trim_end();
        let url = trimmed.trim_end_matches(URL_TRAILING);
        let scheme = url.starts_with("https://") || url.starts_with("http://");
        if scheme && url.len() > "https://".len() {
            found.push((offset..offset + url.len(), url.to_owned()));
        }
        offset += word.len();
    }
    found
}

/// How one transcript line reads: its text with the sender, the tone of
/// unstyled text and whether the whole line is italic (`/me`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LineView {
    pub styled: Styled,
    pub base: Tone,
    pub italic: bool,
}

/// A sender's name tone: their stable tone, or the strong text tone for us.
pub(crate) fn sender_tone(nick: &str, own: bool) -> Tone {
    if own { Tone::Strong } else { nick_tone(nick) }
}

/// The sender's run on a compact line: bold. Its name tone is not drawn, as
/// a text view has no text colour for a run, and a tint would read as an
/// mIRC colour.
const SENDER: RunStyle = RunStyle {
    bold: true,
    italic: false,
    underline: false,
    strikethrough: false,
    monospace: false,
    tone: None,
    link: false,
    reference: false,
    unavailable: false,
    focused: false,
};

pub(crate) fn line_view(entry: &Entry) -> LineView {
    let line = &entry.line;
    let from = line.from.as_deref().unwrap_or_default();
    let body = styled(&line.text);
    let (prefix, base, italic) = match line.kind {
        LineKind::Privmsg => (vec![(from, Some(SENDER)), (" ", None)], Tone::Fg, false),
        LineKind::Action => (
            vec![("* ", None), (from, Some(SENDER)), (" ", None)],
            Tone::Fg,
            true,
        ),
        LineKind::Notice => (
            vec![("-", None), (from, Some(SENDER)), ("- ", None)],
            Tone::Muted,
            false,
        ),
        LineKind::Error => (Vec::new(), Tone::Red, false),
        LineKind::Info => (Vec::new(), Tone::Muted, false),
        _ if from.is_empty() => (Vec::new(), Tone::Muted, false),
        _ => (vec![(from, None), (" ", None)], Tone::Muted, false),
    };
    LineView {
        styled: prefixed(&prefix, body),
        base,
        italic,
    }
}

/// A message row's body under its sender's name: the text without the
/// sender, italic for `/me`, muted for a notice, with references marked.
pub(crate) fn message_view(entry: &Entry, mentions: &Mentions) -> LineView {
    let line = &entry.line;
    let (base, italic) = match line.kind {
        LineKind::Action => (Tone::Fg, true),
        LineKind::Notice => (Tone::Muted, false),
        _ => (Tone::Fg, false),
    };
    LineView {
        styled: with_refs(&line.text, mentions),
        base,
        italic,
    }
}

/// `body` after the plain or styled `prefix` parts, with its ranges moved.
fn prefixed(prefix: &[(&str, Option<RunStyle>)], body: Styled) -> Styled {
    let mut out = Styled::default();
    for (text, style) in prefix {
        let start = out.text.len();
        out.text.push_str(text);
        if let Some(style) = style.filter(|_| !text.is_empty()) {
            out.runs.push((start..out.text.len(), style));
        }
    }
    let offset = out.text.len();
    let shift = |range: &Range<usize>| range.start + offset..range.end + offset;
    out.text.push_str(&body.text);
    out.runs.extend(
        body.runs
            .iter()
            .map(|(range, style)| (shift(range), *style)),
    );
    out.links.extend(
        body.links
            .iter()
            .map(|(range, url)| (shift(range), url.clone())),
    );
    out.refs.extend(
        body.refs
            .iter()
            .map(|(range, found)| (shift(range), found.clone())),
    );
    out
}

#[cfg(test)]
mod tests;
