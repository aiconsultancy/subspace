//! Tiles once, drawn (pull request tiles too, beside the issue tests they
//! share helpers with): the first message to name an issue or a pull request
//! keeps its tile, later mentions are only the inline chip, and no message
//! changes height when others arrive, history loads above or answers land.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Bounds, Pixels, SharedString, TestAppContext, VisualTestContext, px, size};
use subspace_client_irc::Event;
use subspace_framework::contracts::github::{CheckCounts, ResolutionState as PrState};
use subspace_framework::contracts::issues::{
    IssueProgress, IssueState, IssueSummary, ResolutionState, StateType,
};

use crate::fixtures::{channel, message};
use crate::tests::inject;

fn present(cx: &mut VisualTestContext, id: &str) -> bool {
    let id = SharedString::from(id.to_owned());
    cx.update(move |window, _| window.try_find(id).is_some())
}

fn bounds(cx: &mut VisualTestContext, id: &str) -> Bounds<Pixels> {
    let id = SharedString::from(id.to_owned());
    cx.update(move |window, _| window.find(id).bounds())
}

/// A window tall enough to draw every row of these short transcripts.
fn tall(cx: &mut VisualTestContext) {
    cx.update(|window, _| window.resize(size(px(1200.), px(2400.))));
    settle(cx);
}

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..2 {
        cx.run_until_parked();
        cx.update(|window, cx| window.render_frame(cx));
    }
    cx.run_until_parked();
}

/// Lines from `amy` in `#subspace`, from minute 4 on.
fn said(texts: &[&str]) -> Vec<Event> {
    let room = channel("#subspace");
    texts
        .iter()
        .zip(4..)
        .map(|(text, minute)| Event::Line(message(&room, "amy", text, minute)))
        .collect()
}

#[gpui_kit::test]
fn an_issue_named_again_shows_its_tile_only_under_the_first_message(cx: &mut TestAppContext) {
    // The tracker stays alive: a dropped responder withdraws the scope.
    let (_, hub, _tracker, cx) = super::open_board(cx, true, "TST-12 is the gate");
    tall(cx);
    let first = bounds(cx, "irc-line:2:row");
    assert!(present(cx, "irc-line:2:issue-preview:TST-12"));
    inject(
        &hub,
        said(&["TST-12 again", "still TST-12, and TST-18"]),
        cx,
    );
    settle(cx);
    assert!(
        present(cx, "irc-line:2:issue-preview:TST-12"),
        "the first keeps it"
    );
    assert!(!present(cx, "irc-line:3:issue-preview:TST-12"));
    assert!(!present(cx, "irc-line:4:issue-preview:TST-12"));
    assert!(
        present(cx, "irc-line:4:issue-preview:TST-18"),
        "a new issue gets one"
    );
    assert_eq!(bounds(cx, "irc-line:2:row").size, first.size);
    let repeat = bounds(cx, "irc-line:3:row").size.height;
    let tiled = bounds(cx, "irc-line:4:row").size.height;
    assert!(repeat < tiled, "a repeat is text only");
}

#[gpui_kit::test]
fn history_loaded_above_leaves_the_tile_where_it_was(cx: &mut TestAppContext) {
    // The tracker stays alive: a dropped responder withdraws the scope.
    let (_, hub, _tracker, cx) = super::open_board(cx, true, "TST-12 is the gate");
    tall(cx);
    let before = bounds(cx, "irc-line:2:row");
    let room = channel("#subspace");
    let older = Event::History {
        buffer: room.clone(),
        lines: vec![message(&room, "amy", "first TST-12 mention", -30)],
        complete: true,
    };
    inject(&hub, vec![older], cx);
    settle(cx);
    assert!(present(cx, "irc-line:2:issue-preview:TST-12"));
    assert!(present(cx, "irc-line:-1:row"), "the older line is shown");
    assert!(!present(cx, "irc-line:-1:issue-preview:TST-12"));
    assert_eq!(bounds(cx, "irc-line:2:row").size, before.size);
}

#[gpui_kit::test]
fn an_issue_tile_shows_its_tracker_facts_and_progress(cx: &mut TestAppContext) {
    let (_, _, tracker, cx) = super::open_board(cx, true, "TST-40 next");
    let tracker = tracker.expect("a tracker");
    settle(cx);
    let loading = bounds(cx, "irc-line:2:row");
    assert!(present(cx, "irc-line:2:issue-loading:TST-40"));
    let summary = IssueSummary::new(
        "TST-40",
        "Hold the last frame while tiles decode",
        "https://api.example.test/api/v1/issues/TST-40",
        IssueState::new("Todo", StateType::Unstarted),
    )
    .with_markers(true, false)
    .with_assignee(Some("aurora-lyra"))
    .with_assignee_kind(Some("agent"))
    .with_assignee_rank(Some("captain"))
    .with_progress(Some(IssueProgress::new(1, 2)))
    .with_ready_for_agents(Some(true));
    super::answer(&tracker, "TST-40", ResolutionState::Available(summary), cx);
    settle(cx);
    assert!(present(cx, "irc-line:2:issue-preview:TST-40"));
    assert!(
        present(cx, "issue-tile-progress:TST-40"),
        "the progress bar"
    );
    assert!(
        present(cx, "issue-tile-agent:TST-40"),
        "the captain's icon (Small)"
    );
    cx.update(|window, _| {
        let label = window
            .find("irc-line:2:issue-preview:TST-40:open")
            .label()
            .map(str::to_owned)
            .unwrap_or_default();
        for fact in [
            "ready for agents",
            "Tracker",
            "assigned to captain aurora-lyra",
            "1 of 2 done",
        ] {
            assert!(label.contains(fact), "{fact} in {label}");
        }
    });
    assert_eq!(bounds(cx, "irc-line:2:row").size, loading.size);
}

#[gpui_kit::test]
fn a_pull_request_tile_loads_in_place_and_shows_once(cx: &mut TestAppContext) {
    use super::super::pull_requests::{KEY, Prs, answer, details, open_board};
    let (_, hub, board, cx) = open_board(cx, Prs::Running, "[PR:octo-org/api#421] is up");
    let board = board.expect("a PRs fixture");
    settle(cx);
    let tile = format!("irc-line:2:pr-tile:{KEY}");
    let loading = bounds(cx, "irc-line:2:row");
    assert!(present(cx, &format!("irc-line:2:pr-loading:{KEY}")));
    let full = details().with_check_counts(Some(CheckCounts::new(4, 5)));
    answer(&board, PrState::Available(full), cx);
    settle(cx);
    assert!(present(cx, &tile));
    assert!(present(cx, &format!("pr-tile-checks:{KEY}")));
    cx.update(|window, _| {
        let label = window
            .find(SharedString::from(format!("{tile}:open")))
            .label()
            .map(str::to_owned)
            .unwrap_or_default();
        for fact in ["Open", "octo-org/api", "+120 −8", "4 of 5 checks passed"] {
            assert!(label.contains(fact), "{fact} in {label}");
        }
    });
    assert_eq!(bounds(cx, "irc-line:2:row").size, loading.size);
    inject(&hub, said(&["merged [PR:octo-org/api#421]"]), cx);
    settle(cx);
    assert!(!present(cx, &format!("irc-line:3:pr-tile:{KEY}")));
    assert!(present(cx, &tile));
}

#[gpui_kit::test]
fn an_unresolved_pull_request_settles_without_changing_height(cx: &mut TestAppContext) {
    use super::super::pull_requests::{KEY, Prs, answer, open_board};
    let (_, _, board, cx) = open_board(cx, Prs::Running, "[PR:octo-org/api#421] is up");
    let board = board.expect("a PRs fixture");
    settle(cx);
    let loading = bounds(cx, "irc-line:2:row");
    for (state, reason) in [
        (PrState::Unavailable, "Unavailable"),
        (PrState::SignedOut, "Signed out"),
        (PrState::Failed, "Couldn't load"),
    ] {
        answer(&board, state, cx);
        settle(cx);
        assert!(
            present(cx, &format!("irc-line:2:pr-unavailable:{KEY}")),
            "{reason}"
        );
        assert_eq!(bounds(cx, "irc-line:2:row").size, loading.size, "{reason}");
    }
}
