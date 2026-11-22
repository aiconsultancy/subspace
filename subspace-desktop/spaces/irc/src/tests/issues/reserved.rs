//! Chip slots reserved from a message's first frame, so an answer landing
//! never changes the row's height and nothing below or above it moves.
use super::{TRACKER, Tracker, open_board};
use crate::{Irc, fixtures::channel};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Bounds, Entity, Pixels, SharedString, TestAppContext, VisualTestContext, px, size};
use subspace_framework::contracts::issues::{
    Changed, IssueState, IssueSummary, ResolutionState, StateType,
};
use subspace_framework::dashboard::contracts;

const ROW: &str = "irc-line:2:row";

fn row(cx: &mut VisualTestContext) -> Bounds<Pixels> {
    cx.update(|window, _| window.find(ROW).bounds())
}

/// Draws until the answers the frame asked for have landed and are drawn.
fn settle(cx: &mut VisualTestContext) -> Bounds<Pixels> {
    for _ in 0..2 {
        cx.run_until_parked();
        cx.update(|window, cx| window.render_frame(cx));
    }
    cx.run_until_parked();
    row(cx)
}

fn present(cx: &mut VisualTestContext, id: &str) -> bool {
    let id = SharedString::from(id.to_owned());
    cx.update(move |window, _| window.try_find(id).is_some())
}

/// The tracker's answer for `identifier` becomes `state`, announced by `changed`.
fn answer(
    tracker: &Entity<Tracker>,
    identifier: &str,
    state: ResolutionState,
    cx: &mut VisualTestContext,
) {
    cx.update(|_, cx| {
        tracker.update(cx, |tracker, _| {
            tracker.answers.insert(identifier.into(), state);
        });
        assert!(contracts::publish(TRACKER, Changed, cx).is_ok());
    });
}

fn assigned(identifier: &str) -> ResolutionState {
    ResolutionState::Available(
        IssueSummary::new(
            identifier,
            "Fixture title",
            &format!("https://api.example.test/api/v1/issues/{identifier}"),
            IssueState::new("In Progress", StateType::Started),
        )
        .with_assignee(Some("Rosa Fixture")),
    )
}

#[gpui_kit::test]
fn a_reference_without_an_answer_reserves_the_slot_its_chip_fills(cx: &mut TestAppContext) {
    let (_, hub, tracker, cx) = open_board(cx, true, "TST-30 lands today");
    let tracker = tracker.expect("a tracker");
    // The tracker leaves TST-30 out of its answer, so every frame so far is
    // drawn as the first one is: with no answer at all.
    let unanswered = settle(cx);
    cx.update(|_, cx| assert!(hub.read(cx).issues.state("TST-30").is_none()));
    assert!(present(cx, "irc-line:2:issue-loading:TST-30"));
    answer(
        &tracker,
        "TST-30",
        super::summary("TST-30", "Fixture title"),
        cx,
    );
    let resolved = settle(cx);
    assert!(present(cx, "irc-line:2:issue-preview:TST-30"));
    assert!(!present(cx, "irc-line:2:issue-loading:TST-30"));
    assert_eq!(resolved.size.height, unanswered.size.height);
}

#[gpui_kit::test]
fn a_pending_slot_keeps_its_height_when_an_assigned_issue_lands(cx: &mut TestAppContext) {
    let (_, _, tracker, cx) = open_board(cx, true, "TST-13 lands today");
    let tracker = tracker.expect("a tracker");
    let pending = settle(cx);
    assert!(present(cx, "irc-line:2:issue-loading:TST-13"));
    cx.update(|window, _| {
        assert_eq!(
            window.find("irc-line:2:issue-loading:TST-13:slot").label(),
            Some("Loading TST-13…")
        );
    });
    answer(&tracker, "TST-13", assigned("TST-13"), cx);
    let resolved = settle(cx);
    assert!(present(cx, "irc-line:2:issue-preview:TST-13"));
    assert_eq!(resolved.size.height, pending.size.height);
}

#[gpui_kit::test]
fn an_unresolved_reference_keeps_its_slot_and_says_why(cx: &mut TestAppContext) {
    let (_, _, tracker, cx) = open_board(cx, true, "TST-13 lands today");
    let tracker = tracker.expect("a tracker");
    let pending = settle(cx);
    for (state, reason) in [
        (
            ResolutionState::Unavailable,
            "TST-13 isn't available with Subspace API.",
        ),
        (
            ResolutionState::SignedOut,
            "Sign in to Subspace API to see this issue.",
        ),
        (ResolutionState::Failed, "Couldn't reach Subspace API."),
    ] {
        answer(&tracker, "TST-13", state, cx);
        let answered = settle(cx);
        assert!(!present(cx, "irc-line:2:issue-loading:TST-13"));
        cx.update(|window, _| {
            assert_eq!(
                window
                    .find("irc-line:2:issue-unavailable:TST-13:slot")
                    .label(),
                Some(reason)
            );
        });
        assert_eq!(answered.size.height, pending.size.height, "{reason}");
    }
}

#[gpui_kit::test]
fn a_known_answer_draws_its_chip_without_a_placeholder(cx: &mut TestAppContext) {
    let (_, _, tracker, cx) = open_board(cx, true, "TST-12 lands today");
    let tracker = tracker.expect("a tracker");
    settle(cx);
    answer(&tracker, "TST-13", ResolutionState::Unavailable, cx);
    // The new round asks again only after this frame: the frame still has
    // the earlier answer, so it draws the chip.
    cx.update(|window, cx| window.render_frame(cx));
    assert!(present(cx, "irc-line:2:issue-preview:TST-12"));
    assert!(!present(cx, "irc-line:2:issue-loading:TST-12"));
}

#[gpui_kit::test]
fn a_followed_transcript_stays_put_when_answers_land(cx: &mut TestAppContext) {
    const LINES: usize = 80;
    let text = format!("{}TST-13", "A fictional transcript line.\n".repeat(LINES));
    let (board, _, tracker, cx) = open_board(cx, true, &text);
    let tracker = tracker.expect("a tracker");
    let view = conversation(&board, cx);
    let pending = settle(cx);
    assert!(cx.update(|_, cx| view.read(cx).is_following_summary_tail(cx)));
    answer(&tracker, "TST-13", assigned("TST-13"), cx);
    let resolved = settle(cx);
    assert!(present(cx, "irc-line:2:issue-preview:TST-13"));
    assert!(cx.update(|_, cx| view.read(cx).is_following_summary_tail(cx)));
    assert_eq!(resolved, pending, "the row neither grows nor moves");
}

#[gpui_kit::test]
fn reserved_slots_wrap_in_the_chip_row_of_a_narrow_transcript(cx: &mut TestAppContext) {
    const IDS: [&str; 5] = ["TST-21", "TST-22", "TST-23", "TST-24", "TST-25"];
    let (board, _, _, cx) = open_board(cx, true, &IDS.join(" "));
    let view = conversation(&board, cx);
    cx.update(|window, cx| {
        window.resize(size(px(860.), px(960.)));
        view.update(cx, |conversation, cx| assert!(conversation.reveal(2, cx)));
    });
    settle(cx);
    cx.update(|window, _| {
        let transcript = window.find("irc-transcript-pane").bounds();
        let slots: Vec<_> = IDS
            .iter()
            .map(|id| {
                window
                    .find(SharedString::from(format!("irc-line:2:issue-loading:{id}")))
                    .bounds()
            })
            .collect();
        assert!(slots.iter().all(|slot| slot.right() <= transcript.right()));
        let first = slots[0].top();
        assert!(
            slots.iter().any(|slot| slot.top() > first),
            "the slots wrap: {slots:?}"
        );
        assert!(window.find(ROW).bounds().bottom() >= slots[4].bottom());
    });
}

fn conversation(
    board: &Entity<Irc>,
    cx: &mut VisualTestContext,
) -> Entity<crate::conversation::Conversation> {
    cx.update(|_, cx| {
        board.read(cx).dock.open[&channel("#subspace")]
            .entity
            .clone()
    })
}
