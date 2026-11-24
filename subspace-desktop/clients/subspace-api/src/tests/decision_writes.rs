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
            Err(DecisionWriteError::Text),
            "the decision's own words, not the title"
        );
    }
    // 25,001 emoji are 50,002 UTF-16 units, over the API's limit.
    let wide = "\u{1F680}".repeat(25_001);
    assert_eq!(
        NewDecision::new("project-1", &[], &text(&wide, None)),
        Err(DecisionWriteError::Text)
    );
    let long = "x".repeat(201);
    let titled = DecisionText {
        title: &long,
        ..text("fine", None)
    };
    assert_eq!(
        NewDecision::new("project-1", &[], &titled),
        Err(DecisionWriteError::Invalid)
    );
    // 101 emoji are 101 characters but 202 UTF-16 units, as the API counts.
    let emoji = "\u{1F680}".repeat(101);
    let wide = DecisionText {
        title: &emoji,
        ..text("fine", None)
    };
    assert_eq!(
        NewDecision::new("project-1", &[], &wide),
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

#[test]
fn desktop_edits_and_reviews_send_the_shown_version_once() {
    use crate::decision_writes::{DecisionChanges, DecisionReview};

    let mock = Mock::new(|request| {
        if request.path.ends_with("/status") {
            (409, "{}".into())
        } else {
            (200, detail().to_string())
        }
    });
    let api = client(&mock);
    let changes = DecisionChanges {
        context: Some(String::new()),
        supersedes_id: Some(None),
        ..DecisionChanges::default()
    };
    assert!(api.edit_decision("decision-1", 7, &changes).is_ok());
    assert_eq!(
        api.review_decision("decision-1", 8, DecisionReview::Reject)
            .err(),
        Some(DecisionWriteError::Conflict)
    );
    let requests = mock.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(
        (requests[0].method.as_str(), requests[0].path.as_str()),
        ("PUT", "/api/v1/decisions/decision-1")
    );
    assert_eq!(
        serde_json::from_str::<Value>(&requests[0].body).unwrap(),
        json!({"context":"","supersedesId":null,"version":7})
    );
    assert_eq!(
        (requests[1].method.as_str(), requests[1].path.as_str()),
        ("PUT", "/api/v1/decisions/decision-1/status")
    );
    assert_eq!(
        serde_json::from_str::<Value>(&requests[1].body).unwrap(),
        json!({"status":"rejected","version":8})
    );
}
