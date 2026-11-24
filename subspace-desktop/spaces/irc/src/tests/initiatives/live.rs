//! Live fleet state as drawn: the ship channel's inspector with its Ship
//! and People tabs, each state of a fleet that is not live, the Starbase's
//! Ships, the header pill and the navigator dot's tooltip, and the board
//! following a fleet through the shared service while it shows it.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use subspace_client_fleet::{FleetApi, FleetRecord, FleetState, Link, TokenStore};
use subspace_client_irc::Event;
use subspace_framework::core::Secret;
use subspace_framework::dashboard::avatar::AgentIcons;

use super::super::inject;
use super::super::remove::settle;
use super::fleet::ship;
use crate::fixtures::{channel, message};
use crate::hub::Hub;
use crate::live::tests::aurora;
use crate::{Irc, rows, view};

/// The hub told the aurora fleet is in `state`, with the fixture's ships.
pub(super) fn set_fleet(hub: &Entity<Hub>, state: FleetState, cx: &mut VisualTestContext) {
    hub.update(cx, |hub, cx| {
        hub.fleets = [("aurora".to_owned(), aurora(state))].into();
        cx.notify();
    });
    settle(cx);
}

/// `#aurora-lyra` open as [`ship`] opens it, with its inspector's People
/// tab, where the member list is.
pub(super) fn ship_people(
    cx: &mut TestAppContext,
) -> (Entity<Irc>, Entity<Hub>, &mut VisualTestContext) {
    let (board, hub, cx) = ship(cx);
    board.update(cx, |board, cx| {
        board.inspector_tab = view::InspectorTab::People;
        cx.notify();
    });
    settle(cx);
    (board, hub, cx)
}

pub(super) fn live() -> FleetState {
    FleetState::Followed(Link::Live)
}

#[gpui_kit::test]
fn a_live_ship_channel_s_inspector_has_ship_thread_and_people_tabs(cx: &mut TestAppContext) {
    let (board, hub, cx) = ship(cx);
    set_fleet(&hub, live(), cx);
    cx.update(|window, _| {
        let tabs = window.find("workspace-inspector-tabs");
        assert_eq!(tabs.label(), Some("#aurora-lyra details"));
        let tabs = window.within("workspace-inspector-tabs");
        assert!(tabs.try_find_by_label("Ship").is_some());
        assert!(tabs.try_find_by_label("Thread").is_some());
        assert!(tabs.try_find_by_label("People, 4").is_some());
        assert!(window.find("irc-inspector-ship").visible());
        assert_eq!(
            window.find("irc-inspector-captain").label(),
            Some("aurora-lyra, captain, 3 of 4 crew busy"),
            "the captain's icon carries the crew ring"
        );
    });
    cx.update(|window, _| {
        for id in [
            "irc-inspector-state",
            "irc-inspector-crew:wren",
            "irc-inspector-crew:talon",
        ] {
            assert!(window.find(id).visible(), "{id}");
        }
        assert_eq!(
            window.find("irc-inspector-crew:wren").label(),
            Some("Open crew member Wren")
        );
        assert!(
            window.try_find("irc-member:mira").is_none(),
            "no member list here"
        );
    });
    cx.update(|window, cx| window.click(("workspace-inspector-tab", 2usize), cx));
    settle(cx);
    board.read_with(cx, |board, _| {
        assert_eq!(board.inspector_tab, view::InspectorTab::People);
    });
    cx.update(|window, _| {
        assert!(window.try_find("irc-inspector-subject").is_none());
        assert!(window.try_find("irc-inspector-ship").is_none());
    });
    cx.update(|window, _| {
        assert!(
            window.find("irc-member:mira").visible(),
            "today's member list"
        );
    });
}

#[gpui_kit::test]
fn with_icons_off_the_card_closes_up_around_its_text(cx: &mut TestAppContext) {
    let (_, hub, cx) = ship(cx);
    set_fleet(&hub, live(), cx);
    cx.update(|window, cx| {
        assert!(window.find("irc-inspector-captain").visible());
        AgentIcons::Off.apply(cx);
    });
    settle(cx);
    cx.update(|window, _| {
        assert!(window.try_find("irc-inspector-captain").is_none());
        assert!(window.find("irc-inspector-ship").visible());
    });
}

#[gpui_kit::test]
fn a_fleet_that_is_not_live_says_why_and_shows_no_figure(cx: &mut TestAppContext) {
    let (_, hub, cx) = ship(cx);
    for state in [
        FleetState::NotConfigured,
        FleetState::InvalidAddress,
        FleetState::Checking,
        FleetState::NoToken,
        FleetState::AddressChanged {
            saved_for: Some("https://old.example.test".into()),
        },
        FleetState::StoreUnavailable,
        FleetState::Ready,
        FleetState::Followed(Link::Connecting),
        FleetState::Followed(Link::TokenRefused),
        FleetState::Followed(Link::Forbidden),
        FleetState::Followed(Link::Unreachable),
    ] {
        set_fleet(&hub, state.clone(), cx);
        let (title, sentence) = view::inspector_reason(&state).expect("not live");
        let said = cx.update(|window, _| {
            assert!(
                window.try_find("irc-inspector-ship").is_none(),
                "{state:?}: no card"
            );
            assert!(
                window.try_find("irc-inspector-crew:wren").is_none(),
                "{state:?}: no crew"
            );
            window
                .find("irc-inspector-reason")
                .label()
                .map(str::to_owned)
        });
        assert_eq!(said, Some(format!("{title}: {sentence}")), "{state:?}");
        let pill = cx.update(|window, _| {
            window
                .try_find("irc-header-live")
                .and_then(|pill| pill.label().map(str::to_owned))
        });
        assert!(!pill.unwrap_or_default().contains("crew busy"), "{state:?}");
    }
    set_fleet(&hub, FleetState::NoToken, cx);
    let said = cx.update(|window, _| {
        window
            .find("irc-inspector-reason")
            .label()
            .map(str::to_owned)
    });
    assert!(
        said.unwrap_or_default()
            .ends_with("Open Settings › Fleets to do it.")
    );
}

#[gpui_kit::test]
fn the_header_pill_and_the_ship_s_dot_follow_the_live_crew(cx: &mut TestAppContext) {
    let (_, hub, cx) = ship(cx);
    set_fleet(&hub, live(), cx);
    cx.update(|window, _| {
        assert_eq!(
            window.find("irc-header-live").label(),
            Some("Captain aurora-lyra: Lyra is working · 3 of 4 crew busy")
        );
    });
    let dot = format!("irc-row-dot:{}", rows::row_key(&channel("#aurora-lyra")));
    cx.update(|window, _| {
        assert_eq!(
            window.find(dot.clone()).label(),
            Some("aurora-lyra: 3 crew working")
        );
    });
}

#[gpui_kit::test]
fn the_starbase_lists_the_roster_s_ships_with_their_crew(cx: &mut TestAppContext) {
    let (board, hub, cx) = ship(cx);
    let starbase = channel("#aurora-starbase");
    inject(
        &hub,
        vec![
            Event::Joined {
                buffer: starbase.clone(),
            },
            Event::Line(message(&starbase, "mira", "Morning.", 5)),
        ],
        cx,
    );
    board.update(cx, |board, cx| view::reveal(board, &starbase, cx));
    settle(cx);
    set_fleet(&hub, live(), cx);
    cx.update(|window, _| {
        let tabs = window.within("workspace-inspector-tabs");
        assert!(tabs.try_find_by_label("Starbase").is_some());
        assert!(window.find("irc-inspector-ship-row:aurora-lyra").visible());
        assert!(
            window
                .try_find("irc-inspector-ship-row:aurora-orion")
                .is_none(),
            "the roster names no Orion captain"
        );
        assert_eq!(
            window.find("irc-inspector-ship-meter:aurora-lyra").label(),
            Some("3 of 4 crew busy")
        );
    });
    set_fleet(&hub, FleetState::NoToken, cx);
    let said = cx.update(|window, _| {
        window
            .find("irc-inspector-reason")
            .label()
            .map(str::to_owned)
    });
    assert!(said.unwrap_or_default().starts_with("No token: "));
}

#[gpui_kit::test]
fn the_board_follows_a_ready_fleet_while_it_shows_it(cx: &mut TestAppContext) {
    let record = FleetRecord::new("aurora", "Aurora", Some("http://127.0.0.1:9".into()));
    let service = cx.update(|cx| FleetApi::install_new(TokenStore::memory(), vec![record], cx));
    cx.run_until_parked();
    let (board, hub, cx) = ship(cx);
    let following = |board: &Entity<Irc>, cx: &mut VisualTestContext| {
        board.read_with(cx, |board, _| board.watch.following())
    };
    board.update(cx, crate::live::watch::sync);
    settle(cx);
    assert!(following(&board, cx).is_empty(), "no token, no follow");
    hub.read_with(cx, |hub, _| {
        assert_eq!(
            hub.fleets.get("aurora").map(|f| &f.state),
            Some(&FleetState::NoToken)
        );
    });
    let saved = service.update(cx, |service, cx| {
        service.save_token("aurora", Secret::new("fixture-throwaway"), cx)
    });
    settle(cx);
    assert!(cx.foreground_executor().block_test(saved).is_ok());
    settle(cx);
    assert_eq!(following(&board, cx), ["aurora"]);
    hub.read_with(cx, |hub, _| {
        let state = hub.fleets.get("aurora").map(|fleet| fleet.state.clone());
        assert!(matches!(state, Some(FleetState::Followed(_))), "{state:?}");
    });
    // The navigator hidden and nothing of the fleet in front: let go.
    board.update(cx, |board, cx| {
        board.dock.focused = None;
        crate::dock::toggle_buffers(board, cx);
    });
    settle(cx);
    assert!(
        following(&board, cx).is_empty(),
        "a hidden navigator follows nothing"
    );
    board.update(cx, crate::dock::toggle_buffers);
    settle(cx);
    assert_eq!(following(&board, cx), ["aurora"], "shown again");
    board.update_in(cx, |board, window, cx| {
        subspace_framework::dashboard::Dashboard::deactivate(board, window, cx);
    });
    settle(cx);
    assert!(following(&board, cx).is_empty(), "let go once not shown");
}
