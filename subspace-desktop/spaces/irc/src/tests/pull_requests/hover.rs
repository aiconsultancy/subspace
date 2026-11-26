//! A pull request chip's passive hover tile: the card when the PRs Space
//! can read it, "Details unavailable" when it cannot, and nothing private
//! after the account changes.

use std::time::Duration;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Role, TestAppContext, VisualTestContext};
use subspace_framework::contracts::github::{AccessChanged, ResolutionState};
use subspace_framework::dashboard::contracts;

use super::{KEY, PRS, Prs, answer, chips, details, open_board};

const TOOLTIP: &str = "irc-pr-tooltip";
const TEXT: &str = "[PR:octo-org/api#421]";

fn hover(target: &str, cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.hover(gpui_kit::SharedString::from(target.to_owned()), cx));
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    cx.update(|window, cx| window.render_frame(cx));
}

/// The tooltip's rendered text only, excluding the chip and message.
fn visible_text(cx: &mut VisualTestContext) -> String {
    crate::tests::tooltip_text(TOOLTIP, cx)
}

fn chip(cx: &mut VisualTestContext) -> String {
    chips(cx).into_iter().next().expect("a pull request chip")
}

#[gpui_kit::test]
fn hovering_a_readable_pull_request_shows_its_card_at_320_px(cx: &mut TestAppContext) {
    let (_, hub, board, cx) = open_board(cx, Prs::Running, TEXT);
    answer(
        &board.expect("a PRs fixture"),
        ResolutionState::Available(details()),
        cx,
    );
    cx.update(|_, cx| {
        hub.update(cx, |hub, cx| {
            hub.now = chrono::DateTime::parse_from_rfc3339("2026-11-16T12:00:00Z")
                .unwrap()
                .into();
            cx.notify();
        });
    });
    let target = chip(cx);
    hover(&target, cx);
    let text = visible_text(cx);
    for expected in [
        "api#421",
        "octo-org",
        "Open",
        "Keep pull request context beside the conversation",
        "feat/chips → main",
        "+120",
        "−8",
        "octo-dev",
        "Approved",
        "Checks passed",
        "Updated 4m ago",
    ] {
        assert!(text.contains(expected), "{expected} in {text}");
    }
    cx.update(|window, _| {
        assert_eq!(window.find("irc-pr-hover-card").role(), Some(Role::Tooltip));
        let expected =
            window.rem_size() * (320. / subspace_framework::dashboard::density::REFERENCE_REM);
        let actual = window.find(TOOLTIP).bounds().size.width;
        // The tooltip adds Kit's margin either side of the card.
        let card = window.find("irc-pr-hover-card").bounds().size.width;
        assert!(
            (card - expected).abs() <= gpui_kit::px(1.),
            "{card:?} vs {expected:?} (tooltip {actual:?})"
        );
    });
}

/// What hovering the chip says once the PRs Space answered `state`.
fn hovered_after(state: ResolutionState, cx: &mut TestAppContext) -> String {
    let (_, _, board, cx) = open_board(cx, Prs::Running, TEXT);
    answer(&board.expect("a PRs fixture"), state, cx);
    let target = chip(cx);
    hover(&target, cx);
    visible_text(cx)
}

#[gpui_kit::test]
fn a_signed_out_chip_stays_without_details(cx: &mut TestAppContext) {
    assert_eq!(
        hovered_after(ResolutionState::SignedOut, cx),
        "Details unavailable"
    );
}

#[gpui_kit::test]
fn a_chip_the_account_cannot_read_stays_without_details(cx: &mut TestAppContext) {
    assert_eq!(
        hovered_after(ResolutionState::Unavailable, cx),
        "Details unavailable"
    );
}

#[gpui_kit::test]
fn a_chip_whose_read_failed_stays_without_details(cx: &mut TestAppContext) {
    assert_eq!(
        hovered_after(ResolutionState::Failed, cx),
        "Details unavailable"
    );
}

#[gpui_kit::test]
fn without_a_prs_space_the_chip_says_its_details_are_unavailable(cx: &mut TestAppContext) {
    let (_, _, _, cx) = open_board(cx, Prs::Absent, TEXT);
    let target = chip(cx);
    hover(&target, cx);
    assert_eq!(visible_text(cx), "Details unavailable");
}

#[gpui_kit::test]
fn an_account_change_clears_details_before_the_new_account_answers(cx: &mut TestAppContext) {
    let (_, hub, board, cx) = open_board(cx, Prs::Running, TEXT);
    let board = board.expect("a PRs fixture");
    answer(&board, ResolutionState::Available(details()), cx);
    cx.update(|_, cx| {
        assert!(matches!(
            hub.read(cx).pull_requests.state(KEY),
            Some(ResolutionState::Available(_))
        ));
    });
    // The new account cannot read it: the old details must not linger.
    answer(&board, ResolutionState::Unavailable, cx);
    let target = chip(cx);
    hover(&target, cx);
    let text = visible_text(cx);
    assert_eq!(text, "Details unavailable");
    assert!(!text.contains("Keep pull request context"));
}

#[gpui_kit::test]
fn refresh_keeps_the_open_tile_until_an_account_event_clears_it(cx: &mut TestAppContext) {
    let (_, hub, board, cx) = open_board(cx, Prs::Running, TEXT);
    let board = board.expect("a PRs fixture");
    answer(&board, ResolutionState::Available(details()), cx);
    let target = chip(cx);
    hover(&target, cx);
    let before = visible_text(cx);
    assert!(before.contains("Checks passed"));

    answer(&board, ResolutionState::Pending, cx);
    assert_eq!(chip(cx), target);
    assert_eq!(
        visible_text(cx),
        before,
        "a refresh keeps the rendered card intact"
    );

    cx.update(|_, cx| assert!(contracts::publish(PRS, AccessChanged, cx).is_ok()));
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(!matches!(
            hub.read(cx).pull_requests.state(KEY),
            Some(ResolutionState::Available(_))
        ));
    });
    assert!(!visible_text(cx).contains("Keep pull request context"));
}
