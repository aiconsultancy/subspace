//! Fleet-backed networks group listed channels as soon as both sources arrive.

use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use serde_json::json;
use subspace_api_client::model::MemberInitiative;
use subspace_client_irc::{Command, Event, ListedChannel, Target};
use subspace_framework::dashboard::{ListDashboard as _, list::GroupCaption};

use super::super::fleet::aurora;
use crate::fixtures::{channel, network_id};
use crate::hub::Hub;
use crate::tests::{inject, mount, settings};
use crate::{Irc, rows};

#[gpui_kit::test]
fn listed_channels_regroup_when_fleet_details_arrive(cx: &mut TestAppContext) {
    let (board, hub, cx) = mount(cx, settings());
    list_channels(&hub, cx);
    assert_titles(
        &board,
        [
            "Server",
            "#aurora-lyra",
            "#aurora-starbase",
            "#aurora-vega",
            "#lobby",
        ],
        cx,
    );

    publish_fleet(&hub, with_two_ships(), cx);
    assert_grouped(&board, &hub, cx);

    let mut changed = with_two_ships();
    changed[0].fleet.as_mut().unwrap().ships[1].channel = Some("#lobby".into());
    publish_fleet(&hub, changed, cx);
    assert_titles(
        &board,
        ["starbase", "lobby", "lyra", "#aurora-vega", "Server"],
        cx,
    );
}

#[gpui_kit::test]
fn the_first_channel_list_uses_already_loaded_fleet_details(cx: &mut TestAppContext) {
    let (board, hub, cx) = mount(cx, settings());
    publish_fleet(&hub, with_two_ships(), cx);
    assert_titles(&board, ["Server"], cx);
    list_channels(&hub, cx);
    assert_grouped(&board, &hub, cx);
}

fn with_two_ships() -> Vec<MemberInitiative> {
    let mut memberships = aurora();
    let fleet = memberships[0].fleet.as_mut().unwrap();
    fleet.agents.push(
        serde_json::from_value(json!(
            {"rank": "captain", "ircNick": "captain-vega", "shipId": "vega", "available": true}
        ))
        .unwrap(),
    );
    fleet.ships.push(
        serde_json::from_value(json!(
            {"id": "vega", "channel": "#aurora-vega"}
        ))
        .unwrap(),
    );
    memberships
}

fn publish_fleet(
    hub: &Entity<Hub>,
    memberships: Vec<MemberInitiative>,
    cx: &mut VisualTestContext,
) {
    let catalog = hub.read_with(cx, |hub, _| hub.initiatives.catalog.clone());
    catalog.update(cx, |catalog, cx| {
        catalog.set_memberships_for_test(Some("fixture-user".into()), Ok(memberships), cx);
    });
    super::settle(cx);
}

fn list_channels(hub: &Entity<Hub>, cx: &mut VisualTestContext) {
    let channels =
        ["#aurora-vega", "#lobby", "#aurora-starbase", "#aurora-lyra"].map(|name| ListedChannel {
            name: name.into(),
            users: 7,
            topic: String::new(),
        });
    inject(
        hub,
        vec![Event::ChannelList {
            network: network_id(),
            channels: channels.to_vec(),
        }],
        cx,
    );
}

fn assert_titles<const N: usize>(
    board: &Entity<Irc>,
    expected: [&str; N],
    cx: &mut VisualTestContext,
) {
    board.read_with(cx, |board, _| {
        let titles: Vec<&str> = board.list().rows().map(|row| row.title.as_ref()).collect();
        assert_eq!(titles, expected);
        assert!(board.selected_listed().is_none());
        assert!(
            board
                .selected_buffer()
                .is_none_or(|buffer| buffer.target == Target::Server)
        );
    });
}

fn assert_grouped(board: &Entity<Irc>, hub: &Entity<Hub>, cx: &mut VisualTestContext) {
    assert_titles(board, ["starbase", "lyra", "vega", "#lobby", "Server"], cx);
    let listed_key = |name: &str| rows::listed_key(&channel(name));
    board.read_with(cx, |board, cx| {
        let group = &board.list().groups()[0];
        assert_eq!(
            board.group_captions(group, cx),
            [GroupCaption::new(listed_key("#aurora-lyra"), "Ships")]
        );
        let vega = &group.rows[2];
        assert_eq!(vega.key, listed_key("#aurora-vega"));
        assert!(vega.done, "the ship is still unjoined");
        assert_eq!(
            board.row_accessible_label(vega, cx).as_ref(),
            "vega, not joined, 7 users"
        );
        let found = board.search.search("#aurora-vega", 5);
        assert_eq!(
            found[0].1.key, vega.key,
            "search keeps the full channel name"
        );
    });
    hub.read_with(cx, |hub, _| {
        assert!(
            !hub.sent()
                .iter()
                .any(|command| matches!(command, Command::Join { .. }))
        );
    });
}
