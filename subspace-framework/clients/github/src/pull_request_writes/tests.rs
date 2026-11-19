use serde_json::{Value, json};
use subspace_client_test_support::Mock;

use crate::tests::{client, token};

#[test]
fn rest_comment_and_close_use_selected_account_and_exact_bodies() {
    let mock = Mock::new(|request| {
        assert!(
            request
                .headers
                .iter()
                .any(|(key, value)| key == "authorization" && value == "Bearer selected")
        );
        assert!(
            request
                .headers
                .iter()
                .any(|(key, value)| key == "x-github-api-version" && value == "2022-11-28")
        );
        (200, "{}".into())
    });
    let client = client(mock.base_url().into(), &token("selected"));
    assert!(
        client
            .issue_comment("ann", "o/r#42", "  Closing this.\nThanks.  ")
            .is_ok()
    );
    assert!(client.close_pull_request("ann", "o/r#42").is_ok());
    let requests = mock.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(
        (&*requests[0].method, &*requests[0].path),
        ("POST", "/repos/o/r/issues/42/comments")
    );
    assert_eq!(
        serde_json::from_str::<Value>(&requests[0].body).unwrap(),
        json!({"body": "Closing this.\nThanks."})
    );
    assert_eq!(
        (&*requests[1].method, &*requests[1].path),
        ("PATCH", "/repos/o/r/pulls/42")
    );
    assert_eq!(
        serde_json::from_str::<Value>(&requests[1].body).unwrap(),
        json!({"state":"closed"})
    );
}

#[test]
fn rejected_writes_are_mapped_and_never_retried() {
    for status in [400, 401, 403, 422, 500] {
        let mock = Mock::new(move |_| {
            (
                status,
                json!({"message":"private response details"}).to_string(),
            )
        });
        let client = client(mock.base_url().into(), &token("selected"));
        assert!(client.issue_comment("ann", "o/r#42", "Comment").is_err());
        assert!(client.close_pull_request("ann", "o/r#42").is_err());
        assert_eq!(mock.requests().len(), 2, "HTTP {status} must not retry");
    }
}

#[test]
fn invalid_inputs_make_no_requests() {
    let mock = Mock::new(|_| panic!("invalid inputs must not reach GitHub"));
    let client = client(mock.base_url().into(), &token("selected"));
    assert!(
        client
            .close_pull_request("invalid account", "o/r#42")
            .is_err()
    );
    assert!(client.close_pull_request("ann", "bad/key#0").is_err());
    assert!(client.issue_comment("ann", "o/r#42", "  ").is_err());
    assert!(
        client
            .issue_comment("ann", "o/r#42", &"x".repeat(65537))
            .is_err()
    );
    assert!(mock.requests().is_empty());
}
