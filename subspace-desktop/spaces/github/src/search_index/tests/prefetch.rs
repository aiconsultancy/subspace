use super::super::bind_prefetch;
use super::super::{mapping, prefetch_cache};
use std::sync::Arc;
use subspace_framework::client_github::Client;
use subspace_framework::dashboard::full_text::Service;
use subspace_framework::dashboard::full_text::{Hit, Target};

#[test]
fn disabled_search_cache_does_not_bind_a_github_identity() {
    let root =
        std::env::temp_dir().join(format!("subspace-disabled-prefetch-{}", std::process::id()));
    let service = Service::open(root.clone(), "fixture".into(), vec!["github".into()]).unwrap();
    let config = subspace_framework::core::config::Config::default();
    let client = Arc::new(
        Client::new(subspace_framework::agent_tools::test_support::credentials(
            &config,
        ))
        .with_api_url("http://127.0.0.1:0"),
    );
    let error = bind_prefetch(client, service.clone(), "ann").err().unwrap();
    assert_eq!(error, "GitHub search caching is disabled");
    drop(service);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn prefetch_reuses_only_recent_unchanged_pr_content_with_an_indexed_diff() {
    let detail = super::fixtures::detail();
    let diff = super::fixtures::diff(&detail, "src/lib.rs", "+hello".into());
    let mut hit = Hit {
        target: Target {
            scope: mapping::scope("fixture", "https://api.github.com", "Node-A").unwrap(),
            id: detail.item.key.clone(),
        },
        document: mapping::document_with_diff(&detail, Some(&diff), 100, "Node-A", "ann").unwrap(),
        excerpt: String::new(),
        score: 1.0,
    };
    assert!(prefetch_cache::fresh(&hit, &detail.item, "ann", 999));
    assert!(!prefetch_cache::fresh(&hit, &detail.item, "ann", 1_000));
    assert!(!prefetch_cache::fresh(&hit, &detail.item, "ann", 99));
    assert!(!prefetch_cache::fresh(&hit, &detail.item, "bob", 100));
    let mut changed = detail.item.clone();
    changed.updated_at = "different revision".into();
    assert!(!prefetch_cache::fresh(&hit, &changed, "ann", 100));
    hit.target.scope.account = "Node-B".into();
    assert!(!prefetch_cache::fresh(&hit, &detail.item, "ann", 100));
    hit.target.scope.account = "Node-A".into();
    hit.document = mapping::list_document(&detail.item, 100, "Node-A", "ann").unwrap();
    assert!(!prefetch_cache::fresh(&hit, &detail.item, "ann", 100));
    hit.document = mapping::document(&detail, 100, "Node-A", "ann").unwrap();
    assert!(!prefetch_cache::fresh(&hit, &detail.item, "ann", 100));
}

#[test]
fn one_item_github_refuses_never_stops_the_whole_prefetch() {
    use subspace_framework::core::http::Error;
    use subspace_framework::dashboard::prefetch::Failure;
    let status = |status: u16, message: &str| Error::Status {
        status,
        message: message.into(),
    };
    assert_eq!(
        super::super::item_failure(&status(403, "GitHub refused access (HTTP 403).")),
        Failure::Transient,
        "a plain 403 is that one item's refusal"
    );
    for stops in [
        status(401, "GitHub rejected the token."),
        status(403, "Resource protected by organization SAML enforcement."),
    ] {
        assert_eq!(super::super::item_failure(&stops), Failure::Authentication);
    }
    assert_eq!(
        super::super::item_failure(&status(429, "rate limit")),
        Failure::RateLimit
    );
}
