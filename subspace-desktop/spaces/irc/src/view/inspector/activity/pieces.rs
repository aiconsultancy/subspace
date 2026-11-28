//! A body drawn one part at a time (SUB decisions `0c13b784` §3 and
//! `33ccecf6` item 5): each block's label on a line of its own above its
//! text, the blocks a blank line apart, cut into parts drawing at most a
//! part's bytes, markers and all, never inside a character or its marker.
//!
//! One pass over a version of the body finds where its parts start and
//! keeps some of them, never more than a few kilobytes ([`Index`]);
//! moving to a part then walks from the nearest start kept before it, at
//! most one stride of parts, and writes out only that part: a value is
//! walked from the middle as indented JSON ([`json`]), a text sliced, an
//! error's fields read one by one. Nothing before that start is formatted
//! or written out again (SUB decision `4311830a` item 5). To resume inside
//! a value, the walk steps over the earlier members of each object on the
//! path to that start, since a map offers no way to reach a member by
//! position: counted, allocation-free iterator steps, never copied keys.
//! The bound is one stride of parts plus those steps (SUB decision
//! `1cdd7607`).

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

/// The most bytes an [`Index`] keeps, charged to the read or plan that
/// draws its body (SUB decision `4311830a` item 5).
pub(crate) const INDEX_BYTES: usize = 4096;

/// Where some of the parts of one version of a body start, for parts
/// drawing at most `size` bytes: every `stride`-th, sparser as the body
/// grows so that it never keeps more than [`INDEX_BYTES`].
pub(super) struct Index {
    pub(super) size: usize,
    pieces: usize,
    /// The parts from one kept start to the next.
    stride: usize,
    /// Where parts `0`, `stride`, `2 × stride`, … start.
    marks: Vec<Mark>,
}

impl Index {
    pub(super) fn pieces(&self) -> usize {
        self.pieces
    }

    /// The bytes it keeps: never more than [`INDEX_BYTES`].
    pub(super) fn bytes(&self) -> u64 {
        let marks = self.marks.capacity() * size_of::<Mark>();
        let paths: usize = self.marks.iter().map(|mark| mark.at.heap()).sum();
        (size_of::<Self>() + marks + paths) as u64
    }

    /// Counts a part starting at `here`, keeping where it starts when it is
    /// a `stride`-th one, then thins what it keeps while it is full.
    fn add(&mut self, here: Mark) {
        if self.pieces.is_multiple_of(self.stride) {
            self.marks.push(here);
        }
        self.pieces += 1;
        while self.full() {
            self.thin();
        }
    }

    /// Whether it keeps as many starts as it holds room for, or more than
    /// [`INDEX_BYTES`], with more than the first.
    fn full(&self) -> bool {
        let over = self.marks.len() >= MARKS || self.bytes() > INDEX_BYTES as u64;
        over && self.marks.len() > 1
    }

    /// Keeps every other start, twice as far apart.
    fn thin(&mut self) {
        self.stride *= 2;
        let mut at = 0;
        self.marks.retain(|_| {
            at += 1;
            at % 2 == 1
        });
    }
}

/// The most part starts an [`Index`] holds room for: its fixed capacity.
const MARKS: usize = 32;

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
    /// The bytes it keeps beyond itself.
    fn heap(&self) -> usize {
        match self {
            Self::Json(mark) => mark.heap(),
            _ => 0,
        }
    }

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

/// Finds where the parts of `blocks` start, for parts of at most `size`
/// bytes drawn, in one pass, keeping some of them ([`Index`]).
pub(super) fn index(blocks: &[Block<'_>], size: usize) -> Index {
    count_measure();
    let start = Mark {
        block: 0,
        at: At::Gap(0),
    };
    let mut index = Index {
        size,
        pieces: 0,
        stride: 1,
        marks: Vec::with_capacity(MARKS),
    };
    index.add(start.clone());
    let mut len = 0;
    walk(blocks, &start, &mut |ch, here| {
        let unit = unit_len(ch);
        if len > 0 && len + unit > size {
            index.add(here());
            len = 0;
        }
        len += unit;
        true
    });
    index
}

/// Part `wanted` of `blocks`, or its last part when it has fewer: walked
/// from the nearest start `index` keeps before it, at most one stride of
/// parts, and written out no further than its end.
pub(super) fn piece(blocks: &[Block<'_>], index: &Index, wanted: usize) -> Piece {
    let piece = wanted.min(index.pieces().saturating_sub(1));
    let mut text = String::new();
    let (mut part, mut len) = (piece - piece % index.stride, 0);
    if let Some(start) = index.marks.get(piece / index.stride) {
        walk(blocks, start, &mut |ch, _| {
            let unit = unit_len(ch);
            if len > 0 && len + unit > index.size {
                if part == piece {
                    return false;
                }
                (part, len) = (part + 1, 0);
            }
            len += unit;
            walked();
            if part == piece {
                push_visible(&mut text, ch);
            }
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
    stepped(1);
}

/// Counts `steps` of the work of moving to a part: characters walked, or
/// the members of an object stepped over to resume inside it, which
/// serde_json's map offers no way to reach by position.
fn stepped(steps: usize) {
    #[cfg(test)]
    WALKED.with(|walked| walked.set(walked.get() + steps as u64));
    #[cfg(not(test))]
    let _ = steps;
}

/// How much work moving to parts took on this thread: characters walked
/// and members stepped over.
#[cfg(test)]
pub(crate) fn walked_chars() -> u64 {
    WALKED.with(std::cell::Cell::get)
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use subspace_client_fleet::activity::ActivityError;

    use super::{INDEX_BYTES, MARKS, index, piece, walked_chars};
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

    /// Round 6 thermo (P2 4), SUB decision `4311830a` item 5: however many
    /// parts a body is cut in, its index keeps no more than its fixed cap,
    /// sparser as the body grows, and still reaches every part.
    #[test]
    fn an_index_keeps_no_more_than_its_cap_however_many_parts() {
        // Phobos's fixture: a value of about 4 KB in parts of a character.
        let value = json!({"rows": (0..400).map(|row| json!({"n": row})).collect::<Vec<_>>()});
        let blocks = [block(None, Source::Json(&value))];
        let pretty = serde_json::to_string_pretty(&value).unwrap();
        let index = index(&blocks, 1);
        assert_eq!(index.pieces(), pretty.chars().count());
        assert!(
            index.bytes() <= INDEX_BYTES as u64,
            "{} bytes",
            index.bytes()
        );
        assert!(index.marks.len() < MARKS && index.marks.capacity() <= 2 * MARKS);
        let joined: String = (0..index.pieces())
            .map(|wanted| piece(&blocks, &index, wanted).text)
            .collect();
        assert_eq!(joined, pretty);
    }

    /// Round 6 thermo (P2 4), SUB decision `4311830a` item 5: moving to a
    /// part walks from the nearest start the index keeps before it, at
    /// most one stride of parts.
    #[test]
    fn moving_to_a_part_walks_at_most_one_stride_of_parts() {
        let text = "t".repeat(10_000);
        let blocks = [block(None, Source::Text(&text))];
        let index = index(&blocks, 10);
        assert_eq!(index.pieces(), 1_000);
        assert!(index.stride > 1 && index.stride * index.marks.len() >= 1_000);
        for wanted in [999, 0, 517, 998] {
            let before = walked_chars();
            let part = piece(&blocks, &index, wanted);
            let walked = walked_chars() - before;
            assert!(
                walked <= (index.stride * 10) as u64,
                "{walked} for part {wanted}"
            );
            assert_eq!(part.text, "t".repeat(10));
        }
    }

    /// Round 6 thermo (P2 4), SUB decision `4311830a` item 5: the index of
    /// Phobos's value of about 4 KB in parts of 24 bytes keeps no more than
    /// its cap, nor does one of a value a hundred times larger; moving to
    /// any part walks at most one stride of parts, plus the members of the
    /// object it resumes in stepped over, never the value from its start.
    #[test]
    fn the_work_of_moving_to_a_part_is_one_stride_and_the_members_stepped_over() {
        let small: serde_json::Map<_, _> =
            (0..300).map(|n| (format!("k{n:03}"), json!(n))).collect();
        let large: serde_json::Map<_, _> = (0..30_000)
            .map(|n| (format!("k{n:05}"), json!(n)))
            .collect();
        for (members, value) in [(300, small), (30_000, large)] {
            let value = serde_json::Value::Object(value);
            let blocks = [block(None, Source::Json(&value))];
            let pretty = serde_json::to_string_pretty(&value).unwrap();
            let index = index(&blocks, 24);
            assert!(index.bytes() <= INDEX_BYTES as u64, "{}", index.bytes());
            let pieces = index.pieces();
            for wanted in [pieces - 1, pieces / 2, 1, pieces - 2] {
                let before = walked_chars();
                let part = piece(&blocks, &index, wanted);
                let work = walked_chars() - before;
                let bound = (index.stride * 24 + members) as u64;
                assert!(work <= bound, "{work} for part {wanted} of {pieces}");
                assert!(work < pretty.len() as u64 / 2, "{work}: not from the start");
                assert!(pretty.contains(&part.text) && !part.text.is_empty());
            }
        }
    }
}
