//! Text that keeps the IRC presentation, plain or with mIRC codes, as
//! Markdown the shared rich text shows exactly as sent, so every transcript
//! row is one selectable text view. Every character Markdown
//! could read as syntax is escaped; spaces and tabs at a line's start or
//! end become character references, which Markdown neither reads as
//! indentation nor drops. mIRC bold, italic, underline and strikethrough
//! become the inline tags GPUI Kit's `TextView` reads, monospace a code
//! span, URLs links the link policy opens, and references links the board
//! opens. A `TextView` has no text colour for a run, so an mIRC colour
//! becomes a tint of its theme tone behind the text (`<mark>`), as the
//! caller's `tint` gives it.

use std::ops::Range;

use subspace_framework::dashboard::Tone;

use self::shown::{Edges, shown, tagged};
use super::super::link::ref_url;
use super::spans::destination;
use crate::style::{RunStyle, Styled};

/// The `<mark>` colour, as CSS hex, that stands for a tone; `None` leaves
/// the text untinted.
pub(crate) type Tint<'a> = &'a dyn Fn(Tone) -> Option<String>;

/// A hard line break: Markdown keeps the line break after a backslash.
const HARD_BREAK: &str = "\\\n";
/// What ends a line in Markdown: `\n`, `\r\n` or a lone `\r`.
const BREAKS: [char; 2] = ['\n', '\r'];

/// `styled` as Markdown that shows its text as sent, with its formatting,
/// links, references and colours as tints. Each line ending, a lone `\r`
/// included, is a hard break. Trailing ones are left out: Markdown would
/// show a hard break at the very end as a backslash.
pub(crate) fn literal(styled: &Styled, tint: Tint<'_>) -> String {
    let len = styled.text.trim_end_matches(BREAKS).len();
    let looks = pieces(styled, len)
        .into_iter()
        .map(|range| (look(styled, &range, tint), range))
        .collect();
    merged(styled, looks)
        .into_iter()
        .map(|(look, range)| piece(styled, range, look))
        .collect()
}

/// What decides how a piece is written, beyond its text: its formatting
/// without the tone, its tint and its link.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Look {
    style: RunStyle,
    tint: Option<String>,
    target: Option<String>,
}

fn look(styled: &Styled, range: &Range<usize>, tint: Tint<'_>) -> Look {
    let style = covering(&styled.runs, range).copied().unwrap_or_default();
    Look {
        style: RunStyle {
            tone: None,
            ..style
        },
        tint: style.tone.and_then(tint),
        target: target(styled, range),
    }
}

/// Neighbouring pieces written the same way as one, so two code spans never
/// touch: their backticks would run together into one delimiter.
fn merged(styled: &Styled, pieces: Vec<(Look, Range<usize>)>) -> Vec<(Look, Range<usize>)> {
    let mut out: Vec<(Look, Range<usize>)> = Vec::with_capacity(pieces.len());
    for (look, range) in pieces {
        let breaks = |range: &Range<usize>| is_break(&styled.text[range.clone()]);
        match out.last_mut() {
            Some((last, previous)) if *last == look && !breaks(previous) && !breaks(&range) => {
                previous.end = range.end;
            }
            _ => out.push((look, range)),
        }
    }
    out
}

/// The ranges of the first `len` bytes of `styled`'s text that each have
/// one style, link and reference, with every line break on its own.
fn pieces(styled: &Styled, len: usize) -> Vec<Range<usize>> {
    let breaks = line_breaks(&styled.text[..len])
        .into_iter()
        .flat_map(|range| [range.start, range.end]);
    let mut bounds: Vec<usize> = styled
        .runs
        .iter()
        .map(|(range, _)| range)
        .chain(styled.links.iter().map(|(range, _)| range))
        .chain(styled.refs.iter().map(|(range, _)| range))
        .flat_map(|range| [range.start, range.end])
        .chain(breaks)
        .chain([0, len])
        .filter(|at| *at <= len)
        .collect();
    bounds.sort_unstable();
    bounds.dedup();
    bounds.windows(2).map(|pair| pair[0]..pair[1]).collect()
}

/// Each line ending in `text`: `\r\n` as one, `\n`, or a lone `\r`.
fn line_breaks(text: &str) -> Vec<Range<usize>> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        let len = match (bytes[at], bytes.get(at + 1)) {
            (b'\r', Some(b'\n')) => 2,
            (b'\r' | b'\n', _) => 1,
            _ => 0,
        };
        if len > 0 {
            out.push(at..at + len);
        }
        at += len.max(1);
    }
    out
}

/// Whether a piece is a line ending.
fn is_break(text: &str) -> bool {
    matches!(text, "\n" | "\r" | "\r\n")
}

/// One piece as Markdown: a hard break, or its text shown as sent, linked
/// and tagged as its style says.
fn piece(styled: &Styled, range: Range<usize>, look: Look) -> String {
    let text = &styled.text[range.clone()];
    if is_break(text) {
        return HARD_BREAK.to_owned();
    }
    let rest = styled.text[range.end..].trim_end_matches(BREAKS);
    let edges = Edges {
        start: styled.text[..range.start].ends_with(BREAKS) || range.start == 0,
        end: rest.is_empty() || rest.starts_with(BREAKS),
    };
    let shown = shown(text, look.style.monospace, edges);
    let linked = match look.target {
        Some(target) => format!("[{shown}]({target})"),
        None => shown,
    };
    let tagged = tagged(linked, look.style);
    match look.tint {
        Some(color) => format!("<mark color=\"{color}\">{tagged}</mark>"),
        None => tagged,
    }
}

/// What a piece links to: a URL the link policy opens, or a reference.
fn target(styled: &Styled, range: &Range<usize>) -> Option<String> {
    covering(&styled.links, range)
        .and_then(|url| destination(url))
        .or_else(|| covering(&styled.refs, range).map(ref_url))
}

/// The item of `items` whose range holds all of `range`.
fn covering<'a, T>(items: &'a [(Range<usize>, T)], range: &Range<usize>) -> Option<&'a T> {
    items
        .iter()
        .find(|(item, _)| item.start <= range.start && range.end <= item.end)
        .map(|(_, value)| value)
}

mod shown;
#[cfg(test)]
mod tests;
