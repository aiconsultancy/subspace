//! The navigator grouped by fleet, as drawn: a fleet's group with its
//! channels named without the fleet's prefix, a ship led by its captain's
//! icon as the Icons setting asks and ending in its one working dot, `People` for private conversations with people, and a fleet
//! that is not the selected initiative's starting folded.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use serde_json::json;
use subspace_api_client::model::MemberInitiative;
use subspace_client_irc::{ConnectionStatus, Event};
use subspace_framework::dashboard::ListDashboard as _;
use subspace_framework::dashboard::avatar::AgentIcons;

use super::super::remove::settle;
use super::super::{inject, mount, settings};
use super::fleet::{aurora, ship};
use crate::fixtures::{channel, message, network_id, query};
use crate::hub::Hub;
use crate::{Irc, rows};

/// The group names and their rows' titles, as the list holds them.
fn outline(board: &Entity<Irc>, cx: &mut VisualTestContext) -> Vec<(String, Vec<String>)> {
    board.read_with(cx, |board, _| {
        board
            .list
            .groups()
            .iter()
            .map(|group| {
                let rows = group.rows.iter().map(|row| row.title.to_string()).collect();
                (group.name.to_string(), rows)
            })
            .collect()
    })
}

/// `line`, marked by the runtime as mentioning us.
fn mention(mut line: subspace_client_irc::Line) -> subspace_client_irc::Line {
    line.highlight = true;
    line
}

/// The ship channel open, the Starbase joined with a line that mentions
/// us, and private conversations with mira and with the captain.
fn populated(cx: &mut TestAppContext) -> (Entity<Irc>, &mut VisualTestContext) {
    let (board, hub, cx) = ship(cx);
    let starbase = channel(STARBASE);
    let line = mention(message(&starbase, "mira", "subspace: the review moved", 4));
    inject(
        &hub,
        vec![
            Event::Joined { buffer: starbase },
            Event::Joined {
                buffer: query("mira"),
            },
            Event::Joined {
                buffer: query("aurora-lyra"),
            },
            Event::Line(line),
        ],
        cx,
    );
    settle(cx);
    (board, cx)
}

const STARBASE: &str = "#aurora-starbase";

#[gpui_kit::test]
fn the_navigator_draws_a_fleet_s_channels_and_people_apart(cx: &mut TestAppContext) {
    let (board, cx) = populated(cx);
    let titles = |rows: &[&str]| rows.iter().map(|row| (*row).to_owned()).collect::<Vec<_>>();
    assert_eq!(
        outline(&board, cx),
        [
            ("Aurora".to_owned(), titles(&["starbase", "lyra"])),
            ("People".to_owned(), titles(&["mira"])),
            ("Local Ergo".to_owned(), titles(&["Server", "aurora-lyra"])),
        ]
    );
    let lyra = rows::row_key(&channel("#aurora-lyra"));
    let starbase = rows::row_key(&channel(STARBASE));
    cx.update(|window, _| {
        assert_eq!(
            window.find(format!("irc-row-avatar:{lyra}")).label(),
            Some("aurora-lyra, captain"),
            "by default a ship leads with its captain's small icon"
        );
        assert!(window.find(format!("irc-row:{starbase}")).visible());
        assert!(
            window
                .try_find(format!("irc-row-avatar:{starbase}"))
                .is_none(),
            "the Starbase leads with the #"
        );
        assert!(window.try_find(format!("irc-row-dot:{starbase}")).is_none());
    });
    cx.update(|_, cx| AgentIcons::Off.apply(cx));
    settle(cx);
    cx.update(|window, _| {
        assert!(
            window.try_find(format!("irc-row-avatar:{lyra}")).is_none(),
            "with Icons off a ship leads with the # too"
        );
        assert!(window.find(format!("irc-row:{lyra}")).visible());
    });
    board.read_with(cx, |board, cx| {
        let mira = rows::row_key(&query("mira"));
        let row = board.list.rows().find(|row| row.key == mira).cloned();
        let label = row.map(|row| board.row_accessible_label(&row, cx).to_string());
        assert_eq!(
            label.as_deref(),
            Some("mira, here"),
            "mira shares #aurora-lyra"
        );
    });
}

#[gpui_kit::test]
fn an_unread_row_is_emphasised_with_its_mention_mark_and_count(cx: &mut TestAppContext) {
    let (board, cx) = populated(cx);
    let starbase = rows::row_key(&channel(STARBASE));
    board.read_with(cx, |board, cx| {
        let row = board
            .list
            .rows()
            .find(|row| row.key == starbase)
            .cloned()
            .expect("the Starbase row");
        assert!(board.row_emphasis(&row, cx), "unread rows are emphasised");
        let marks: Vec<&str> = row
            .trailing
            .iter()
            .map(|glyph| glyph.text.as_ref())
            .collect();
        assert_eq!(marks, ["@", "1"], "a mention's mark, then the count");
        assert_eq!(
            board.row_accessible_label(&row, cx).as_ref(),
            "starbase, 1 unread, 1 mentioning you"
        );
    });
}

/// The ship row's dot's accessible name.
fn dot(lyra: &str, cx: &mut VisualTestContext) -> Option<String> {
    cx.update(|window, _| {
        window
            .find(format!("irc-row-dot:{lyra}"))
            .label()
            .map(str::to_owned)
    })
}

#[gpui_kit::test]
fn a_ship_s_one_dot_says_working_while_its_captain_has_a_status(cx: &mut TestAppContext) {
    let (board, hub, cx) = ship(cx);
    let lyra = rows::row_key(&channel("#aurora-lyra"));
    assert_eq!(dot(&lyra, cx).as_deref(), Some("aurora-lyra: idle"));
    inject(
        &hub,
        vec![Event::NickStatus {
            network: network_id(),
            nick: "aurora-lyra".into(),
            status: Some("Reviewing NAV-7".into()),
        }],
        cx,
    );
    settle(cx);
    assert_eq!(dot(&lyra, cx).as_deref(), Some("aurora-lyra: working"));
    board.read_with(cx, |board, cx| {
        let row = board.list.rows().find(|row| row.key == lyra).cloned();
        let row = row.expect("the ship row");
        assert!(row.trailing.is_empty(), "no working text");
        assert_eq!(
            board.row_accessible_label(&row, cx).as_ref(),
            "lyra, aurora-lyra: working"
        );
    });
}

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
