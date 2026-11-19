use super::{client, common};
use crate::{Client, Error, fixture_from_env};
use subspace_framework::client_test_support::Mock;
use subspace_framework::core::Secret;

#[test]
fn user_agent_accept_and_no_authorization_org_or_cookie_headers() {
    let mock = Mock::with_headers(|request| {
        let (status, body) = common(request);
        (
            status,
            vec![("Set-Cookie".into(), "fixture=1".into())],
            body,
        )
    });
    let api = client(&mock);
    assert_eq!(api.me().unwrap().user_id, "me");
    assert!(api.me().is_ok());
    for request in mock.requests() {
        assert_eq!(request.path, "/api/v1/me");
        assert!(request.headers.contains(&(
            "user-agent".into(),
            format!("Subspace/{}", env!("CARGO_PKG_VERSION"))
        )));
        assert!(
            request
                .headers
                .contains(&("accept".into(), "application/json".into()))
        );
        assert!(
            !request
                .headers
                .iter()
                .any(|(key, _)| matches!(key.as_str(), "authorization" | "x-org-id" | "cookie"))
        );
    }
}

#[test]
fn only_explicit_debug_fixture_sends_access_assertion() {
    let mock = Mock::new(common);
    let api = Client::loopback(
        &format!("{}/api", mock.base_url()),
        Some(Secret::new("fixture-only-jwt")),
    )
    .unwrap();
    assert!(api.me().is_ok());
    assert!(
        mock.requests()[0]
            .headers
            .contains(&("authorization".into(), "Bearer fixture-only-jwt".into()))
    );
    assert_eq!(format!("{:?}", Secret::new("fixture-only-jwt")), "***");
}

#[test]
fn access_redirect_is_detected_without_following_it() {
    let mock = Mock::with_headers(|_| {
        (
            302,
            vec![(
                "Location".into(),
                "https://login.example.test/edge/login".into(),
            )],
            "".into(),
        )
    });
    assert!(matches!(client(&mock).me(), Err(Error::SignInNeeded)));
    assert_eq!(mock.requests().len(), 1);
}

#[test]
fn plain_401_403_and_problem_401_need_sign_in_but_problem_403_is_forbidden() {
    for status in [401, 403] {
        for mime in [
            "text/html",
            "application/json",
            "application/problem+json; charset=utf-8",
        ] {
            let mock = Mock::with_headers(move |_| {
                (
                    status,
                    vec![("content-type".into(), mime.into())],
                    "opaque error".into(),
                )
            });
            let expected = if status == 403 && mime.starts_with("application/problem+json") {
                Error::Forbidden
            } else {
                Error::SignInNeeded
            };
            assert_eq!(client(&mock).me().unwrap_err(), expected);
        }
    }
}

#[test]
fn unrelated_redirects_are_refused_and_never_followed() {
    let target = Mock::new(common);
    let location = format!("{}/api/v1/me", target.base_url());
    let mock = Mock::with_headers(move |_| {
        (
            307,
            vec![("Location".into(), location.clone())],
            String::new(),
        )
    });
    assert!(matches!(
        client(&mock).me(),
        Err(Error::UnexpectedStatus(307))
    ));
    assert!(target.requests().is_empty());
}

#[test]
fn invalid_segments_never_reach_the_transport() {
    let mock = Mock::new(common);
    for reference in ["", "../me", "SUB-1?x=1", "SUB%2f1", "with space"] {
        assert!(matches!(
            client(&mock).issue(reference),
            Err(Error::InvalidRequest)
        ));
    }
    assert!(mock.requests().is_empty());
}

#[test]
fn json_is_bounded_and_unknown_fields_are_forward_compatible() {
    let oversized = Mock::new(|_| {
        (
            200,
            "x".repeat(crate::transport::MAX_JSON_BYTES as usize + 1),
        )
    });
    assert!(matches!(
        client(&oversized).me(),
        Err(Error::InvalidResponse)
    ));
    let unknown = Mock::new(|_| {
        (200, r#"{"userId":"me","kind":"future-kind","displayName":"Ada Fixture","futureField":true}"#.into())
    });
    assert_eq!(client(&unknown).me().unwrap().kind, "future-kind");
}

#[test]
fn fixture_environment_is_debug_only_numeric_loopback_and_credential_free() {
    assert!(fixture_from_env(false, Some("http://127.0.0.1/api")).is_none());
    assert!(fixture_from_env(true, Some("http://127.0.0.1/api")).is_some());
    assert!(fixture_from_env(true, Some("http://[::1]/api")).is_some());
    for url in [
        "http://localhost/api",
        "https://example.invalid/api",
        "http://127.0.0.1.evil.invalid/api",
        "http://u:p@127.0.0.1/api",
        "http://127.0.0.1/api?token=x",
        "http://127.0.0.1/api#secret",
    ] {
        assert!(fixture_from_env(true, Some(url)).is_none());
    }
    assert_eq!(
        crate::transport::READ_TIMEOUT,
        std::time::Duration::from_secs(45)
    );
}
