//! Strict, canonical GitHub pull request links. Parsing the original bytes
//! avoids accepting URL-parser rewrites of dot segments, escapes or backslashes.

const PREFIX: &str = "https://github.com/";
const MAX_OWNER: usize = 39;
const MAX_REPOSITORY: usize = 100;
/// Matches the GitHub client's supported item-number range.
const NUMBER_LIMIT: u32 = 1_000_000_000;
/// Pull request tabs whose URL still names the pull request itself.
const LENIENT_SUFFIXES: [&str; 2] = ["/files", "/commits"];

/// A validated pull request identity. It contains no account or credentials.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PullRequestLink {
    repo: String,
    number: u32,
}

impl PullRequestLink {
    /// Accepts only `https://github.com/<owner>/<repo>/pull/<number>`.
    /// Owner logins use ASCII letters/digits and single internal hyphens;
    /// repository names use ASCII letters/digits, `.`, `_` and `-`.
    /// Userinfo, ports, suffix paths, queries, fragments and escapes are refused.
    pub fn parse(url: &str) -> Option<Self> {
        let parts: Vec<_> = url.strip_prefix(PREFIX)?.split('/').collect();
        let [owner, repo, "pull", number] = parts.as_slice() else {
            return None;
        };
        if !valid_owner(owner) || !valid_repository(repo) || !valid_number(number) {
            return None;
        }
        Some(Self {
            repo: format!("{owner}/{repo}"),
            number: number.parse().ok()?,
        })
    }

    /// Accepts what [`parse`](Self::parse) accepts, also with one of the
    /// suffixes GitHub's own pages add: a query, a fragment (such as
    /// `#discussion_r1`), `/files`, `/commits` or a trailing `/`. The
    /// identity is the same pull request; callers keep the original URL to
    /// open it as written.
    // qual:api - Consumed by app crates through the Framework facade.
    pub fn parse_lenient(url: &str) -> Option<Self> {
        let end = url.find(['?', '#']).unwrap_or(url.len());
        let path = &url[..end];
        let path = path.strip_suffix('/').unwrap_or(path);
        let path = LENIENT_SUFFIXES
            .iter()
            .find_map(|suffix| path.strip_suffix(suffix))
            .unwrap_or(path);
        Self::parse(path)
    }

    /// The pull request a qualified `owner/repository#number` key names, as
    /// [`key`](Self::key) writes it. A key without an owner is refused.
    pub fn from_key(key: &str) -> Option<Self> {
        let (repo, number) = key.split_once('#')?;
        let (owner, name) = repo.split_once('/')?;
        if !valid_owner(owner) || !valid_repository(name) || !valid_number(number) {
            return None;
        }
        Some(Self {
            repo: repo.to_owned(),
            number: number.parse().ok()?,
        })
    }

    pub fn repo(&self) -> &str {
        &self.repo
    }

    pub fn number(&self) -> u32 {
        self.number
    }

    /// The board's `owner/repository#number` item identity.
    pub fn key(&self) -> String {
        format!("{}#{}", self.repo, self.number)
    }

    pub fn url(&self) -> String {
        format!("{PREFIX}{}/pull/{}", self.repo, self.number)
    }
}

fn valid_owner(owner: &str) -> bool {
    !owner.is_empty()
        && owner.len() <= MAX_OWNER
        && owner.starts_with(|c: char| c.is_ascii_alphanumeric())
        && owner.ends_with(|c: char| c.is_ascii_alphanumeric())
        && !owner.contains("--")
        && owner
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

fn valid_repository(repo: &str) -> bool {
    !repo.is_empty()
        && repo.len() <= MAX_REPOSITORY
        && !matches!(repo, "." | "..")
        && repo
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

fn valid_number(number: &str) -> bool {
    !number.is_empty()
        && number.bytes().all(|b| b.is_ascii_digit())
        && number
            .parse::<u32>()
            .is_ok_and(|n| (1..NUMBER_LIMIT).contains(&n))
}
