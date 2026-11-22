//! Recording a decision from Channels: what is sent, and how each refusal
//! reads. Fictional values only.
use super::client;
use super::decisions::detail;
use crate::decision_writes::{DecisionText, DecisionWriteError, IrcEvidence, NewDecision};
use serde_json::{Value, json};
use subspace_framework::client_test_support::Mock;

fn text<'a>(decision: &'a str, message: Option<IrcEvidence<'a>>) -> DecisionText<'a> {
    DecisionText {
        title: "Decode tiles on a worker pool",
        decision,
        context: Some("From #aurora-lyra."),
        decided_on: "2026-11-22",
        message,
    }
}

#[test]
fn a_decision_posts_once_as_the_signed_in_account_with_its_irc_evidence() {
    let mock = Mock::new(|_| (201, detail().to_string()));
    let evidence = IrcEvidence {
        reference: "irc://acme/#aurora-lyra msgid=fixture-1",
        excerpt: "mira: decode tiles on a worker pool",
    };
    let decision = NewDecision::new(
        "project-1",
        &["issue-2"],
        &text("Decode tiles on a worker pool.", Some(evidence)),
    )
    .unwrap();
    let recorded = client(&mock).log_decision(&decision).unwrap();
    assert_eq!(recorded.title, "Use the decision client");
    let requests = mock.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].method, "POST");
    assert_eq!(requests[0].path, "/api/v1/decisions");
    assert_eq!(
        serde_json::from_str::<Value>(&requests[0].body).unwrap(),
        json!({
            "projectId": "project-1",
            "title": "Decode tiles on a worker pool",
            "decidedOn": "2026-11-22",
            "decision": "Decode tiles on a worker pool.",
            "context": "From #aurora-lyra.",
            "issueIds": ["issue-2"],
            "evidenceKind": "ircMessage",
            "evidenceReference": "irc://acme/#aurora-lyra msgid=fixture-1",
            "evidenceExcerpt": "mira: decode tiles on a worker pool"
        }),
        "no maker provenance: the server records the signed-in account"
    );
}

#[test]
fn invalid_decisions_are_refused_before_anything_is_sent() {
    for bad in ["", "   ", "nul\0"] {
        assert_eq!(
            NewDecision::new("project-1", &[], &text(bad, None)),
            Err(DecisionWriteError::Invalid)
        );
    }
    let long = "x".repeat(201);
    let titled = DecisionText {
        title: &long,
        ..text("fine", None)
    };
    assert_eq!(
        NewDecision::new("project-1", &[], &titled),
        Err(DecisionWriteError::Invalid)
    );
    let dated = DecisionText {
        decided_on: "22/11/2026",
        ..text("fine", None)
    };
    assert_eq!(
        NewDecision::new("project-1", &[], &dated),
        Err(DecisionWriteError::Invalid)
    );
}

#[test]
fn refusals_read_in_plain_words_and_are_never_replayed() {
    for (status, expected) in [
        (400, DecisionWriteError::Refused),
        (401, DecisionWriteError::SignIn),
        (403, DecisionWriteError::Forbidden),
        (404, DecisionWriteError::Unavailable),
        (409, DecisionWriteError::ProjectArchived),
        (429, DecisionWriteError::RateLimited),
        (500, DecisionWriteError::Unconfirmed),
    ] {
        let mock = Mock::with_headers(move |_| {
            (
                status,
                vec![("Content-Type".into(), "application/problem+json".into())],
                json!({"detail":"fixture"}).to_string(),
            )
        });
        let decision = NewDecision::new("project-1", &[], &text("fine", None)).unwrap();
        assert_eq!(client(&mock).log_decision(&decision).err(), Some(expected));
        assert_eq!(mock.requests().len(), 1);
        assert!(!expected.to_string().contains("fixture"));
    }
}
