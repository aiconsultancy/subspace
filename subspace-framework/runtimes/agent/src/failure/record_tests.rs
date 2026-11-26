//! The log records and redaction of a failed turn.

use serde_json::json;

#[cfg(feature = "copilot")]
use super::log::outcome;
use super::tests::turn;
use super::{FailureCategory, TurnFailure};
#[cfg(feature = "copilot")]
use super::{log_failure, log_turn_done};

/// The last `agent` record for chat `chat`.
#[cfg(feature = "copilot")]
fn record_of(chat: &str) -> subspace_core::log::Record {
    subspace_core::log::recent()
        .into_iter()
        .rev()
        .find(|record| {
            record.target == "agent"
                && record
                    .fields
                    .iter()
                    .any(|(key, value)| key == "chat" && value == chat)
        })
        .expect("an agent record")
}

// Fake credentials, split so secret scanners do not flag the source.
#[cfg(feature = "copilot")]
const TOKEN: &str = concat!("ghp_", "abcdefghijklmnopqrstuvwxyz0123456789");
#[cfg(feature = "copilot")]
const BEARER_VALUE: &str = concat!("abc.def.", "ghijklmnop");

#[cfg(feature = "copilot")]
#[test]
fn credentials_and_the_turns_own_text_are_removed_from_the_details_and_the_record() {
    let prompt = "Fix my config with the release plan";
    let context = "proxy settings of the build agent";
    let mut sent = turn();
    sent.begin(1, vec![prompt.into(), context.into(), "short".into()]);
    let data = json!({
        "errorType": "query",
        "message": format!(
            "Echoed: {prompt}. Context: {context}. Authorization: Bearer {BEARER_VALUE} and {TOKEN}"
        ),
        "providerCallId": format!("call-{TOKEN}"),
        "errorCode": format!("Bearer {BEARER_VALUE}"),
        "url": format!("https://example.invalid/limits?token={TOKEN}"),
        "originatingMessageId": format!("echo {prompt}")
    });
    let failure = TurnFailure::from_session_error(&data, &sent, Some("1.0.105"));
    log_failure(7_301, &failure);
    let record = record_of("7301");
    assert_eq!(record.level, subspace_core::log::Level::Error);
    assert_eq!(record.message, "turn failed");
    let line = record.line();
    let details = failure.details_text();
    for shown in [&details, &line, &format!("{:?}", failure.url)] {
        for hidden in [TOKEN, BEARER_VALUE, prompt, context] {
            assert!(!shown.contains(hidden), "{hidden} in {shown}");
        }
    }
    assert!(details.contains("[message text removed]"), "{details}");
    assert!(line.contains("category=no-response"), "{line}");
}

#[test]
fn a_submitted_text_shorter_than_four_characters_is_kept() {
    let mut sent = turn();
    sent.begin(0, vec!["fix".into(), "Fix it".into()]);
    let data = json!({"errorType": "query", "message": "Fix it failed; fix later"});
    let failure = TurnFailure::from_session_error(&data, &sent, None);
    assert_eq!(
        failure.message, "[message text removed] failed; fix later",
        "four characters or more are removed, three are kept"
    );
}

#[cfg(feature = "copilot")]
#[test]
fn a_turn_done_record_says_how_the_turn_ended() {
    assert_eq!(outcome(None), "done");
    assert_eq!(outcome(Some(FailureCategory::Stopped)), "stopped");
    assert_eq!(
        outcome(Some(FailureCategory::NotResponding)),
        "not_responding"
    );
    assert_eq!(outcome(Some(FailureCategory::NoResponse)), "failed");
    // Debug records pass only at the debug level; nothing in this crate's
    // tests depends on a quieter level.
    subspace_core::log::set_level(subspace_core::log::Level::Debug);
    log_turn_done(7_303, &turn(), Some(FailureCategory::Limit));
    let record = record_of("7303");
    assert_eq!(record.message, "turn done");
    assert!(
        record
            .fields
            .contains(&("outcome".to_owned(), "failed".to_owned())),
        "{:?}",
        record.fields
    );
}

#[test]
fn a_refused_send_keeps_its_own_error_type() {
    let failure = TurnFailure::rejected("send_rejected", "Refused", &turn(), None);
    assert_eq!(failure.category, FailureCategory::Unknown);
    assert_eq!(failure.error_type, "send_rejected");
    assert_eq!(failure.message, "Refused");
}
