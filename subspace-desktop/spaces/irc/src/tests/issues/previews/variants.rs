//! The native card and chip controls keep their names and activation paths.
use super::super::{TRACKER, open_board};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{TestAppContext, px};
use std::{cell::RefCell, rc::Rc};
use subspace_framework::contracts::issues::{
    Changed, IssueProgress, IssueState, IssueSummary, PullRequestSummary, ResolutionState,
    StateType,
};
use subspace_framework::dashboard::{DashboardEvent, contracts};
const TILE: &str = "irc-line:2:issue-preview:TST-12";

fn summary(identifier: &str) -> IssueSummary {
    IssueSummary::new(
        identifier,
        "Fixture tile",
        &format!("https://api.example.test/api/v1/issues/{identifier}"),
        IssueState::new("In progress", StateType::Started),
    )
}
#[gpui_kit::test]
fn compact_chips_keep_scoped_click_and_reference_keyboard_navigation(cx: &mut TestAppContext) {
    let (board, _, tracker, cx) = open_board(cx, true, "TST-12 TST-15 TST-16");
    let tracker = tracker.unwrap();
    let events = Rc::new(RefCell::new(Vec::new()));
    let _subscription = cx.update(|window, cx| {
        tracker.update(cx, |tracker, _| {
            for identifier in ["TST-12", "TST-15", "TST-16"] {
                tracker.answers.insert(
                    identifier.into(),
                    ResolutionState::Available(summary(identifier)),
                );
            }
        });
        assert!(contracts::publish(TRACKER, Changed, cx).is_ok());
        let sink = events.clone();
        let subscription = cx.subscribe(&board, move |_, event: &DashboardEvent, _| {
            if let DashboardEvent::Navigate(link) = event {
                sink.borrow_mut().push(link.url.clone());
            }
        });
        window.render_frame(cx);
        subscription
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.find(TILE).bounds().size.height < px(40.));
        assert_eq!(
            window.find("irc-line:2:issue-preview:TST-12:open").label(),
            Some("Open TST-12 in Issues: Fixture tile, In progress")
        );
        window.click("irc-line:2:issue-preview:TST-12:open", cx);
    });
    cx.update(|window, cx| {
        subspace_framework::dashboard::chat_input::bind_keys(cx);
        crate::conversation::bind_keys(cx);
        let room = crate::fixtures::channel("#subspace");
        let conversation = board.read(cx).dock.open[&room].entity.clone();
        let input = conversation.read(cx).composer().clone();
        input.update(cx, |input, cx| input.focus(window, cx));
    });
    cx.simulate_keystrokes("shift-tab tab enter");
    cx.run_until_parked();
    let events = events.borrow();
    assert_eq!(events.len(), 2);
    assert!(
        events
            .iter()
            .all(|url| url == "https://api.example.test/api/v1/issues/TST-12?profile=dev")
    );
}
#[gpui_kit::test]
fn tracker_accessible_name_includes_progress_and_available_pr_facts(cx: &mut TestAppContext) {
    let (_, _, tracker, cx) = open_board(cx, true, "TST-12");
    let tracker = tracker.unwrap();
    cx.update(|_, cx| {
        let issue = summary("TST-12")
            .with_markers(true, false)
            .with_progress(Some(IssueProgress::new(3, 7)))
            .with_pull_requests(Some(
                PullRequestSummary::new(5, 1, 0, 4).with_checks(Some("failure")),
            ));
        tracker.update(cx, |tracker, _| {
            tracker
                .answers
                .insert("TST-12".into(), ResolutionState::Available(issue));
        });
        assert!(contracts::publish(TRACKER, Changed, cx).is_ok());
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        let tile = window.find("irc-line:2:issue-preview:TST-12:open");
        let label = tile.label().unwrap();
        assert!(
            label.contains("Tracker")
                && label.contains("3 of 7 done")
                && label.contains("4 merged")
        );
        assert!(!label.contains("due ") && !label.contains("assigned to"));
    });
}

#[gpui_kit::test]
fn chip_rows_wrap_and_keep_long_titles_inside_the_transcript(cx: &mut TestAppContext) {
    let text = "TST-18 TST-12 TST-15 TST-16 TST-17 TST-19 TST-20";
    let (board, _, tracker, cx) = open_board(cx, true, text);
    let tracker = tracker.unwrap();
    cx.update(|_, cx| {
        tracker.update(cx, |tracker, _| {
            for identifier in ["TST-15", "TST-16", "TST-17", "TST-19", "TST-20"] {
                tracker.answers.insert(
                    identifier.into(),
                    ResolutionState::Available(summary(identifier)),
                );
            }
        });
        assert!(contracts::publish(TRACKER, Changed, cx).is_ok());
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        let room = crate::fixtures::channel("#subspace");
        let conversation = board.read(cx).dock.open[&room].entity.clone();
        conversation.update(cx, |conversation, cx| assert!(conversation.reveal(2, cx)));
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        let transcript = window.find("irc-transcript-pane").bounds();
        for identifier in text.split_whitespace() {
            let chip = window
                .find(gpui_kit::SharedString::from(format!(
                    "irc-line:2:issue-preview:{identifier}"
                )))
                .bounds();
            assert!(
                chip.right() <= transcript.right(),
                "{identifier} stays inside the transcript"
            );
            assert!(chip.size.height < px(40.));
        }
        let first = window.find("irc-line:2:issue-preview:TST-18").bounds();
        let last = window.find("irc-line:2:issue-preview:TST-20").bounds();
        assert!(last.origin.y > first.origin.y, "the chip row wraps");
    });
}
#[gpui_kit::test]
fn a_single_available_issue_is_always_a_chip(cx: &mut TestAppContext) {
    let (_, _, tracker, cx) = open_board(cx, true, "TST-12 TST-15");
    let tracker = tracker.unwrap();
    let phases = [
        ResolutionState::Unavailable,
        ResolutionState::Available(summary("TST-15")),
    ];
    for second in phases {
        cx.update(|_, cx| {
            tracker.update(cx, |tracker, _| {
                let first = ResolutionState::Available(summary("TST-12"));
                tracker.answers.insert("TST-12".into(), first);
                tracker.answers.insert("TST-15".into(), second);
            });
            assert!(contracts::publish(TRACKER, Changed, cx).is_ok());
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.render_frame(cx);
            let height = window.find(TILE).bounds().size.height;
            assert!(height < px(40.), "chip height {height:?}");
        });
    }
}
