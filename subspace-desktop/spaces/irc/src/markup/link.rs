//! The links references become in rewritten Markdown, and back.

use super::{PrRef, Ref};

/// The URL scheme of a reference link in rewritten Markdown.
pub(super) const SCHEME: &str = "subspace-irc:";
const CHANNEL_PATH: &str = "channel/";
const NICK_PATH: &str = "nick/";
const ISSUE_PATH: &str = "issue/";
const DECISION_PATH: &str = "decision/";
/// A pull request's link names the URL to open; its identity comes from it.
const PR_PATH: &str = "pr/";
/// A percent escape: `%` and two hex digits.
const ESCAPE_LEN: usize = 3;
const HEX_RADIX: u32 = 16;

/// The link a reference becomes in rewritten Markdown.
pub(crate) fn ref_url(found: &Ref) -> String {
    let (path, name) = match found {
        Ref::Channel(name) => (CHANNEL_PATH, name),
        Ref::Nick(name) => (NICK_PATH, name),
        Ref::Issue(identifier) => (ISSUE_PATH, identifier),
        Ref::Decision(identifier) => (DECISION_PATH, identifier),
        Ref::PullRequest(found) => (PR_PATH, &found.url),
    };
    let encoded: String = name
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' => {
                char::from(byte).to_string()
            }
            _ => format!("%{byte:02X}"),
        })
        .collect();
    format!("{SCHEME}{path}{encoded}")
}

/// The reference a rewritten link names, if it is one.
pub(crate) fn parse_ref_url(url: &str) -> Option<Ref> {
    let path = url.strip_prefix(SCHEME)?;
    let (make, encoded): (fn(String) -> Ref, &str) = match path {
        _ if path.starts_with(CHANNEL_PATH) => (Ref::Channel, &path[CHANNEL_PATH.len()..]),
        _ if path.starts_with(NICK_PATH) => (Ref::Nick, &path[NICK_PATH.len()..]),
        _ if path.starts_with(ISSUE_PATH) => (Ref::Issue, &path[ISSUE_PATH.len()..]),
        _ if path.starts_with(DECISION_PATH) => (Ref::Decision, &path[DECISION_PATH.len()..]),
        _ if path.starts_with(PR_PATH) => (pull_request, &path[PR_PATH.len()..]),
        _ => return None,
    };
    let name = decode(encoded)?;
    (!name.is_empty()).then(|| make(name)).filter(valid)
}

/// A pull request reference to `url`; [`valid`] refuses one that names none.
fn pull_request(url: String) -> Ref {
    Ref::PullRequest(PrRef::from_url(&url).unwrap_or(PrRef {
        key: String::new(),
        url,
    }))
}

/// The pull requests `source` links, in order.
pub(crate) fn linked_pull_requests(source: &str) -> Vec<PrRef> {
    let prefix = format!("{SCHEME}{PR_PATH}");
    source
        .match_indices(&prefix)
        .filter_map(|(at, _)| {
            let rest = &source[at..];
            let end = rest.find([')', ' ', '>', '"']).unwrap_or(rest.len());
            match parse_ref_url(&rest[..end]) {
                Some(Ref::PullRequest(found)) => Some(found),
                _ => None,
            }
        })
        .collect()
}

/// Whether a decoded reference names something: a pull request link must
/// name a pull request URL.
fn valid(found: &Ref) -> bool {
    match found {
        Ref::Decision(identifier) => {
            subspace_framework::contracts::decisions::is_identifier(identifier)
        }
        Ref::PullRequest(found) => !found.key.is_empty(),
        _ => true,
    }
}

/// Percent-decoding of a reference name; `None` for malformed escapes or
/// text that is not UTF-8.
fn decode(encoded: &str) -> Option<String> {
    let bytes = encoded.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while let Some(&byte) = bytes.get(at) {
        if byte == b'%' {
            let hex = encoded.get(at + 1..at + ESCAPE_LEN)?;
            out.push(u8::from_str_radix(hex, HEX_RADIX).ok()?);
            at += ESCAPE_LEN;
        } else {
            out.push(byte);
            at += 1;
        }
    }
    String::from_utf8(out).ok()
}
