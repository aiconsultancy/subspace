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
    assert!(draft.description.contains("\n````\n\nFrom `IRC message fixture-msg-1 by mira in #aurora-lyra on Acme IRC, 2026-11-13 09:13 UTC`."));
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
    assert!(
        draft.text.starts_with(
            "```text\nDecode tiles on a worker pool.\n```\n\nFrom `IRC message fixture-msg-1"
        ),
        "fenced, with its reference in a code span: {}",
        draft.text
    );
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

#[test]
fn hostile_names_and_ids_stay_inside_a_code_span() {
    let hostile = Quoted {
        author: "[mira](https://evil.example)".into(),
        time: at(13),
        text: "Decode on a pool".into(),
        msgid: Some("`x` [id](https://evil.example)".into()),
        place: "#@[Ada](member:00000000-0000-0000-0000-000000000001) on Acme IRC".into(),
    };
    let draft = message::issue_draft(&hostile, "NAV", "Decode").unwrap();
    let reference = draft
        .description
        .rsplit_once("\n\nFrom ")
        .map(|(_, line)| line)
        .unwrap();
    assert!(
        reference.starts_with("``IRC message ") && reference.ends_with(" UTC``."),
        "one code span longer than the ID's backticks: {reference}"
    );
    assert_eq!(
        reference.matches("``").count(),
        2,
        "nothing closes it early"
    );
    let decision = message::decision_draft(&hostile, "NAV-2", "Decode", String::new()).unwrap();
    let (evidence, _) = decision.message.unwrap();
    assert!(evidence.starts_with("``IRC message ") && evidence.ends_with(" UTC``"));
    assert_eq!(
        message::code_span("#[docs](https://evil.example) on Acme IRC"),
        "`#[docs](https://evil.example) on Acme IRC`"
    );
    assert_eq!(message::code_span("`edge`"), "`` `edge` ``", "padded");
}

#[test]
fn a_long_message_is_written_whole_and_protected_past_what_a_preview_would_show() {
    let tail = " ![x](https://evil.example/x.png) [docs](https://evil.example) @[Ada](member:00000000-0000-0000-0000-000000000001) ```";
    let text = format!("{}{tail}", "a".repeat(300));
    let message = quoted(&text);
    let issue = message::issue_draft(&message, "NAV", "Long").unwrap();
    let decision = message::decision_draft(&message, "NAV-2", "Long", String::new()).unwrap();
    for body in [&issue.description, &decision.text] {
        assert!(
            body.contains(tail.trim_end_matches('`').trim()),
            "written whole"
        );
        assert!(
            body.starts_with("````text\n"),
            "a fence longer than the text's own backticks"
        );
        assert!(body.contains("\n````\n\nFrom `IRC message "));
    }
}
