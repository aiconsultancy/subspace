//! A message's Make issue and Record decision drafts and their choices.

use super::super::quote::{self as message, Kind, Quoted};
use super::super::write;
use crate::fixtures::at;

fn quoted(text: &str) -> Quoted {
    Quoted {
        author: "mira".into(),
        time: at(13),
        text: text.into(),
        msgid: Some("fixture-msg-1".into()),
        place: "#aurora-lyra on Acme IRC".into(),
    }
}

#[test]
fn a_message_becomes_an_issue_quoted_as_text() {
    let message =
        quoted("Pan stalls ```here``` @[Ada](member:00000000-0000-0000-0000-000000000001)");
    let draft = message::issue_draft(&message, "NAV", "  Pan stalls  ").unwrap();
    assert_eq!(draft.title, "Pan stalls");
    assert_eq!(draft.project, "NAV");
    assert!(
        draft.description.starts_with("````text\n"),
        "a fence longer than any in the text"
    );
    assert!(draft.description.contains("\n````\n\nFrom IRC message fixture-msg-1 by mira in #aurora-lyra on Acme IRC, 2026-11-13 09:13 UTC."));
    assert!(draft.summary.chars().count() <= write::SUMMARY_CHARS);
    assert!(message::issue_draft(&message, "NAV", "   ").is_err());
    assert!(message::issue_draft(&message, "NAV", &"x".repeat(201)).is_err());
}

#[test]
fn a_message_s_decision_keeps_a_reference_back_to_it() {
    let draft = message::decision_draft(
        &quoted("Decode tiles on a worker pool."),
        "NAV-2",
        "Decode on a worker pool",
        "Recorded in Channels from #aurora-lyra on Acme IRC.".into(),
    )
    .unwrap();
    assert_eq!(draft.text, "Decode tiles on a worker pool.");
    let (reference, excerpt) = draft.message.unwrap();
    assert!(reference.contains("fixture-msg-1") && reference.contains("#aurora-lyra"));
    assert_eq!(excerpt, "mira: Decode tiles on a worker pool.");
}

#[test]
fn the_dialog_chooses_the_message_s_only_issue_or_project() {
    let named = ["NAV-2".to_owned()];
    let in_play = ["MAP-3".to_owned(), "NAV-7".to_owned()];
    let projects = ["NAV".to_owned(), "MAP".to_owned()];
    assert_eq!(
        message::choices(Kind::RecordDecision, &named, &in_play, &projects),
        (
            vec!["NAV-2".into(), "MAP-3".into(), "NAV-7".into()],
            Some("NAV-2".into())
        )
    );
    let two = ["NAV-2".to_owned(), "NAV-7".to_owned()];
    assert_eq!(
        message::choices(Kind::RecordDecision, &two, &[], &projects).1,
        None,
        "two named issues: the person picks"
    );
    assert_eq!(
        message::choices(Kind::MakeIssue, &[], &in_play, &projects)
            .1
            .as_deref(),
        Some("MAP"),
        "the newest issue in play's project"
    );
    assert_eq!(
        message::choices(Kind::MakeIssue, &named, &in_play, &projects)
            .1
            .as_deref(),
        Some("NAV")
    );
}
