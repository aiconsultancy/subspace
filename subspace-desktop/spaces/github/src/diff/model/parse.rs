use std::ops::Range;

use similar::{ChangeTag, TextDiff};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    Context,
    Add,
    Del,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffLine {
    pub kind: LineKind,
    pub old: Option<u32>,
    pub new: Option<u32>,
    pub text: String,
    pub words: Vec<Range<usize>>,
    pub no_newline: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hunk {
    pub header: String,
    pub old_start: u32,
    pub old_len: u32,
    pub new_start: u32,
    pub new_len: u32,
    pub lines: Range<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Patch {
    pub hunks: Vec<Hunk>,
    pub lines: Vec<DiffLine>,
}

fn parse_header(line: &str) -> Option<(u32, u32, u32, u32)> {
    let rest = line.strip_prefix("@@ -")?;
    let (ranges, _) = rest.split_once(" @@")?;
    let (old, new) = ranges.split_once(" +")?;
    let range = |text: &str| match text.split_once(',') {
        Some((start, len)) => Some((start.parse().ok()?, len.parse().ok()?)),
        None => Some((text.parse().ok()?, 1)),
    };
    let (old_start, old_len) = range(old)?;
    let (new_start, new_len) = range(new)?;
    Some((old_start, old_len, new_start, new_len))
}

fn annotate_words(patch: &mut Patch) {
    for (del, add) in change_pairs(&patch.lines) {
        if let Some((old_words, new_words)) =
            word_ranges(&patch.lines[del].text, &patch.lines[add].text)
        {
            patch.lines[del].words = old_words;
            patch.lines[add].words = new_words;
        }
    }
}

pub fn parse_patch(patch: &str) -> Patch {
    PatchParser::parse(patch)
}

#[derive(Default)]
struct PatchParser {
    out: Patch,
    old: u32,
    new: u32,
}

impl PatchParser {
    fn parse(patch: &str) -> Patch {
        let mut parser = Self::default();
        patch.lines().for_each(|raw| parser.read_line(raw));
        parser.finish()
    }

    fn read_line(&mut self, raw: &str) {
        if let Some((old_start, old_len, new_start, new_len)) = parse_header(raw) {
            if let Some(last) = self.out.hunks.last_mut() {
                last.lines.end = self.out.lines.len();
            }
            let line = self.out.lines.len();
            self.out.hunks.push(Hunk {
                header: raw.to_string(),
                old_start,
                old_len,
                new_start,
                new_len,
                lines: line..line,
            });
            (self.old, self.new) = (old_start, new_start);
            return;
        }
        if self.out.hunks.is_empty() {
            return;
        }
        let Some((kind, text)) = (match raw.as_bytes().first() {
            Some(b'+') => Some((LineKind::Add, &raw[1..])),
            Some(b'-') => Some((LineKind::Del, &raw[1..])),
            Some(b' ') => Some((LineKind::Context, &raw[1..])),
            Some(b'\\') => {
                if let Some(last) = self.out.lines.last_mut() {
                    last.no_newline = true;
                }
                None
            }
            None => Some((LineKind::Context, "")),
            Some(_) => None,
        }) else {
            return;
        };
        let (old_no, new_no) = match kind {
            LineKind::Add => (None, Some(self.new)),
            LineKind::Del => (Some(self.old), None),
            LineKind::Context => (Some(self.old), Some(self.new)),
        };
        if kind != LineKind::Add {
            self.old += 1;
        }
        if kind != LineKind::Del {
            self.new += 1;
        }
        self.out.lines.push(DiffLine {
            kind,
            old: old_no,
            new: new_no,
            text: text.to_string(),
            words: Vec::new(),
            no_newline: false,
        });
    }

    fn finish(mut self) -> Patch {
        if let Some(last) = self.out.hunks.last_mut() {
            last.lines.end = self.out.lines.len();
        }
        annotate_words(&mut self.out);
        self.out
    }
}

pub fn change_pairs(lines: &[DiffLine]) -> Vec<(usize, usize)> {
    let mut pairs = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        if lines[index].kind != LineKind::Del {
            index += 1;
            continue;
        }
        let dels = index;
        while index < lines.len() && lines[index].kind == LineKind::Del {
            index += 1;
        }
        let adds = index;
        while index < lines.len() && lines[index].kind == LineKind::Add {
            index += 1;
        }
        let count = (adds - dels).min(index - adds);
        pairs.extend((0..count).map(|i| (dels + i, adds + i)));
    }
    pairs
}

const WORD_RANGE_MAX_BYTES: usize = 2000;
const WORD_SIMILARITY_MIN: f32 = 0.25;

pub fn word_ranges(old: &str, new: &str) -> Option<(Vec<Range<usize>>, Vec<Range<usize>>)> {
    if old.len() + new.len() > WORD_RANGE_MAX_BYTES {
        return None;
    }
    let diff = TextDiff::from_words(old, new);
    let (mut old_at, mut new_at) = (0usize, 0usize);
    let (mut old_ranges, mut new_ranges) = (Vec::new(), Vec::new());
    let mut same = 0usize;
    for change in diff.iter_all_changes() {
        let value = change.value();
        let len = value.len();
        match change.tag() {
            ChangeTag::Equal => {
                same += value.chars().filter(|c| !c.is_whitespace()).count();
                old_at += len;
                new_at += len;
            }
            ChangeTag::Delete => {
                old_ranges.push(old_at..old_at + len);
                old_at += len;
            }
            ChangeTag::Insert => {
                new_ranges.push(new_at..new_at + len);
                new_at += len;
            }
        }
    }
    let visible = |text: &str| text.chars().filter(|c| !c.is_whitespace()).count();
    let total = visible(old) + visible(new);
    if total == 0 || (2 * same) as f32 / (total as f32) < WORD_SIMILARITY_MIN {
        return None;
    }
    Some((merge_words(old, old_ranges), merge_words(new, new_ranges)))
}

fn merge_words(text: &str, ranges: Vec<Range<usize>>) -> Vec<Range<usize>> {
    let mut out: Vec<Range<usize>> = Vec::new();
    for range in ranges {
        match out.last_mut() {
            Some(last) if text[last.end..range.start].trim().is_empty() => last.end = range.end,
            _ => out.push(range),
        }
    }
    out.retain(|range| !text[range.clone()].trim().is_empty());
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Gap {
    pub old_start: u32,
    pub new_start: u32,
    pub len: Option<u32>,
}

fn gap(old_start: u32, new_start: u32, len: Option<u32>) -> Gap {
    Gap {
        old_start,
        new_start,
        len,
    }
}

pub fn gaps(hunks: &[Hunk], new_lines: Option<u32>) -> Vec<Gap> {
    let mut out = Vec::with_capacity(hunks.len() + 1);
    let (mut old_next, mut new_next) = (1u32, 1u32);
    for hunk in hunks {
        let new_start = hunk.new_start.max(1);
        let len = new_start.saturating_sub(new_next);
        out.push(gap(
            old_next,
            new_next,
            Some(if hunk.new_len == 0 && hunk.new_start == 0 {
                0
            } else {
                len
            }),
        ));
        old_next = hunk.old_start + hunk.old_len;
        new_next = hunk.new_start + hunk.new_len;
        if hunk.old_len == 0 {
            old_next = hunk.old_start + 1;
        }
        if hunk.new_len == 0 {
            new_next = hunk.new_start + 1;
        }
    }
    out.push(gap(
        old_next,
        new_next,
        new_lines.map(|total| (total + 1).saturating_sub(new_next)),
    ));
    out
}
