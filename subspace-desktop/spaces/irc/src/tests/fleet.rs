//! Fleet roles on the board: the rank mark after an agent's name in the
//! transcript and the member list's note.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use serde_json::json;
use subspace_api_client::model::MemberInitiative;
use subspace_client_irc::{ConnectionStatus, Event};

use super::remove::settle;
use super::{inject, mount, settings};
use crate::fixtures::{channel, member, message, network_id};
use crate::hub::Hub;
use crate::{Irc, view};

const SHIP: &str = "#aurora-lyra";

/// One initiative whose fleet, `aurora`, has captain Lyra and rear admiral
/// Admiral. The test hub reads any fleet's server as its loopback network.
fn aurora() -> Vec<MemberInitiative> {
    serde_json::from_value(json!([{
        "id": "initiative-aurora", "name": "Fallow Reach", "channels": "#aurora-starbase #aurora-lyra",
        "fleetId": "aurora",
        "fleet": {"id": "aurora", "slug": "aurora", "name": "Aurora", "ircHost": "irc.example.test",
                  "ircPort": 6697, "ircTls": true, "rearAdmiralName": "Admiral",
                  "captainNames": ["Lyra"]}
    }]))
    .unwrap()
}

/// `#aurora-lyra` open, with a line each from mira (a person), the captain
/// and the rear admiral.
fn ship(cx: &mut TestAppContext) -> (Entity<Irc>, Entity<Hub>, &mut VisualTestContext) {
    let (board, hub, cx) = mount(cx, settings());
    hub.update(cx, |hub, cx| {
        hub.initiatives.catalog.clone().update(cx, |catalog, cx| {
            catalog.set_memberships_for_test(Some("fixture-user".into()), Ok(aurora()), cx)
        })
    });
    let room = channel(SHIP);
    inject(
        &hub,
        vec![
            Event::Status {
                network: network_id(),
                status: ConnectionStatus::Connected {
                    nick: "subspace".into(),
                },
            },
            Event::Joined {
                buffer: room.clone(),
            },
            Event::Members {
                buffer: room.clone(),
                members: vec![
                    member("mira", "@"),
                    member("aurora-lyra", ""),
                    member("aurora-admiral", ""),
                ],
            },
            Event::Line(message(&room, "mira", "Morning, Lyra.", 1)),
            Event::Line(message(&room, "aurora-lyra", "Wren has NAV-7.", 2)),
            Event::Line(message(&room, "aurora-admiral", "Noted.", 3)),
        ],
        cx,
    );
    board.update(cx, |board, cx| view::reveal(board, &room, cx));
    settle(cx);
    (board, hub, cx)
}

/// The sequence number of `nick`'s line in the ship channel.
fn seq_of(hub: &Entity<Hub>, nick: &str, cx: &mut VisualTestContext) -> i64 {
    hub.read_with(cx, |hub, _| {
        hub.state
            .buffer(&channel(SHIP))
            .and_then(|held| {
                held.entries
                    .iter()
                    .find(|entry| entry.line.from.as_deref() == Some(nick))
                    .map(|entry| entry.seq)
            })
            .unwrap_or_else(|| panic!("no line from {nick}"))
    })
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
fn an_agent_s_line_carries_its_rank_mark_and_a_person_s_does_not(cx: &mut TestAppContext) {
    let (_, hub, cx) = ship(cx);
    let [mira, lyra, admiral] =
        ["mira", "aurora-lyra", "aurora-admiral"].map(|nick| seq_of(&hub, nick, cx));
    cx.update(|window, _| {
        let rank = |seq: i64| window.try_find(format!("irc-line:{seq}:rank"));
        assert!(rank(mira).is_none(), "a person's line has no mark");
        assert_eq!(
            rank(lyra).and_then(|found| found.label().map(str::to_owned)),
            Some("captain".to_owned())
        );
        assert_eq!(
            rank(admiral).and_then(|found| found.label().map(str::to_owned)),
            Some("rear admiral".to_owned())
        );
    });
    assert!(drawn("irc-member:aurora-lyra", cx).contains("captain"));
    assert!(drawn("irc-member:aurora-admiral", cx).contains("rear admiral"));
    assert!(!drawn("irc-member:mira", cx).contains("captain"));
}
