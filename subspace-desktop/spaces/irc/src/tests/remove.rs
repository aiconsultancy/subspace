//! Removing items from the list on the real board: each row's menu offers
//! Leave channel or Close conversation, the item leaves the list and its
//! panel, and it stays out after a restart until it is messaged or joined
//! again.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use subspace_client_irc::{BufferKey, Command, Event, LineKind};
use subspace_framework::dashboard::ListDashboard as _;

use super::{channel_events, inject, mount, settings};
use crate::fixtures::{at, channel, message, query};
use crate::hub::{self, Hub};
use crate::state::{REMOVED_KEY, Removed, server_key};
use crate::{Irc, rows, view};

pub(super) fn settle(cx: &mut VisualTestContext) {
    cx.run_until_parked();
    cx.update(|window, cx| window.render_frame(cx));
    cx.run_until_parked();
}

/// A board with `#subspace` and a private conversation with mira, open.
fn board(cx: &mut TestAppContext) -> (Entity<Irc>, Entity<Hub>, &mut VisualTestContext) {
    let (board, hub, cx) = mount(cx, settings());
    let mut events = channel_events();
    events.push(Event::Line(message(&query("mira"), "mira", "hi", 4)));
    inject(&hub, events, cx);
    board.update(cx, |board, cx| view::reveal(board, &query("mira"), cx));
    settle(cx);
    (board, hub, cx)
}

fn titles(board: &Entity<Irc>, cx: &mut VisualTestContext) -> Vec<String> {
    board.read_with(cx, |board, _| {
        board.list().groups()[0]
            .rows
            .iter()
            .map(|row| row.title.to_string())
            .collect()
    })
}

/// Opens `buffer`'s row menu and returns its item labels in order.
fn row_menu(buffer: &BufferKey, cx: &mut VisualTestContext) -> Vec<String> {
    menu(format!("irc-row:{}", rows::row_key(buffer)).into(), cx)
}

/// Opens the menu of the line element `id` is in and returns its item
/// labels in order.
pub(super) fn menu(id: gpui_kit::SharedString, cx: &mut VisualTestContext) -> Vec<String> {
    cx.update(|window, cx| window.right_click(id, cx));
    settle(cx);
    cx.update(|window, _| {
        let menu = window.within("popup-menu");
        (0usize..32)
            .filter_map(|index| menu.try_find(index))
            .filter_map(|item| item.label().map(str::to_owned))
            .collect()
    })
}

/// Clicks the open menu's item labelled `label`.
pub(super) fn choose(label: &str, cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        let mut menu = window.within("popup-menu");
        let index = (0usize..32)
            .find(|index| {
                menu.try_find(*index)
                    .is_some_and(|item| item.label() == Some(label))
            })
            .unwrap_or_else(|| panic!("no menu item {label}"));
        menu.click(index, cx);
    });
    settle(cx);
}

#[gpui_kit::test]
fn each_row_menu_offers_its_removal_and_the_server_none(cx: &mut TestAppContext) {
    let (_, hub, cx) = board(cx);
    let labels = row_menu(&query("mira"), cx);
    assert!(
        labels.contains(&"Close conversation".to_owned()),
        "{labels:?}"
    );
    assert!(!labels.contains(&"Leave channel".to_owned()));
    cx.update(|window, cx| window.press("escape", cx));
    settle(cx);

    let labels = row_menu(&channel("#subspace"), cx);
    assert!(labels.contains(&"Leave channel".to_owned()), "{labels:?}");
    cx.update(|window, cx| window.press("escape", cx));
    settle(cx);

    let labels = row_menu(&server_key(&crate::fixtures::network_id()), cx);
    let removals = ["Leave channel", "Remove from list", "Close conversation"];
    assert!(
        !labels
            .iter()
            .any(|label| removals.contains(&label.as_str())),
        "{labels:?}"
    );
    cx.update(|window, cx| window.press("escape", cx));
    settle(cx);

    // Kicked: there is nothing to leave, so the item says what it does.
    let kicked = Event::Left {
        buffer: channel("#subspace"),
    };
    inject(&hub, vec![kicked], cx);
    let labels = row_menu(&channel("#subspace"), cx);
    assert!(
        labels.contains(&"Remove from list".to_owned()),
        "{labels:?}"
    );
    assert!(!labels.contains(&"Leave channel".to_owned()));
}

#[gpui_kit::test]
fn a_closed_conversation_stays_out_after_a_restart_until_a_new_message(cx: &mut TestAppContext) {
    let (board, hub, cx) = board(cx);
    let mira = query("mira");
    row_menu(&mira, cx);
    choose("Close conversation", cx);
    assert_eq!(titles(&board, cx), ["Server", "#subspace"]);
    let server = server_key(&crate::fixtures::network_id());
    board.read_with(cx, |board, _| {
        assert!(!board.dock.open.contains_key(&mira), "its panel closed");
        assert_eq!(
            board.dock.focused.as_ref(),
            Some(&server),
            "the server buffer comes to the front"
        );
        assert_eq!(board.list().selected(), Some(&rows::row_key(&server)));
    });
    let saved = cx.update(|_, cx| subspace_framework::dashboard::prefs::get(cx, REMOVED_KEY));
    assert!(saved.is_some_and(|text| text.contains("mira")));
    hub.read_with(cx, |hub, _| {
        assert!(
            !hub.sent()
                .iter()
                .any(|command| matches!(command, Command::Part { .. })),
            "closing a private conversation sends nothing to the server"
        );
        assert!(
            hub.sent().contains(&Command::Forget {
                buffer: mira.clone()
            }),
            "but the session forgets it, so notices go to the server buffer"
        );
    });

    // A restart: a new hub reads the state file, and the server replays.
    let restarted = cx.update(|_, cx| hub::install_for_test(&settings(), cx));
    board.update(cx, view::bind_hub);
    inject(
        &restarted,
        vec![Event::Line(message(&mira, "mira", "hi", 4))],
        cx,
    );
    settle(cx);
    assert_eq!(titles(&board, cx), ["Server"], "the replay stays out");

    inject(
        &restarted,
        vec![Event::Line(message(&mira, "mira", "are you there?", 40))],
        cx,
    );
    settle(cx);
    assert_eq!(titles(&board, cx), ["Server", "mira"], "a new message");
    let saved = cx.update(|_, cx| subspace_framework::dashboard::prefs::get(cx, REMOVED_KEY));
    assert_eq!(saved.as_deref(), Some("[]"), "and the removal is forgotten");
}

#[gpui_kit::test]
fn leave_channel_parts_and_removes_it_from_the_list(cx: &mut TestAppContext) {
    let (board, hub, cx) = board(cx);
    let room = channel("#subspace");
    row_menu(&room, cx);
    choose("Leave channel", cx);
    assert_eq!(titles(&board, cx), ["Server", "mira"]);
    board.read_with(cx, |board, _| {
        assert_eq!(
            board.list().selected(),
            Some(&rows::row_key(&query("mira"))),
            "the conversation in front keeps the selection"
        );
    });
    hub.read_with(cx, |hub, _| {
        assert!(hub.sent().contains(&Command::Part {
            buffer: room.clone(),
            reason: String::new(),
        }));
        assert!(
            hub.sent().contains(&Command::Forget {
                buffer: room.clone(),
            }),
            "and the runtime stops joining it at connect"
        );
    });

    let part = crate::fixtures::line(&room, LineKind::Part, "subspace", "", 41);
    let left = Event::Left {
        buffer: room.clone(),
    };
    inject(&hub, vec![Event::Line(part), left], cx);
    settle(cx);
    assert_eq!(titles(&board, cx), ["Server", "mira"], "the echo stays out");

    let joined = Event::Joined {
        buffer: room.clone(),
    };
    hub.update(cx, |hub, cx| hub.ingest(vec![joined], at(42), cx));
    settle(cx);
    assert_eq!(
        titles(&board, cx),
        ["Server", "#subspace", "mira"],
        "joined again"
    );
}

#[gpui_kit::test]
fn a_notification_reopens_a_closed_conversation_but_not_a_left_channel(cx: &mut TestAppContext) {
    let (board, hub, cx) = board(cx);
    let (mira, room) = (query("mira"), channel("#subspace"));
    for buffer in [&mira, &room] {
        board.update(cx, |board, cx| crate::remove::remove(board, buffer, cx));
        settle(cx);
    }
    assert_eq!(titles(&board, cx), ["Server"]);

    board.update(cx, |board, cx| view::show_notified(board, &mira, cx));
    settle(cx);
    assert_eq!(titles(&board, cx), ["Server", "mira"]);
    board.read_with(cx, |board, _| {
        assert_eq!(board.dock.focused.as_ref(), Some(&mira));
    });

    board.update(cx, |board, cx| view::show_notified(board, &room, cx));
    settle(cx);
    assert_eq!(
        titles(&board, cx),
        ["Server", "mira"],
        "the channel stays left"
    );
    hub.read_with(cx, |hub, _| {
        assert!(
            !hub.sent()
                .iter()
                .any(|command| matches!(command, Command::Join { .. })),
            "and is not joined again"
        );
    });
}

/// The channels of the runtime's latest configuration.
fn configured_channels(hub: &Entity<Hub>, cx: &mut TestAppContext) -> Option<Vec<String>> {
    hub.read_with(cx, |hub, _| {
        hub.sent().iter().rev().find_map(|command| match command {
            Command::Configure(networks) => Some(networks[0].channels.clone()),
            _ => None,
        })
    })
}

#[gpui_kit::test]
fn a_left_channel_is_not_joined_at_startup_until_settings_add_it_again(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| subspace_framework::dashboard::prefs::init_file(None, cx));
    let mut removed = Removed::default();
    removed.insert(&channel("#subspace"), None);
    cx.update(|cx| subspace_framework::dashboard::prefs::set(cx, REMOVED_KEY, removed.encode()));
    let mut saved = settings();
    saved.networks[0].channels = vec!["#crew".into(), "#subspace".into()];
    let hub = cx.update(|cx| hub::install_for_test(&saved, cx));
    assert_eq!(
        configured_channels(&hub, cx),
        Some(vec!["#crew".to_owned()]),
        "a restart does not join the channel left"
    );

    let mut without = saved.clone();
    without.networks[0].channels = vec!["#crew".into()];
    let left = |hub: &Entity<Hub>, cx: &mut TestAppContext| {
        hub.read_with(cx, |hub, _| {
            hub.state.removed.get(&channel("#subspace")).is_some()
        })
    };
    hub.update(cx, |hub, cx| hub.apply(without.clone(), cx));
    assert!(left(&hub, cx), "dropping it from Settings keeps it left");
    hub.update(cx, |hub, cx| hub.apply(saved.clone(), cx));
    assert_eq!(
        configured_channels(&hub, cx),
        Some(vec!["#crew".to_owned(), "#subspace".to_owned()]),
        "adding it in Settings joins it again"
    );
    assert!(!left(&hub, cx));
}
