//! Keyboard and pointer navigation of the Ship tab's crew detail.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{ScrollDelta, TestAppContext, VisualTestContext, point, px};
use subspace_client_fleet::{FleetApi, FleetRecord, FleetState, TokenStore};
use subspace_client_irc::Event;

use super::super::{inject, remove::settle};
use super::fleet::ship;
use super::live::{live, set_fleet};
use crate::{fixtures::channel, view};

const ROW: &str = "irc-inspector-crew:wren";
const BACK: &str = "irc-crew-back";
const SHIP: (&str, usize) = ("workspace-inspector-tab", 0);
const THREAD: (&str, usize) = ("workspace-inspector-tab", 1);

pub(super) fn install_fleet_api(cx: &mut VisualTestContext) {
    cx.update(|_, cx| FleetApi::install_new(TokenStore::memory(), vec![], cx));
}

pub(super) fn round_trip(cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        window.click(SHIP, cx);
        for _ in 0..20 {
            if window.find(ROW).focused() == Some(true) {
                break;
            }
            window.focus_next(cx);
            window.render_frame(cx);
        }
        assert_eq!(window.find(ROW).focused(), Some(true));
        window.press("enter", cx);
        assert_eq!(window.find(BACK).label(), Some("Back to Ship"));
        assert_eq!(window.find(BACK).focused(), Some(true));
        window.press("space", cx);
        assert_eq!(window.find(ROW).focused(), Some(true));
        window.click(ROW, cx);
        assert!(window.find(BACK).visible());
        window.click(BACK, cx);
        assert!(window.find(ROW).visible());
    });
}

#[gpui_kit::test]
fn a_roster_without_fleet_api_shows_plain_rows(cx: &mut TestAppContext) {
    let (board, hub, cx) = ship(cx);
    set_fleet(&hub, live(), cx);
    cx.update(|window, cx| {
        assert!(window.find(ROW).visible());
        assert_ne!(window.find(ROW).label(), Some("Open crew member Wren"));
        window.click(ROW, cx);
        assert!(window.try_find(BACK).is_none());
    });
    board.read_with(cx, |board, _| assert!(board.inspector_crew.is_none()));
}

#[gpui_kit::test]
fn activating_ship_from_the_keyboard_keeps_its_focus_and_tab_order(cx: &mut TestAppContext) {
    let (_, hub, cx) = ship(cx);
    set_fleet(&hub, live(), cx);
    cx.update(|window, cx| {
        window.click(THREAD, cx);
        assert_eq!(window.find(THREAD).selected(), Some(true));
        for _ in 0..30 {
            if window.find(SHIP).focused() == Some(true) {
                break;
            }
            window.focus_next(cx);
            window.render_frame(cx);
        }
        assert_eq!(window.find(SHIP).focused(), Some(true));
        assert_eq!(window.find(SHIP).selected(), Some(false));
        window.press("enter", cx);
        assert_eq!(window.find(SHIP).selected(), Some(true));
        assert_eq!(window.find(SHIP).focused(), Some(true));
        // Follow the same focus traversal as Tab in the app shell.
        window.focus_next(cx);
        window.render_frame(cx);
        assert_eq!(window.find(THREAD).focused(), Some(true));
    });
}

#[gpui_kit::test]
fn thread_tab_pauses_refresh_until_the_ship_tab_returns(cx: &mut TestAppContext) {
    let (board, hub, cx) = ship(cx);
    set_fleet(&hub, live(), cx);
    install_fleet_api(cx);
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.click(ROW, cx);
    });
    board.read_with(cx, |board, cx| {
        assert!(view::inspector::crew::visible(board, cx))
    });
    cx.update(|window, cx| window.click(THREAD, cx));
    set_fleet(&hub, live(), cx);
    board.read_with(cx, |board, cx| {
        assert!(board.inspector_crew.is_some());
        assert!(!view::inspector::crew::visible(board, cx));
    });
    cx.update(|window, cx| window.click(SHIP, cx));
    board.read_with(cx, |board, cx| {
        assert!(view::inspector::crew::visible(board, cx))
    });
}

#[gpui_kit::test]
fn crew_detail_explains_the_missing_token_even_without_a_roster(cx: &mut TestAppContext) {
    let (_, hub, cx) = ship(cx);
    set_fleet(&hub, live(), cx);
    let api = cx.update(|window, cx| {
        let api = FleetApi::install_new(TokenStore::memory(), vec![], cx);
        window.render_frame(cx);
        window.click(ROW, cx);
        assert!(window.find(BACK).visible());
        api
    });
    let record = FleetRecord::new("aurora", "Aurora", Some("http://127.0.0.1:9".into()));
    api.update(cx, |api, cx| api.set_fleets(vec![record], cx));
    settle(cx);
    set_fleet(&hub, FleetState::NoToken, cx);
    cx.update(|window, _| {
        assert!(window.find(BACK).visible());
        let reason_node = window.find("irc-inspector-reason");
        let reason = reason_node.label().unwrap_or_default();
        assert!(reason.starts_with("No token: "), "{reason}");
        assert!(reason.contains("Settings › Fleets"), "{reason}");
        assert!(!reason.contains("Showing the ship's roster"), "{reason}");
    });
}

#[gpui_kit::test]
fn crew_task_progress_draws_only_the_selected_page(cx: &mut TestAppContext) {
    use gpui_kit::base::test_support::texts;

    const PAGER: &str = "irc-crew-task-pages:named:nav:progress";
    const LAST_PAGE: usize = 125;
    const TAIL: &str = "🌊\t\r\u{202e}\x1bfinish";
    const VISIBLE_TAIL: &str = "🌊⟨U+0009⟩⟨U+000D⟩⟨U+202E⟩⟨U+001B⟩finish";
    let (board, hub, cx) = ship(cx);
    let first = "λ".repeat(512);
    let mut snapshot = crate::live::tests::snapshot();
    let mut member = snapshot
        .ships
        .remove("aurora-lyra")
        .unwrap()
        .crew_members
        .into_iter()
        .find(|member| member.id == "wren")
        .unwrap();
    let task = member.contexts[0].current.as_mut().unwrap();
    task.progress = first.clone() + &"x".repeat(64_000 - 512 - TAIL.chars().count()) + TAIL;
    let summary = task.summary.clone();
    set_fleet(&hub, live(), cx);
    install_fleet_api(cx);
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.click(ROW, cx);
        board.update(cx, |board, cx| {
            view::inspector::crew::seed_roster_for_test(board, member, cx)
        });
        window.render_frame(cx);
        assert!(window.find(BACK).visible());
        let drawn = texts(window);
        assert!(!drawn.iter().any(|text| text.text().contains("finish")));
        assert_eq!(
            drawn.iter().filter(|text| text.text() == summary).count(),
            1
        );
        assert!(drawn.iter().any(|text| text.text() == first));
        window.within(PAGER).click(LAST_PAGE, cx);
        window.render_frame(cx);
        let drawn = texts(window);
        assert!(!drawn.iter().any(|text| text.text() == first));
        assert!(drawn.iter().any(|text| text.text().ends_with(VISIBLE_TAIL)));
    });
}

#[gpui_kit::test]
fn an_expanded_roster_opens_the_thirtieth_member(cx: &mut TestAppContext) {
    const LAST: &str = "irc-inspector-crew:member-30";
    const TOGGLE: &str = "irc-crew-toggle-roster";
    let (_, hub, cx) = ship(cx);
    let mut snapshot = crate::live::tests::snapshot();
    let members = &mut snapshot.ships.get_mut("aurora-lyra").unwrap().crew_members;
    *members = (1..=30)
        .map(|i| {
            let mut member = subspace_client_fleet::model::CrewMember::default();
            member.id = format!("member-{i}");
            member.name = format!("Member {i}");
            member
        })
        .collect();
    hub.update(cx, |hub, cx| {
        hub.fleets = [(
            "aurora".to_owned(),
            crate::live::FleetLive::new(live(), Some(&snapshot)),
        )]
        .into();
        cx.notify();
    });
    settle(cx);
    install_fleet_api(cx);
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.try_find(LAST).is_none());
        assert_eq!(window.find(TOGGLE).label(), Some("Show all 30 members"));
        window.click(TOGGLE, cx);
        window.render_frame(cx);
        window.scroll(
            "workspace-inspector-body",
            ScrollDelta::Pixels(point(px(0.), px(-1600.))),
            cx,
        );
    });
    settle(cx);
    cx.update(|window, cx| {
        assert_eq!(
            window.find(LAST).label(),
            Some("Open crew member Member 30")
        );
        window.click(LAST, cx);
        assert!(window.find(BACK).visible());
    });
}

#[gpui_kit::test]
fn back_to_ship_focuses_the_ship_tab_when_no_crew_row_remains(cx: &mut TestAppContext) {
    let (_, hub, cx) = ship(cx);
    set_fleet(&hub, live(), cx);
    install_fleet_api(cx);
    cx.update(|window, cx| window.click(ROW, cx));
    let mut snapshot = crate::live::tests::snapshot();
    snapshot
        .ships
        .get_mut("aurora-lyra")
        .unwrap()
        .crew_members
        .clear();
    hub.update(cx, |hub, cx| {
        hub.fleets = [(
            "aurora".to_owned(),
            crate::live::FleetLive::new(live(), Some(&snapshot)),
        )]
        .into();
        cx.notify();
    });
    settle(cx);
    cx.update(|window, cx| {
        assert_eq!(window.find(BACK).focused(), Some(true));
        window.press("enter", cx);
        assert_eq!(window.find(SHIP).focused(), Some(true));
        assert!(window.try_find(ROW).is_none());
    });
}

#[gpui_kit::test]
fn focusing_another_buffer_clears_crew_detail(cx: &mut TestAppContext) {
    let (board, hub, cx) = ship(cx);
    set_fleet(&hub, live(), cx);
    install_fleet_api(cx);
    cx.update(|window, cx| window.click(ROW, cx));
    let other = channel("#aurora-starbase");
    inject(
        &hub,
        vec![Event::Joined {
            buffer: other.clone(),
        }],
        cx,
    );
    board.update(cx, |board, cx| {
        assert!(board.inspector_crew.is_some());
        view::reveal(board, &other, cx);
    });
    settle(cx);
    board.read_with(cx, |board, _| {
        assert_eq!(board.dock.focused.as_ref(), Some(&other));
        assert!(board.inspector_crew.is_none());
    });
}
