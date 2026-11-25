//! The captain's turns inside the Ship tab: opened from the card's
//! Activity action and left with Back, newest turn first with the running
//! one open, what the view says when it has no turns to show, a failed
//! read with **Try again** and the end of a long tool output in reach.
//! How text and tool input show is in [`text`], when the view closes in
//! [`close`], and items read page by page in [`load`].

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Entity, ScrollDelta, TestAppContext, VisualTestContext, point, px};
use serde_json::{Value, json};
use subspace_client_fleet::activity::{ActivityNews, AgentActivity};
use subspace_client_fleet::{Error, FleetState, Link};

use super::super::remove::settle;
use super::fleet::ship;
use super::live::{live, set_fleet};
use crate::Irc;
use crate::view::inspector::activity::retry_scheduled;

mod close;
mod draw;
mod load;
mod text;

/// The last line of the long tool output.
const END: &str = "END-OF-OUTPUT";

fn turn(id: &str, status: &str, reconnects: u32, items: Value) -> Value {
    json!({
        "id": id, "seq": 5, "status": status, "startedAt": 1000, "updatedAt": 2000,
        "endedAt": null, "reconnects": reconnects,
        "usage": {"steps": 1, "tokens": {"input": 10, "output": 5, "reasoning": 0,
                                          "cacheRead": 0, "cacheWrite": 0}, "cost": 0.5},
        "items": items
    })
}

/// A snapshot of `example` turns: an older completed one and a running
/// one with a reply and a tool call whose output is `output`.
fn snapshot(collected: bool, output: &str) -> AgentActivity {
    let running = json!([
        {"id": "i1", "seq": 4, "startedAt": 1, "updatedAt": 2, "kind": "text",
         "status": "completed", "text": "Looking at\nthe logs"},
        {"id": "i2", "seq": 5, "startedAt": 1, "updatedAt": 2, "kind": "tool",
         "callId": "call-1", "name": "bash", "status": "completed",
         "rawInput": "", "input": {"command": "cat example.log"}, "executed": true,
         "metadata": null, "content": [{"type": "text", "text": output}], "error": null}
    ]);
    let done = json!([
        {"id": "i9", "seq": 2, "startedAt": 1, "updatedAt": 2, "kind": "text",
         "status": "completed", "text": "Earlier reply"}
    ]);
    serde_json::from_value(json!({
        "agent": {"id": "aurora-lyra", "role": "captain", "backend": "opencode",
                  "collected": collected},
        "turns": [turn("t-old", "completed", 0, done), turn("t-now", "running", 1, running)],
        "cursor": {"generation": "g1", "seq": 5}
    }))
    .unwrap()
}

/// The turn view's fetch answers `result`, on a fleet read on a timer.
fn answer(board: &Entity<Irc>, result: Result<AgentActivity, Error>, cx: &mut VisualTestContext) {
    answer_on(board, Link::Polling, result, cx);
}

/// The turn view's fetch answers `result`, on a fleet whose stream is
/// `link`: a live one carries the captain on connection 1.
fn answer_on(
    board: &Entity<Irc>,
    link: Link,
    result: Result<AgentActivity, Error>,
    cx: &mut VisualTestContext,
) {
    board.update(cx, |board, cx| {
        let join = board.inspector_activity.as_mut().expect("open").join_mut();
        join.set_fleet_state(&FleetState::Followed(link));
        if link == Link::Live {
            let mut news = ActivityNews::default();
            news.connection = Some(1);
            join.absorb(news);
        }
        assert!(join.start_fetch());
        crate::view::inspector::activity::finish_fetch(board, result, cx);
    });
    settle(cx);
}

/// Asks the turn view to read the snapshot again.
fn refresh(board: &Entity<Irc>, cx: &mut VisualTestContext) {
    board.update(cx, |board, _| {
        board
            .inspector_activity
            .as_mut()
            .expect("open")
            .join_mut()
            .refresh();
    });
}

/// The live Ship tab of `#aurora-lyra` with its turns open.
fn opened(cx: &mut TestAppContext) -> (Entity<Irc>, &mut VisualTestContext) {
    let (board, hub, cx) = ship(cx);
    set_fleet(&hub, live(), cx);
    cx.update(|window, cx| window.click("irc-inspector-activity", cx));
    settle(cx);
    (board, cx)
}

/// The visible text drawn inside the element `id`, joined with ` | `.
fn drawn(id: &str, cx: &mut VisualTestContext) -> String {
    cx.update(|window, _| {
        gpui_kit::base::test_support::texts(window)
            .into_iter()
            .filter(|text| text.path().contains(&id.to_owned().into()) && text.visible())
            .map(|text| text.text().to_owned())
            .collect::<Vec<_>>()
            .join(" | ")
    })
}

#[gpui_kit::test]
fn the_captain_card_opens_the_turns_and_back_returns_to_the_ship(cx: &mut TestAppContext) {
    let (board, hub, cx) = ship(cx);
    set_fleet(&hub, live(), cx);
    cx.update(|window, cx| {
        assert_eq!(
            window.find("irc-inspector-activity").label(),
            Some("Activity")
        );
        window.click("irc-inspector-activity", cx);
    });
    settle(cx);
    cx.update(|window, _| {
        assert!(window.find("irc-turns").visible());
        assert_eq!(
            window.find("irc-activity-back").label(),
            Some("Back to Ship")
        );
        assert!(
            window.try_find("irc-inspector-ship").is_none(),
            "the turns take the tab"
        );
    });
    assert_eq!(drawn("irc-activity-loading", cx), "Loading turns…");
    cx.update(|window, cx| window.click("irc-activity-back", cx));
    settle(cx);
    cx.update(|window, _| {
        assert!(window.try_find("irc-turns").is_none());
        assert!(window.find("irc-inspector-ship").visible());
    });
    board.read_with(cx, |board, _| assert!(board.inspector_activity.is_none()));
}

#[gpui_kit::test]
fn turns_show_newest_first_with_the_running_one_open(cx: &mut TestAppContext) {
    let (board, cx) = opened(cx);
    answer(&board, Ok(snapshot(true, "a\nb")), cx);
    cx.update(|window, _| {
        let now = window.find("irc-activity-turn:t-now");
        let old = window.find("irc-activity-turn:t-old");
        assert_eq!(
            now.label(),
            Some("Running turn · 1 steps · 15 tokens · $0.5000")
        );
        assert!(
            now.bounds().origin.y < old.bounds().origin.y,
            "newest first"
        );
        assert!(window.try_find("irc-activity-loading").is_none());
    });
    assert_eq!(
        drawn("irc-activity:t-now:i1:text", cx),
        "Looking at\nthe logs"
    );
    assert_eq!(
        drawn("irc-activity:t-now:i2:input:command", cx),
        "cat example.log"
    );
    assert_eq!(
        drawn("irc-activity-reconnected:t-now", cx),
        "Some changes may be missing (the stream reconnected)"
    );
    assert_eq!(
        drawn("irc-activity:t-old:i9:text", cx),
        "",
        "an ended turn is closed"
    );
    cx.update(|window, cx| window.click("irc-activity-turn:t-old", cx));
    settle(cx);
    assert_eq!(drawn("irc-activity:t-old:i9:text", cx), "Earlier reply");
}

#[gpui_kit::test]
fn a_gateway_without_the_route_and_an_uncollected_backend_say_so(cx: &mut TestAppContext) {
    let (board, cx) = opened(cx);
    answer(&board, Err(Error::NotFound), cx);
    assert_eq!(
        drawn("irc-activity-error", cx),
        "Activity isn't available on this gateway."
    );
    cx.update(|window, _| {
        assert!(
            window.try_find("irc-activity-retry").is_none(),
            "a gateway without the route is not tried again"
        );
    });
    board.update(cx, |board, _| {
        let selection = board.inspector_activity.as_mut().unwrap();
        assert!(!retry_scheduled(selection));
        assert!(selection.join_mut().retry(), "only when asked");
    });
    let mut uncollected = snapshot(false, "");
    uncollected.turns.clear();
    answer(&board, Ok(uncollected), cx);
    assert_eq!(drawn("irc-activity-error", cx), "");
    assert_eq!(
        drawn("irc-activity-uncollected", cx),
        "This officer's backend isn't collected yet."
    );
}

#[gpui_kit::test]
fn the_end_of_a_long_tool_output_is_in_reach(cx: &mut TestAppContext) {
    let (board, cx) = opened(cx);
    let mut output: String = (0..400)
        .map(|line| format!("line {line} of the log\n"))
        .collect();
    output.push_str(END);
    answer(&board, Ok(snapshot(true, &output)), cx);
    let id = "irc-activity:t-now:i2:out:0";
    let bottom = |cx: &mut VisualTestContext| {
        cx.update(|window, _| {
            let body = window.find("workspace-inspector-body").bounds();
            let text = gpui_kit::base::test_support::texts(window)
                .into_iter()
                .find(|text| text.path().contains(&id.into()) && text.text().ends_with(END))
                .expect("the output, whole");
            (text.bounds().bottom(), body.bottom())
        })
    };
    let (end, view) = bottom(cx);
    assert!(end > view, "the output runs past the inspector");
    for _ in 0..40 {
        cx.update(|window, cx| {
            window.scroll(
                "workspace-inspector-body",
                ScrollDelta::Pixels(point(px(0.), px(-2000.))),
                cx,
            );
        });
        settle(cx);
    }
    let (end, view) = bottom(cx);
    assert!(end <= view + px(1.), "scrolled to, the output's end shows");
}

/// A snapshot of one running turn, `t-now`, with `items`.
fn running(items: Value) -> AgentActivity {
    serde_json::from_value(json!({
        "agent": {"id": "aurora-lyra", "role": "captain", "backend": "opencode",
                  "collected": true},
        "turns": [turn("t-now", "running", 0, items)],
        "cursor": {"generation": "g1", "seq": 5}
    }))
    .unwrap()
}

/// Whether the turn view is open.
fn is_open(board: &Entity<Irc>, cx: &mut VisualTestContext) -> bool {
    board.read_with(cx, |board, _| board.inspector_activity.is_some())
}

#[gpui_kit::test]
fn a_failed_read_keeps_its_error_with_try_again_until_the_turns_arrive(cx: &mut TestAppContext) {
    let (board, cx) = opened(cx);
    answer_on(&board, Link::Live, Err(Error::Busy(None)), cx);
    assert_eq!(
        drawn("irc-activity-error", cx),
        "Stargate is busy. Subspace will try again."
    );
    cx.update(|window, _| {
        assert_eq!(window.find("irc-activity-retry").label(), Some("Try again"));
    });
    let scheduled = |board: &Entity<Irc>, cx: &mut VisualTestContext| {
        board.read_with(cx, |board, _| {
            retry_scheduled(board.inspector_activity.as_ref().unwrap())
        })
    };
    assert!(scheduled(&board, cx), "and tried again on a timer");
    cx.update(|window, cx| window.click("irc-activity-retry", cx));
    settle(cx);
    assert!(!scheduled(&board, cx), "asked for, the timer goes");
    answer_on(&board, Link::Live, Ok(snapshot(true, "a")), cx);
    assert_eq!(drawn("irc-activity-error", cx), "");
    cx.update(|window, _| {
        assert!(window.try_find("irc-activity-retry").is_none());
        assert!(window.find("irc-activity-turn:t-now").visible());
    });
    assert_eq!(
        drawn("irc-activity:t-now:i1:text", cx),
        "Looking at\nthe logs"
    );
}
