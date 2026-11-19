//! A message's lines sorted into blocks, with Slack's code fences and quotes
//! read as Slack reads them:
//!
//! - ```` ```code``` ```` on one line is a code block;
//! - text after an opening fence is the block's first line, unless it is one
//!   word, which names the code's language as in Markdown;
//! - a fence at the end of a code line closes the block, and text after a
//!   closing fence is a line of its own;
//! - `> ` quotes its line and `>>>` quotes the rest of the message.

use super::super::{FENCE, indent, list_marker};

/// The marker that quotes one line.
const QUOTE: &str = "> ";
/// The marker that quotes the rest of the message.
const QUOTE_REST: &str = ">>>";
/// Characters of a code block's language name, such as `rust` or `c++`.
const LANGUAGE: &[char] = &['+', '-', '#', '.', '_'];

/// What kind of block a line belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Block {
    Paragraph,
    List,
    /// A quoted line; its text is the line after the marker.
    Quote,
    /// A line of code, kept as written.
    Code,
    /// A fence that opens or closes code; its text is the code's language.
    Fence,
    Blank,
}

/// The lines of a message in their blocks.
#[derive(Debug, Default)]
pub(super) struct Blocks<'a> {
    pub lines: Vec<(Block, &'a str)>,
    /// Whether a code block is still open at the end.
    pub fenced: bool,
    /// Whether `>>>` quoted the rest of the message.
    quoting: bool,
    /// The indentation of the first item of the current list.
    list_indent: Option<usize>,
}

/// The lines of `text` in their blocks. A lone carriage return ends a line,
/// as it does in Markdown.
pub(super) fn sort(text: &str) -> Blocks<'_> {
    let mut blocks = Blocks::default();
    text.lines()
        .flat_map(|line| line.split('\r'))
        .for_each(|line| blocks.line(line));
    if blocks.fenced {
        blocks.unlabel();
    }
    blocks
}

impl<'a> Blocks<'a> {
    fn line(&mut self, line: &'a str) {
        if self.fenced {
            self.code(line);
        } else if self.quoting {
            self.push((Block::Quote, line.trim()));
        } else {
            self.outside(line);
        }
    }

    /// Adds a line. A list's lines lose the first item's indentation, so an
    /// indented list stays a list rather than becoming code. Indentation is
    /// spaces and tabs, so the cut is always between characters.
    fn push(&mut self, (block, text): (Block, &'a str)) {
        let text = match block {
            Block::List => {
                let indent = indent(text);
                let base = *self.list_indent.get_or_insert(indent);
                &text[indent.min(base)..]
            }
            _ => {
                self.list_indent = None;
                text
            }
        };
        self.lines.push((block, text));
    }

    /// A line inside a code block: code, or a fence that closes it.
    fn code(&mut self, line: &'a str) {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix(FENCE) {
            self.close();
            let rest = rest.trim_start_matches('`').trim();
            if !rest.is_empty() {
                self.outside(rest);
            }
        } else if let Some(code) = line.trim_end().strip_suffix(FENCE) {
            self.push((Block::Code, code));
            self.close();
        } else {
            self.push((Block::Code, line));
        }
    }

    fn close(&mut self) {
        self.unlabel();
        self.push((Block::Fence, ""));
        self.fenced = false;
    }

    /// An opening fence's language when no code followed it: Slack's
    /// ```` ```word ```` then shows `word` as code rather than hiding it.
    fn unlabel(&mut self) {
        if let Some((Block::Fence, word)) = self.lines.last().copied()
            && !word.is_empty()
        {
            self.lines.pop();
            self.push((Block::Fence, ""));
            self.push((Block::Code, word));
        }
    }

    /// A line outside code: a fence, `>>>`, or a line of its own kind.
    fn outside(&mut self, line: &'a str) {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix(FENCE) {
            self.open(rest);
        } else if let Some(rest) = trimmed.strip_prefix(QUOTE_REST) {
            self.quoting = true;
            self.quote_rest(rest);
        } else {
            self.push(kind(line));
        }
    }

    /// The text after `>>>` on its line, quoted when there is any.
    fn quote_rest(&mut self, rest: &'a str) {
        let rest = rest.trim();
        if !rest.is_empty() {
            self.push((Block::Quote, rest));
        }
    }

    /// An opening fence and what follows it on its line: nothing, the
    /// code's language, the first line of code, or the whole block.
    fn open(&mut self, rest: &'a str) {
        let rest = rest.trim_start_matches('`').trim_end();
        let one_line = rest
            .strip_suffix(FENCE)
            .map(|code| code.trim_end_matches('`').trim())
            .filter(|code| !code.is_empty());
        if let Some(code) = one_line {
            self.push((Block::Fence, ""));
            self.push((Block::Code, code));
            self.push((Block::Fence, ""));
            return;
        }
        let language = !rest.is_empty()
            && rest
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || LANGUAGE.contains(&c));
        self.push((Block::Fence, if language { rest } else { "" }));
        if !language && !rest.trim().is_empty() {
            self.push((Block::Code, rest.trim_start()));
        }
        self.fenced = true;
    }
}

/// A line that neither fences code nor quotes the rest of the message: a
/// blank, a quoted line (`> text`, or `>` alone), a list item or a
/// paragraph line. A list marker with no text after it is a paragraph line,
/// which shows it as typed: an empty item under another would underline that
/// item as a heading.
fn kind(line: &str) -> (Block, &str) {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        (Block::Blank, "")
    } else if let Some(rest) = trimmed.strip_prefix(QUOTE) {
        (Block::Quote, rest.trim())
    } else if trimmed == QUOTE.trim() {
        (Block::Quote, "")
    } else if list_marker(line)
        .is_some_and(|marker| !line[indent(line) + marker.len()..].trim().is_empty())
    {
        (Block::List, line)
    } else {
        (Block::Paragraph, line)
    }
}
