//! The dock on the real board: opening buffers from the list, tabs and
//! splits, closing, the member list following focus, the navigator and the
//! inspector shown and hidden, and the layout saved and restored in buffer
//! terms.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use subspace_client_irc::{BufferKey, Event};
use subspace_framework::dashboard::{ListDashboard as _, list};

use super::layout::{Axis, KEY, Layout, Pane, Side, decode};
use super::persist::layout_of;
use super::{Where, close, request, toggle_buffers, toggle_members};
use crate::Irc;
use crate::fixtures::{channel, message, query};
use crate::hub::Hub;
use crate::rows;
use crate::state::server_key;
use crate::tests::{channel_events, inject, mount, settings};

fn settle(cx: &mut VisualTestContext) {
    cx.run_until_parked();
    cx.update(|window, cx| window.render_frame(cx));
    cx.run_until_parked();
}

fn board_with_buffers(
    cx: &mut TestAppContext,
) -> (Entity<Irc>, Entity<Hub>, &mut VisualTestContext) {
    let (board, hub, cx) = mount(cx, settings());
    let mut events = channel_events();
    events.push(Event::Joined {
        buffer: channel("#crew"),
    });
    events.push(Event::Line(message(&query("mira"), "mira", "hi", 4)));
    inject(&hub, events, cx);
    settle(cx);
    (board, hub, cx)
}

fn open(board: &Entity<Irc>, buffer: &BufferKey, place: Where, cx: &mut VisualTestContext) {
    let buffer = buffer.clone();
    board.update(cx, |board, cx| request(board, buffer, place, cx));
    settle(cx);
}

fn layout(board: &Entity<Irc>, cx: &mut VisualTestContext) -> Layout {
    board.read_with(cx, layout_of)
}

fn focused(board: &Entity<Irc>, cx: &mut VisualTestContext) -> Option<BufferKey> {
    board.read_with(cx, |board, _| board.dock.focused.clone())
}

#[gpui_kit::test]
fn the_server_buffer_opens_first_and_a_list_click_opens_a_tab(cx: &mut TestAppContext) {
    let (board, _, cx) = board_with_buffers(cx);
    let server = server_key(&crate::fixtures::network_id());
    assert_eq!(focused(&board, cx), Some(server.clone()));
    let room = channel("#subspace");
    board.update(cx, |board, cx| {
        let key = rows::row_key(&room);
        list::update(board, cx, |list| list.select(key.clone()));
        assert!(
            board.on_activate(&key, cx),
            "the board opens buffers itself"
        );
    });
    settle(cx);
    assert_eq!(focused(&board, cx), Some(room.clone()));
    assert_eq!(
        layout(&board, cx).center,
        Some(Pane::Tabs {
            buffers: vec![server, room.clone()],
            active: 1,
        })
    );
    open(&board, &room, Where::Tab, cx);
    assert_eq!(
        layout(&board, cx).buffers().len(),
        2,
        "an open buffer is focused, not opened twice"
    );
}

#[gpui_kit::test]
fn buffers_open_beside_and_below_the_focused_one(cx: &mut TestAppContext) {
    let (board, _, cx) = board_with_buffers(cx);
    let room = channel("#subspace");
    open(&board, &room, Where::Tab, cx);
    open(&board, &channel("#crew"), Where::Right, cx);
    open(&board, &query("mira"), Where::Below, cx);
    let Some(Pane::Split { axis, children }) = layout(&board, cx).center else {
        panic!("a split");
    };
    assert_eq!(axis, Axis::Horizontal);
    assert_eq!(children.len(), 2);
    let Pane::Split { axis, children } = &children[1].0 else {
        panic!("#crew and mira share a column: {children:?}");
    };
    assert_eq!(*axis, Axis::Vertical);
    assert_eq!(children.len(), 2);
    assert_eq!(focused(&board, cx), Some(query("mira")));
}

#[gpui_kit::test]
fn closing_a_panel_forgets_it_and_the_member_list_follows_focus(cx: &mut TestAppContext) {
    let (board, _, cx) = board_with_buffers(cx);
    let room = channel("#subspace");
    open(&board, &room, Where::Tab, cx);
    cx.update(|window, _| {
        assert!(
            window.try_find("irc-member:alice").is_some(),
            "the channel's members"
        );
    });
    open(&board, &query("mira"), Where::Tab, cx);
    cx.update(|window, _| {
        assert!(window.try_find("irc-member:alice").is_none());
        assert!(
            window.try_find("irc-members").is_none(),
            "no member list for a private conversation"
        );
    });
    board.update(cx, |board, cx| {
        super::with_window(board, cx, |board, window, cx| {
            close(board, &query("mira"), window, cx)
        });
    });
    settle(cx);
    board.read_with(cx, |board, _| {
        assert!(!board.dock.open.contains_key(&query("mira")));
    });
    assert!(!layout(&board, cx).buffers().contains(&query("mira")));
}

#[gpui_kit::test]
fn the_member_list_sits_in_the_inspector_at_the_window_edge(cx: &mut TestAppContext) {
    let (board, _, cx) = board_with_buffers(cx);
    open(&board, &channel("#subspace"), Where::Tab, cx);
    cx.update(|window, _| {
        let list = window.find("irc-members").bounds();
        let inspector = window.find("irc-inspector").bounds();
        let pane = window.find("irc-pane").bounds();
        let edge = window.viewport_size().width;
        assert!(
            (f32::from(inspector.right()) - f32::from(edge)).abs() < 1.,
            "no inset: the inspector {inspector:?} reaches the window's edge {edge:?}"
        );
        assert!(
            inspector.left() >= pane.right(),
            "the inspector {inspector:?} is beside the pane {pane:?}"
        );
        assert!(
            list.left() >= inspector.left() && list.right() <= inspector.right(),
            "the list {list:?} lies inside the inspector {inspector:?}"
        );
        assert_eq!(
            window.find("workspace-inspector-title").label(),
            Some("Members · 2"),
            "the inspector's title counts the channel's members"
        );
    });
}

#[gpui_kit::test]
fn hiding_the_navigator_and_the_inspector_is_saved_and_restored(cx: &mut TestAppContext) {
    let (board, _, cx) = board_with_buffers(cx);
    open(&board, &channel("#subspace"), Where::Tab, cx);
    cx.update(|window, _| {
        assert!(window.try_find("workspace-navigator").is_some());
        assert!(window.try_find("irc-inspector").is_some());
        assert!(window.try_find("irc-conversation-menu").is_none());
    });
    board.update(cx, |board, cx| {
        toggle_buffers(board, cx);
        toggle_members(board, cx);
    });
    settle(cx);
    cx.update(|window, _| {
        assert!(window.try_find("workspace-navigator").is_none());
        assert!(window.try_find("irc-inspector").is_none());
        assert_eq!(
            window.find("irc-conversation-menu").label(),
            Some("Conversation: #subspace"),
            "the toolbar's menu keeps the conversations reachable"
        );
    });
    let stored = cx.update(|_, cx| subspace_framework::dashboard::prefs::get(cx, KEY));
    let stored = decode(stored.as_deref()).expect("the layout was saved");
    assert_eq!(
        (stored.buffers, stored.members),
        (Side { open: false }, Side { open: false })
    );

    board.update(cx, |board, cx| {
        board.dock.reset();
        // As a board starts, before its stored layout is read.
        board.dock.buffers = Side::default();
        board.dock.members = Side::default();
        super::restore(board, cx);
    });
    settle(cx);
    board.read_with(cx, |board, _| {
        assert!(!board.dock.buffers_open() && !board.dock.members_open());
    });
}

#[gpui_kit::test]
fn the_layout_is_saved_and_restored_in_buffer_terms(cx: &mut TestAppContext) {
    let (board, _, cx) = board_with_buffers(cx);
    open(&board, &channel("#subspace"), Where::Tab, cx);
    open(&board, &query("mira"), Where::Below, cx);
    let saved = layout(&board, cx);
    let stored = cx.update(|_, cx| subspace_framework::dashboard::prefs::get(cx, KEY));
    assert_eq!(decode(stored.as_deref()), Some(saved.clone()));

    board.update(cx, |board, cx| {
        board.dock.reset();
        super::restore(board, cx);
    });
    settle(cx);
    assert_eq!(layout(&board, cx).center, saved.center, "restored");
    board.read_with(cx, |board, _| assert_eq!(board.dock.open.len(), 3));
}

#[gpui_kit::test]
fn restoring_drops_a_conversation_removed_from_the_list(cx: &mut TestAppContext) {
    let (board, hub, cx) = board_with_buffers(cx);
    open(&board, &channel("#subspace"), Where::Tab, cx);
    open(&board, &query("mira"), Where::Below, cx);
    assert!(layout(&board, cx).buffers().contains(&query("mira")));
    // Removed while the saved layout still holds it, as after a crash.
    hub.update(cx, |hub, _| hub.state.removed.insert(&query("mira"), None));

    board.update(cx, |board, cx| {
        board.dock.reset();
        super::restore(board, cx);
    });
    settle(cx);
    let restored = layout(&board, cx).buffers();
    assert!(!restored.contains(&query("mira")), "{restored:?}");
    assert!(restored.contains(&channel("#subspace")));
}
