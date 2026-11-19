//! The GitHub repository a report files into, supplied by the app.

/// GitHub's longest owner (user or organisation) name.
const MAX_OWNER: usize = 39;
/// GitHub's longest repository name.
const MAX_NAME: usize = 100;

/// A GitHub repository as `owner/name`, checked so it can be placed in API
/// paths and URLs as it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssueRepository(String);

impl IssueRepository {
    /// `owner/name`, trimmed. The owner is letters, digits and inner
    /// hyphens; the name is letters, digits, `-`, `_` and `.`, but not `.`
    /// or `..`.
    pub fn parse(value: &str) -> Option<Self> {
        let value = value.trim();
        let (owner, name) = value.split_once('/')?;
        (valid_owner(owner) && valid_name(name)).then(|| Self(value.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The repository's issue list.
    pub fn issues_url(&self) -> String {
        format!("https://github.com/{}/issues", self.0)
    }

    /// GitHub's blank new-issue editor for this repository.
    pub fn new_issue_url(&self) -> String {
        format!("{}/new?template=blank", self.issues_url())
    }

    /// Only a positive issue number in this repository, over HTTPS, is safe
    /// to open.
    pub fn is_created_issue_url(&self, url: &str) -> bool {
        let prefix = format!("{}/", self.issues_url());
        let Some(number) = url.strip_prefix(&prefix) else {
            return false;
        };
        !number.is_empty()
            && number.bytes().all(|byte| byte.is_ascii_digit())
            && number.parse::<u64>().is_ok_and(|number| number > 0)
    }
}

fn valid_owner(owner: &str) -> bool {
    (1..=MAX_OWNER).contains(&owner.len())
        && owner
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        && !owner.starts_with('-')
        && !owner.ends_with('-')
}

fn valid_name(name: &str) -> bool {
    (1..=MAX_NAME).contains(&name.len())
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        && name != "."
        && name != ".."
}
