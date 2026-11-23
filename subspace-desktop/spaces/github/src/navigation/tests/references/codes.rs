//! Which GitHub answers refuse access: a GraphQL `NOT_FOUND` does, a
//! query GitHub rejects does not.

use gpui_kit::TestAppContext;
use subspace_framework::client_test_support::Mock;
use subspace_framework::contracts::github::ResolutionState;

use super::super::KEY;
use super::checks::listed_only;
use super::{ask, routed};

/// What answers for the listed [`KEY`] after one reference read that
/// GitHub answered with GraphQL `errors` only.
fn after_graphql_errors(errors: &'static str, cx: &mut TestAppContext) -> ResolutionState {
    let mock = Mock::new(move |_| {
        let body = format!(r#"{{"data":{{"repository":null}},"errors":{errors}}}"#);
        (200, body)
    });
    let (board, _listener, cx) = routed(cx, Some(&mock));
    board.update(cx, listed_only);
    ask(KEY, cx);
    cx.run_until_parked();
    assert_eq!(mock.requests().len(), 1);
    ask(KEY, cx)
}

#[gpui_kit::test]
fn a_query_github_rejects_is_a_failure_not_a_refusal(cx: &mut TestAppContext) {
    let errors =
        r#"[{"message":"Field 'x' doesn't exist","extensions":{"code":"undefinedField"}}]"#;
    assert!(
        matches!(
            after_graphql_errors(errors, cx),
            ResolutionState::Available(_)
        ),
        "the held list still answers"
    );
}

#[gpui_kit::test]
fn a_graphql_not_found_is_a_refusal(cx: &mut TestAppContext) {
    let errors = r#"[{"type":"NOT_FOUND","message":"Could not resolve to a Repository with the name 'octo-org/api'."}]"#;
    assert!(matches!(
        after_graphql_errors(errors, cx),
        ResolutionState::Unavailable
    ));
}
