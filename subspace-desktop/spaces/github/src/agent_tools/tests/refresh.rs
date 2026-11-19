//! Refresh (REQ-17/18) for `github_read`: cancellation, index-only-one
//! coverage, and the inactive-scope write refusal. Coalescing and
//! monotonicity are unit-tested on [`subspace_framework::agent_tools::refresh::Refresher`]
//! directly.
use super::super::GithubTools;
use crate::sample;
use crate::search_index::{mapping, publish_refreshed};
use std::sync::Arc;
use subspace_framework::agent_tools::refresh::Refresher;
use subspace_framework::agent_tools::test_support;
use subspace_framework::agent_tools::{CallMonitor, ReadService};
use subspace_framework::client_github::Client;
use subspace_framework::client_github::model::GhItemDetail;
use subspace_framework::core::config::Config;
use subspace_framework::dashboard::full_text::{Policy, Service, Target};

struct Cancelled;
impl CallMonitor for Cancelled {
    fn cancelled(&self) -> bool {
        true
    }
}

fn config() -> Config {
    let mut config = Config {
        repos: vec!["work/repo".into()],
        ..Default::default()
    };
    config.github.token = Some(subspace_framework::core::Secret::new("test-token"));
    config
}

fn tools() -> GithubTools {
    let config = Arc::new(config());
    GithubTools {
        client: Client::new(test_support::credentials(&config)),
        config,
        search: None,
        refresher: Refresher::default(),
    }
}

#[tokio::test]
async fn a_cancelled_refresh_never_reaches_the_service() {
    let service = tools();
    let result = service.read_monitored(super::super::Read::Refresh { key: None }, &Cancelled);
    assert!(result.is_err());
}

fn detail(key: &str) -> GhItemDetail {
    let mut item = sample::items().remove(0);
    item.key = key.into();
    GhItemDetail {
        item,
        body_html: "<p>Body café</p>".into(),
        reviewers: vec![],
        check_runs: vec![],
        events: vec![],
        viewer_can_review: false,
        viewer_can_comment: false,
        changed_files: None,
    }
}

fn root(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "subspace-github-refresh-{name}-{}-{}",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap()
    ))
}

fn scope(account: &str) -> subspace_framework::dashboard::full_text::Scope {
    mapping::scope("fixture", "https://api.github.com", account).unwrap()
}

#[test]
fn indexing_one_item_leaves_the_other_cached_documents_untouched() {
    let root = root("index-one");
    let service = Service::open(root.clone(), "fixture".into(), vec!["github".into()]).unwrap();
    service
        .set_policy(
            "github",
            Policy {
                enabled: true,
                ..Default::default()
            },
        )
        .unwrap();
    let scope = scope("u1");
    service.activate(&scope).unwrap();
    let stale = chrono::Utc::now().timestamp() - 3_600;
    let seeded = [
        stale_document("work/repo#1", stale),
        stale_document("work/repo#2", stale),
    ];
    service.upsert(&scope, &seeded).unwrap();

    publish_refreshed(&service, &scope, &detail("work/repo#1"), "u1").unwrap();

    let refreshed = read_one(&service, &scope, "work/repo#1");
    let untouched = read_one(&service, &scope, "work/repo#2");
    assert!(refreshed.refreshed_at > stale);
    assert_eq!(untouched.refreshed_at, stale);
    // Windows cannot delete SQLite files that are still open.
    drop(service);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_refresh_for_a_scope_that_is_not_active_writes_nothing_and_never_activates_it() {
    let root = root("inactive-scope");
    let service = Service::open(root.clone(), "fixture".into(), vec!["github".into()]).unwrap();
    service
        .set_policy(
            "github",
            Policy {
                enabled: true,
                ..Default::default()
            },
        )
        .unwrap();
    let active = scope("u1");
    let other = scope("u2");
    service.activate(&active).unwrap();

    let error = publish_refreshed(&service, &other, &detail("work/repo#1"), "u2").unwrap_err();
    assert!(subspace_framework::dashboard::search_adapter::inactive_scope(&error));
    assert!(read_one_opt(&service, &other, "work/repo#1").is_none());

    // The active scope is still active and writable: refresh never touched it.
    publish_refreshed(&service, &active, &detail("work/repo#1"), "u1").unwrap();
    assert!(read_one_opt(&service, &active, "work/repo#1").is_some());
    drop(service);
    std::fs::remove_dir_all(root).unwrap();
}

fn stale_document(
    key: &str,
    refreshed_at: i64,
) -> subspace_framework::dashboard::full_text::Document {
    subspace_framework::dashboard::full_text::Document {
        id: key.into(),
        title: "old".into(),
        body: "old body".into(),
        metadata: "work/repo".into(),
        parent_id: Some("work/repo".into()),
        kind: "GitHub refreshed issue/PR".into(),
        url: Some(format!("https://github.com/{key}")),
        navigation: key.into(),
        refreshed_at,
        payload: None,
    }
}

fn read_one(
    service: &Service,
    scope: &subspace_framework::dashboard::full_text::Scope,
    id: &str,
) -> subspace_framework::dashboard::full_text::Document {
    read_one_opt(service, scope, id).unwrap()
}

fn read_one_opt(
    service: &Service,
    scope: &subspace_framework::dashboard::full_text::Scope,
    id: &str,
) -> Option<subspace_framework::dashboard::full_text::Document> {
    service
        .read(&Target {
            scope: scope.clone(),
            id: id.into(),
        })
        .unwrap()
}

#[test]
fn an_incomplete_identity_never_produces_a_scope() {
    assert!(mapping::scope("fixture", "", "u1").is_none());
    assert!(mapping::scope("fixture", "https://api.github.com", "").is_none());
}
