//! `publish_refreshed` writes the same self relation identity as prefetch
//! and single-item load, so a refreshed item is found the same way.
use super::super::mapping::scope;
use super::fixtures::detail;

#[test]
fn a_refreshed_item_is_published_with_its_self_identity_and_is_found_offline() {
    use subspace_framework::dashboard::full_text::{
        Policy, ReferenceIdentity, ReferenceMatch, Service,
    };

    let detail = detail();
    let repo = detail.item.repo.clone();
    let number = u64::from(detail.item.number);
    let root = std::env::temp_dir().join(format!(
        "subspace-github-publish-refreshed-{}-{}",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap()
    ));
    let service = Service::open(root.clone(), "app".into(), vec!["github".into()]).unwrap();
    let defaults = Policy::default();
    service
        .set_policy(
            "github",
            Policy {
                enabled: true,
                retention_days: defaults.retention_days,
                max_bytes: defaults.max_bytes,
            },
        )
        .unwrap();
    let github_scope = scope("app", "https://api.github.com", "Node-A").unwrap();
    service.activate(&github_scope).unwrap();

    super::super::publish_refreshed(&service, &github_scope, &detail, "ann").unwrap();

    let reference = ReferenceIdentity::github(&repo, number).unwrap();
    let related = service
        .related_items(
            &reference,
            &subspace_framework::dashboard::full_text::RelatedQuery {
                limit: 10,
                boards: vec!["github".into()],
                ..Default::default()
            },
        )
        .unwrap();
    assert!(
        related
            .iter()
            .any(|item| item.hit.document.id == detail.item.key
                && item.relation_type == "self"
                && item.matched == ReferenceMatch::Exact),
        "expected the refreshed item to be found through its own self identity"
    );

    drop(service);
    std::fs::remove_dir_all(root).unwrap();
}
