//! A body too large for the room left even once room was made, drawn one
//! part at a time (SUB decision `0c13b784` §3): each block's label on a
//! line of its own above its text, the blocks a blank line apart, cut into
//! parts drawing at most [`PART`] bytes, markers and all, never inside a
//! character or its marker. Only the part shown is formatted and kept:
//! a value is written out up to that part and no further is kept.

use std::io::Write as _;

use super::blocks::{Block, Source, error_text};
use super::parts::PART;
use super::size::{Utf8Sink, count_measure};
use super::text::{push_visible, unit_len};

/// One part of a body drawn in parts.
pub(super) struct Piece {
    /// The part's text, as drawn.
    pub(super) text: String,
    /// Which part it is, from 0.
    pub(super) piece: usize,
    /// How many parts the body has.
    pub(super) pieces: usize,
}

/// Part `wanted` of the body `blocks`, or its last part when it has
/// fewer.
pub(super) fn piece(blocks: &[Block<'_>], wanted: usize) -> Piece {
    let piece = cut(blocks, wanted);
    if piece.piece == wanted {
        piece
    } else {
        cut(blocks, piece.pieces - 1)
    }
}

/// Reads the whole body, keeping part `wanted` only.
fn cut(blocks: &[Block<'_>], wanted: usize) -> Piece {
    count_measure();
    let mut window = Window {
        wanted,
        piece: 0,
        len: 0,
        text: String::new(),
    };
    for (index, block) in blocks.iter().enumerate() {
        if index > 0 {
            window.take("\n\n");
        }
        if let Some(label) = &block.label {
            window.take(label);
            window.take("\n");
        }
        feed(&mut window, block.source);
    }
    let pieces = window.piece + 1;
    Piece {
        piece: wanted.min(window.piece),
        pieces,
        text: window.text,
    }
}

/// Gives `window` what `source` draws, a value as it is written out.
fn feed(window: &mut Window, source: Source<'_>) {
    match source {
        Source::Text(text) => window.take(text),
        Source::Json(value) => feed_json(window, value),
        Source::Error(error) => window.take(&error_text(error)),
    }
}

/// Gives `window` a value as it is written out as indented JSON, never
/// holding it whole.
fn feed_json(window: &mut Window, value: &serde_json::Value) {
    let mut sink = Utf8Sink::new(|text: &str| window.take(text));
    _ = serde_json::to_writer_pretty(&mut sink, value);
    _ = sink.flush();
}

/// Counts the parts of a body as it goes by, and keeps the one wanted.
struct Window {
    wanted: usize,
    /// The part being read.
    piece: usize,
    /// The bytes it draws so far.
    len: usize,
    text: String,
}

impl Window {
    fn take(&mut self, text: &str) {
        for ch in text.chars() {
            let unit = unit_len(ch);
            if self.len > 0 && self.len + unit > PART {
                self.piece += 1;
                self.len = 0;
            }
            self.len += unit;
            if self.piece == self.wanted {
                push_visible(&mut self.text, ch);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{PART, piece};
    use crate::view::inspector::activity::blocks::{Block, Source};
    use crate::view::inspector::activity::parts::LONG;

    fn block<'a>(label: Option<&str>, source: Source<'a>) -> Block<'a> {
        Block {
            suffix: "b".to_owned(),
            label: label.map(str::to_owned),
            source,
            long: LONG,
        }
    }

    /// SUB decision `0c13b784` §3: every byte of the body is in exactly
    /// one part, each drawing at most one part's bytes, markers and all.
    #[test]
    fn a_body_in_parts_keeps_every_byte_in_one_part_within_a_part() {
        let text = format!("{}\u{0}tail", "a\u{1b}".repeat(PART / 3));
        let value = json!({"k": "v".repeat(PART)});
        let blocks = [
            block(None, Source::Text(&text)),
            block(Some("Input · k"), Source::Json(&value)),
        ];
        let first = piece(&blocks, 0);
        let mut joined = String::new();
        for wanted in 0..first.pieces {
            let part = piece(&blocks, wanted);
            assert_eq!((part.piece, part.pieces), (wanted, first.pieces));
            assert!(part.text.len() <= PART, "{}", part.text.len());
            joined.push_str(&part.text);
        }
        let pretty = serde_json::to_string_pretty(&value).unwrap();
        let whole = format!(
            "{}\n\nInput · k\n{pretty}",
            crate::view::inspector::activity::text::visible(&text)
        );
        assert_eq!(joined, whole);
        assert_eq!(piece(&blocks, 99).piece, first.pieces - 1, "the last");
    }
}
