//! A message rewritten as Markdown limited to the safe subset, when it uses
//! formatting: Slack's and Markdown's bold, italic and strikethrough, code,
//! quotes, lists and links. Other block syntax and HTML are escaped, each
//! line keeps its own line, and references become links the board opens.

mod blocks;
mod emphasis;
mod inline;
mod lines;
mod literal;
mod spans;

use self::blocks::Block;
use self::inline::Inline;
pub(crate) use self::literal::literal;
pub(crate) use self::spans::destination;
use std::ops::Range;

use super::{FENCE, Mentions, Ref};

/// A message rewritten for the shared rich text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Rendered {
    /// Markdown limited to the safe subset: no images, and no HTML but the
    /// inline tags [`literal`] writes itself (`<b>`, `<i>`, `<u>`, `<s>`
    /// and `<mark color>`), which no message text can open.
    pub markdown: String,
    /// The text as shown, without formatting markers, for the row's
    /// accessible name.
    pub plain: String,
    /// The issue and pull request references outside code, every one in
    /// reading order, a pull request with the URL as written, each with
    /// where it is in the message's text.
    pub refs: Vec<(Range<usize>, Ref)>,
}

/// `text` as safe Markdown when it uses formatting, code, a quote, a list
/// or a link; `None` when it is plain and keeps the plain presentation.
pub(crate) fn to_markdown(text: &str, mentions: &Mentions) -> Option<Rendered> {
    let blocks = blocks::sort(text);
    let fence = fence(&blocks.lines);
    let lines: Vec<(Block, Inline)> = blocks
        .lines
        .iter()
        .enumerate()
        .map(|(index, (block, line))| {
            let inline = rewrite(*block, line, mentions, index == 0, &fence);
            (*block, inline.at(offset(text, line)))
        })
        .collect();
    let formatted = lines
        .iter()
        .any(|(block, line)| line.formatted || !matches!(block, Block::Paragraph | Block::Blank));
    formatted.then(|| join(&lines, blocks.fenced.then_some(fence.as_str())))
}

/// Where `line`, a line of `text`, starts in it. Every line with a
/// reference is a part of `text`; only a fence's or blank line's empty text
/// may not be, and it has none.
fn offset(text: &str, line: &str) -> usize {
    text.substr_range(line).map_or(0, |range| range.start)
}

/// The fence every code block of the message is written with: longer than
/// any run of backticks in its code, so no line of code can close it.
fn fence(lines: &[(Block, &str)]) -> String {
    let longest = lines
        .iter()
        .filter(|(block, _)| *block == Block::Code)
        .flat_map(|(_, line)| line.split(|c| c != '`'))
        .map(str::len)
        .max()
        .unwrap_or_default();
    "`".repeat(longest.max(FENCE.len() - 1) + 1)
}

/// One line in its block: code and fences as written, a list item with its
/// marker, and a paragraph or quoted line without block syntax.
fn rewrite(block: Block, line: &str, mentions: &Mentions, first: bool, fence: &str) -> Inline {
    match block {
        Block::Paragraph | Block::Quote => lines::paragraph(block, line, mentions, first),
        Block::List => lines::item(line, mentions),
        Block::Code | Block::Fence | Block::Blank => lines::kept(block, line, fence),
    }
}

/// Lines joined so each keeps its own line: a hard break between paragraph
/// lines and between quoted lines, a blank line where a list or a quote
/// starts or ends, and `closing` after a code block left open. The shown
/// text has one line per line, without fences.
fn join(lines: &[(Block, Inline)], closing: Option<&str>) -> Rendered {
    let mut markdown = String::new();
    let mut plain = Vec::new();
    let mut previous: Option<Block> = None;
    for (block, line) in lines {
        markdown.push_str(previous.map_or("", |previous| separator(previous, *block)));
        markdown.push_str(&line.markdown);
        if *block != Block::Fence {
            plain.push(line.plain.as_str());
        }
        previous = Some(*block);
    }
    if let Some(fence) = closing {
        markdown.push('\n');
        markdown.push_str(fence);
    }
    Rendered {
        markdown,
        plain: plain.join("\n"),
        refs: lines
            .iter()
            .flat_map(|(_, line)| line.refs.iter().cloned())
            .collect(),
    }
}

/// What goes between a line of `previous` block and one of `next`.
fn separator(previous: Block, next: Block) -> &'static str {
    match (previous, next) {
        (Block::Paragraph, Block::Paragraph) | (Block::Quote, Block::Quote) => "  \n",
        (Block::Quote, Block::Blank) | (Block::Blank, Block::Quote) => "\n",
        (Block::Quote, _)
        | (_, Block::Quote)
        | (Block::Paragraph, Block::List)
        | (Block::List, Block::Paragraph) => "\n\n",
        _ => "\n",
    }
}

#[cfg(test)]
mod parsed;
#[cfg(test)]
mod tests;
