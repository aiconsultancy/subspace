use serde_json::json;
use subspace_framework::client_github::Client;
use subspace_framework::client_github::model::{ItemKind, PrState};
use subspace_framework::client_test_support::Mock;

use super::decision::{Submission, opening_allowed, validate_submission};
use super::is_closable;
use super::write::{WriteError, comment_to_post, failure_message, write};
use crate::sample;

fn client(mock: &Mock) -> Client {
    let mut config = subspace_framework::core::config::Config::default();
    config.github.token = Some(subspace_framework::core::Secret::new("fixture"));
    Client::new(subspace_framework::agent_tools::test_support::credentials(
        &config,
    ))
    .with_api_url(mock.base_url())
}

#[test]
fn only_open_pull_requests_including_drafts_are_closable() {
    let mut item = sample::items().remove(0);
    for kind in [ItemKind::Pr, ItemKind::Issue] {
        for state in [
            PrState::Open,
            PrState::Draft,
            PrState::Closed,
            PrState::Merged,
        ] {
            item.kind = kind;
            item.state = state;
            assert_eq!(
                is_closable(&item),
                kind == ItemKind::Pr && matches!(state, PrState::Open | PrState::Draft)
            );
        }
    }
}

#[test]
fn comment_is_optional_but_has_the_client_byte_limit() {
    for text in ["", " \n ", &"x".repeat(65536)] {
        assert_eq!(
            validate_submission(false, false, text, true),
            Submission::Ready
        );
    }
    assert_eq!(
        validate_submission(false, false, &"é".repeat(32769), true),
        Submission::Invalid("Keep the comment within 64 KB.")
    );
}

#[test]
fn busy_and_composing_submissions_are_ignored_before_validation() {
    for (writing, composing) in [(true, false), (false, true), (true, true)] {
        assert_eq!(
            validate_submission(writing, composing, &"x".repeat(65537), false),
            Submission::Ignore
        );
    }
    assert_eq!(
        validate_submission(false, false, "", false),
        Submission::Invalid("GitHub is unavailable. Check the account connection and try again.")
    );
}

#[test]
fn opening_requires_no_write_or_existing_confirmation() {
    for (writing, confirming, expected) in [
        (false, false, true),
        (true, false, false),
        (false, true, false),
        (true, true, false),
    ] {
        assert_eq!(opening_allowed(writing, confirming), expected);
    }
}

#[test]
fn only_an_unposted_nonblank_comment_is_sent_with_its_original_text() {
    for text in ["", " \n ", " A comment\n "] {
        assert_eq!(comment_to_post(text, true), None);
    }
    assert_eq!(comment_to_post("", false), None);
    assert_eq!(comment_to_post(" \n ", false), None);
    assert_eq!(
        comment_to_post(" A comment\n ", false),
        Some(" A comment\n ")
    );
}

#[test]
fn credential_failures_explain_sign_in_without_exposing_the_raw_error() {
    use subspace_framework::core::{credentials::CredentialError, http::Error};
    for failure in [
        WriteError::Comment(Error::Credential(CredentialError(
            "private credential details".into(),
        ))),
        WriteError::Close(Error::Credential(CredentialError(
            "private credential details".into(),
        ))),
    ] {
        for posted in [false, true] {
            let message = failure_message(posted, &failure);
            assert!(message.contains(
                "No GitHub account is signed in for this board. Sign in with gh, then try again."
            ));
            assert!(!message.contains("private"));
            assert!(!message.contains("repository permissions"));
        }
    }
}

#[test]
fn comment_then_close_and_close_only_have_exact_request_order() {
    for comment in ["Closing this.\nThanks.", " \n "] {
        let mock = Mock::new(|_| (200, "{}".into()));
        let result = write(&client(&mock), "ann", "o/r#42", comment, false);
        assert!(result.result.is_ok());
        let requests = mock.requests();
        let sequence: Vec<_> = requests
            .iter()
            .map(|r| (r.method.as_str(), r.path.as_str()))
            .collect();
        let mut expected = Vec::new();
        if !comment.trim().is_empty() {
            expected.push(("POST", "/repos/o/r/issues/42/comments"));
        }
        expected.push(("PATCH", "/repos/o/r/pulls/42"));
        assert_eq!(sequence, expected);
        assert_eq!(result.posted, !comment.trim().is_empty());
    }
}

#[test]
fn failed_comment_stops_before_close_and_keeps_an_authored_error() {
    let mock = Mock::new(|_| (403, json!({"message":"private response body"}).to_string()));
    let result = write(&client(&mock), "ann", "o/r#42", "My draft", false);
    assert!(!result.posted);
    let error = result.result.err().unwrap();
    assert!(matches!(error, WriteError::Comment(_)));
    assert!(failure_message(false, &error).contains("comment was not posted"));
    assert!(!failure_message(false, &error).contains("private"));
    let requests = mock.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].method, "POST");
}

#[test]
fn failed_close_retains_posted_state_and_retry_does_not_comment_again() {
    for status in [403, 422] {
        let mock = Mock::new(move |request| {
            if request.method == "PATCH" {
                (
                    status,
                    json!({"message":"private response body"}).to_string(),
                )
            } else {
                (201, "{}".into())
            }
        });
        let client = client(&mock);
        let first = write(&client, "ann", "o/r#42", "My draft", false);
        assert!(first.posted);
        let message = failure_message(first.posted, &first.result.err().unwrap());
        assert!(message.contains("comment was posted"));
        assert!(message.contains("still open"));
        assert!(!message.contains("private"));
        assert_eq!(mock.requests().len(), 2);
        let retry = write(&client, "ann", "o/r#42", "My draft", first.posted);
        assert!(retry.result.is_err());
        let methods: Vec<_> = mock.requests().into_iter().map(|r| r.method).collect();
        assert_eq!(methods, ["POST", "PATCH", "PATCH"]);
    }
}

mod ui;
