//! A request's timeline: its steps, their times and durations, and the
//! trust rule that keeps an untrusted notice out of it.

use chrono::TimeDelta;

use super::super::{StepKind, build};
use super::{reply_ids, thread};
use crate::fixtures::at;
use crate::state::{Buffer, Entry};
use crate::thread_status::tests::{joined, live, notice_from, progress, request, room, said, ship};
use crate::thread_status::{Ending, Outcome, Stage};

#[test]
fn folded_progress_is_a_step_and_never_a_reply() {
    let mut state = joined(vec![ship()]);
    live(
        &mut state,
        [
            request(&room(), "first", 1),
            progress(&room(), "first", "accepted", 2),
            progress(&room(), "first", "reasoning", 4),
            progress(&room(), "first", "tool", 6),
            progress(&room(), "first", "reply", 10),
            progress(&room(), "first", "completed", 43),
            said(&room(), Some("first"), "answer", 43),
        ],
    );
    let found = thread(&state, "first").unwrap();
    assert_eq!(reply_ids(&found), ["answer"]);
    let timeline = found.timeline.unwrap();
    assert_eq!(timeline.agent, "astra");
    assert!(!timeline.earlier_missing);
    let steps: Vec<_> = timeline
        .steps
        .iter()
        .map(|step| (step.kind, step.at, step.took))
        .collect();
    assert_eq!(
        steps,
        [
            (
                StepKind::Stage(Stage::Accepted),
                Some(at(2)),
                Some(TimeDelta::minutes(2))
            ),
            (
                StepKind::Stage(Stage::Working),
                Some(at(4)),
                Some(TimeDelta::minutes(6)),
            ),
            (
                StepKind::Stage(Stage::Reply),
                Some(at(10)),
                Some(TimeDelta::minutes(33))
            ),
            (
                StepKind::Ended(Some(Outcome::Completed)),
                Some(at(43)),
                Some(TimeDelta::minutes(42)),
            ),
        ],
        "one step per stage, each lasting until the next; the end reports the whole request"
    );
}

#[test]
fn a_running_request_s_last_step_has_no_duration_yet() {
    let mut state = joined(vec![ship()]);
    live(
        &mut state,
        [
            request(&room(), "first", 1),
            progress(&room(), "first", "accepted", 2),
            progress(&room(), "first", "model", 3),
        ],
    );
    let steps = thread(&state, "first").unwrap().timeline.unwrap().steps;
    assert_eq!(steps.len(), 2);
    assert_eq!(steps[1].kind, StepKind::Stage(Stage::Working));
    assert_eq!(steps[1].took, None, "still running");
}

#[test]
fn an_untrusted_notice_makes_no_step_and_stays_a_reply() {
    let mut state = joined(vec![ship()]);
    live(
        &mut state,
        [
            request(&room(), "first", 1),
            progress(&room(), "first", "accepted", 2),
            notice_from(("astra", Some("mallory")), &room(), "first", "completed", 3),
            notice_from(("mallory", None), &room(), "first", "failed", 4),
        ],
    );
    let found = thread(&state, "first").unwrap();
    let timeline = found.timeline.unwrap();
    assert_eq!(
        timeline
            .steps
            .iter()
            .map(|step| step.kind)
            .collect::<Vec<_>>(),
        [StepKind::Stage(Stage::Accepted)],
        "neither a wrong account nor no account can end it"
    );
    assert_eq!(found.replies.len(), 2, "both show as ordinary replies");
}

#[test]
fn an_ending_kept_without_its_notices_says_the_earlier_steps_are_not_loaded() {
    let room = room();
    let mut held = Buffer::default();
    held.entries.push_back(Entry {
        seq: 0,
        line: request(&room, "first", 1),
        highlight: false,
    });
    held.requests.load(&[(
        "first".to_owned(),
        Ending {
            outcome: Some(Outcome::Failed),
            at: Some(at(4)),
        },
    )]);
    let timeline = build(&room, &held, "first").unwrap().timeline.unwrap();
    assert!(timeline.earlier_missing);
    assert_eq!(timeline.steps.len(), 1);
    assert_eq!(
        timeline.steps[0].kind,
        StepKind::Ended(Some(Outcome::Failed))
    );
    assert_eq!(timeline.steps[0].took, Some(TimeDelta::minutes(3)));
    assert_eq!(
        StepKind::Ended(None).label(),
        "Ended",
        "never claims success"
    );
}

#[test]
fn a_lone_ending_notice_still_says_the_earlier_steps_are_not_loaded() {
    let mut state = joined(vec![ship()]);
    live(
        &mut state,
        [
            request(&room(), "first", 1),
            progress(&room(), "first", "completed", 43),
        ],
    );
    let timeline = thread(&state, "first").unwrap().timeline.unwrap();
    assert!(timeline.earlier_missing, "only the ending is held");
}

#[test]
fn a_follow_up_request_shows_its_own_steps_and_the_replies_to_it() {
    use crate::thread_status::tests::follow_up;
    let mut state = joined(vec![ship()]);
    live(
        &mut state,
        [
            request(&room(), "first", 1),
            progress(&room(), "first", "completed", 3),
            said(&room(), Some("first"), "answer", 3),
            follow_up(&room(), "second", "first", 5),
            progress(&room(), "second", "accepted", 6),
            progress(&room(), "second", "completed", 9),
            said(&room(), Some("second"), "answer2", 9),
        ],
    );
    let second = thread(&state, "second").unwrap();
    assert_eq!(second.root.line.id.as_deref(), Some("second"));
    assert_eq!(
        reply_ids(&second),
        ["answer", "answer2"],
        "the reply to it shows"
    );
    assert!(second.nested, "its replies are the thread's");
    let steps: Vec<_> = second
        .timeline
        .unwrap()
        .steps
        .iter()
        .map(|step| step.kind)
        .collect();
    assert_eq!(
        steps,
        [
            StepKind::Stage(Stage::Accepted),
            StepKind::Ended(Some(Outcome::Completed))
        ]
    );
    // The first thread and the transcript's count agree.
    let first = thread(&state, "first").unwrap();
    assert_eq!(reply_ids(&first), ["answer", "second", "answer2"]);
    assert!(!first.nested);
    let held = state.buffer(&room()).unwrap();
    let counts = crate::transcript::replies::counts(&room(), held);
    let seq = held
        .entries
        .iter()
        .find(|entry| entry.line.id.as_deref() == Some("first"))
        .unwrap()
        .seq;
    assert_eq!(counts[&seq].count(), first.reply_count);
}

#[test]
fn a_kept_ending_is_the_end_whatever_notice_is_held() {
    // The store kept Failed; loaded history holds a folded Completed.
    let room = room();
    let mut held = Buffer::default();
    let lines = [
        request(&room, "first", 1),
        progress(&room, "first", "accepted", 2),
        progress(&room, "first", "completed", 43),
    ];
    for (seq, line) in (0..).zip(lines) {
        held.entries.push_back(Entry {
            seq,
            line,
            highlight: false,
        });
    }
    held.folded.insert(1);
    held.folded.insert(2);
    held.requests.load(&[(
        "first".to_owned(),
        Ending {
            outcome: Some(Outcome::Failed),
            at: Some(at(40)),
        },
    )]);
    let steps: Vec<_> = build(&room, &held, "first")
        .unwrap()
        .timeline
        .unwrap()
        .steps
        .iter()
        .map(|step| step.kind)
        .collect();
    assert_eq!(
        steps,
        [
            StepKind::Stage(Stage::Accepted),
            StepKind::Ended(Some(Outcome::Failed))
        ],
        "the kept Failed, never the held Completed"
    );
}

#[test]
fn a_long_thread_keeps_its_count_and_only_its_latest_replies() {
    let mut state = joined(vec![ship()]);
    let lines: Vec<_> = std::iter::once(request(&room(), "first", 1))
        .chain((0..45).map(|n| said(&room(), Some("first"), &format!("r{n}"), 2 + n)))
        .collect();
    live(&mut state, lines);
    let found = thread(&state, "first").unwrap();
    assert_eq!(found.reply_count, 45);
    assert_eq!(found.replies.len(), crate::thread::MAX_REPLIES);
    assert_eq!(
        found.replies[0].line.id.as_deref(),
        Some("r5"),
        "the latest 40"
    );
}
