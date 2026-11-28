//! A JSON value written out as indented JSON, exactly as
//! `serde_json::to_string_pretty` writes it, one character at a time from
//! any point named by a [`Mark`]: drawing a part starts where the part
//! does, walking down to it by index, and formats nothing before it.

use serde_json::Value;

/// A point in a value's indented JSON: the token it is in, by the path of
/// child indices down to it, and how far into that token.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Mark {
    /// The child index at each depth: down to the element for
    /// [`Token::Start`], to the child being introduced for [`Token::Sep`],
    /// [`Token::Key`] and [`Token::Colon`], and to the container for
    /// [`Token::Close`].
    path: Vec<usize>,
    token: Token,
    /// The byte in a plain token; in a string, its unit: 0 for the opening
    /// quote, `1 + b` for the character at byte `b`, then the closing quote.
    unit: usize,
    /// The byte within a string unit's escape.
    sub: usize,
}

impl Mark {
    /// The bytes it keeps beyond itself: its path.
    pub(super) fn heap(&self) -> usize {
        self.path.capacity() * size_of::<usize>()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Token {
    /// A scalar, a string, an empty container or an opening bracket.
    Start,
    /// The line break and indent before a child.
    Sep,
    /// A field's key, quoted.
    Key,
    /// `: ` after a key.
    Colon,
    /// The line break, indent and closing bracket of a container.
    Close,
}

/// Each character of `root` written out, from `from` on (from its start
/// when `None`), given to `take` with a way to name where it is. Stops,
/// returning `false`, as soon as `take` does.
pub(super) fn walk(
    root: &Value,
    from: Option<&Mark>,
    take: &mut dyn FnMut(char, &dyn Fn() -> Mark) -> bool,
) -> bool {
    let mut walker = Walker {
        frames: Vec::new(),
        take,
    };
    let started = match from {
        None => walker.start(root, (0, 0)),
        Some(mark) => walker.resume(root, mark),
    };
    started && walker.run()
}

/// The children of a container still to come.
enum Children<'v> {
    Array(std::slice::Iter<'v, Value>),
    Object(serde_json::map::Iter<'v>),
    /// None left: a container resumed at its closing bracket.
    Done,
}

/// Each child still to come, with its key in an object.
impl<'v> Iterator for Children<'v> {
    type Item = (Option<&'v str>, &'v Value);

    fn next(&mut self) -> Option<Self::Item> {
        self.nth(0)
    }

    /// Skips the children before child `n`: an array's at once, an
    /// object's one by one, counted as work ([`super::stepped`]), since
    /// serde_json's map offers no way to reach a member by position.
    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        match self {
            Self::Array(items) => items.nth(n).map(|item| (None, item)),
            Self::Object(fields) => {
                super::stepped(n);
                fields
                    .nth(n)
                    .map(|(key, value)| (Some(key.as_str()), value))
            }
            Self::Done => None,
        }
    }
}

/// A container being written out.
struct Frame<'v> {
    children: Children<'v>,
    /// How many children were taken: the current one is `taken - 1`.
    taken: usize,
    close: char,
}

impl<'v> Frame<'v> {
    /// `value` entered, when it is a container with children.
    fn of(value: &'v Value) -> Option<Self> {
        let (children, close) = match value {
            Value::Array(items) if !items.is_empty() => (Children::Array(items.iter()), ']'),
            Value::Object(fields) if !fields.is_empty() => (Children::Object(fields.iter()), '}'),
            _ => return None,
        };
        Some(Self {
            children,
            taken: 0,
            close,
        })
    }

    /// `value` entered at child `index`, with that child and its key.
    fn at(value: &'v Value, index: usize) -> Option<(Self, Option<&'v str>, &'v Value)> {
        Self::of(value).and_then(|mut frame| {
            let (key, child) = frame.children.nth(index)?;
            frame.taken = index + 1;
            Some((frame, key, child))
        })
    }
}

/// How serde_json opens `value`, a container: empty, or its bracket.
fn opening(value: &Value) -> &'static str {
    match value {
        Value::Array(items) if items.is_empty() => "[]",
        Value::Object(fields) if fields.is_empty() => "{}",
        Value::Array(_) => "[",
        _ => "{",
    }
}

/// The room serde_json's longest escape, `\u001f`, needs, and any
/// character's UTF-8.
const ESCAPE: usize = 8;

/// The first character serde_json writes in a string as itself, not as a
/// `\u00XX` escape.
const UNESCAPED: u32 = 0x20;

struct Walker<'v, 't> {
    frames: Vec<Frame<'v>>,
    take: &'t mut dyn FnMut(char, &dyn Fn() -> Mark) -> bool,
}

/// `depth` levels of serde_json's two-space indent.
fn indent(depth: usize) -> String {
    "  ".repeat(depth)
}

/// The closing bracket of `value`, a container.
fn close_of(value: &Value) -> char {
    if value.is_array() { ']' } else { '}' }
}

/// `ch` as serde_json escapes it inside a string, written into `buf`.
fn escaped(ch: char, buf: &mut [u8; ESCAPE]) -> &str {
    let short = match ch {
        '"' => Some("\\\""),
        '\\' => Some("\\\\"),
        '\u{8}' => Some("\\b"),
        '\u{c}' => Some("\\f"),
        '\n' => Some("\\n"),
        '\r' => Some("\\r"),
        '\t' => Some("\\t"),
        _ => None,
    };
    if let Some(short) = short {
        buf[..short.len()].copy_from_slice(short.as_bytes());
        return std::str::from_utf8(&buf[..short.len()]).unwrap_or_default();
    }
    let code = u32::from(ch);
    if code < UNESCAPED {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        buf[..4].copy_from_slice(b"\\u00");
        buf[4] = HEX[(code >> 4) as usize];
        buf[5] = HEX[(code & 0xF) as usize];
        return std::str::from_utf8(&buf[..6]).unwrap_or_default();
    }
    ch.encode_utf8(buf)
}

impl<'v> Walker<'v, '_> {
    /// Where the walk is, in token `token` at `(unit, sub)`.
    fn mark(frames: &[Frame<'_>], token: Token, (unit, sub): (usize, usize)) -> Mark {
        let depth = match token {
            Token::Close => frames.len().saturating_sub(1),
            _ => frames.len(),
        };
        Mark {
            path: frames[..depth]
                .iter()
                .map(|frame| frame.taken - 1)
                .collect(),
            token,
            unit,
            sub,
        }
    }

    /// Gives `take` the rest of the plain token `text` from byte `from`.
    fn plain(&mut self, token: Token, text: &str, from: usize) -> bool {
        let frames = &self.frames;
        let take = &mut self.take;
        text.get(from..)
            .unwrap_or_default()
            .char_indices()
            .all(|(at, ch)| take(ch, &|| Self::mark(frames, token, (from + at, 0))))
    }

    /// Gives `take` the rest of the string `text`, quoted and escaped, from
    /// unit `unit`, byte `sub` of its escape.
    fn string(&mut self, token: Token, text: &str, (unit, sub): (usize, usize)) -> bool {
        let frames = &self.frames;
        let take = &mut self.take;
        let mut unit_chars = |unit: usize, out: &str, from: usize| {
            out.get(from..)
                .unwrap_or_default()
                .char_indices()
                .all(|(at, ch)| take(ch, &|| Self::mark(frames, token, (unit, from + at))))
        };
        if unit == 0 && !unit_chars(0, "\"", sub) {
            return false;
        }
        let begin = unit.saturating_sub(1).min(text.len());
        let mut buf = [0; ESCAPE];
        for (at, ch) in text.get(begin..).unwrap_or_default().char_indices() {
            let from = if unit > 0 && at == 0 { sub } else { 0 };
            if !unit_chars(1 + begin + at, escaped(ch, &mut buf), from) {
                return false;
            }
        }
        let end = text.len() + 1;
        let from = if unit == end { sub } else { 0 };
        unit_chars(end, "\"", from)
    }

    /// Writes the start of `value` from `at`, and enters it when it is a
    /// container with children.
    fn start(&mut self, value: &'v Value, at: (usize, usize)) -> bool {
        match value {
            Value::String(text) => self.string(Token::Start, text, at),
            Value::Array(_) | Value::Object(_) => self.open(value, at.0),
            scalar => self.plain(Token::Start, &scalar.to_string(), at.0),
        }
    }

    /// Writes the opening of `value`, a container, from byte `from`, and
    /// enters it when it has children.
    fn open(&mut self, value: &'v Value, from: usize) -> bool {
        let opened = self.plain(Token::Start, opening(value), from);
        self.frames.extend(Frame::of(value));
        opened
    }

    /// Writes what introduces the current child of the innermost
    /// container from token `token` at `at`: its line break and indent,
    /// then its key and `: ` in an object.
    fn introduce(&mut self, key: Option<&str>, token: Token, at: (usize, usize)) -> bool {
        let (taken, depth) = (
            self.frames.last().map_or(0, |frame| frame.taken),
            self.frames.len(),
        );
        let from = |here: Token| if here == token { at } else { (0, 0) };
        if token == Token::Sep {
            let sep = if taken == 1 { "\n" } else { ",\n" };
            if !self.plain(Token::Sep, &format!("{sep}{}", indent(depth)), at.0) {
                return false;
            }
        }
        let Some(key) = key else {
            return true;
        };
        (token == Token::Colon || self.string(Token::Key, key, from(Token::Key)))
            && self.plain(Token::Colon, ": ", from(Token::Colon).0)
    }

    /// Writes the closing of the innermost container from byte `from`, and
    /// leaves it.
    fn close(&mut self, from: usize) -> bool {
        let Some(close) = self.frames.last().map(|frame| frame.close) else {
            return true;
        };
        let text = format!("\n{}{close}", indent(self.frames.len() - 1));
        let done = self.plain(Token::Close, &text, from);
        self.frames.pop();
        done
    }

    /// Writes everything after where the walk is.
    fn run(&mut self) -> bool {
        while let Some(frame) = self.frames.last_mut() {
            let Some((key, child)) = frame.children.next() else {
                if !self.close(0) {
                    return false;
                }
                continue;
            };
            frame.taken += 1;
            if !self.introduce(key, Token::Sep, (0, 0)) || !self.start(child, (0, 0)) {
                return false;
            }
        }
        true
    }

    /// Walks down `mark`'s path, then writes the rest of its token.
    fn resume(&mut self, root: &'v Value, mark: &Mark) -> bool {
        self.descend(root, &mark.path)
            .is_none_or(|(value, key)| self.resume_at(value, key, mark))
    }

    /// Enters each container down `path` from `root`: the value at its
    /// end, with its key, or `None` when the path is not in `root`.
    fn descend(&mut self, root: &'v Value, path: &[usize]) -> Option<(&'v Value, Option<&'v str>)> {
        path.iter().try_fold((root, None), |(value, _), &index| {
            let (frame, key, child) = Frame::at(value, index)?;
            self.frames.push(frame);
            Some((child, key))
        })
    }

    /// Writes the rest of `mark`'s token in `value`, keyed `key`, then
    /// what follows it.
    fn resume_at(&mut self, value: &'v Value, key: Option<&str>, mark: &Mark) -> bool {
        let at = (mark.unit, mark.sub);
        match mark.token {
            Token::Start => self.start(value, at),
            Token::Close => self.reclose(value, mark.unit),
            token => self.introduce(key, token, at) && self.start(value, (0, 0)),
        }
    }

    /// Writes the closing of `value`, a container with every child
    /// written, from byte `from`.
    fn reclose(&mut self, value: &Value, from: usize) -> bool {
        self.frames.push(Frame {
            children: Children::Done,
            taken: 1,
            close: close_of(value),
        });
        self.close(from)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::{Mark, walk};

    /// Every character of `value` with where it is.
    fn marked(value: &Value) -> Vec<(char, Mark)> {
        let mut all = Vec::new();
        walk(value, None, &mut |ch, here| {
            all.push((ch, here()));
            true
        });
        all
    }

    /// Written out from the start, a value is serde_json's indented JSON;
    /// resumed from any character's mark, it is the rest of it.
    #[test]
    fn a_value_resumes_from_any_character_as_serde_json_writes_it() {
        let value = json!({
            "a\u{1}\"\\\n\t\u{202e}é🐈": [1, -2.5, true, null, "x\u{1f}y", [], {}, [[]], {"k": {}}],
            "b": {"c": "", "d": [{"e": "f"}]},
            "": "last"
        });
        let pretty = serde_json::to_string_pretty(&value).unwrap();
        let all = marked(&value);
        assert_eq!(all.iter().map(|(ch, _)| *ch).collect::<String>(), pretty);
        let chars: Vec<char> = pretty.chars().collect();
        for (at, (_, mark)) in all.iter().enumerate() {
            let mut rest = String::new();
            walk(&value, Some(mark), &mut |ch, _| {
                rest.push(ch);
                true
            });
            assert_eq!(rest, chars[at..].iter().collect::<String>(), "{mark:?}");
        }
    }

    /// Scalars, strings and empty containers alone, as roots.
    #[test]
    fn a_scalar_root_is_written_as_serde_json_writes_it() {
        for value in [json!(null), json!(7), json!("q\""), json!([]), json!({})] {
            let written: String = marked(&value).into_iter().map(|(ch, _)| ch).collect();
            assert_eq!(written, serde_json::to_string_pretty(&value).unwrap());
        }
    }
}
