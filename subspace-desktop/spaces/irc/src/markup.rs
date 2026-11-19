//! Message text beyond mIRC codes: `#channel`, `@nick` and issue references, and
//! formatting written the way Slack reads it (bold, italic, strikethrough,
//! inline code, code blocks, quotes, bullet and numbered lists, links),
//! whether a person or a bot sent it. Plain text stays plain. Formatted text
//! is rewritten so the shared rich text shows only that subset: other block
//! syntax and HTML are escaped, each line break is kept, and references
//! become links the board opens. Pure, so every rule is tested without GPUI.

use std::ops::Range;

mod issue;
mod link;
mod markdown;
mod pull_request;

pub(crate) use issue::IssueKeys;
pub(crate) use link::{linked_pull_requests, parse_ref_url};
use markdown::to_markdown;
pub(crate) use markdown::{Rendered, destination, literal};
pub(crate) use pull_request::PrRef;

/// Characters that end a sentence rather than a reference or URL.
const TRAILING: &[char] = &['.', ',', ';', ':', '!', '?', ')', ']', '\'', '"', '>'];
/// Characters that may open a parenthetical before a reference.
const LEADING: &[char] = &['(', '"', '\''];
/// Characters that end a channel name in text: markup, not the name.
const NOT_IN_CHANNEL: &[char] = &['#', ',', '\u{7}', '[', ']', '(', ')', '<', '>', '*', '`'];
/// A code fence.
pub(super) const FENCE: &str = "```";
/// Characters that open a list item.
pub(super) const BULLETS: &[&str] = &["- ", "* ", "• ", "+ "];
/// The most digits an ordered list item's number may have in Markdown.
const MAX_NUMBER_DIGITS: usize = 9;
/// What indents a line in Markdown.
const INDENT: &[char] = &[' ', '\t'];
/// Nick characters beyond letters and digits (RFC 2812 `special` and `-`).
const NICK_SPECIAL: &[char] = &['[', ']', '\\', '`', '_', '^', '{', '|', '}', '-'];

/// Something in message text that the board can open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Ref {
    Channel(String),
    Nick(String),
    /// An issue identifier, such as `TST-827`, from plain text.
    Issue(String),
    /// A GitHub pull request, from a `[PR:owner/repo#n]` marker or its URL.
    PullRequest(PrRef),
}

/// What a message's text may mention: the channel's members, lower-cased,
/// and the issues of the board that answers the `issues` contract.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Mentions {
    nicks: Vec<String>,
    issues: IssueKeys,
}

impl Mentions {
    pub fn new<'a>(nicks: impl IntoIterator<Item = &'a str>) -> Self {
        Self {
            nicks: nicks.into_iter().map(str::to_lowercase).collect(),
            issues: IssueKeys::default(),
        }
    }

    /// Recognises issue references with `issues` too.
    pub fn with_issues(mut self, issues: IssueKeys) -> Self {
        self.issues = issues;
        self
    }

    /// Names whose recognition changed; issue-key changes need full invalidation.
    pub(crate) fn changed_nicks<'a>(&'a self, other: &'a Self) -> Option<Vec<&'a str>> {
        if self.issues != other.issues {
            return None;
        }
        let a: std::collections::HashSet<_> = self.nicks.iter().map(String::as_str).collect();
        let b: std::collections::HashSet<_> = other.nicks.iter().map(String::as_str).collect();
        Some(a.symmetric_difference(&b).copied().collect())
    }

    /// The member `nick` names, ignoring case.
    fn contains(&self, nick: &str) -> bool {
        let nick = nick.to_lowercase();
        self.nicks.contains(&nick)
    }
}

/// A recognised word: its range within the word and what it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Found {
    Url(Range<usize>),
    Ref(Range<usize>, Ref),
}

fn is_channel_name(name: &str) -> bool {
    name.len() > 1
        && name.starts_with('#')
        && name.chars().any(char::is_alphabetic)
        && !name[1..].contains(NOT_IN_CHANNEL)
}

fn is_nick(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || NICK_SPECIAL.contains(&c))
}

/// What `word` holds: a pull request, a URL, a `#channel`, an
/// `@member`, an issue identifier or, when it is the message's first word,
/// `member:` or `member,` addressing someone.
pub(super) fn classify(word: &str, mentions: &Mentions, first: bool) -> Option<Found> {
    let (start, core) = core(word);
    pull_request(word, start, core)
        .or_else(|| linked(core, start))
        .or_else(|| named(core, start, mentions))
        .or_else(|| issue(core, start, mentions))
        .or_else(|| addressed(word, mentions, first))
}

/// Where `word` starts after an opening bracket or quote, and the word
/// without them and without trailing punctuation.
fn core(word: &str) -> (usize, &str) {
    let start = word.len() - word.trim_start_matches(LEADING).len();
    (start, word[start..].trim_end_matches(TRAILING))
}

/// A pull request marker or URL, before it could read as an ordinary link.
fn pull_request(word: &str, start: usize, core: &str) -> Option<Found> {
    let (len, found) = pull_request::found(word, start, core)?;
    Some(Found::Ref(start..start + len, Ref::PullRequest(found)))
}

/// An ordinary `http(s)` URL.
fn linked(core: &str, start: usize) -> Option<Found> {
    let range = start..start + core.len();
    let url = (core.starts_with("https://") || core.starts_with("http://"))
        && core.len() > "https://".len();
    url.then_some(Found::Url(range))
}

/// A `#channel`, or an `@member` of the channel.
fn named(core: &str, start: usize, mentions: &Mentions) -> Option<Found> {
    let range = start..start + core.len();
    if is_channel_name(core) {
        return Some(Found::Ref(range, Ref::Channel(core.to_owned())));
    }
    core.strip_prefix('@')
        .filter(|nick| is_nick(nick) && mentions.contains(nick))
        .map(|nick| Found::Ref(range, Ref::Nick(nick.to_owned())))
}

/// The issue identifier `core` starts with.
fn issue(core: &str, start: usize, mentions: &Mentions) -> Option<Found> {
    let len = mentions.issues.leading(core)?;
    Some(Found::Ref(
        start..start + len,
        Ref::Issue(core[..len].to_owned()),
    ))
}

/// `member:` or `member,` as the message's first word.
fn addressed(word: &str, mentions: &Mentions, first: bool) -> Option<Found> {
    word.strip_suffix([':', ','])
        .filter(|nick| first && is_nick(nick) && mentions.contains(nick))
        .map(|nick| Found::Ref(0..nick.len(), Ref::Nick(nick.to_owned())))
}

/// The words of `text` with their byte offsets.
fn words(text: &str) -> impl Iterator<Item = (usize, &str)> {
    text.split_inclusive(char::is_whitespace)
        .scan(0, |offset, word| {
            let at = *offset;
            *offset += word.len();
            Some((at, word.trim_end()))
        })
        .filter(|(_, word)| !word.is_empty())
}

/// The channel, member and issue references in plain `text`, by byte range.
pub(crate) fn refs(text: &str, mentions: &Mentions) -> Vec<(Range<usize>, Ref)> {
    words(text)
        .enumerate()
        .filter_map(
            |(index, (at, word))| match classify(word, mentions, index == 0) {
                Some(Found::Ref(range, found)) => Some((at + range.start..at + range.end, found)),
                _ => None,
            },
        )
        .collect()
}

/// The distinct issue identifiers message `text` refers to, in order.
pub(crate) fn issues(text: &str, mentions: &Mentions, rendered: Option<&Rendered>) -> Vec<String> {
    distinct_issues(
        found_refs(text, mentions, rendered).filter_map(|found| match found {
            Ref::Issue(identifier) => Some(identifier),
            _ => None,
        }),
    )
}

/// The distinct pull requests message `text` refers to, by canonical key,
/// in order.
pub(crate) fn pull_requests(
    text: &str,
    mentions: &Mentions,
    rendered: Option<&Rendered>,
) -> Vec<String> {
    distinct_issues(
        found_refs(text, mentions, rendered).filter_map(|found| match found {
            Ref::PullRequest(found) => Some(found.key),
            _ => None,
        }),
    )
}

/// The references of message `text` in reading order: those outside code
/// when it is `rendered` as Markdown, or else all of its plain text's.
fn found_refs(
    text: &str,
    mentions: &Mentions,
    rendered: Option<&Rendered>,
) -> impl Iterator<Item = Ref> {
    let found: Vec<Ref> = match rendered {
        Some(rendered) => rendered
            .refs
            .iter()
            .map(|(_, found)| found.clone())
            .collect(),
        None => refs(&subspace_client_irc::format::strip(text), mentions)
            .into_iter()
            .map(|(_, found)| found)
            .collect(),
    };
    found.into_iter()
}

fn distinct_issues(identifiers: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for identifier in identifiers {
        if !found.contains(&identifier) {
            found.push(identifier);
        }
    }
    found
}

/// A message's text for the shared rich text: safe Markdown when it uses
/// formatting. `None` keeps the IRC presentation, which plain text and any
/// text with mIRC codes use: mIRC colours have no Markdown form, so mIRC
/// codes win and Markdown characters beside them stay as typed.
pub(crate) fn rich(text: &str, mentions: &Mentions) -> Option<Rendered> {
    let coded = subspace_client_irc::format::strip(text) != text;
    (!coded).then(|| to_markdown(text, mentions)).flatten()
}

/// The list marker `line` starts with, after its indentation: `- `, `* `,
/// `• `, `+ ` or `1. ` / `1) ` with at most nine digits, as Markdown reads
/// them.
pub(super) fn list_marker(line: &str) -> Option<&str> {
    let item = &line[indent(line)..];
    if let Some(bullet) = BULLETS.iter().find(|bullet| item.starts_with(**bullet)) {
        return Some(&item[..bullet.len()]);
    }
    let digits = item.len() - item.trim_start_matches(|c: char| c.is_ascii_digit()).len();
    let after = &item[digits..];
    ((1..=MAX_NUMBER_DIGITS).contains(&digits)
        && (after.starts_with(". ") || after.starts_with(") ")))
    .then(|| &item[..digits + ". ".len()])
}

/// The length of `line`'s indentation: its leading spaces and tabs, the
/// only indentation Markdown reads.
pub(super) fn indent(line: &str) -> usize {
    line.len() - line.trim_start_matches(INDENT).len()
}

#[cfg(test)]
mod issue_tests;
#[cfg(test)]
mod pr_tests;
#[cfg(test)]
mod tests;
