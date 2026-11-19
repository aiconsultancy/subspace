//! Input checks (GH-09, DF-07). A failed check is an
//! [`Error::Status`] with status 400.

use subspace_core::http::Error;

use crate::model::{ReviewEvent, SECTIONS, SectionDef};

const MAX_NUMBER: u32 = 1_000_000_000;
const MAX_LOGIN_LENGTH: usize = 39;
const MAX_KEY_NUMBER_LENGTH: usize = 10;
const SHA_LENGTH: usize = 40;
const MAX_PATH_LENGTH: usize = 1024;
/// `MAX_MARKDOWN_BYTES` of `server/write.ts`.
pub const MAX_BODY_BYTES: usize = 64 * 1024;
pub const MAX_ASSIGNEE_CHANGES: usize = 10;

pub fn invalid(message: impl Into<String>) -> Error {
    Error::Status {
        status: 400,
        message: message.into(),
    }
}

/// An issue or pull request: `owner/name#number`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemRef {
    pub owner: String,
    pub name: String,
    pub number: u32,
}

impl ItemRef {
    pub fn repo(&self) -> String {
        format!("{}/{}", self.owner, self.name)
    }
}

/// A gh login: letters, digits and hyphens, plus `_` for EMU logins, at
/// most 39 characters.
pub fn check_login(value: &str) -> Result<&str, Error> {
    let mut chars = value.chars();
    let valid = value.len() <= MAX_LOGIN_LENGTH
        && chars.next().is_some_and(|c| c.is_ascii_alphanumeric())
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if valid {
        Ok(value)
    } else {
        Err(invalid("account must be a GitHub login."))
    }
}

pub fn check_section(id: &str) -> Result<&'static SectionDef, Error> {
    SECTIONS
        .iter()
        .find(|section| section.id == id)
        .ok_or_else(|| {
            let ids: Vec<&str> = SECTIONS.iter().map(|section| section.id).collect();
            invalid(format!("section must be one of: {}.", ids.join(", ")))
        })
}

fn is_repo_part(part: &str) -> bool {
    !part.is_empty()
        && part
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

/// `owner/name`.
pub fn check_repo(value: &str) -> Result<(String, String), Error> {
    match value.split_once('/') {
        Some((owner, name)) if is_repo_part(owner) && is_repo_part(name) => {
            Ok((owner.to_owned(), name.to_owned()))
        }
        _ => Err(invalid("repo must be owner/name.")),
    }
}

pub fn check_number(value: u32) -> Result<u32, Error> {
    if (1..MAX_NUMBER).contains(&value) {
        Ok(value)
    } else {
        Err(invalid("number must be a positive integer."))
    }
}

/// `owner/name#number`, the [`GhItem::key`](crate::model::GhItem::key).
pub fn check_key(key: &str) -> Result<ItemRef, Error> {
    let (repo, number) = key
        .rsplit_once('#')
        .ok_or_else(|| invalid("key must be owner/name#number."))?;
    let (owner, name) = check_repo(repo)?;
    let digits = !number.is_empty()
        && number.len() <= MAX_KEY_NUMBER_LENGTH
        && number.bytes().all(|b| b.is_ascii_digit());
    let number = digits
        .then(|| number.parse::<u32>().ok())
        .flatten()
        .ok_or_else(|| invalid("number must be a positive integer."))?;
    Ok(ItemRef {
        owner,
        name,
        number: check_number(number)?,
    })
}

/// A review or comment body: at most 64 KB, and not blank when required.
pub fn check_body(body: &str, required: bool) -> Result<&str, Error> {
    if body.len() > MAX_BODY_BYTES {
        return Err(invalid("body must be at most 64 KB."));
    }
    if required && body.trim().is_empty() {
        return Err(invalid("body must not be empty."));
    }
    Ok(body)
}

/// The review body is required except for an approval.
pub fn check_review_body(event: ReviewEvent, body: &str) -> Result<&str, Error> {
    check_body(body, event != ReviewEvent::Approve)
}

/// At most ten valid logins, de-duplicated in order.
pub fn check_logins(field: &str, logins: &[String]) -> Result<Vec<String>, Error> {
    if logins.len() > MAX_ASSIGNEE_CHANGES {
        return Err(invalid(format!(
            "{field} must be a list of at most {MAX_ASSIGNEE_CHANGES} logins."
        )));
    }
    let mut unique: Vec<String> = Vec::new();
    for login in logins {
        check_login(login).map_err(|_| invalid(format!("{field} contains an invalid login.")))?;
        if !unique.contains(login) {
            unique.push(login.clone());
        }
    }
    Ok(unique)
}

/// A 40-character lowercase commit SHA.
pub fn is_sha(value: &str) -> bool {
    value.len() == SHA_LENGTH
        && value
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

pub fn check_sha(value: &str) -> Result<&str, Error> {
    if is_sha(value) {
        Ok(value)
    } else {
        Err(invalid("ref must be a 40-character commit SHA."))
    }
}

/// A relative repository path: no leading slash, no backslash, no control
/// characters, no empty, `.` or `..` segments.
pub fn check_path(value: &str) -> Result<&str, Error> {
    if value.is_empty() || value.chars().count() > MAX_PATH_LENGTH {
        return Err(invalid(format!(
            "path must be 1 to {MAX_PATH_LENGTH} characters."
        )));
    }
    if value.starts_with('/') || value.contains('\\') || value.chars().any(char::is_control) {
        return Err(invalid("path must be a relative repository path."));
    }
    if value
        .split('/')
        .any(|segment| matches!(segment, "" | "." | ".."))
    {
        return Err(invalid("path must not contain empty, . or .. segments."));
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::{
        check_body, check_key, check_login, check_logins, check_path, check_repo,
        check_review_body, check_section, check_sha,
    };
    use crate::model::ReviewEvent;

    #[test]
    fn keys_and_repos() {
        let item = check_key("octo-org/app.rs#12").unwrap();
        assert_eq!((item.repo(), item.number), ("octo-org/app.rs".into(), 12));
        for bad in [
            "octo/app",
            "octo/app#0",
            "octo/app#-1",
            "octo/app#1000000000",
            "octo/a/b#1",
            "/app#1",
            "octo/app#12345678901",
        ] {
            assert!(check_key(bad).is_err(), "{bad}");
        }
        assert!(check_repo("o/r x").is_err());
    }

    #[test]
    fn logins_and_sections() {
        assert!(check_login("Alex-J-Example_emu").is_ok());
        assert!(check_login("-ann").is_err());
        assert!(check_login(&"a".repeat(40)).is_err());
        assert!(check_section("review").is_ok());
        assert!(check_section("nope").is_err());
        let logins = ["a".to_owned(), "b".to_owned(), "a".to_owned()];
        assert_eq!(check_logins("add", &logins).unwrap(), ["a", "b"]);
        assert!(check_logins("add", &vec!["a".to_owned(); 11]).is_err());
        assert!(check_logins("add", &["bad login".to_owned()]).is_err());
    }

    #[test]
    fn bodies() {
        assert!(check_review_body(ReviewEvent::Approve, "").is_ok());
        assert!(check_review_body(ReviewEvent::RequestChanges, "  ").is_err());
        assert!(check_body(&"x".repeat(64 * 1024 + 1), false).is_err());
    }

    #[test]
    fn shas_and_paths() {
        assert!(check_sha(&"a".repeat(40)).is_ok());
        assert!(check_sha(&"A".repeat(40)).is_err());
        assert!(check_path("src/main.rs").is_ok());
        for bad in ["", "/etc", "a//b", "a/../b", "./a", "a\\b", "a\nb"] {
            assert!(check_path(bad).is_err(), "{bad:?}");
        }
    }
}
