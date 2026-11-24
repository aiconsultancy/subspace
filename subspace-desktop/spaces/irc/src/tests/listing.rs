//! Channels from the server's list on the real board: greyed rows we are
//! not in, joined from their menu, a double click or Enter, and the network
//! header's Refresh channel list.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Entity, SharedString, TestAppContext, VisualTestContext};
use subspace_client_irc::{BufferKey, Command, Event, ListedChannel};
use subspace_framework::dashboard::ListDashboard as _;

use super::remove::{choose, menu as menu_labels, settle};
use super::{channel_events, inject, mount, settings};
use crate::fixtures::{channel, network_id};
use crate::hub::Hub;
use crate::{Irc, rows};

/// A connected board on a saved network with no fleet, in `#subspace`.
fn board(cx: &mut TestAppContext) -> (Entity<Irc>, Entity<Hub>, &mut VisualTestContext) {
    let (board, hub, cx) = mount(cx, settings());
    let mut events = channel_events();
    events.push(Event::ChannelList {
        network: network_id(),
        channels: ["#rust", "#subspace", "#aurora-lyra"]
            .map(|name| ListedChannel {
                name: name.into(),
                users: 7,
                topic: String::new(),
            })
            .to_vec(),
    });
    inject(&hub, events, cx);
    settle(cx);
    (board, hub, cx)
}

fn row_id(channel: &BufferKey) -> SharedString {
    format!("irc-row:{}", rows::listed_key(channel)).into()
}

fn joins(hub: &Entity<Hub>, cx: &mut VisualTestContext) -> Vec<String> {
    hub.read_with(cx, |hub, _| {
        hub.sent()
            .iter()
            .filter_map(|command| match command {
                Command::Join { channel, key, .. } if key.is_none() => Some(channel.clone()),
                _ => None,
            })
            .collect()
    })
}

#[gpui_kit::test]
fn an_unjoined_channel_is_greyed_and_its_menu_joins_and_opens_it(cx: &mut TestAppContext) {
    let (board, hub, cx) = board(cx);
    let rust = channel("#rust");
    cx.update(|_, cx| {
        let board = board.read(cx);
        let rows = &board.list().groups()[0].rows;
        let titles: Vec<&str> = rows.iter().map(|row| row.title.as_ref()).collect();
        assert_eq!(titles, ["Server", "#subspace", "#aurora-lyra", "#rust"]);
        assert!(
            board
                .group_captions(&board.list().groups()[0], cx)
                .is_empty()
        );
        assert!(rows[3].done, "greyed");
        assert_eq!(
            board.row_accessible_label(&rows[3], cx).as_ref(),
            "#rust, not joined, 7 users"
        );
    });

    let labels = menu_labels(row_id(&rust), cx);
    assert_eq!(labels, ["Join", "Copy title"]);
    choose("Join", cx);
    assert_eq!(joins(&hub, cx), ["#rust"]);
    board.read_with(cx, |board, _| {
        assert_ne!(
            board.dock.focused.as_ref(),
            Some(&rust),
            "not before the join"
        );
    });

    inject(
        &hub,
        vec![Event::Joined {
            buffer: rust.clone(),
        }],
        cx,
    );
    settle(cx);
    board.read_with(cx, |board, _| {
        assert_eq!(
            board.dock.focused.as_ref(),
            Some(&rust),
            "the channel opens"
        );
        assert_eq!(board.list().selected(), Some(&rows::row_key(&rust)));
        let row = board.list().groups()[0]
            .rows
            .iter()
            .find(|row| row.title.as_ref() == "#rust");
        assert!(row.is_some_and(|row| !row.done), "a normal channel now");
    });
    let labels = menu_labels(format!("irc-row:{}", rows::row_key(&rust)).into(), cx);
    assert!(labels.contains(&"Leave channel".to_owned()), "{labels:?}");
    assert!(!labels.contains(&"Join".to_owned()));
}

#[gpui_kit::test]
fn a_click_selects_an_unjoined_channel_and_a_double_click_joins_it(cx: &mut TestAppContext) {
    let (board, hub, cx) = board(cx);
    let rust = channel("#rust");
    cx.update(|window, cx| window.click(row_id(&rust), cx));
    settle(cx);
    assert!(joins(&hub, cx).is_empty(), "a click only selects");
    board.read_with(cx, |board, _| {
        assert_eq!(board.list().selected(), Some(&rows::listed_key(&rust)));
    });
    cx.update(|_, cx| {
        let board = board.read(cx);
        assert!(
            crate::listing::menu_command(board, cx).is_some(),
            "the selected unjoined channel offers Join"
        );
        let menu = crate::listing::group_menu(&board.list().groups()[0]);
        assert_eq!(
            menu.chat,
            subspace_framework::dashboard::row_menu::MenuChat::Off
        );
        assert!(!menu.title.is_empty());
    });
    cx.update(|window, cx| window.double_click(row_id(&rust), cx));
    settle(cx);
    assert_eq!(joins(&hub, cx), ["#rust"]);
}

#[gpui_kit::test]
fn enter_joins_the_selected_unjoined_channel(cx: &mut TestAppContext) {
    let (board, hub, cx) = board(cx);
    let rust = channel("#rust");
    cx.update(|window, cx| {
        board.update(cx, |board, cx| {
            crate::listing::reveal(board, &rust, cx);
            board.focus.focus(window, cx);
        });
        window.dispatch_action(Box::new(subspace_framework::dashboard::Activate), cx);
    });
    settle(cx);
    assert_eq!(joins(&hub, cx), ["#rust"]);
}

#[gpui_kit::test]
fn the_network_header_refreshes_the_channel_list(cx: &mut TestAppContext) {
    let (_, hub, cx) = board(cx);
    let header = format!("list-group-count:{}", rows::group_key(&network_id()));
    let labels = menu_labels(header.into(), cx);
    assert_eq!(labels, ["Refresh channel list", "Copy title", "Fold group"]);
    choose("Refresh channel list", cx);
    hub.read_with(cx, |hub, _| {
        assert!(hub.sent().contains(&Command::ListChannels {
            network: network_id()
        }));
    });
}
