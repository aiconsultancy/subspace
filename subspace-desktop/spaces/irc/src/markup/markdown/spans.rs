//! What Markdown reads as one unit in a line, so no formatting marker inside
//! it pairs with one outside: code spans, safe links and bare URLs.

use std::ops::Range;

use subspace_framework::dashboard::links::safe_url;

use super::super::TRAILING;

/// The formatting marker characters.
pub(super) const MARKS: &[char] = &['*', '_', '~'];
/// What a bare URL must not hold: an autolink ends at either, and the rest
/// would read as HTML.
pub(super) const NOT_IN_URL: &[char] = &['<', '>'];
/// The schemes of a bare URL.
const URL_SCHEMES: [&str; 2] = ["https://", "http://"];
/// The deepest a link destination's parentheses may nest, as the parser
/// allows (`RESOURCE_DESTINATION_BALANCE_MAX` in the `markdown` crate).
const MAX_NESTING: usize = 32;

/// What a link destination writes with a backslash: brackets that could end
/// it, the escape character itself and the start of an entity. Markdown
/// decodes these escapes, so the URL keeps them as they are.
const ESCAPED: &[char] = &['(', ')', '\\', '&'];
/// What a link destination writes as a percent escape: what cannot be part
/// of one at all, a space or an angle bracket. `safe_url` already encodes
/// these, so this only guards.
const ENCODED: &[char] = &['<', '>', ' '];

/// A `[text](destination)` link at the start of some text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Link {
    /// The link text, without its brackets.
    pub text: Range<usize>,
    /// The destination as the rewritten Markdown writes it.
    pub url: String,
    /// The whole link's length.
    pub len: usize,
}

/// A link destination as the rewritten Markdown writes it: the URL the
/// shared link policy opens (`http`, `https` or `mailto`), with brackets,
/// backslashes and `&` escaped with a backslash, which Markdown decodes, so
/// the link opens exactly that URL and no entity changes it. `None` when the
/// policy would not open it.
pub(crate) fn destination(raw: &str) -> Option<String> {
    if let Some(found) = super::super::link::thread_ref(raw) {
        return Some(super::super::link::ref_url(&found));
    }
    let url = safe_url(raw)?;
    Some(
        url.chars()
            .map(|c| match c {
                _ if ESCAPED.contains(&c) => format!("\\{c}"),
                _ if c.is_ascii_control() || ENCODED.contains(&c) => {
                    format!("%{:02X}", u32::from(c))
                }
                _ => c.to_string(),
            })
            .collect(),
    )
}

/// The length of the code span at the start of `rest`: its run of
/// backticks, the code and the next run of exactly as many. `None` when the
/// line does not close it.
pub(super) fn code_span(rest: &str) -> Option<usize> {
    let run = rest.len() - rest.trim_start_matches('`').len();
    let mut at = run;
    while let Some(found) = rest[at..].find('`') {
        let start = at + found;
        let len = rest[start..].len() - rest[start..].trim_start_matches('`').len();
        if len == run {
            return Some(start + len);
        }
        at = start + len;
    }
    None
}

/// The `[text](destination)` link at the start of `rest` when it is well
/// formed and the shared link policy opens its destination (`http`,
/// `https` or `mailto`). Anything else, including this board's own
/// reference scheme, is not a link.
pub(super) fn link(rest: &str) -> Option<Link> {
    let text_end = rest.find(']')?;
    let after = rest[text_end + 1..].strip_prefix('(')?;
    let close = destination_end(after)?;
    let text = &rest[1..text_end];
    let url = destination(&after[..close])?;
    let start = text_end + "](".len();
    let well_formed = !text.trim().is_empty() && !text.contains('[');
    well_formed.then(|| Link {
        text: 1..text_end,
        url,
        len: start + close + ")".len(),
    })
}

/// Where a typed link destination ends: at the `)` that balances its
/// parentheses. `None` at a space, the end of the line or a parenthesis
/// nested deeper than [`MAX_NESTING`], so each `[` scans a bounded stretch.
/// The rewritten Markdown writes its own destination, so this only decides
/// how much of the typed text the link covers.
fn destination_end(after: &str) -> Option<usize> {
    let mut depth = 0_usize;
    for (at, c) in after.char_indices() {
        match c {
            '(' if depth == MAX_NESTING => return None,
            '(' => depth += 1,
            ')' if depth == 0 => return Some(at),
            ')' => depth -= 1,
            _ if c.is_whitespace() => return None,
            _ => {}
        }
    }
    None
}

/// The length of the bare `http` or `https` URL that starts at `at` in
/// `line`, after a character that is not a letter or digit, up to the next
/// space and without trailing punctuation or formatting markers. A URL with
/// an angle bracket is not one.
pub(super) fn url(line: &str, at: usize) -> Option<usize> {
    let rest = &line[at..];
    let starts = URL_SCHEMES.iter().any(|scheme| rest.starts_with(scheme))
        && line[..at]
            .chars()
            .next_back()
            .is_none_or(|before| !before.is_alphanumeric());
    if !starts {
        return None;
    }
    let word = rest.split(char::is_whitespace).next().unwrap_or_default();
    let core = word.trim_end_matches(|c| TRAILING.contains(&c) || MARKS.contains(&c));
    (core.len() > "https://".len() && !core.contains(NOT_IN_URL)).then_some(core.len())
}
