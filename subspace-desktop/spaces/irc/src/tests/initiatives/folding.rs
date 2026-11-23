//! Which fleet groups start folded: a fleet that is not the selected
//! initiative's, unless it is a network's only fleet whose group took in
//! the network's own rows, and what a folded group says it hides.

use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use serde_json::json;
use subspace_api_client::model::MemberInitiative;
use subspace_client_irc::{ConnectionStatus, Event};
use subspace_framework::dashboard::ListDashboard as _;

use super::super::remove::settle;
use super::super::{inject, mount, settings};
use super::fleet::aurora;
use super::navigator::{STARBASE, mention};
use crate::fixtures::{channel, message, network_id, query};
use crate::hub::Hub;
use crate::{Irc, rows};

fn borealis() -> MemberInitiative {
    serde_json::from_value(json!({
        "id": "initiative-borealis", "name": "Night Watch", "channels": "#borealis-vega",
        "fleetId": "borealis",
        "fleet": {"id": "borealis", "slug": "borealis", "name": "Borealis",
                  "ircHost": "irc.example.test", "ircPort": 6697, "ircTls": true,
                  "agents": [{"userId": "u-vega", "name": "borealis-vega", "rank": "captain",
                              "ircNick": "borealis-vega", "available": true}]}
    }))
    .unwrap()
}

/// Two fleets on one network, the aurora initiative picked when `pick`,
/// with a channel of each fleet joined.
fn two_fleets(
    cx: &mut TestAppContext,
    pick: bool,
) -> (Entity<Irc>, Entity<Hub>, &mut VisualTestContext) {
    let (board, hub, cx) = mount(cx, settings());
    let mut initiatives = aurora();
    initiatives.push(borealis());
    hub.update(cx, |hub, cx| {
        hub.initiatives.catalog.clone().update(cx, |catalog, cx| {
            catalog.set_memberships_for_test(Some("fixture-user".into()), Ok(initiatives), cx);
            if pick {
                catalog.pick("initiative-aurora", cx);
            }
        })
    });
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
                buffer: channel("#aurora-starbase"),
            },
            Event::Joined {
                buffer: channel("#borealis-vega"),
            },
        ],
        cx,
    );
    settle(cx);
    (board, hub, cx)
}

#[gpui_kit::test]
fn fleets_other_than_the_selected_initiative_s_start_folded(cx: &mut TestAppContext) {
    let (board, _, cx) = two_fleets(cx, true);
    let aurora = rows::fleet_group_key(&network_id(), "aurora");
    let borealis = rows::fleet_group_key(&network_id(), "borealis");
    board.read_with(cx, |board, _| {
        assert!(
            !board.list.is_folded(&aurora),
            "the selected initiative's fleet"
        );
        assert!(board.list.is_folded(&borealis));
    });
    board.update(cx, |board, cx| {
        crate::view::reveal_row(board, &channel("#borealis-vega"), cx);
    });
    settle(cx);
    board.update(cx, crate::view::sync);
    board.read_with(cx, |board, _| {
        assert!(
            !board.list.is_folded(&borealis),
            "revealed, and not folded again by the next sync"
        );
        assert_eq!(
            board.list.selected(),
            Some(&rows::row_key(&channel("#borealis-vega")))
        );
    });
}

#[gpui_kit::test]
fn a_network_s_only_fleet_keeps_its_other_channels_open_when_another_fleet_is_picked(
    cx: &mut TestAppContext,
) {
    // The person is in a private conversation when the initiatives arrive,
    // with one of a fleet on no configured network picked: Aurora, this
    // network's only fleet, is not the selected one, and its group takes in
    // the network's other channels and its Server row.
    let (board, hub, cx) = mount(cx, settings());
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
                buffer: channel("#lounge"),
            },
            Event::Joined {
                buffer: channel(STARBASE),
            },
            Event::Joined {
                buffer: query("mira"),
            },
        ],
        cx,
    );
    settle(cx);
    board.update(cx, |board, cx| {
        crate::view::reveal_row(board, &query("mira"), cx)
    });
    settle(cx);
    let mut elsewhere = borealis();
    elsewhere.fleet = None;
    let mut initiatives = aurora();
    initiatives.push(elsewhere);
    hub.update(cx, |hub, cx| {
        hub.initiatives.catalog.clone().update(cx, |catalog, cx| {
            catalog.set_memberships_for_test(Some("fixture-user".into()), Ok(initiatives), cx);
            catalog.pick("initiative-borealis", cx);
        })
    });
    settle(cx);
    let aurora = rows::fleet_group_key(&network_id(), "aurora");
    board.read_with(cx, |board, _| {
        assert_eq!(board.list.selected(), Some(&rows::row_key(&query("mira"))));
        let group = board.list.groups().iter().find(|group| group.key == aurora);
        let titles: Vec<_> = group
            .map(|group| group.rows.iter().map(|row| row.title.to_string()).collect())
            .unwrap_or_default();
        assert!(
            titles.iter().any(|title| title.contains("lounge")),
            "{titles:?}"
        );
        assert!(
            !board.list.is_folded(&aurora),
            "the network's own channels and Server row stay open"
        );
    });
}

#[gpui_kit::test]
fn with_no_initiative_selected_every_fleet_starts_open(cx: &mut TestAppContext) {
    let (board, _, cx) = two_fleets(cx, false);
    board.read_with(cx, |board, _| {
        for fleet in ["aurora", "borealis"] {
            let key = rows::fleet_group_key(&network_id(), fleet);
            assert!(!board.list.is_folded(&key), "{fleet}");
        }
    });
}

#[gpui_kit::test]
fn a_folded_fleet_says_it_hides_a_mention(cx: &mut TestAppContext) {
    let (board, hub, cx) = two_fleets(cx, true);
    let borealis = rows::fleet_group_key(&network_id(), "borealis");
    let labels = |board: &Irc, cx: &gpui_kit::App| {
        let group = board
            .list
            .groups()
            .iter()
            .find(|group| group.key == borealis);
        group.map_or_else(Vec::new, |group| {
            board
                .group_indicators(group, cx)
                .into_iter()
                .map(|mark| mark.label.to_string())
                .collect::<Vec<_>>()
        })
    };
    assert!(cx.update(|_, cx| labels(board.read(cx), cx)).is_empty());
    let line = mention(message(
        &channel("#borealis-vega"),
        "mira",
        "vega, a word?",
        9,
    ));
    inject(&hub, vec![Event::Line(line)], cx);
    board.update(cx, crate::view::sync);
    assert_eq!(
        cx.update(|_, cx| labels(board.read(cx), cx)),
        ["You were mentioned in this group"]
    );
}

#[gpui_kit::test]
fn a_folded_fleet_that_takes_in_the_network_s_rows_opens_as_the_network_s_group_was(
    cx: &mut TestAppContext,
) {
    let (board, hub, cx) = two_fleets(cx, true);
    let group = rows::fleet_group_key(&network_id(), "borealis");
    board.read_with(cx, |board, _| assert!(board.list.is_folded(&group)));
    // Aurora's initiative goes: Borealis is now the network's only fleet,
    // and its group takes in the network's own rows, which showed.
    hub.update(cx, |hub, cx| {
        hub.initiatives.catalog.clone().update(cx, |catalog, cx| {
            catalog.set_memberships_for_test(Some("fixture-user".into()), Ok(vec![borealis()]), cx);
        })
    });
    settle(cx);
    board.read_with(cx, |board, _| {
        let held = board.list.groups().iter().find(|held| held.key == group);
        assert!(
            held.and_then(rows::absorbed_network).is_some(),
            "the group took in the network's rows"
        );
        assert!(
            !board.list.is_folded(&group),
            "the network's rows that showed still show"
        );
    });
}
