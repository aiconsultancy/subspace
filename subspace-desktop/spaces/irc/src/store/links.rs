//! GitHub pull request and issue links in a message, as exact references,
//! so a pull request's related items list the IRC messages that link it.

use subspace_framework::dashboard::full_text::{Reference, ReferenceIdentity, ReferenceMatch};

const HOST: &str = "github.com/";
/// The store keeps at most this many references per record.
const MAX_REFERENCES: usize = 64;

/// One reference per distinct GitHub item linked in `text`.
pub(crate) fn references(text: &str) -> Vec<Reference> {
    let mut found: Vec<Reference> = Vec::new();
    for item in text
        .match_indices(HOST)
        .filter_map(|(at, _)| item(&text[at + HOST.len()..]))
    {
        if found.len() < MAX_REFERENCES && !found.contains(&item) {
            found.push(item);
        }
    }
    found
}

/// `owner/repo/pull/12` or `owner/repo/issues/12` at the start of `path`.
fn item(path: &str) -> Option<Reference> {
    let mut parts = path.split('/');
    let (owner, repo, kind, number) = (parts.next()?, parts.next()?, parts.next()?, parts.next()?);
    let relation = match kind {
        "pull" => "mentioned-pull-request",
        "issues" => "mentioned-issue",
        _ => return None,
    };
    let digits: String = number.chars().take_while(char::is_ascii_digit).collect();
    let number = digits.parse::<u64>().ok()?;
    let valid = |part: &str| {
        !part.is_empty()
            && part
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || "-_.".contains(character))
    };
    if !valid(owner) || !valid(repo) {
        return None;
    }
    Some(Reference {
        identity: ReferenceIdentity::github(&format!("{owner}/{repo}"), number).ok()?,
        relation_type: relation.to_owned(),
        matched: ReferenceMatch::Exact,
    })
}

#[cfg(test)]
mod tests {
    use super::references;
    use subspace_framework::dashboard::full_text::ReferenceIdentity;

    #[test]
    fn pull_requests_and_issues_are_exact_references_once_each() {
        let found = references(
            "see https://github.com/octo-org/api/pull/42#discussion, github.com/octo-org/api/pull/42 \
             and https://github.com/Octo-Org/web/issues/7) but not github.com/octo-org/api/actions/1",
        );
        let identities: Vec<(ReferenceIdentity, &str)> = found
            .iter()
            .map(|reference| (reference.identity.clone(), reference.relation_type.as_str()))
            .collect();
        assert_eq!(
            identities,
            [
                (
                    ReferenceIdentity::github("octo-org/api", 42).unwrap(),
                    "mentioned-pull-request"
                ),
                (
                    ReferenceIdentity::github("octo-org/web", 7).unwrap(),
                    "mentioned-issue"
                ),
            ]
        );
    }

    #[test]
    fn text_without_a_complete_link_has_no_references() {
        assert!(references("github.com/octo-org").is_empty());
        assert!(references("github.com/octo org/api/pull/1").is_empty());
        assert!(references("github.com/octo-org/api/pull/x").is_empty());
    }
}
