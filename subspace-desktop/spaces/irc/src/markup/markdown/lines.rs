//! One line rewritten in its block: code and fences as written, a list item
//! with its marker, a quoted line after `> `, and a paragraph line without
//! the block syntax Markdown would read at its start.

use subspace_framework::dashboard::rich::escape_markdown;

use super::super::{Mentions, indent, list_marker};
use super::blocks::Block;
use super::inline::{Inline, inline};

/// What a rule or setext underline is made of.
const RULE: &[char] = &['-', '=', '_', '*'];
/// What a paragraph or quoted line must not start with, as it would start
/// a list item.
const ITEM_MARKERS: &[char] = &['-', '+'];
/// What closes an ordered list item's number.
const NUMBER_ENDS: &[char] = &['.', ')'];

/// A code line, fence or blank line, kept as written; a fence is `fence`
/// and the code's language. Code and fences count as formatting; a fence
/// shows nothing.
pub(super) fn kept(block: Block, line: &str, fence: &str) -> Inline {
    let markdown = match block {
        Block::Fence => format!("{fence}{line}"),
        Block::Code => line.to_owned(),
        _ => String::new(),
    };
    Inline {
        markdown,
        plain: if block == Block::Code { line } else { "" }.to_owned(),
        formatted: matches!(block, Block::Code | Block::Fence),
        ..Inline::default()
    }
}

/// A list item: its indentation and marker (`•` as `-`), then its text as
/// a paragraph line, so `- ---` or `- > x` show as typed.
pub(super) fn item(line: &str, mentions: &Mentions) -> Inline {
    let (lead, item) = line.split_at(indent(line));
    let marker = list_marker(line).unwrap_or_default();
    let body = &item[marker.len()..];
    let shown = if marker == "• " { "- " } else { marker };
    let body = body.trim_start();
    let text = text(body, mentions, false).at(line.len() - body.len());
    Inline {
        markdown: format!("{lead}{shown}{}", text.markdown),
        plain: format!("{lead}{marker}{}", text.plain),
        formatted: true,
        ..text
    }
}

/// A paragraph line, or the text of a quoted line after `> ` (`>` alone
/// when it is empty). `first` marks the message's first line.
pub(super) fn paragraph(block: Block, line: &str, mentions: &Mentions, first: bool) -> Inline {
    let trimmed = line.trim_start();
    let text = text(trimmed, mentions, first).at(line.len() - trimmed.len());
    if block != Block::Quote {
        return text;
    }
    let markdown = match text.markdown.as_str() {
        "" => ">".to_owned(),
        markdown => format!("> {markdown}"),
    };
    Inline {
        markdown,
        formatted: true,
        ..text
    }
}

/// A line's text, escaped where it would make a rule or setext underline,
/// or start a quote or list item.
fn text(line: &str, mentions: &Mentions, first: bool) -> Inline {
    let rule =
        !line.trim().is_empty() && line.chars().all(|c| c.is_whitespace() || RULE.contains(&c));
    if rule {
        return Inline {
            markdown: escape_markdown(line),
            plain: line.to_owned(),
            formatted: false,
            ..Inline::default()
        };
    }
    let text = inline(line, mentions, first);
    Inline {
        markdown: block_start(&text.markdown),
        ..text
    }
}

/// `markdown` with the character escaped that would make it start a quote,
/// a setext underline or a list item: `>`, `=`, a bare `-` or `+`, or a
/// number and `.` or `)` before a space or the end.
fn block_start(markdown: &str) -> String {
    let digits = markdown.len()
        - markdown
            .trim_start_matches(|c: char| c.is_ascii_digit())
            .len();
    let (head, rest) = markdown.split_at(digits);
    let next = rest.chars().next();
    let item_end = rest[next.map_or(0, char::len_utf8)..]
        .chars()
        .next()
        .is_none_or(char::is_whitespace);
    let escape = match next {
        Some('>' | '=') => digits == 0,
        Some(c) if ITEM_MARKERS.contains(&c) => digits == 0 && item_end,
        Some(c) if NUMBER_ENDS.contains(&c) => digits > 0 && item_end,
        _ => false,
    };
    if escape {
        format!("{head}\\{rest}")
    } else {
        markdown.to_owned()
    }
}
