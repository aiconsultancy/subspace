use super::{client, serve, token};
use crate::RepositoryRef;
use serde_json::json;
use std::sync::Arc;
use subspace_core::http::Error;

fn resolve_error(path: &'static str, status: u16, body: &'static str) -> Error {
    let url = serve(Arc::new(move |request| {
        if request.line.contains(path) {
            return (status, body.into());
        }
        let value = if request.body.contains("ViewerBound") {
            json!({"data":{"viewer":{"id":"U_test","login":"ann"}}})
        } else if request.line.contains("/user/memberships/orgs/") {
            json!({"state":"active","organization":{"login":"acme"}})
        } else {
            json!({"full_name":"acme/board","default_branch":"trunk","owner":{"type":"Organization"},"permissions":{"pull":true}})
        };
        (200, value.to_string())
    }));
    client(url, &token("dummy-test-only-token"))
        .bind_read("ann")
        .unwrap()
        .resolve_repository("acme/board", &RepositoryRef::DefaultBranch)
        .unwrap_err()
}

#[test]
fn package_failures_identify_membership_repository_or_reference_without_response_text() {
    let cases = [
        ("/user/memberships/orgs/acme ", "organization membership"),
        ("/repos/acme/board ", "repository"),
        ("/commits/", "branch, tag or commit"),
    ];
    for (path, context) in cases {
        let error = resolve_error(
            path,
            404,
            r#"{"message":"dummy-test-only-response-secret"}"#,
        );
        assert!(matches!(error, Error::Status { status: 404, .. }));
        assert!(error.to_string().contains(context), "{error}");
        assert!(!error.to_string().contains("dummy-test-only"));
    }
}

#[test]
fn malformed_package_json_names_the_failed_stage_without_echoing_the_body() {
    for (path, context) in [
        ("/user/memberships/orgs/acme ", "organization membership"),
        ("/repos/acme/board ", "repository"),
        ("/commits/", "branch, tag or commit"),
    ] {
        let error = resolve_error(path, 200, "dummy-test-only malformed response");
        assert!(error.to_string().contains("malformed"), "{error}");
        assert!(error.to_string().contains(context), "{error}");
        assert!(!error.to_string().contains("dummy-test-only"));
    }
}

#[test]
fn package_status_errors_distinguish_auth_proxy_rate_limit_and_service_failures() {
    for (status, context) in [
        (401, "credential"),
        (403, "repository read access"),
        (407, "proxy"),
        (415, "requested download format"),
        (422, "GitHub rejected the request"),
        (429, "rate limit"),
        (503, "unavailable"),
    ] {
        let error = resolve_error("/repos/acme/board ", status, "dummy-test-only error body");
        assert!(matches!(error, Error::Status { status: code, .. } if code == status));
        assert!(error.to_string().contains(context), "{error}");
        assert!(!error.to_string().contains("dummy-test-only"));
    }
}
