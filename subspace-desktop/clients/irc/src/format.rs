//! mIRC formatting codes (bold, italics, underline, strikethrough, monospace,
//! reverse, colours) as styled spans.

/// One run of text with a single style.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Span {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikethrough: bool,
    pub monospace: bool,
    /// mIRC colour numbers 0–98.
    pub fg: Option<u8>,
    pub bg: Option<u8>,
}

const BOLD: char = '\u{02}';
const COLOUR: char = '\u{03}';
const HEX_COLOUR: char = '\u{04}';
const RESET: char = '\u{0f}';
const MONOSPACE: char = '\u{11}';
const REVERSE: char = '\u{16}';
const ITALIC: char = '\u{1d}';
const STRIKETHROUGH: char = '\u{1e}';
const UNDERLINE: char = '\u{1f}';
const CODES: [char; 9] = [
    BOLD,
    COLOUR,
    HEX_COLOUR,
    RESET,
    MONOSPACE,
    REVERSE,
    ITALIC,
    STRIKETHROUGH,
    UNDERLINE,
];
/// The highest real colour; 99 means "default".
const MAX_COLOUR: u8 = 98;
const MAX_COLOUR_DIGITS: usize = 2;
const HEX_DIGITS: usize = 6;

/// The styled runs of `text`, in order. Empty runs are omitted.
pub fn parse(text: &str) -> Vec<Span> {
    let mut builder = Builder::default();
    segments(text).into_iter().for_each(|segment| {
        let (code, rest) = split_code(segment);
        let (colours, rest) = read_colours(code, rest);
        builder.apply(code, colours);
        builder.push(rest);
    });
    builder.finish()
}

/// `text` without formatting codes.
pub fn strip(text: &str) -> String {
    parse(text).into_iter().map(|span| span.text).collect()
}

/// `text` split before every formatting code.
fn segments(text: &str) -> Vec<&str> {
    let mut starts: Vec<usize> = text
        .char_indices()
        .filter(|(_, c)| CODES.contains(c))
        .map(|(index, _)| index)
        .collect();
    starts.insert(0, 0);
    starts.push(text.len());
    starts
        .windows(2)
        .map(|pair| &text[pair[0]..pair[1]])
        .filter(|segment| !segment.is_empty())
        .collect()
}

/// The code a segment starts with (`None` for plain text) and the rest.
fn split_code(segment: &str) -> (Option<char>, &str) {
    let mut chars = segment.chars();
    match chars.next() {
        Some(c) if CODES.contains(&c) => (Some(c), chars.as_str()),
        _ => (None, segment),
    }
}

/// Colour arguments after a colour code: `Some((fg, bg))`, where `fg` is
/// `None` when the code had no digits (which resets the colours).
type Colours = Option<(Option<u8>, Option<u8>)>;

fn read_colours(code: Option<char>, rest: &str) -> (Colours, &str) {
    match code {
        Some(COLOUR) => {
            let (fg, rest) = digits(rest);
            let (bg, rest) = match (fg, rest.strip_prefix(',')) {
                (Some(_), Some(after)) if after.starts_with(|c: char| c.is_ascii_digit()) => {
                    digits(after)
                }
                _ => (None, rest),
            };
            (Some((fg, bg)), rest)
        }
        Some(HEX_COLOUR) => {
            let rest = skip_hex(rest);
            let rest = rest.strip_prefix(',').map_or(rest, skip_hex);
            (Some((None, None)), rest)
        }
        _ => (None, rest),
    }
}

/// Up to two leading digits as a colour number.
fn digits(text: &str) -> (Option<u8>, &str) {
    let len = text
        .chars()
        .take(MAX_COLOUR_DIGITS)
        .take_while(char::is_ascii_digit)
        .count();
    let value = text[..len].parse::<u8>().ok();
    (value, &text[len..])
}

fn skip_hex(text: &str) -> &str {
    let len = text
        .chars()
        .take(HEX_DIGITS)
        .take_while(char::is_ascii_hexdigit)
        .count();
    match len == HEX_DIGITS {
        true => &text[len..],
        false => text,
    }
}

#[derive(Default)]
struct Builder {
    spans: Vec<Span>,
    style: Span,
}

impl Builder {
    fn apply(&mut self, code: Option<char>, colours: Colours) {
        let style = &mut self.style;
        match code {
            Some(BOLD) => style.bold = !style.bold,
            Some(ITALIC) => style.italic = !style.italic,
            Some(UNDERLINE) => style.underline = !style.underline,
            Some(STRIKETHROUGH) => style.strikethrough = !style.strikethrough,
            Some(MONOSPACE) => style.monospace = !style.monospace,
            Some(REVERSE) => std::mem::swap(&mut style.fg, &mut style.bg),
            Some(RESET) => *style = Span::default(),
            _ => {}
        }
        match colours {
            Some((None, _)) => (style.fg, style.bg) = (None, None),
            Some((Some(fg), bg)) => {
                style.fg = (fg <= MAX_COLOUR).then_some(fg);
                if let Some(bg) = bg {
                    style.bg = (bg <= MAX_COLOUR).then_some(bg);
                }
            }
            None => {}
        }
    }

    fn push(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        // `style` never holds text, so equal styles compare equal here.
        match self.spans.last_mut() {
            Some(last)
                if Span {
                    text: String::new(),
                    ..last.clone()
                } == self.style =>
            {
                last.text.push_str(text);
            }
            _ => self.spans.push(Span {
                text: text.to_string(),
                ..self.style.clone()
            }),
        }
    }

    fn finish(self) -> Vec<Span> {
        self.spans
    }
}

#[cfg(test)]
mod tests;
