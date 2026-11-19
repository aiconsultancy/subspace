//! One line's inline text as safe Markdown: paired formatting markers as
//! CommonMark emphasis, code spans and safe links as written, URLs and
//! references as links, and every other character that Markdown could read
//! as syntax escaped. Beside it, the text as shown, for accessible names.

use std::ops::Range;

use subspace_framework::dashboard::rich::escape_markdown;

use super::super::link::ref_url;
use super::super::{Found, Mentions, Ref, classify};
use super::emphasis::{self, Marker};
use super::spans::{self, MARKS, NOT_IN_URL};

/// Characters escaped wherever they are literal: they would start a table,
/// a heading, an entity, math or an escape, or (`:` and `@`) let a GFM
/// autolink literal such as `https://…` or `ops@example.org` form. Such a
/// literal runs to the next space and swallows the syntax in it, such as
/// a code span's opening backtick, so the code after it would be read as
/// Markdown. The board links real URLs itself.
const ESCAPED: &[char] = &['|', '#', '&', '$', '\\', ':', '@'];
/// A literal `<`, as an entity rather than `\<`, which a GFM autolink
/// literal could take in, leaving the `<` to start HTML.
const LESS_THAN: &str = "&lt;";
/// What starts a GFM `www.` autolink literal; its `.` is escaped.
const WWW: &str = "www";

/// A line rewritten.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct Inline {
    pub markdown: String,
    /// The text as shown: without markers, with link text for links.
    pub plain: String,
    /// Whether it uses formatting, code or a link.
    pub formatted: bool,
    /// The issue and pull request references, each one in reading order
    /// with where it is in the text.
    pub refs: Vec<(Range<usize>, Ref)>,
}

impl Inline {
    /// The line as part of a text in which it starts at `at`.
    pub fn at(self, at: usize) -> Self {
        let refs = self.refs.into_iter();
        Self {
            refs: refs
                .map(|(range, found)| (range.start + at..range.end + at, found))
                .collect(),
            ..self
        }
    }
}

/// What one step of the rewrite writes, and how much text it reads.
struct Piece {
    markdown: String,
    plain: String,
    used: usize,
    formatted: bool,
    /// Whether a word may start after it: after an opening marker.
    opens: bool,
    /// The issue or pull request it links, and where it is in the piece.
    found: Option<(Range<usize>, Ref)>,
}

impl Piece {
    /// Text shown as written, with `markdown` as its Markdown.
    fn text(markdown: String, plain: &str) -> Self {
        Self {
            markdown,
            plain: plain.to_owned(),
            used: plain.len(),
            formatted: false,
            opens: false,
            found: None,
        }
    }

    /// Formatting that reads `used` bytes and shows `plain`.
    fn formatting(markdown: String, plain: &str, used: usize) -> Self {
        Self {
            markdown,
            plain: plain.to_owned(),
            used,
            formatted: true,
            opens: false,
            found: None,
        }
    }
}

/// `text` as Markdown with its formatting markers, code spans and safe
/// links kept, references and URLs as links, and everything else literal.
/// `first` marks the message's first line, where `nick:` addresses someone.
pub(super) fn inline(text: &str, mentions: &Mentions, first: bool) -> Inline {
    let markers = emphasis::markers(text);
    let spaces: Vec<usize> = text
        .char_indices()
        .filter(|(_, c)| c.is_whitespace())
        .map(|(at, _)| at)
        .collect();
    let mut out = Inline::default();
    let mut at = 0;
    let mut word_start = true;
    while at < text.len() {
        let ahead = &markers[markers.partition_point(|marker| marker.range.start < at)..];
        let addressing = first && out.plain.trim().is_empty();
        let piece = ahead
            .first()
            .filter(|marker| marker.range.start == at)
            .map(marker)
            .or_else(|| {
                let end = || word_end(&spaces, at, ahead, text.len());
                word_start
                    .then(|| reference(&text[at..end()], mentions, addressing))
                    .flatten()
            })
            .unwrap_or_else(|| literal(text, at));
        out.markdown.push_str(&piece.markdown);
        out.plain.push_str(&piece.plain);
        out.formatted |= piece.formatted;
        out.refs.extend(
            piece
                .found
                .map(|(range, found)| (range.start + at..range.end + at, found)),
        );
        at += piece.used;
        word_start = piece.opens || text[..at].ends_with(char::is_whitespace);
    }
    out
}

/// A formatting marker as its CommonMark marker.
fn marker(marker: &Marker) -> Piece {
    Piece {
        opens: marker.opens,
        ..Piece::formatting(marker.style.markdown().to_owned(), "", marker.range.len())
    }
}

/// Where the word that starts at `at` ends: at the next of the text's
/// `spaces` or of the formatting markers `ahead`, both in order, or at the
/// end of the text, `len`.
fn word_end(spaces: &[usize], at: usize, ahead: &[Marker], len: usize) -> usize {
    let space = spaces
        .get(spaces.partition_point(|space| *space < at))
        .copied()
        .unwrap_or(len);
    ahead
        .iter()
        .map(|marker| marker.range.start)
        .find(|start| *start > at)
        .map_or(space, |start| start.min(space))
}

/// A URL or reference that `word` starts with, as a link that shows the
/// word as typed, with any opening bracket or quote before it. A URL links
/// to what the link policy opens, written by [`spans::destination`]; one
/// with an angle bracket, or that the policy would not open, is not a link.
fn reference(word: &str, mentions: &Mentions, first: bool) -> Option<Piece> {
    let (range, target, found) = match classify(word, mentions, first)? {
        Found::Url(range) if word[range.clone()].contains(NOT_IN_URL) => return None,
        Found::Url(range) => {
            let target = spans::destination(&word[range.clone()])?;
            (range, target, None)
        }
        Found::Ref(range, found) => (range, ref_url(&found), Some(found)),
    };
    let leading = &word[..range.start];
    let label = label(&word[range.clone()]);
    let piece = Piece::text(format!("{leading}[{label}]({target})"), &word[..range.end]);
    Some(Piece {
        found: found
            .filter(|found| matches!(found, Ref::Issue(_) | Ref::PullRequest(_)))
            .map(|found| (range, found)),
        ..piece
    })
}

/// Link text that shows `text` as typed.
pub(super) fn label(text: &str) -> String {
    escape_markdown(text).replace("\\<", LESS_THAN)
}

/// The character or span at `at` in `text`, written so Markdown shows it as
/// typed: a closed code span or a safe link as written, an unclosed run of backticks, an image's `!`, `<`, the
/// `.` of `www.` and other syntax escaped.
fn literal(text: &str, at: usize) -> Piece {
    let rest = &text[at..];
    let c = rest.chars().next().unwrap_or_default();
    let after_www = text
        .get(at.saturating_sub(WWW.len())..at)
        .is_some_and(|before| before.eq_ignore_ascii_case(WWW));
    match c {
        '.' if after_www => Piece::text("\\.".to_owned(), "."),
        '`' => code(rest),
        '[' => match spans::link(rest) {
            Some(link) => {
                let text = &rest[link.text];
                let markdown = format!("[{}]({})", label(text), link.url);
                Piece::formatting(markdown, text, link.len)
            }
            None => Piece::text("\\[".to_owned(), "["),
        },
        '!' if rest[1..].starts_with('[') => Piece::text("\\!".to_owned(), "!"),
        '<' => Piece::text(LESS_THAN.to_owned(), "<"),
        _ if MARKS.contains(&c) || ESCAPED.contains(&c) => {
            Piece::text(format!("\\{c}"), &rest[..c.len_utf8()])
        }
        _ => Piece::text(c.to_string(), &rest[..c.len_utf8()]),
    }
}

/// The code span at the start of `rest` as written, shown as Markdown shows
/// it: its code, less one space at each end when both ends have one. Its
/// backticks are escaped when the line does not close it.
fn code(rest: &str) -> Piece {
    let run = rest.len() - rest.trim_start_matches('`').len();
    match spans::code_span(rest) {
        Some(len) => {
            let code = &rest[run..len - run];
            let padded = code.len() > 1
                && code.starts_with(' ')
                && code.ends_with(' ')
                && !code.trim().is_empty();
            let shown = if padded {
                &code[1..code.len() - 1]
            } else {
                code
            };
            Piece::formatting(rest[..len].to_owned(), shown, len)
        }
        None => Piece::text("\\`".repeat(run), &rest[..run]),
    }
}
