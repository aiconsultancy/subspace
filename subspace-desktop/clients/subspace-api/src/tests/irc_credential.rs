//! The IRC sign-in the API issues: what is sent, what is accepted, and how
//! an API without the feature or a failing one reads. Fictional values only.
use super::client;
use crate::{Client, Error};
use subspace_framework::client_test_support::Mock;
use subspace_framework::core::Secret;

const ISSUED: &str =
    r#"{"account":"ada","password":"sirc1.xxxxxxxx","expiresAt":"2026-11-23T21:10:00Z"}"#;

#[test]
fn a_credential_is_one_post_as_the_signed_in_person() {
    let mock = Mock::new(|_| (200, ISSUED.into()));
    let api = Client::loopback(
        &format!("{}/api", mock.base_url()),
        Some(Secret::new("fixture-only-jwt")),
    )
    .unwrap();
    let credential = api.irc_credential().unwrap();
    assert_eq!(credential.account, "ada");
    assert_eq!(credential.password.expose(), "sirc1.xxxxxxxx");
    assert_eq!(
        credential.expires_at.to_rfc3339(),
        "2026-11-23T21:10:00+00:00"
    );
    let requests = mock.requests();
    assert_eq!(requests.len(), 1, "never retried");
    assert_eq!(requests[0].method, "POST");
    assert_eq!(requests[0].path, "/api/v1/me/irc-credential");
    assert!(
        requests[0]
            .headers
            .contains(&("authorization".into(), "Bearer fixture-only-jwt".into()))
    );
}

#[test]
fn the_password_never_reaches_a_formatted_credential() {
    let mock = Mock::new(|_| (200, ISSUED.into()));
    let credential = client(&mock).irc_credential().unwrap();
    let shown = format!("{credential:?}");
    assert!(shown.contains("ada"), "{shown}");
    assert!(!shown.contains("xxxxxxxx"), "{shown}");
}

#[test]
fn an_api_without_the_feature_answers_not_found() {
    let mock = Mock::new(|_| (404, r#"{"title":"Not Found"}"#.into()));
    assert_eq!(client(&mock).irc_credential().unwrap_err(), Error::NotFound);
}

#[test]
fn refusals_and_failures_keep_their_meaning() {
    for (status, expected) in [
        (401, Error::SignInNeeded),
        (500, Error::UnexpectedStatus(500)),
        (503, Error::UnexpectedStatus(503)),
    ] {
        let mock = Mock::new(move |_| (status, "opaque".into()));
        assert_eq!(client(&mock).irc_credential().unwrap_err(), expected);
        assert_eq!(mock.requests().len(), 1);
    }
}

#[test]
fn an_unreachable_api_is_a_network_error() {
    let api = Client::loopback("http://127.0.0.1:9/api", None).unwrap();
    assert_eq!(api.irc_credential().unwrap_err(), Error::Network);
}

#[test]
fn a_credential_an_irc_line_cannot_carry_is_refused() {
    for body in [
        r#"{"account":"ada@laptop","password":"sirc1.x","expiresAt":"2026-11-23T21:10:00Z"}"#,
        r#"{"account":"","password":"sirc1.x","expiresAt":"2026-11-23T21:10:00Z"}"#,
        r#"{"account":"ada","password":"","expiresAt":"2026-11-23T21:10:00Z"}"#,
        r#"{"account":"ada","password":"two words","expiresAt":"2026-11-23T21:10:00Z"}"#,
        r#"{"account":"ada","password":"sirc1.x","expiresAt":"tomorrow"}"#,
        r#"{"account":"ada","password":"sirc1.x"}"#,
        "not json",
    ] {
        let mock = Mock::new(move |_| (200, body.into()));
        assert_eq!(
            client(&mock).irc_credential().unwrap_err(),
            Error::InvalidResponse,
            "{body}"
        );
    }
}

#[test]
fn the_caller_reports_a_username_when_the_api_allocates_one() {
    let with = Mock::new(|_| {
        (
            200,
            r#"{"userId":"me","kind":"human","displayName":"Ada Fixture","username":"ada"}"#.into(),
        )
    });
    assert_eq!(client(&with).me().unwrap().username.as_deref(), Some("ada"));
    let without = Mock::new(|_| {
        (
            200,
            r#"{"userId":"me","kind":"human","displayName":"Ada Fixture"}"#.into(),
        )
    });
    assert_eq!(client(&without).me().unwrap().username, None);
}

#[test]
fn nett_issues_no_irc_credentials_yet_and_is_not_asked() {
    let mock = Mock::new(|_| (200, ISSUED.into()));
    let api =
        Client::nett_loopback(&format!("{}/api", mock.base_url()), Some("org"), None).unwrap();
    assert_eq!(api.irc_credential().unwrap_err(), Error::NotFound);
    assert!(mock.requests().is_empty());
}
