//! Document mapping, retained-detail fencing, and cache fixture coverage.
use super::super::mapping::{document, document_with_diff, list_document, retained, scope};
use subspace_framework::client_github::model::{
    DiffResponse, EventKind, GhDiffFile, GhEvent, GhFileStatus, GhItemDetail, GhUser,
};
use subspace_framework::dashboard::full_text::{Hit, Target};

pub(super) fn detail() -> GhItemDetail {
    GhItemDetail {
        item: crate::sample::items().remove(0),
        body_html: "<p>Full issue café body</p>".into(),
        reviewers: vec![],
        check_runs: vec![],
        events: vec![GhEvent {
            kind: EventKind::Review,
            author: Some(GhUser {
                login: "reviewer".into(),
                name: None,
            }),
            created_at: "2026-10-17".into(),
            body_html: "<p>Review discussion</p>".into(),
            review_state: None,
            url: None,
        }],
        viewer_can_review: true,
        viewer_can_comment: true,
        changed_files: None,
        merge_target: None,
        viewer_approved: false,
    }
}

pub(super) fn diff(detail: &GhItemDetail, path: &str, patch: String) -> DiffResponse {
    DiffResponse {
        repo: detail.item.repo.clone(),
        number: detail.item.number,
        base_sha: "base".into(),
        head_sha: "head".into(),
        total: 1,
        files: vec![GhDiffFile {
            path: path.into(),
            previous_path: None,
            status: GhFileStatus::Modified,
            additions: 1,
            deletions: 0,
            patch: Some(patch),
        }],
    }
}

fn search_count(
    service: &subspace_framework::dashboard::full_text::Service,
    text: String,
) -> usize {
    use subspace_framework::dashboard::full_text::{Query, Ranking};

    service
        .search(&Query {
            text,
            boards: vec!["github".into()],
            limit: 5,
            ranking: Ranking::Bm25,
        })
        .unwrap()
        .hits
        .len()
}

#[test]
fn fetched_body_and_activity_are_searchable_with_original_age() {
    let detail = detail();
    let document = document(&detail, 123, "Node-A", "VerifiedLogin").unwrap();
    assert!(document.body.contains("Full issue café body"));
    assert!(document.body.contains("Review discussion"));
    assert!(document.body.contains("reviewer"));
    assert!(!document.body.contains("<p>"));
    assert_eq!(document.refreshed_at, 123);
    assert_eq!(document.navigation, detail.item.key);
}

#[test]
fn retained_target_preserves_exact_item_without_reusing_cached_write_permissions() {
    let detail = detail();
    let document = document(&detail, 123, "Node-A", "VerifiedLogin").unwrap();
    let mut hit = Hit {
        target: Target {
            scope: scope("app", "https://api.github.com", "Node-A").unwrap(),
            id: detail.item.key.clone(),
        },
        document,
        excerpt: String::new(),
        score: 1.0,
    };
    let retained_detail = retained(&hit).unwrap();
    assert_eq!(retained_detail.detail.item, detail.item);
    assert_eq!(retained_detail.login, "verifiedlogin");
    assert!(!retained_detail.detail.viewer_can_comment);
    assert!(!retained_detail.detail.viewer_can_review);
    hit.document.navigation = "different/repository#1".into();
    assert!(retained(&hit).is_none());
}

#[test]
fn account_is_exact_verified_node_id_never_a_login_fallback() {
    assert!(scope("app", "https://api.github.com", " ").is_none());
    assert_eq!(
        scope("app", "https://api.github.com", "Node-A")
            .unwrap()
            .account,
        "Node-A"
    );
}

#[test]
fn disabled_cache_keeps_normal_validation_without_identity_requirement() {
    let mut config = subspace_framework::core::config::Config::default();
    config.github.token = Some(subspace_framework::core::Secret::new("fixture-token"));
    let client = subspace_framework::client_github::Client::new(
        subspace_framework::agent_tools::test_support::credentials(&config),
    )
    .with_api_url("http://127.0.0.1:0");
    assert!(matches!(
        super::super::load::prepare(&client, None, None, "fixture-user", "invalid-key"),
        super::super::load::Prepared::Live(Err(_), false)
    ));
}

#[test]
fn reused_login_does_not_reuse_a_different_principals_payload_or_native_details() {
    let detail = detail();
    let first = scope("app", "https://api.github.com", "Node-A").unwrap();
    let second = scope("app", "https://api.github.com", "Node-B").unwrap();
    let mut hit = Hit {
        target: Target {
            scope: second.clone(),
            id: detail.item.key.clone(),
        },
        document: document(&detail, 123, "Node-A", "reused-login").unwrap(),
        excerpt: String::new(),
        score: 1.0,
    };
    assert!(retained(&hit).is_none());
    let mut account = crate::Account::default();
    account
        .details
        .insert(detail.item.key.clone(), detail.clone());
    let mut data = std::collections::HashMap::from([("reused-login".into(), account)]);
    assert!(super::super::selection::reset_reassigned_login(
        &mut data,
        Some(&first),
        &second,
        "reused-login"
    ));
    assert!(!data.contains_key("reused-login"));
    hit.document.payload = Some(serde_json::to_string(&detail).unwrap());
    assert!(retained(&hit).is_none());
}

#[test]
fn retained_section_titles_keep_original_age_and_read_only_capabilities() {
    let item = detail().item;
    let document = list_document(&item, 42, "Node-A", "ann").unwrap();
    assert_eq!(document.title, item.title);
    assert_eq!(document.refreshed_at, 42);
    let hit = Hit {
        target: Target {
            scope: scope("app", "https://api.github.com", "Node-A").unwrap(),
            id: item.key.clone(),
        },
        document,
        excerpt: String::new(),
        score: 1.0,
    };
    let retained = retained(&hit).unwrap();
    assert_eq!(retained.detail.item, item);
    assert!(!retained.detail.viewer_can_comment);
    assert!(!retained.detail.viewer_can_review);
}

#[test]
fn diff_only_text_is_searchable_and_oversized_content_has_a_marker() {
    let detail = detail();
    let diff = diff(
        &detail,
        "src/rare.rs",
        format!("+unique_diff_token{}", "x".repeat(60_000)),
    );
    let document = document_with_diff(&detail, Some(&diff), 1, "Node-A", "ann").unwrap();
    assert!(document.body.contains("unique_diff_token"));
    assert!(
        document
            .body
            .contains("[GitHub content truncated to fit the local search cache]")
    );
    assert!(document.body.len() <= 48 * 1024);
}

#[test]
fn retained_title_and_diff_text_are_found_offline() {
    use subspace_framework::dashboard::full_text::{Policy, Service};

    let detail = detail();
    let now = chrono::Utc::now().timestamp();
    let title = list_document(&detail.item, now, "Node-A", "ann").unwrap();
    let diff = diff(&detail, "src/offline.rs", "+diff_only_offline_token".into());
    let enriched = document_with_diff(&detail, Some(&diff), now, "Node-A", "ann").unwrap();
    let root = std::env::temp_dir().join(format!("github-search-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
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
    let scope = scope("app", "https://api.github.com", "Node-A").unwrap();
    service.activate(&scope).unwrap();
    service.upsert(&scope, &[title]).unwrap();
    assert_eq!(search_count(&service, detail.item.title.clone()), 1);
    service.upsert(&scope, &[enriched]).unwrap();
    assert_eq!(search_count(&service, "diff_only_offline_token".into()), 1);
    drop(service);
    std::fs::remove_dir_all(root).unwrap();
}
