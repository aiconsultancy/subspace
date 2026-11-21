//! The Space with its navigator hidden: the toolbar's actions follow the
//! conversation it names, and the board keeps a mounted focus target for its
//! keys.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use subspace_client_irc::{Command, NetworkId};
use subspace_framework::dashboard::list;

use super::{channel_events, inject, mount, settings};
use crate::config::Settings;
use crate::fixtures::{channel, config, network_id};
use crate::hub::Hub;
use crate::{Irc, dock, rows, view};

fn home() -> NetworkId {
    NetworkId("home".into())
}

/// The fixture network, connected and showing `#subspace`, and a second
/// network, `home`, that is not connected.
fn two_networks() -> Settings {
    let mut other = config();
    other.id = home();
    other.name = "Home Ergo".into();
    other.channels = Vec::new();
    let mut settings = settings();
    settings.networks.push(other);
    settings
}

fn settle(cx: &mut VisualTestContext) {
    cx.run_until_parked();
    cx.update(|window, cx| window.render_frame(cx));
    cx.run_until_parked();
}

fn showing_subspace(cx: &mut TestAppContext) -> (Entity<Irc>, Entity<Hub>, &mut VisualTestContext) {
    let (board, hub, cx) = mount(cx, two_networks());
    inject(&hub, channel_events(), cx);
    let room = channel("#subspace");
    board.update(cx, |board, cx| view::reveal(board, &room, cx));
    settle(cx);
    board.read_with(cx, |board, _| assert_eq!(board.dock.focused, Some(room)));
    (board, hub, cx)
}

#[gpui_kit::test]
fn with_the_navigator_hidden_connection_acts_on_the_named_conversation(cx: &mut TestAppContext) {
    let (board, hub, cx) = showing_subspace(cx);
    // The other network's header is selected, then the navigator is hidden:
    // the toolbar still names #subspace on the fixture network.
    board.update(cx, |board, cx| {
        list::update(board, cx, |list| list.select(rows::group_key(&home())));
        dock::toggle_buffers(board, cx);
    });
    settle(cx);
    board.read_with(cx, |board, cx| {
        assert_eq!(board.toolbar_network(cx), Some(network_id()));
        assert_eq!(board.toolbar_buffer(), Some(&channel("#subspace")));
    });
    let before = hub.read_with(cx, |hub, _| hub.sent().len());
    cx.update(|window, cx| window.click("irc-connection", cx));
    settle(cx);
    hub.read_with(cx, |hub, _| {
        assert_eq!(
            hub.sent()[before..],
            [Command::Disconnect {
                network: network_id(),
                reason: String::new(),
            }],
            "the network of the conversation the toolbar names"
        );
    });
}

#[gpui_kit::test]
fn with_the_navigator_shown_the_toolbar_acts_on_the_selection(cx: &mut TestAppContext) {
    let (board, _, cx) = showing_subspace(cx);
    board.update(cx, |board, cx| {
        list::update(board, cx, |list| list.select(rows::group_key(&home())));
    });
    settle(cx);
    board.read_with(cx, |board, cx| {
        assert_eq!(board.toolbar_network(cx), Some(home()));
        assert_eq!(board.toolbar_buffer(), None);
    });
}

#[gpui_kit::test]
fn with_the_navigator_hidden_the_board_keeps_its_focus_target(cx: &mut TestAppContext) {
    let (board, _, cx) = showing_subspace(cx);
    board.update(cx, dock::toggle_buffers);
    settle(cx);
    cx.update(|window, cx| {
        assert!(window.try_find("workspace-navigator").is_none());
        let focus = board.read(cx).focus.clone();
        focus.focus(window, cx);
        window.render_frame(cx);
        let contexts = window.context_stack();
        assert!(
            contexts.iter().any(|context| context.contains(crate::ID)),
            "the board's key context is on the focused path: {contexts:?}"
        );
    });
}
