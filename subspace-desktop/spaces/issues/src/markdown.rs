//! Subspace API Markdown preparation, independent of client DTOs and presentation.

mod links;
mod recovery;
mod rewrite;

use markdown::{Constructs, ParseOptions};

pub struct Media {
    id: String,
    name: String,
    content_type: String,
    size: u64,
}

impl Media {
    pub fn new(id: &str, name: &str, content_type: &str, size: u64) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            content_type: content_type.into(),
            size,
        }
    }
}

pub struct MentionStatus {
    id: String,
    name: String,
}

impl MentionStatus {
    pub fn new(id: &str, name: &str) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

pub struct Options<'a> {
    media: &'a [Media],
    mentions: &'a [MentionStatus],
}

impl<'a> Options<'a> {
    pub fn new(media: &'a [Media], mentions: &'a [MentionStatus]) -> Self {
        Self { media, mentions }
    }
}

/// Prepares Subspace API Markdown for dashboard's `rich_text_prepared`. No I/O occurs.
/// Code and ordinary link destinations retain their original source bytes.
pub fn prepare_markdown(source: &str, options: &Options<'_>) -> String {
    rewrite::prepare(source, options, &parse_options())
}

fn parse_options() -> ParseOptions {
    ParseOptions {
        constructs: Constructs {
            // The board displays HTML literally. Disabling HTML also lets us find
            // Markdown nested in what CommonMark otherwise treats as raw HTML.
            html_flow: false,
            html_text: false,
            ..Constructs::gfm()
        },
        ..ParseOptions::gfm()
    }
}

/// Generated display text must not introduce Markdown syntax.
fn literal(text: &str) -> String {
    let mut result = String::new();
    for character in text.chars() {
        if character.is_ascii_punctuation() {
            result.push('\\');
        }
        result.push(character);
    }
    result
}

fn literal_label(text: &str) -> String {
    // A generated label has no surrounding Markdown hard-break syntax left.
    literal(text).replace('\n', "  \n")
}

pub(crate) fn format_file_size(size: u64) -> String {
    const BASE: f64 = 1_000.;
    const DECIMAL_PRECISION: f64 = 10.;
    const UNITS: [&str; 7] = ["bytes", "KB", "MB", "GB", "TB", "PB", "EB"];
    let mut value = size as f64;
    let mut unit = 0;
    while (value * DECIMAL_PRECISION).round() / DECIMAL_PRECISION >= BASE && unit < UNITS.len() - 1
    {
        value /= BASE;
        unit += 1;
    }
    match (unit, size) {
        (0, 1) => "1 byte".into(),
        (0, _) => format!("{size} bytes"),
        _ => format!(
            "{} {}",
            format!("{value:.1}").trim_end_matches(".0"),
            UNITS[unit]
        ),
    }
}
