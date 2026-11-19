//! Slack's formatting markers in one line: `*bold*`, `_italic_` and
//! `~strike~`, with Markdown's `**bold**` and `~~strike~~`. A marker formats
//! only when it opens at the start of a word and pairs with a marker of the
//! same kind that closes at the end of one, on the same line, so
//! `snake_case_names`, `2*3*4` and `src/*.rs` stay literal. Code spans, links
//! and URLs are read as units first: nothing inside them pairs.

use std::ops::Range;

use super::spans::{self, MARKS};

/// What may come just before an opening marker, besides a space or the
/// start of the line: an opening bracket or quote, or another marker.
const BEFORE_OPEN: &[char] = &['(', '[', '{', '"', '\'', '“', '‘', '*', '_', '~'];

/// What a pair of markers does to the text between them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Style {
    Bold,
    Italic,
    Strike,
}

impl Style {
    /// The style of a run of `len` `mark` characters: one or two `*` are
    /// bold, one `_` is italic, one or two `~` strike through. Other runs,
    /// such as `__` or `***`, format nothing.
    fn of(mark: char, len: usize) -> Option<Self> {
        match (mark, len) {
            ('*', 1 | 2) => Some(Self::Bold),
            ('_', 1) => Some(Self::Italic),
            ('~', 1 | 2) => Some(Self::Strike),
            _ => None,
        }
    }

    /// The marker the rewritten Markdown uses for this style.
    // qual:allow(dry, repeated_matches) reason: "This enum has one mapping of its own; rustqual groups unrelated enums only because their three-arm variant matches share one shape."
    pub(super) fn markdown(self) -> &'static str {
        match self {
            Self::Bold => "**",
            Self::Italic => "_",
            Self::Strike => "~~",
        }
    }
}

/// A formatting marker: where it is in the line, what it formats and
/// whether it opens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Marker {
    pub range: Range<usize>,
    pub style: Style,
    pub opens: bool,
}

/// A run of one marker character that may open or close.
#[derive(Debug, Clone)]
struct Run {
    range: Range<usize>,
    mark: char,
    style: Style,
    opens: bool,
    closes: bool,
}

/// The formatting markers of `line`, in order. A run that can close pairs
/// with the nearest open run of the same character and length; runs opened
/// after that one stay literal, as in CommonMark.
pub(super) fn markers(line: &str) -> Vec<Marker> {
    let mut open: Vec<Run> = Vec::new();
    let mut found = Vec::new();
    for run in runs(line) {
        let partner = open.iter().rposition(|candidate| {
            run.closes && candidate.mark == run.mark && candidate.range.len() == run.range.len()
        });
        if let Some(at) = partner {
            let closing = Marker {
                range: run.range,
                style: run.style,
                opens: false,
            };
            let opening = Marker {
                range: open.remove(at).range,
                opens: true,
                ..closing.clone()
            };
            open.truncate(at);
            found.extend([opening, closing]);
        } else if run.opens {
            open.push(run);
        }
    }
    found.sort_by_key(|marker| marker.range.start);
    found
}

/// Every run of a marker character in `line` outside code spans, links and
/// URLs.
fn runs(line: &str) -> Vec<Run> {
    let mut runs = Vec::new();
    let mut at = 0;
    while let Some(c) = line[at..].chars().next() {
        let rest = &line[at..];
        let len = || rest.len() - rest.trim_start_matches(c).len();
        at += match c {
            '`' => spans::code_span(rest).unwrap_or_else(len),
            '[' => spans::link(rest).map_or(c.len_utf8(), |link| link.len),
            'h' => spans::url(line, at).unwrap_or(c.len_utf8()),
            _ if MARKS.contains(&c) => {
                let len = len();
                runs.extend(run(line, at..at + len, c));
                len
            }
            _ => c.len_utf8(),
        };
    }
    runs
}

/// The run of `mark` over `range` in `line`, if its length styles text and
/// it is not enclosed by punctuation, as in `(*.rs)` or `"*"`. It can open
/// before text, after a space, the line's start, an opening bracket or
/// quote or another marker; it can close after text, before a space,
/// punctuation or the line's end.
fn run(line: &str, range: Range<usize>, mark: char) -> Option<Run> {
    let style = Style::of(mark, range.len())?;
    let before = line[..range.start].chars().next_back();
    let after = line[range.end..].chars().next();
    let punctuation = |c: char| !c.is_alphanumeric() && !c.is_whitespace() && !MARKS.contains(&c);
    if before.is_some_and(punctuation) && after.is_some_and(punctuation) {
        return None;
    }
    Some(Run {
        opens: after.is_some_and(|c| !c.is_whitespace())
            && before.is_none_or(|c| c.is_whitespace() || BEFORE_OPEN.contains(&c)),
        closes: before.is_some_and(|c| !c.is_whitespace())
            && after.is_none_or(|c| !c.is_alphanumeric()),
        range,
        mark,
        style,
    })
}
