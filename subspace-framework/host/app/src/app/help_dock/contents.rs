//! Contents for bundled board help. The prepared Markdown is split before each
//! H1 and H2 heading, so the Help panel can render every part as a child of its
//! scroll container and scroll a heading to the top by child index.
//!
//! Only ATX headings (`#` and `##`) are boundaries. H3 and deeper headings stay
//! inside their part. Headings in fenced code blocks are text. Link reference
//! definitions do not cross a boundary; bundled help does not use them.

/// The fewest H1 and H2 headings that make a contents list useful.
const MIN_CONTENTS_HEADINGS: usize = 3;
/// CommonMark allows up to three spaces before a heading or a fence.
const MAX_BLOCK_INDENT: usize = 3;
/// A code fence is at least three backticks or tildes.
const MIN_FENCE_LENGTH: usize = 3;

/// The heading levels that the contents list shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum HeadingLevel {
    One,
    Two,
}

/// One contents entry. `section` is the index of the part that starts with
/// this heading, so two headings with the same title have different targets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Heading {
    pub(super) level: HeadingLevel,
    pub(super) title: String,
    pub(super) section: usize,
}

/// A help document split into parts at its H1 and H2 headings.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct HelpSections {
    sections: Vec<String>,
    headings: Vec<Heading>,
}

impl HelpSections {
    /// Splits prepared Markdown. Joining the parts gives the document again,
    /// less any blank lines before the first heading.
    pub(super) fn parse(markdown: &str) -> Self {
        markdown
            .split_inclusive('\n')
            .map(|line| (line, fence(line), atx_heading(line)))
            .fold(Splitter::default(), |splitter, (line, fence, heading)| {
                splitter.push(line, fence, heading)
            })
            .finish()
    }

    /// The Markdown parts in document order.
    pub(super) fn sections(&self) -> &[String] {
        &self.sections
    }

    /// The H1 and H2 headings in document order.
    pub(super) fn headings(&self) -> &[Heading] {
        &self.headings
    }

    /// The panel shows contents only for a document with enough headings.
    pub(super) fn shows_contents(&self) -> bool {
        self.headings.len() >= MIN_CONTENTS_HEADINGS
    }

    /// The heading that starts `section`, if any.
    pub(super) fn heading_for(&self, section: usize) -> Option<&Heading> {
        self.headings
            .iter()
            .find(|heading| heading.section == section)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Fence {
    marker: char,
    length: usize,
    bare: bool,
}

struct RawHeading<'a> {
    level: HeadingLevel,
    text: &'a str,
    section: usize,
}

#[derive(Default)]
struct Splitter<'a> {
    sections: Vec<String>,
    current: String,
    headings: Vec<RawHeading<'a>>,
    open_fence: Option<Fence>,
}

impl<'a> Splitter<'a> {
    fn push(
        mut self,
        line: &str,
        fence: Option<Fence>,
        heading: Option<(HeadingLevel, &'a str)>,
    ) -> Self {
        match (self.open_fence, fence, heading) {
            (Some(open), Some(close), _)
                if close.bare && close.marker == open.marker && close.length >= open.length =>
            {
                self.open_fence = None;
            }
            (None, Some(open), _) => self.open_fence = Some(open),
            (None, None, Some((level, text))) => {
                if !self.current.trim().is_empty() {
                    self.sections.push(std::mem::take(&mut self.current));
                }
                self.current.clear();
                self.headings.push(RawHeading {
                    level,
                    text,
                    section: self.sections.len(),
                });
            }
            _ => {}
        }
        self.current.push_str(line);
        self
    }

    fn finish(mut self) -> HelpSections {
        if !self.current.trim().is_empty() {
            self.sections.push(self.current);
        }
        HelpSections {
            sections: self.sections,
            headings: self
                .headings
                .into_iter()
                .map(|raw| Heading {
                    level: raw.level,
                    title: plain_title(raw.text),
                    section: raw.section,
                })
                .collect(),
        }
    }
}

/// The block text of `line` when it is indented by at most three spaces.
fn block_text(line: &str) -> Option<&str> {
    let line = line.trim_end_matches(['\n', '\r']);
    let text = line.trim_start_matches(' ');
    (line.len() - text.len() <= MAX_BLOCK_INDENT).then_some(text)
}

/// The code fence that `line` opens or closes, if any.
fn fence(line: &str) -> Option<Fence> {
    let text = block_text(line)?;
    let marker = text.chars().next().filter(|c| matches!(c, '`' | '~'))?;
    let rest = text.trim_start_matches(marker);
    let length = text.len() - rest.len();
    (length >= MIN_FENCE_LENGTH).then(|| Fence {
        marker,
        length,
        bare: rest.trim().is_empty(),
    })
}

/// The level and raw text of an H1 or H2 ATX heading.
fn atx_heading(line: &str) -> Option<(HeadingLevel, &str)> {
    let text = block_text(line)?;
    let rest = text.trim_start_matches('#');
    if !(rest.is_empty() || rest.starts_with([' ', '\t'])) {
        return None;
    }
    match text.len() - rest.len() {
        1 => Some((HeadingLevel::One, rest)),
        2 => Some((HeadingLevel::Two, rest)),
        _ => None,
    }
}

/// The heading text a reader sees: no closing `#` sequence, emphasis, code
/// marks, link targets, images or HTML tags, with whitespace collapsed.
pub(super) fn plain_title(raw: &str) -> String {
    let text = without_closing_sequence(raw);
    let characters: Vec<char> = text.chars().collect();
    let plain = (0..characters.len())
        .map(|at| InlineChar {
            previous: at.checked_sub(1).map(|before| characters[before]),
            current: characters[at],
            next: characters.get(at + 1).copied(),
        })
        .fold(PlainText::default(), PlainText::push);
    plain.text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Removes an ATX closing sequence: trailing `#` characters after a space.
fn without_closing_sequence(raw: &str) -> &str {
    let trimmed = raw.trim();
    let open = trimmed.trim_end_matches('#');
    if open.is_empty() {
        ""
    } else if open.len() < trimmed.len() && open.ends_with([' ', '\t']) {
        open.trim_end()
    } else {
        trimmed
    }
}

#[derive(Clone, Copy)]
struct InlineChar {
    previous: Option<char>,
    current: char,
    next: Option<char>,
}

impl InlineChar {
    /// `_` marks emphasis only at a word edge; `snake_case` keeps it.
    fn is_emphasis_underscore(self) -> bool {
        let word = |c: Option<char>| c.is_some_and(char::is_alphanumeric);
        self.current == '_' && !(word(self.previous) && word(self.next))
    }
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Mode {
    #[default]
    Text,
    Escaped,
    Code,
    LinkTarget,
    Url,
    Tag,
}

#[derive(Default)]
struct PlainText {
    text: String,
    mode: Mode,
}

impl PlainText {
    fn push(mut self, inline: InlineChar) -> Self {
        let character = inline.current;
        let next = inline.next.unwrap_or(' ');
        let (keep, mode) = match (self.mode, character) {
            (Mode::Escaped, _) => (true, Mode::Text),
            (Mode::Code, '`') | (Mode::Url, ')') | (Mode::Tag, '>') => (false, Mode::Text),
            (Mode::Code, _) => (true, Mode::Code),
            (Mode::LinkTarget, '(') => (false, Mode::Url),
            (Mode::LinkTarget, _) => (true, Mode::Text),
            (Mode::Url | Mode::Tag, _) => (false, self.mode),
            (Mode::Text, '\\') => (false, Mode::Escaped),
            (Mode::Text, '`') => (false, Mode::Code),
            (Mode::Text, ']') if next == '(' => (false, Mode::LinkTarget),
            (Mode::Text, '<') if next.is_ascii_alphabetic() || next == '/' => (false, Mode::Tag),
            (Mode::Text, '!') if next == '[' => (false, Mode::Text),
            (Mode::Text, '[' | ']' | '*' | '~') => (false, Mode::Text),
            (Mode::Text, _) => (!inline.is_emphasis_underscore(), Mode::Text),
        };
        if keep {
            self.text.push(character);
        }
        self.mode = mode;
        self
    }
}

#[cfg(test)]
#[path = "contents_tests.rs"]
mod tests;
