//! Real pointer and keyboard entry points share one passive summary view.
use super::{TRACKER, Tracker, open_board};
use gpui_kit::base::test_support::snapshots;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Entity, Role, TestAppContext, VisualTestContext};
use std::time::Duration;
use subspace_framework::contracts::issues::{
    Changed, IssueProgress, IssueState, IssueSummary, LatestActivity, ResolutionState, StateType,
};
use subspace_framework::dashboard::contracts;

const CHIP: &str = "irc-line:2:issue-preview:TST-12";
const INLINE: &str = "irc-reference:Some(0..36)";
const TOOLTIP: &str = "irc-issue-tooltip";

fn summary(tracker: bool) -> IssueSummary {
    IssueSummary::new(
        "TST-12",
        "Fixture hover title",
        "https://api.example.test/api/v1/issues/TST-12",
        IssueState::new("In progress", StateType::Started),
    )
    .with_summary(Some("Explain the work beside its conversation so people can understand the problem without opening another page."))
    .with_priority(Some(2))
    .with_markers(tracker, false)
    .with_progress(Some(IssueProgress::new(3, 7)))
    .with_latest_activity(Some(LatestActivity::new(
        "issue.changed",
        "2026-11-16T11:56:00Z",
    )))
}

fn settle(cx: &mut VisualTestContext) {
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    cx.update(|window, cx| window.render_frame(cx));
}

fn answer(tracker: &Entity<Tracker>, state: ResolutionState, cx: &mut VisualTestContext) {
    cx.update(|_, cx| {
        tracker.update(cx, |tracker, _| {
            tracker.answers.insert("TST-12".into(), state);
        });
        assert!(contracts::publish(TRACKER, Changed, cx).is_ok());
    });
    settle(cx);
}

fn hover(target: &str, cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.hover(gpui_kit::SharedString::from(target.to_owned()), cx));
    settle(cx);
}

/// The tooltip's rendered text only, excluding the chip and message.
fn visible_text(cx: &mut VisualTestContext) -> String {
    crate::tests::tooltip_text(TOOLTIP, cx)
}

fn assert_passive(cx: &mut VisualTestContext) {
    cx.update(|window, _| {
        let inside: Vec<_> = snapshots(window)
            .into_iter()
            .filter(|node| node.path().contains(&TOOLTIP.into()))
            .collect();
        assert!(!inside.is_empty());
        assert!(
            inside
                .iter()
                .all(|node| node.role() != Some(Role::Button) && node.focused().is_none())
        );
    });
}

fn assert_card(cx: &mut VisualTestContext) {
    assert_passive(cx);
    cx.update(|window, _| {
        assert_eq!(
            window.find("irc-issue-hover-card").role(),
            Some(Role::Tooltip)
        );
    });
}

#[gpui_kit::test]
fn chip_hover_reveals_accent_content_using_the_hub_clock(cx: &mut TestAppContext) {
    let (_, hub, tracker, cx) = open_board(cx, true, "TST-12");
    answer(
        &tracker.unwrap(),
        ResolutionState::Available(summary(false)),
        cx,
    );
    cx.update(|window, cx| {
        hub.update(cx, |hub, cx| {
            hub.now = chrono::DateTime::parse_from_rfc3339("2026-11-16T12:00:00Z")
                .unwrap()
                .into();
            cx.notify();
        });
        assert!(window.find_all(TOOLTIP).is_empty());
    });
    hover(CHIP, cx);
    let text = visible_text(cx);
    assert!(
        text.contains("Explain the work beside its conversation"),
        "{text}"
    );
    assert!(text.contains("High") && text.contains("4m ago"), "{text}");
    assert!(
        !text.contains("3 of 7 done"),
        "accent content, not tracker: {text}"
    );
    assert_card(cx);
    cx.update(|window, _| {
        let expected =
            window.rem_size() * (400. / subspace_framework::dashboard::density::REFERENCE_REM);
        let actual = window.find(TOOLTIP).bounds().size.width;
        assert!(
            (actual - expected).abs() <= gpui_kit::px(1.),
            "{actual:?} vs {expected:?}"
        );
    });
}

#[gpui_kit::test]
fn chip_hover_reveals_tracker_progress_without_accent_priority(cx: &mut TestAppContext) {
    let (_, _, tracker, cx) = open_board(cx, true, "TST-12");
    answer(
        &tracker.unwrap(),
        ResolutionState::Available(summary(true)),
        cx,
    );
    hover(CHIP, cx);
    let text = visible_text(cx);
    assert!(
        text.contains("Explain the work beside its conversation"),
        "{text}"
    );
    assert!(text.contains("3 of 7 done"), "{text}");
    assert!(!text.contains("High"), "tracker composition: {text}");
    assert_card(cx);
}

#[gpui_kit::test]
fn inline_hover_keeps_notes_and_updates_to_the_card_in_place(cx: &mut TestAppContext) {
    let (_, _, tracker, cx) = open_board(cx, true, "TST-12");
    let tracker = tracker.unwrap();
    answer(&tracker, ResolutionState::SignedOut, cx);
    hover(INLINE, cx);
    for (state, expected) in [
        (ResolutionState::Pending, "Loading TST-12…"),
        (
            ResolutionState::Unavailable,
            "TST-12 isn't available with Subspace API.",
        ),
        (
            ResolutionState::SignedOut,
            "Sign in to Subspace API to see this issue.",
        ),
        (ResolutionState::Failed, "Couldn't reach Subspace API."),
    ] {
        answer(&tracker, state, cx);
        let text = visible_text(cx);
        assert_eq!(text, expected);
        assert_passive(cx);
    }
    for (is_tracker, expected) in [(false, "High"), (true, "3 of 7 done")] {
        answer(
            &tracker,
            ResolutionState::Available(summary(is_tracker)),
            cx,
        );
        let text = visible_text(cx);
        assert!(text.contains(expected), "{text}");
        assert!(!text.contains("Loading") && !text.contains("Couldn't reach"));
        assert_card(cx);
    }
    answer(&tracker, ResolutionState::Pending, cx);
    assert!(visible_text(cx).contains("3 of 7 done"));
    assert_card(cx);
}

#[gpui_kit::test]
fn keyboard_reference_focus_uses_the_same_passive_card(cx: &mut TestAppContext) {
    let (board, _, tracker, cx) = open_board(cx, true, "TST-12");
    answer(
        &tracker.unwrap(),
        ResolutionState::Available(summary(true)),
        cx,
    );
    cx.update(|window, cx| {
        subspace_framework::dashboard::chat_input::bind_keys(cx);
        crate::conversation::bind_keys(cx);
        let room = crate::fixtures::channel("#subspace");
        let conversation = board.read(cx).dock.open[&room].entity.clone();
        let input = conversation.read(cx).composer().clone();
        input.update(cx, |input, cx| input.focus(window, cx));
    });
    cx.simulate_keystrokes("shift-tab tab");
    settle(cx);
    assert!(visible_text(cx).contains("3 of 7 done"));
    assert_card(cx);
    cx.simulate_keystrokes("escape");
    settle(cx);
    cx.update(|window, _| assert!(window.find_all(TOOLTIP).is_empty()));
}
