//! GitHub pull request references: the canonical `[PR:owner/repo#n]`
//! marker, a full pull request URL (GitHub's `/files`, `/commits`, query and
//! fragment suffixes included) and a Markdown link whose label is the marker
//! and whose destination is the same pull request. Each names one canonical,
//! lower-cased `owner/repo#n` identity; the URL as written is kept to open
//! and copy. A marker without an owner, or a bare `#123`, is never guessed.

use subspace_framework::contracts::github::recognition::PullRequestLink;
use subspace_framework::dashboard::links::safe_url;

use super::TRAILING;

const MARKER: &str = "[PR:";
/// A marker inside a Markdown link's label: `[[PR:…]](url)`.
const WRAPPED: &str = "[[PR:";
/// What a URL to open as written must not hold.
const NOT_IN_URL: &[char] = &['<', '>'];

/// A pull request reference: its canonical identity and the URL to open,
/// as written, or the canonical URL for a marker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PrRef {
    /// `owner/repo#n`, lower-cased.
    pub key: String,
    pub url: String,
}

impl PrRef {
    fn new(link: &PullRequestLink, url: &str) -> Self {
        Self {
            key: key(link),
            url: url.to_owned(),
        }
    }

    /// The pull request a URL names, keeping the URL as written.
    pub fn from_url(url: &str) -> Option<Self> {
        let link = PullRequestLink::parse_lenient(url).filter(|_| openable(url))?;
        Some(Self::new(&link, url))
    }

    /// The compact label a chip shows: `repo#n`.
    pub fn label(&self) -> &str {
        self.key
            .split_once('/')
            .map_or(&self.key, |(_, label)| label)
    }

    /// The label a chip shows beside `others`, the pull requests of the
    /// same message.
    /// `owner/repo#n` only when another owner's repository of the same name
    /// is among them; `repo#n` otherwise.
    pub fn label_among(&self, others: &[PrRef]) -> String {
        let (owner, repo) = owner_and_repo(&self.key);
        let clash = others.iter().any(|other| {
            let (other_owner, other_repo) = owner_and_repo(&other.key);
            other_repo == repo && other_owner != owner
        });
        match clash {
            true => self.key.clone(),
            false => self.label().to_owned(),
        }
    }
}

/// Whether `url` can open as written.
fn openable(url: &str) -> bool {
    !url.contains(NOT_IN_URL) && safe_url(url).is_some()
}

/// The owner and repository name of a canonical key.
fn owner_and_repo(key: &str) -> (&str, &str) {
    let (owner, rest) = key.split_once('/').unwrap_or(("", key));
    (owner, rest.split_once('#').map_or(rest, |(repo, _)| repo))
}

/// The canonical identity of `link`.
fn key(link: &PullRequestLink) -> String {
    link.key().to_ascii_lowercase()
}

/// A marker label at the start of some text, and the destination of the
/// Markdown link it labels, if any.
struct Marker<'a> {
    link: PullRequestLink,
    /// The bytes the label covers.
    label_end: usize,
    /// Whether the marker sits inside a link label: `[[PR:…]](url)`.
    wrapped: bool,
    /// The destination written in parentheses after the label, and its length.
    destination: Option<(&'a str, usize)>,
}

/// The marker at the start of `rest`, with how many bytes it covers: a bare
/// `[PR:owner/repo#n]`, or one as a Markdown link's label
/// (`[PR:…](url)` or `[[PR:…]](url)`) whose destination is the same pull
/// request. A link to anything else is not a reference.
pub(super) fn marker(rest: &str) -> Option<(usize, PrRef)> {
    let parsed = parse_marker(rest)?;
    parsed.destination.map_or_else(
        || bare_marker(&parsed),
        |(url, len)| linked_marker(&parsed, url, len),
    )
}

/// The label and destination of a marker at the start of `rest`; `None` when
/// there is no marker or its link is not closed.
fn parse_marker(rest: &str) -> Option<Marker<'_>> {
    let wrapped = rest.starts_with(WRAPPED);
    let inner = usize::from(wrapped);
    let close = inner + rest[inner..].strip_prefix(MARKER)?.find(']')? + MARKER.len();
    let link = PullRequestLink::from_key(&rest[inner + MARKER.len()..close])?;
    let label_end = close + 1 + usize::from(wrapped);
    if wrapped && rest.get(close + 1..label_end) != Some("]") {
        return None;
    }
    let destination = rest[label_end..].strip_prefix('(').map(|after| {
        after
            .find([')', '('])
            .filter(|end| after[*end..].starts_with(')'))
            .map(|end| (&after[..end], end))
    });
    if destination == Some(None) {
        return None;
    }
    Some(Marker {
        link,
        label_end,
        wrapped,
        destination: destination.flatten(),
    })
}

/// A marker with no link: it names the canonical URL. A wrapped label needs
/// its link.
fn bare_marker(parsed: &Marker<'_>) -> Option<(usize, PrRef)> {
    Some(parsed).filter(|parsed| !parsed.wrapped).map(|parsed| {
        (
            parsed.label_end,
            PrRef::new(&parsed.link, &parsed.link.url()),
        )
    })
}

/// A marker labelling a link to `url`, `len` bytes long: one reference only
/// when the link names the same pull request.
fn linked_marker(parsed: &Marker<'_>, url: &str, len: usize) -> Option<(usize, PrRef)> {
    PrRef::from_url(url)
        .filter(|found| found.key == key(&parsed.link))
        .map(|found| (parsed.label_end + len + "()".len(), found))
}

/// The marker or pull request URL `word` holds after its first `start`
/// bytes, with only trailing punctuation after it, by range in `word`.
pub(super) fn found(word: &str, start: usize, core: &str) -> Option<(usize, PrRef)> {
    let rest = &word[start..];
    marker(rest).map_or_else(
        || PrRef::from_url(core).map(|found| (core.len(), found)),
        |(len, found)| ends_marker(rest, len).then_some((len, found)),
    )
}

/// Whether only trailing punctuation follows the first `len` bytes of `rest`.
fn ends_marker(rest: &str, len: usize) -> bool {
    rest[len..].chars().all(|c| TRAILING.contains(&c))
}
