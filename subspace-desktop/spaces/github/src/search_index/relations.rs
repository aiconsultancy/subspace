//! Normalised relation identities for a fetched GitHub item, from fields the
//! service already supplied. Never guesses a repository or branch.
use subspace_framework::client_github::model::GhItem;
use subspace_framework::dashboard::full_text::{
    IndexMetadata, Reference, ReferenceIdentity, ReferenceMatch,
};

/// This item's own canonical identity plus the relations the service
/// supplied. `pub(crate)`: reused by `agent_tools`'s refresh action so a
/// refreshed item writes the same relation metadata.
pub(crate) fn metadata(item: &GhItem) -> IndexMetadata {
    let mut references = Vec::new();
    push_self(&mut references, item);
    push_repository(&mut references, &item.repo);
    push_head_branch(&mut references, item);
    IndexMetadata {
        project: Some(item.repo.clone()),
        references,
    }
}

/// Writes `item`'s relation metadata for the retained document `id`. A
/// failure here never fails the write that already indexed the document.
pub(super) fn publish(
    service: &subspace_framework::dashboard::full_text::Service,
    scope: &subspace_framework::dashboard::full_text::Scope,
    item: &GhItem,
    id: &str,
) {
    let _ = service.index_metadata(scope, id, &metadata(item));
}

fn push_self(references: &mut Vec<Reference>, item: &GhItem) {
    if let Ok(identity) = ReferenceIdentity::github(&item.repo, u64::from(item.number)) {
        references.push(Reference {
            identity,
            relation_type: "self".into(),
            matched: ReferenceMatch::Exact,
        });
    }
}

fn push_repository(references: &mut Vec<Reference>, repo: &str) {
    if let Ok(identity) = ReferenceIdentity::repository(repo) {
        references.push(Reference {
            identity,
            relation_type: "repository".into(),
            matched: ReferenceMatch::Exact,
        });
    }
}

fn push_head_branch(references: &mut Vec<Reference>, item: &GhItem) {
    let Some(head_ref) = item.head_ref.as_deref() else {
        return;
    };
    if head_ref.trim().is_empty() {
        return;
    }
    if let Ok(identity) = ReferenceIdentity::branch(&item.repo, head_ref) {
        references.push(Reference {
            identity,
            relation_type: "branch".into(),
            matched: ReferenceMatch::Exact,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::metadata;
    use subspace_framework::dashboard::full_text::{ReferenceIdentity, ReferenceMatch};

    fn item(
        repo: &str,
        head_ref: Option<&str>,
    ) -> subspace_framework::client_github::model::GhItem {
        let mut item = crate::sample::items().remove(0);
        item.repo = repo.into();
        item.head_ref = head_ref.map(Into::into);
        item
    }

    #[test]
    fn a_pull_request_writes_self_repository_and_head_branch_identities() {
        let fixture = item("octo-org/api", Some("feature/login"));
        let number = fixture.number;
        let metadata = metadata(&fixture);
        assert_eq!(metadata.project.as_deref(), Some("octo-org/api"));
        assert_eq!(
            metadata.references,
            vec![
                subspace_framework::dashboard::full_text::Reference {
                    identity: ReferenceIdentity::github("octo-org/api", u64::from(number)).unwrap(),
                    relation_type: "self".into(),
                    matched: ReferenceMatch::Exact,
                },
                subspace_framework::dashboard::full_text::Reference {
                    identity: ReferenceIdentity::repository("octo-org/api").unwrap(),
                    relation_type: "repository".into(),
                    matched: ReferenceMatch::Exact,
                },
                subspace_framework::dashboard::full_text::Reference {
                    identity: ReferenceIdentity::branch("octo-org/api", "feature/login").unwrap(),
                    relation_type: "branch".into(),
                    matched: ReferenceMatch::Exact,
                },
            ]
        );
    }

    #[test]
    fn an_issue_without_a_head_branch_omits_the_branch_identity_never_guessing_one() {
        let metadata = metadata(&item("octo-org/api", None));
        assert_eq!(metadata.references.len(), 2);
        assert!(
            metadata
                .references
                .iter()
                .all(|reference| reference.relation_type != "branch")
        );
    }
}
