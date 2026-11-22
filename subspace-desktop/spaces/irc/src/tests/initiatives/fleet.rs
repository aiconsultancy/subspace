//! Fleet roles on the board: the rank mark after an agent's name in the
//! transcript, the member list's note, a ship channel's heading with its
//! captain's live status, and the toolbar's More menu holding the
//! connection commands.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use serde_json::json;
use subspace_api_client::model::MemberInitiative;
use subspace_client_irc::{Command, ConnectionStatus, Event};

use super::super::remove::{choose, settle};
use super::super::{inject, mount, settings};
use crate::fixtures::{channel, member, message, network_id};
use crate::hub::Hub;
use crate::{Irc, view};

const SHIP: &str = "#aurora-lyra";

/// One initiative whose fleet, `aurora`, has captain Lyra (`aurora-lyra`)
/// and rear admiral Vance (`aurora-admiral`), by the nicks the API records.
/// The test hub reads any fleet's server as its loopback network.
pub(crate) fn aurora() -> Vec<MemberInitiative> {
    serde_json::from_value(json!([{
        "id": "initiative-aurora", "name": "Fallow Reach", "channels": "#aurora-starbase #aurora-lyra",
        "fleetId": "aurora",
        "fleet": {"id": "aurora", "slug": "aurora", "name": "Aurora", "ircHost": "irc.example.test",
                  "ircPort": 6697, "ircTls": true, "rearAdmiralName": "admiral-vance",
                  "captainNames": ["Lyra"],
                  "agents": [
                      {"userId": "u-vance", "name": "admiral-vance", "rank": "rearAdmiral",
                       "ircNick": "aurora-admiral", "available": true},
                      {"userId": "u-lyra", "name": "Lyra", "rank": "captain", "shipId": "s-lattice",
                       "ship": "Lattice", "ircNick": "aurora-lyra", "available": true}
                  ],
                  "ships": [{"id": "s-lattice", "name": "Lattice", "channel": null}]}
    }]))
    .unwrap()
}

/// `#aurora-lyra` open, with a line each from mira (a person), the captain
/// and the rear admiral.
pub(super) fn ship(cx: &mut TestAppContext) -> (Entity<Irc>, Entity<Hub>, &mut VisualTestContext) {
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
                    signed_member("aurora-lyra"),
                    signed_member("aurora-admiral"),
                    member("aurora-vance", ""),
                ],
            },
            Event::Line(message(&room, "mira", "Morning, Lyra.", 1)),
            Event::Line(signed(message(&room, "aurora-lyra", "Wren has NAV-7.", 2))),
            Event::Line(signed(message(&room, "aurora-admiral", "Noted.", 3))),
        ],
        cx,
    );
    board.update(cx, |board, cx| view::reveal(board, &room, cx));
    settle(cx);
    (board, hub, cx)
}

/// A channel member signed in to the account of its own nick, as fleet
/// servers require.
pub(crate) fn signed_member(nick: &str) -> subspace_client_irc::Member {
    subspace_client_irc::Member {
        account: Some(nick.to_owned()),
        ..member(nick, "")
    }
}

/// `line` with its sender's account: the server's word that the nick is
/// that account.
pub(crate) fn signed(mut line: subspace_client_irc::Line) -> subspace_client_irc::Line {
    line.account = line.from.clone();
    line
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

#[gpui_kit::test]
fn a_ship_channel_s_heading_shows_its_glyph_and_its_captain_s_live_status(cx: &mut TestAppContext) {
    let (_, hub, cx) = ship(cx);
    cx.update(|window, _| {
        assert!(
            window
                .within("irc-toolbar")
                .find("irc-header-kind")
                .visible()
        );
        assert!(
            window.try_find("irc-header-live").is_none(),
            "no status, no pill: nothing invents an idle state"
        );
    });
    inject(
        &hub,
        vec![Event::NickStatus {
            network: network_id(),
            nick: "aurora-lyra".into(),
            status: Some("Lyra is delegating NAV-2 to Juniper".into()),
        }],
        cx,
    );
    settle(cx);
    cx.update(|window, _| {
        assert_eq!(
            window.find("irc-header-live").label(),
            Some("Captain aurora-lyra: Lyra is delegating NAV-2 to Juniper")
        );
    });
}

#[gpui_kit::test]
fn the_connection_controls_live_in_the_more_menu_beside_find(cx: &mut TestAppContext) {
    let (_, hub, cx) = ship(cx);
    cx.update(|window, _| {
        let row = window.within("irc-toolbar");
        assert!(row.try_find("irc-connection").is_none(), "not inline");
        assert!(row.try_find("irc-join").is_none(), "not inline");
        assert!(row.find("irc-find").visible());
        assert_eq!(row.find("irc-find").label(), Some("Find in conversation"));
    });
    cx.update(|window, cx| window.click("irc-more", cx));
    settle(cx);
    let labels = cx.update(|window, _| {
        let menu = window.within("popup-menu");
        (0usize..32)
            .filter_map(|index| menu.try_find(index))
            .filter_map(|item| item.label().map(str::to_owned))
            .collect::<Vec<_>>()
    });
    assert!(
        labels.starts_with(&["Disconnect".to_owned(), "Join…".to_owned()]),
        "{labels:?}"
    );
    assert!(labels.contains(&"Leave channel".to_owned()), "{labels:?}");
    choose("Disconnect", cx);
    hub.read_with(cx, |hub, _| {
        assert!(hub.sent().contains(&Command::Disconnect {
            network: network_id(),
            reason: String::new(),
        }));
    });
}

#[gpui_kit::test]
fn an_agent_s_nick_without_its_account_earns_no_rank(cx: &mut TestAppContext) {
    let (_, hub, cx) = ship(cx);
    let room = channel(SHIP);
    inject(
        &hub,
        vec![
            Event::Members {
                buffer: room.clone(),
                members: vec![member("mira", "@"), member("aurora-lyra", "")],
            },
            Event::Line(message(&room, "aurora-lyra", "I am the captain.", 4)),
        ],
        cx,
    );
    settle(cx);
    let impostor = hub.read_with(cx, |hub, _| {
        hub.state
            .buffer(&room)
            .and_then(|held| {
                held.entries
                    .iter()
                    .find(|entry| entry.line.text == "I am the captain.")
            })
            .map(|entry| entry.seq)
            .unwrap_or_default()
    });
    cx.update(|window, _| {
        assert!(
            window
                .try_find(format!("irc-line:{impostor}:rank"))
                .is_none(),
            "a line without the agent's account has no mark"
        );
    });
    assert!(!drawn("irc-member:aurora-lyra", cx).contains("captain"));
}

#[gpui_kit::test]
fn the_hub_builds_a_network_s_roster_once_until_the_catalogue_changes(cx: &mut TestAppContext) {
    let (_, hub, cx) = ship(cx);
    let (first, again) = hub.read_with(cx, |hub, cx| {
        (hub.roster(&network_id(), cx), hub.roster(&network_id(), cx))
    });
    assert!(std::rc::Rc::ptr_eq(&first, &again), "kept, not rebuilt");
    assert_eq!(first.rank("aurora-lyra"), Some(crate::fleet::Rank::Captain));
    hub.update(cx, |hub, cx| {
        hub.initiatives.catalog.clone().update(cx, |catalog, cx| {
            catalog.set_memberships_for_test(Some("fixture-user".into()), Ok(Vec::new()), cx)
        })
    });
    settle(cx);
    let after = hub.read_with(cx, |hub, cx| hub.roster(&network_id(), cx));
    assert_eq!(
        after.rank("aurora-lyra"),
        None,
        "rebuilt for the new catalogue"
    );
}

#[gpui_kit::test]
fn the_find_button_opens_find_in_the_conversation(cx: &mut TestAppContext) {
    use subspace_framework::dashboard::find_in_page;
    let (_, _, cx) = ship(cx);
    // The shell hosts find; here a global listener stands in for it.
    let opened = std::rc::Rc::new(std::cell::Cell::new(0));
    let heard = std::rc::Rc::clone(&opened);
    cx.update(|_, cx| {
        cx.on_action(move |_: &find_in_page::Open, _| heard.set(heard.get() + 1));
    });
    cx.update(|window, cx| window.click("irc-find", cx));
    settle(cx);
    assert_eq!(
        opened.get(),
        1,
        "the search button asks for find, as Command-F does"
    );
}

#[gpui_kit::test]
fn captains_and_the_rear_admiral_have_agent_avatars_and_people_keep_theirs(
    cx: &mut TestAppContext,
) {
    let (_, hub, cx) = ship(cx);
    let [mira, lyra, admiral] =
        ["mira", "aurora-lyra", "aurora-admiral"].map(|nick| seq_of(&hub, nick, cx));
    cx.update(|window, _| {
        let agent = |seq: i64| {
            window
                .try_find(format!("irc-line:{seq}:agent"))
                .and_then(|found| found.label().map(str::to_owned))
        };
        assert_eq!(agent(mira), None, "a person keeps the square initials");
        assert_eq!(agent(lyra).as_deref(), Some("aurora-lyra, captain"));
        assert_eq!(
            agent(admiral).as_deref(),
            Some("aurora-admiral, rear admiral")
        );
        let member = |nick: &str| {
            window
                .try_find(format!("irc-member-agent:{nick}"))
                .and_then(|found| found.label().map(str::to_owned))
        };
        assert_eq!(
            member("aurora-lyra").as_deref(),
            Some("aurora-lyra, captain")
        );
        assert_eq!(member("mira"), None);
        assert_eq!(
            window.within("irc-toolbar").find("irc-header-kind").label(),
            Some("aurora-lyra, captain"),
            "the ship's heading leads with its captain"
        );
    });
}
