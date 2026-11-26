//! A body drawn one part at a time (SUB decisions `0c13b784` §3 and
//! `33ccecf6` item 5): each block's label on a line of its own above its
//! text, the blocks a blank line apart, cut into parts drawing at most a
//! part's bytes, markers and all, never inside a character or its marker.
//!
//! One pass over a version of the body finds where every part starts
//! ([`Index`]); moving to a part then writes out only that part, from its
//! start: a value is written from the middle as indented JSON
//! ([`json`]), a text sliced, an error's fields read one by one. Nothing
//! before the part is formatted or scanned again.

use subspace_client_fleet::activity::ActivityError;

use super::blocks::{Block, Source, error_segments};
use super::size::count_measure;
use super::text::{push_visible, unit_len};

mod json;

/// One part of a body drawn in parts.
pub(super) struct Piece {
    /// The part's text, as drawn.
    pub(super) text: String,
    /// Which part it is, from 0.
    pub(super) piece: usize,
    /// How many parts the body has.
    pub(super) pieces: usize,
}

/// Where each part of one version of a body starts, for parts drawing at
/// most `size` bytes.
pub(super) struct Index {
    pub(super) size: usize,
    starts: Vec<Mark>,
}

impl Index {
    pub(super) fn pieces(&self) -> usize {
        self.starts.len()
    }
}

/// A point in a body: a block, and where in it.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Mark {
    block: usize,
    at: At,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum At {
    /// The blank line before the block, by byte.
    Gap(usize),
    /// Its label and the line break after it, by byte.
    Label(usize),
    /// Its text, by byte.
    Text(usize),
    /// A failure's field, and the byte within it.
    Error(usize, usize),
    /// A point in a value written out.
    Json(json::Mark),
}

impl At {
    /// The order of the parts of a block.
    fn rank(&self) -> u8 {
        match self {
            Self::Gap(_) => 0,
            Self::Label(_) => 1,
            _ => 2,
        }
    }
}

/// What each character walked is given to, with a way to name where it
/// is; the walk stops when it answers `false`.
type Take<'t> = dyn FnMut(char, &dyn Fn() -> Mark) -> bool + 't;

/// Finds where each part of `blocks` starts, for parts of at most `size`
/// bytes drawn, in one pass.
pub(super) fn index(blocks: &[Block<'_>], size: usize) -> Index {
    count_measure();
    let start = Mark {
        block: 0,
        at: At::Gap(0),
    };
    let mut starts = vec![start.clone()];
    let mut len = 0;
    walk(blocks, &start, &mut |ch, here| {
        let unit = unit_len(ch);
        if len > 0 && len + unit > size {
            starts.push(here());
            len = 0;
        }
        len += unit;
        true
    });
    Index { size, starts }
}

/// Part `wanted` of `blocks`, or its last part when it has fewer: written
/// out from where `index` says it starts, and no further.
pub(super) fn piece(blocks: &[Block<'_>], index: &Index, wanted: usize) -> Piece {
    let piece = wanted.min(index.pieces().saturating_sub(1));
    let mut text = String::new();
    let mut len = 0;
    if let Some(start) = index.starts.get(piece) {
        walk(blocks, start, &mut |ch, _| {
            let unit = unit_len(ch);
            if len > 0 && len + unit > index.size {
                return false;
            }
            len += unit;
            walked();
            push_visible(&mut text, ch);
            true
        });
    }
    Piece {
        text,
        piece,
        pieces: index.pieces(),
    }
}

/// Gives `take` each character of `blocks` from `from` on.
fn walk(blocks: &[Block<'_>], from: &Mark, take: &mut Take<'_>) {
    for (index, block) in blocks.iter().enumerate().skip(from.block) {
        let at = (index == from.block).then_some(&from.at);
        if !walk_block((index, block), at, take) {
            return;
        }
    }
}

/// Gives `take` each character of block `index` from `at` on (from its
/// start when `None`): its gap, its label, then its text.
fn walk_block((index, block): (usize, &Block<'_>), at: Option<&At>, take: &mut Take<'_>) -> bool {
    let rank = at.map_or(0, At::rank);
    let mark = |at: At| Mark { block: index, at };
    let gap = index > 0 && rank == 0;
    let label = block.label.filter(|_| rank <= 1);
    (!gap || walk_gap(at, take, &mark))
        && label.is_none_or(|label| walk_label(label, at, take, &mark))
        && walk_source(block.source, at.filter(|at| at.rank() == 2), take, &mark)
}

/// Where a block's walk names a point in it.
type Marker<'m> = dyn Fn(At) -> Mark + 'm;

/// The blank line before a block, from `at` on.
fn walk_gap(at: Option<&At>, take: &mut Take<'_>, mark: &Marker<'_>) -> bool {
    let from = match at {
        Some(At::Gap(from)) => *from,
        _ => 0,
    };
    text("\n\n", from, take, &|byte| mark(At::Gap(byte)))
}

/// A block's label and the line break after it, from `at` on.
fn walk_label(label: &str, at: Option<&At>, take: &mut Take<'_>, mark: &Marker<'_>) -> bool {
    let from = match at {
        Some(At::Label(from)) => *from,
        _ => 0,
    };
    text(&format!("{label}\n"), from, take, &|byte| {
        mark(At::Label(byte))
    })
}

/// A block's text, from `at` on.
fn walk_source(
    source: Source<'_>,
    at: Option<&At>,
    take: &mut Take<'_>,
    mark: &Marker<'_>,
) -> bool {
    match (source, at) {
        (Source::Text(source), Some(At::Text(from))) => {
            text(source, *from, take, &|byte| mark(At::Text(byte)))
        }
        (Source::Text(source), _) => text(source, 0, take, &|byte| mark(At::Text(byte))),
        (Source::Json(value), Some(At::Json(from))) => walk_json(value, Some(from), take, mark),
        (Source::Json(value), _) => walk_json(value, None, take, mark),
        (Source::Error(error), Some(At::Error(field, byte))) => {
            walk_error(error, (*field, *byte), take, mark)
        }
        (Source::Error(error), _) => walk_error(error, (0, 0), take, mark),
    }
}

/// A value written out as indented JSON, from `from` on.
fn walk_json(
    value: &serde_json::Value,
    from: Option<&json::Mark>,
    take: &mut Take<'_>,
    mark: &Marker<'_>,
) -> bool {
    json::walk(value, from, &mut |ch, here| {
        take(ch, &|| mark(At::Json(here())))
    })
}

/// A failure's fields one by one, from byte `byte` of field `field` on.
fn walk_error(
    error: &ActivityError,
    (field, byte): (usize, usize),
    take: &mut Take<'_>,
    mark: &Marker<'_>,
) -> bool {
    error_segments(error)
        .iter()
        .enumerate()
        .skip(field)
        .all(|(index, segment)| {
            let from = if index == field { byte } else { 0 };
            text(segment, from, take, &|byte| mark(At::Error(index, byte)))
        })
}

/// Gives `take` each character of `source` from byte `from` on.
fn text(source: &str, from: usize, take: &mut Take<'_>, mark: &dyn Fn(usize) -> Mark) -> bool {
    source
        .get(from..)
        .unwrap_or_default()
        .char_indices()
        .all(|(at, ch)| take(ch, &|| mark(from + at)))
}

#[cfg(test)]
thread_local! {
    static WALKED: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// Counts a character written out for a part.
fn walked() {
    #[cfg(test)]
    WALKED.with(|walked| walked.set(walked.get() + 1));
}

/// How many characters were written out for parts on this thread.
#[cfg(test)]
pub(crate) fn walked_chars() -> u64 {
    WALKED.with(std::cell::Cell::get)
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use subspace_client_fleet::activity::ActivityError;

    use super::{index, piece, walked_chars};
    use crate::view::inspector::activity::blocks::{Block, Source, error_text};
    use crate::view::inspector::activity::parts::{LONG, PART};
    use crate::view::inspector::activity::size::measured;
    use crate::view::inspector::activity::text::visible;

    fn block<'a>(label: Option<&'static str>, source: Source<'a>) -> Block<'a> {
        Block {
            suffix: "b".to_owned(),
            label,
            source,
            long: LONG,
        }
    }

    fn failure() -> ActivityError {
        serde_json::from_value(json!({
            "type": "Overloaded", "message": "slow \u{202e} down", "status": 529,
            "retryAfterMs": 10, "responseBody": "body\u{0}".repeat(40)
        }))
        .unwrap()
    }

    /// SUB decision `0c13b784` §3: every byte of the body is in exactly
    /// one part, each drawing at most one part's bytes, markers and all.
    #[test]
    fn a_body_in_parts_keeps_every_byte_in_one_part_within_a_part() {
        let text = format!("{}\u{0}tail", "a\u{1b}".repeat(PART / 3));
        let value = json!({"k": "v".repeat(PART)});
        let blocks = [
            block(None, Source::Text(&text)),
            block(Some("Input"), Source::Json(&value)),
        ];
        let index = index(&blocks, PART);
        let mut joined = String::new();
        for wanted in 0..index.pieces() {
            let part = piece(&blocks, &index, wanted);
            assert_eq!((part.piece, part.pieces), (wanted, index.pieces()));
            assert!(part.text.len() <= PART, "{}", part.text.len());
            joined.push_str(&part.text);
        }
        let pretty = serde_json::to_string_pretty(&value).unwrap();
        let whole = format!("{}\n\nInput\n{pretty}", visible(&text));
        assert_eq!(joined, whole);
        assert_eq!(
            piece(&blocks, &index, 99).piece,
            index.pieces() - 1,
            "the last"
        );
    }

    /// Round 5 thermo (P2), SUB decision `33ccecf6` item 5: parts cut at
    /// every size, through labels, gaps, texts, values and a failure's
    /// fields, join to the body drawn whole.
    #[test]
    fn parts_of_any_size_join_to_the_whole_body() {
        let value = json!({"a": [1, "x\u{1}y", {"b": null}], "c\u{202e}": {}});
        let error = failure();
        let blocks = [
            block(Some("Status"), Source::Text("run\u{2028}ning")),
            block(None, Source::Json(&value)),
            block(Some("Error"), Source::Error(&error)),
            block(Some("Output"), Source::Text("")),
        ];
        let pretty = serde_json::to_string_pretty(&value).unwrap();
        let whole = visible(&format!(
            "Status\nrun\u{2028}ning\n\n{pretty}\n\nError\n{}\n\nOutput\n",
            error_text(&error)
        ));
        for size in [1, 12, 13, 50, 4096] {
            let index = index(&blocks, size);
            let joined: String = (0..index.pieces())
                .map(|wanted| piece(&blocks, &index, wanted).text)
                .collect();
            assert_eq!(joined, whole, "parts of {size}");
        }
    }

    /// Round 5 thermo (P2), SUB decision `33ccecf6` item 5: a body is
    /// scanned once per version; moving to a part, the last of a large
    /// value or failure included, writes out that part alone.
    #[test]
    fn moving_to_a_part_writes_out_that_part_alone() {
        let value = json!({"rows": (0..20_000).map(|row| json!({"n": row})).collect::<Vec<_>>()});
        let error = failure();
        let blocks = [
            block(None, Source::Json(&value)),
            block(Some("Error"), Source::Error(&error)),
        ];
        let index = index(&blocks, PART);
        assert!(index.pieces() > 4, "{}", index.pieces());
        let measured_once = measured();
        for wanted in [index.pieces() - 1, 2, 0, index.pieces() - 1] {
            let before = walked_chars();
            let part = piece(&blocks, &index, wanted);
            let walked = walked_chars() - before;
            assert!(walked <= PART as u64, "{walked} characters for one part");
            assert!(!part.text.is_empty() && part.text.len() <= PART);
        }
        assert_eq!(measured(), measured_once, "no part scans the body again");
    }
}
