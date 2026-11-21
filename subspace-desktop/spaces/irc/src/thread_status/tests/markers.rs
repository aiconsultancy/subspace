//! Which notices carry a progress marker, the label of every stage, and
//! the agent's name as the line shows it.

use subspace_client_irc::{Line, LineKind};

use super::{is_folded, joined, label, live, progress, request, room, ship};
use crate::fixtures::network_id;
use crate::state::server_key;
use crate::thread_status::model::agent_name;
use crate::thread_status::{Marker, PROGRESS_TAG, Phase, marker};

#[test]
fn only_a_tagged_notice_replying_to_a_message_with_a_known_value_is_a_marker() {
    let tagged = progress(&room(), "first", "tool", 1);
    assert_eq!(
        marker(&tagged),
        Some(Marker {
            phase: Phase::Tool,
            disconnected: false,
        })
    );
    let refused: [fn(&mut Line); 11] = [
        |line| line.client_tags.clear(),
        |line| {
            _ = line
                .client_tags
                .insert(PROGRESS_TAG.into(), "compiling".into())
        },
        |line| {
            _ = line
                .client_tags
                .insert(PROGRESS_TAG.into(), "tool,later".into())
        },
        |line| {
            _ = line
                .client_tags
                .insert(PROGRESS_TAG.into(), "completed,disconnected".into())
        },
        |line| {
            _ = line
                .client_tags
                .insert(PROGRESS_TAG.into(), "completed,".into())
        },
        |line| _ = line.client_tags.insert(PROGRESS_TAG.into(), "tool,".into()),
        |line| line.reply_to = None,
        |line| line.own = true,
        |line| line.kind = LineKind::Privmsg,
        |line| line.kind = LineKind::Action,
        |line| line.buffer = server_key(&network_id()),
    ];
    for (case, change) in refused.iter().enumerate() {
        let mut line = tagged.clone();
        change(&mut line);
        assert_eq!(marker(&line), None, "case {case}");
    }
}

#[test]
fn every_stage_maps_to_one_compact_label() {
    let stages = [
        ("received", Some("Receipt unconfirmed")),
        ("submitting", Some("Receipt unconfirmed")),
        ("accepted", Some("Accepted")),
        ("delivered", Some("Accepted")),
        ("queued", Some("Queued")),
        ("model", Some("Thinking")),
        ("reasoning", Some("Thinking")),
        ("tool", Some("Using a tool")),
        ("retry", Some("Waiting to retry")),
        ("waiting", Some("Waiting for your answer")),
        ("clarified", Some("Answer received")),
        ("reply", Some("Writing a reply")),
        ("completed", None),
        ("failed", None),
    ];
    for (stage, expected) in stages {
        let mut state = joined(vec![ship()]);
        live(
            &mut state,
            [
                request(&room(), "first", 1),
                progress(&room(), "first", stage, 2),
            ],
        );
        assert_eq!(label(&state, "first"), expected, "{stage}");
        assert!(is_folded(&state, &room(), "astra-first-2"), "{stage}");
    }
    assert_eq!(Phase::Waiting.label(false), Some("Waiting for an answer"));
    for value in ["tool,disconnected", "waiting,disconnected"] {
        assert_eq!(
            marker(&progress(&room(), "first", value, 2)).map(|marker| marker.disconnected),
            Some(true),
            "{value}"
        );
    }
}

#[test]
fn the_agent_name_is_one_bounded_line_of_plain_text() {
    assert_eq!(agent_name("astra"), "astra");
    assert_eq!(agent_name("as\u{3}4tra\n"), "as4tra");
    let long = "a".repeat(40);
    let shown = agent_name(&long);
    assert_eq!(shown.chars().count(), 32);
    assert!(shown.ends_with('…'));
    assert_eq!(agent_name(&"b".repeat(32)), "b".repeat(32));
}
