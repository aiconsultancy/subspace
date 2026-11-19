//! How one piece of literal text is written: escaped so it shows as typed,
//! or as a code span, inside its formatting tags.

use super::super::inline::label;
use crate::style::RunStyle;

/// Spaces and tabs at a line's start or end, as character references.
const SPACE: &str = "&#32;";
const TAB: &str = "&#9;";
/// What indents a line.
const INDENT: [char; 2] = [' ', '\t'];

/// Whether a piece starts and ends a line.
#[derive(Debug, Clone, Copy)]
pub(super) struct Edges {
    pub start: bool,
    pub end: bool,
}

/// `text` as a code span when it is monospace, else escaped.
pub(super) fn shown(text: &str, monospace: bool, edges: Edges) -> String {
    if monospace && !text.trim().is_empty() {
        code(text)
    } else {
        escaped(text, edges)
    }
}

/// `text` escaped so it shows as typed. Spaces and tabs that start or end
/// a line become character references, as Markdown would read them as
/// indentation or drop them.
fn escaped(text: &str, edges: Edges) -> String {
    let body = if edges.start {
        text.trim_start_matches(INDENT)
    } else {
        text
    };
    let leading = &text[..text.len() - body.len()];
    let body = if edges.end {
        body.trim_end_matches(INDENT)
    } else {
        body
    };
    let trailing = &text[leading.len() + body.len()..];
    format!(
        "{}{}{}",
        references(leading),
        label(body),
        references(trailing)
    )
}

/// Spaces and tabs as character references.
fn references(blank: &str) -> String {
    blank
        .chars()
        .map(|c| if c == '\t' { TAB } else { SPACE })
        .collect()
}

/// `text` as a code span that shows it exactly: fenced by one more
/// backtick than its longest run, and padded with a space at each end,
/// which Markdown strips again, when it starts or ends with a backtick or
/// a space. `text` is not blank.
fn code(text: &str) -> String {
    let longest = text
        .split(|c| c != '`')
        .map(str::len)
        .max()
        .unwrap_or_default();
    let fence = "`".repeat(longest + 1);
    let edges = ['`', ' '];
    let pad = if text.starts_with(edges) || text.ends_with(edges) {
        " "
    } else {
        ""
    };
    format!("{fence}{pad}{text}{pad}{fence}")
}

/// `markdown` inside the inline tags of `style`'s bold, italic, underline
/// and strikethrough.
pub(super) fn tagged(markdown: String, style: RunStyle) -> String {
    [
        (style.bold, "b"),
        (style.italic, "i"),
        (style.underline, "u"),
        (style.strikethrough, "s"),
    ]
    .into_iter()
    .filter(|(on, _)| *on)
    .fold(markdown, |inner, (_, tag)| {
        format!("<{tag}>{inner}</{tag}>")
    })
}
