//! Agents' nick-wide status lines on the board: above the composer, beside
//! who is typing, and with the member in the inspector.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{TestAppContext, VisualTestContext};
use subspace_client_irc::{ConnectionStatus, Event, Typing};
use subspace_framework::dashboard::Dashboard as _;

use super::{channel_events, inject, mount, settings};
use crate::fixtures::{channel, member, network_id};
use crate::view;

fn status(nick: &str, text: &str) -> Event {
    Event::NickStatus {
        network: network_id(),
        nick: nick.into(),
        status: Some(text.into()),
    }
}

fn typing(nick: &str) -> Event {
    Event::Typing {
        buffer: channel("#subspace"),
        nick: nick.into(),
        state: Typing::Active,
    }
}

/// `#subspace` open with galactica among its members, galactica's status,
/// a status from discovery, who is not in the channel, and alice and
/// galactica typing.
fn open(cx: &mut TestAppContext) -> &mut VisualTestContext {
    let (board, hub, cx) = mount(cx, settings());
    let room = channel("#subspace");
    let mut events = channel_events();
    events.extend([
        Event::Members {
            buffer: room.clone(),
            members: vec![
                member("alice", "@"),
                member("bob", ""),
                member("galactica", ""),
            ],
        },
        status("Galactica", "Galactica is thinking…\nabout the plan"),
        status("discovery", "Discovery is noodling on it…"),
        typing("alice"),
        typing("galactica"),
    ]);
    inject(&hub, events, cx);
    board.update(cx, |board, cx| view::reveal(board, &room, cx));
    cx.run_until_parked();
    cx.update(|window, cx| window.render_frame(cx));
    cx
}

/// The visible text drawn inside the element `id`, joined with ` | `.
fn drawn(id: &'static str, cx: &mut VisualTestContext) -> String {
    cx.update(|window, _| {
        gpui_kit::base::test_support::texts(window)
            .into_iter()
            .filter(|text| text.path().contains(&id.into()) && text.visible())
            .map(|text| text.text().to_owned())
            .collect::<Vec<_>>()
            .join(" | ")
    })
}

#[gpui_kit::test]
fn an_agent_s_status_shows_above_the_composer_beside_a_person_typing(cx: &mut TestAppContext) {
    let cx = open(cx);
    cx.update(|window, _| {
        assert_eq!(
            window.find("irc-activity").label(),
            Some("Status of Galactica: Galactica is thinking… about the plan; alice is typing…"),
            "galactica, also typing, shows its status only; discovery is not here"
        );
    });
    assert_eq!(
        drawn("irc-activity", cx),
        "Galactica is thinking… about the plan · alice is typing…",
        "one line, the status as the agent sent it"
    );
}

#[gpui_kit::test]
fn the_member_row_shows_the_status_and_says_it(cx: &mut TestAppContext) {
    let cx = open(cx);
    cx.update(|window, _| {
        assert_eq!(
            window.find("irc-member:galactica").label(),
            Some("Open private messages with galactica, Galactica is thinking… about the plan")
        );
        assert_eq!(
            window.find("irc-member:alice").label(),
            Some("Open private messages with alice"),
            "a member without a status reads as before"
        );
        let panel = window.find("irc-members").bounds();
        let row = window.find("irc-member:galactica").bounds();
        assert!(
            row.left() >= panel.left() && row.right() <= panel.right(),
            "the taller row {row:?} stays inside the member list {panel:?}"
        );
    });
    let shown = drawn("irc-member:galactica", cx);
    assert!(
        shown.contains("galactica") && shown.contains("Galactica is thinking… about the plan"),
        "the nick and its status: {shown}"
    );
    assert!(
        !drawn("irc-members", cx).contains("Discovery is noodling on it…"),
        "a nick that is not a member shows no status in the list"
    );
}

#[gpui_kit::test]
fn a_dropped_connection_takes_the_statuses_with_it(cx: &mut TestAppContext) {
    let (board, hub, cx) = mount(cx, settings());
    let room = channel("#subspace");
    let mut events = channel_events();
    events.push(status("alice", "Alice is thinking…"));
    inject(&hub, events, cx);
    board.update(cx, |board, cx| view::reveal(board, &room, cx));
    cx.run_until_parked();
    cx.update(|window, cx| window.render_frame(cx));
    assert_eq!(drawn("irc-activity", cx), "Alice is thinking…");
    let dropped = ConnectionStatus::Reconnecting {
        attempt: 1,
        delay_secs: 2,
        error: "Connection reset".into(),
    };
    let connected = ConnectionStatus::Connected {
        nick: "subspace".into(),
    };
    for status in [dropped, connected] {
        inject(
            &hub,
            vec![Event::Status {
                network: network_id(),
                status,
            }],
            cx,
        );
    }
    assert_eq!(
        drawn("irc-activity", cx),
        "",
        "nothing stale after reconnecting"
    );
    cx.update(|window, _| {
        assert_eq!(window.find("irc-activity").label(), None);
        assert_eq!(
            window.find("irc-member:alice").label(),
            Some("Open private messages with alice")
        );
    });
}

#[gpui_kit::test]
fn status_and_badge_summarise_networks_and_attention(cx: &mut TestAppContext) {
    let (board, hub, cx) = mount(cx, settings());
    inject(&hub, channel_events(), cx);
    cx.update(|_, cx| {
        assert_eq!(
            board.read(cx).status(cx).as_deref(),
            Some("IRC · 1 network · 1 connected"),
            "the status bar carries what the heading said"
        );
        assert_eq!(board.read(cx).badge(cx).as_deref(), Some("1"));
    });
}
