use super::{client, serve, token};
use crate::RepositoryRef;
use serde_json::json;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

#[test]
fn public_repo_does_not_bypass_membership_or_retry_with_changed_token() {
    let credential = token("dummy-test-only-bound-token");
    let swap = credential.clone();
    let calls = Arc::new(AtomicUsize::new(0));
    let seen = calls.clone();
    let url = serve(Arc::new(move |request| {
        assert_eq!(request.authorization, "Bearer dummy-test-only-bound-token");
        seen.fetch_add(1, Ordering::SeqCst);
        if request.body.contains("ViewerBound") {
            *swap.lock().unwrap() = "dummy-test-only-other-token".into();
            return (
                200,
                json!({"data":{"viewer":{"id":"U_test","login":"ann"}}}).to_string(),
            );
        }
        if request.line.starts_with("GET /user/memberships/orgs/acme ") {
            return (
                404,
                json!({"message":"dummy-test-only-bound-token"}).to_string(),
            );
        }
        let body = if request.line.starts_with("GET /repos/acme/public ") {
            json!({"full_name":"acme/public","default_branch":"trunk","owner":{"type":"Organization"},"permissions":{"pull":true},"private":false})
        } else {
            assert!(request.line.contains("/commits/"));
            json!({"sha":"a".repeat(40)})
        };
        (200, body.to_string())
    }));
    let bound = client(url, &credential).bind_read("ann").unwrap();
    let error = bound
        .resolve_repository("acme/public", &RepositoryRef::DefaultBranch)
        .unwrap_err();
    assert!(matches!(
        error,
        subspace_core::http::Error::Status { status: 404, .. }
    ));
    assert!(!error.to_string().contains("dummy-test-only"));
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[test]
fn each_operation_checks_membership_and_repository_before_commit_or_download() {
    let memberships = Arc::new(AtomicUsize::new(0));
    let repositories = Arc::new(AtomicUsize::new(0));
    let (members_seen, repos_seen) = (memberships.clone(), repositories.clone());
    let commit = "a".repeat(40);
    let expected_commit = commit.clone();
    let url = serve(Arc::new(move |request| {
        assert_eq!(request.authorization, "Bearer dummy-test-only-token");
        let body = if request.body.contains("ViewerBound") {
            json!({"data":{"viewer":{"id":"U_test","login":"ann"}}})
        } else if request.line.starts_with("GET /user/memberships/orgs/acme ") {
            members_seen.fetch_add(1, Ordering::SeqCst);
            json!({"state":"active","organization":{"login":"acme"}})
        } else if request.line.starts_with("GET /repos/acme/board ") {
            repos_seen.fetch_add(1, Ordering::SeqCst);
            json!({"full_name":"acme/board","default_branch":"trunk","owner":{"type":"Organization"},"permissions":{"pull":true}})
        } else if request.line.contains("/commits/") {
            assert!(request.line.contains("refs%2Fheads%2Ftrunk"));
            json!({"sha":expected_commit})
        } else {
            assert!(request.line.contains("/zipball/"));
            json!({"dummy":"not a real ZIP; tests native client bytes only"})
        };
        (200, body.to_string())
    }));
    let bound = client(url, &token("dummy-test-only-token"))
        .bind_read("ann")
        .unwrap();
    let resolved = bound
        .resolve_repository("acme/board", &RepositoryRef::DefaultBranch)
        .unwrap();
    assert_eq!(resolved.commit, commit);
    assert_eq!(resolved.reference, RepositoryRef::Branch("trunk".into()));
    assert!(
        !bound
            .download_repository("acme/board", &commit)
            .unwrap()
            .is_empty()
    );
    assert_eq!(memberships.load(Ordering::SeqCst), 2);
    assert_eq!(repositories.load(Ordering::SeqCst), 2);
}

#[test]
fn organization_repository_and_release_discovery_stay_bound_to_one_credential() {
    let url = serve(Arc::new(move |request| {
        assert_eq!(request.authorization, "Bearer dummy-test-only-token");
        if request.line.contains("/releases/assets/") {
            assert_eq!(request.accept, "application/octet-stream");
        }
        (200, discovery_reply(&request.line).to_string())
    }));
    let bound = client(url, &token("dummy-test-only-token"))
        .bind_read("ann")
        .unwrap();
    assert_eq!(bound.organizations().unwrap()[0].login, "acme");
    assert_eq!(
        bound.repositories("acme").unwrap()[0].full_name,
        "acme/board"
    );
    assert_eq!(bound.package_releases("acme/board").unwrap()[0].id, 7);
    assert_eq!(
        bound.package_release("acme/board", 7).unwrap().tag_name,
        "v1"
    );
    assert!(
        !bound
            .download_release("acme/board", 7, "board.zip", false)
            .unwrap()
            .is_empty()
    );
    assert!(
        bound
            .download_release("acme/board", 7, "another.zip", false)
            .is_err()
    );
}

fn discovery_reply(line: &str) -> serde_json::Value {
    let path = line
        .split_whitespace()
        .nth(1)
        .unwrap()
        .split('?')
        .next()
        .unwrap();
    let membership = json!({"state":"active","organization":{"login":"acme"}});
    let release = json!({"id":7,"tag_name":"v1","draft":false,"prerelease":false,"published_at":"2026-10-01T00:00:00Z","assets":[{"id":42,"name":"board.zip","size":10}]});
    match path {
        "/graphql" => json!({"data":{"viewer":{"id":"U_test","login":"ann"}}}),
        "/user/memberships/orgs" => json!([membership]),
        "/user/memberships/orgs/acme" => membership,
        "/orgs/acme/repos" => json!([{"full_name":"acme/board","default_branch":"trunk"}]),
        "/repos/acme/board/releases/assets/42" => json!({"dummy":"test-only asset bytes"}),
        "/repos/acme/board" => {
            json!({"full_name":"acme/board","default_branch":"trunk","owner":{"type":"Organization"},"permissions":{"pull":true}})
        }
        "/repos/acme/board/releases/7" => release,
        "/repos/acme/board/releases" => json!([release]),
        _ => panic!("Unexpected package fixture request"),
    }
}

#[test]
fn pagination_fails_bounded_instead_of_returning_an_incomplete_discovery() {
    let pages = Arc::new(AtomicUsize::new(0));
    let seen = pages.clone();
    let url = serve(Arc::new(move |request| {
        let body = if request.body.contains("ViewerBound") {
            json!({"data":{"viewer":{"id":"U_test","login":"ann"}}})
        } else if request.line.contains("/user/memberships/orgs/acme ") {
            json!({"state":"active","organization":{"login":"acme"}})
        } else {
            assert!(request.line.contains("/orgs/acme/repos?"));
            seen.fetch_add(1, Ordering::SeqCst);
            json!(vec![
                json!({"full_name":"acme/test-only-board","default_branch":"trunk"});
                100
            ])
        };
        (200, body.to_string())
    }));
    let bound = client(url, &token("dummy-test-only-token"))
        .bind_read("ann")
        .unwrap();
    assert!(
        bound
            .repositories("acme")
            .unwrap_err()
            .to_string()
            .contains("1,000")
    );
    assert_eq!(pages.load(Ordering::SeqCst), 10);
}

#[test]
fn repository_archive_uses_github_json_accept_not_release_asset_media_type() {
    let commit = "a".repeat(40);
    let url = serve(Arc::new(move |request| {
        assert_eq!(request.authorization, "Bearer dummy-test-only-token");
        if request.line.contains("/zipball/") {
            if request.accept != "application/vnd.github+json" {
                return (
                    415,
                    json!({"message":"test-only unsupported Accept"}).to_string(),
                );
            }
            return (200, "test-only archive bytes".into());
        }
        (200, discovery_reply(&request.line).to_string())
    }));
    let bound = client(url, &token("dummy-test-only-token"))
        .bind_read("ann")
        .unwrap();
    assert_eq!(
        bound.download_repository("acme/board", &commit).unwrap(),
        b"test-only archive bytes"
    );
}
