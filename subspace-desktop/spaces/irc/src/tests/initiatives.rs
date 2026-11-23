mod ask;
mod direct;
mod fleet;
mod folding;
mod header;
mod live;
mod navigator;
mod notice;
mod scope;
mod thread;
mod thread_gone;
mod views;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Entity, TestAppContext, VisualTestContext, px, size};
use serde_json::json;
use subspace_api_client::Error;
use subspace_api_client::model::MemberInitiative;
use subspace_client_irc::{Command, Event};

use super::{inject, mount, settings};
use crate::fixtures::{channel, network_id};
use crate::hub::Hub;
use crate::view;

fn catalogue() -> Vec<MemberInitiative> {
    serde_json::from_value(json!([
        {"id":"dc94172e-aa0e-424a-8acd-8b7544b4ca0c","name":"Design Studio","channels":"#design, #releases"},
        {"id":"99469809-1f67-4f56-9d66-47a052e0e92b","name":"Observatory","channels":"#data, #forecast"}
    ])).unwrap()
}

/// The fixture user's memberships: the two initiatives in `catalogue`.
fn member_of_the_catalogue(hub: &Entity<Hub>, cx: &mut VisualTestContext) {
    hub.update(cx, |hub, cx| {
        hub.initiatives.catalog.clone().update(cx, |catalog, cx| {
            catalog.set_memberships_for_test(Some("fixture-user".into()), Ok(catalogue()), cx)
        })
    });
}

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..3 {
        cx.run_until_parked();
        cx.update(|window, cx| window.render_frame(cx));
    }
}

#[gpui_kit::test]
fn searchable_picker_switches_channels_and_survives_a_transient_api_error(cx: &mut TestAppContext) {
    let (board, hub, cx) = mount(cx, settings());
    cx.update(|_, cx| subspace_framework::dashboard::bind_keys(cx));
    // Wide enough for the pane between the navigator and the inspector.
    cx.simulate_resize(size(px(1280.), px(520.)));
    member_of_the_catalogue(&hub, cx);
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.find("workspace-navigator-title").visible());
        window.click("irc-initiative-button", cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.render_frame(cx));
    cx.run_until_parked();
    cx.simulate_keystrokes("o b s e r v a t o r y enter");
    cx.run_until_parked();
    let selected = Command::SetAutojoin {
        network: network_id(),
        channels: Some(vec!["#data".into(), "#forecast".into()]),
    };
    hub.read_with(cx, |hub, _| assert!(hub.sent().contains(&selected)));
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find("irc-initiative-button").label(),
            Some("Initiative: Observatory")
        );
    });
    hub.update(cx, |hub, cx| {
        hub.initiatives.catalog.clone().update(cx, |catalog, cx| {
            catalog.set_memberships_for_test(None, Err(Error::Network), cx)
        })
    });
    hub.read_with(cx, |hub, _| assert_eq!(hub.sent().last(), Some(&selected)));
    hub.update(cx, |hub, cx| {
        hub.initiatives.catalog.clone().update(cx, |catalog, cx| {
            catalog.set_memberships_for_test(Some("other-user".into()), Err(Error::Network), cx)
        })
    });
    hub.read_with(cx, |hub, _| {
        assert_eq!(
            hub.sent().last(),
            Some(&Command::SetAutojoin {
                network: network_id(),
                channels: None
            })
        );
    });
    board.read_with(cx, |board, _| {
        assert!(board.initiative_menu.host().is_none())
    });
}

#[gpui_kit::test]
fn retired_conversation_tabs_close_without_overwriting_local_layout(cx: &mut TestAppContext) {
    let (board, hub, cx) = mount(cx, settings());
    let room = channel("#subspace");
    let next = channel("#other");
    inject(
        &hub,
        vec![
            Event::Joined {
                buffer: room.clone(),
            },
            Event::Joined {
                buffer: next.clone(),
            },
        ],
        cx,
    );
    board.update(cx, |board, cx| view::reveal(board, &room, cx));
    settle(cx);
    let server = crate::state::server_key(&network_id());
    cx.update(|window, cx| {
        board.update(cx, |board, cx| {
            crate::dock::close(board, &server, window, cx)
        });
    });
    settle(cx);
    board.read_with(cx, |board, _| assert_eq!(board.dock.open.len(), 1));
    let local_layout = cx
        .update(|_, cx| subspace_framework::dashboard::prefs::get(cx, "irc.dock-layout").unwrap());
    inject(&hub, vec![Event::AutojoinRetired(room.clone())], cx);
    cx.run_until_parked();
    board.read_with(cx, |board, _| assert!(!board.dock.open.contains_key(&room)));
    hub.read_with(cx, |hub, _| assert!(hub.state.buffer(&room).is_none()));
    cx.update(|_, cx| {
        assert_eq!(
            subspace_framework::dashboard::prefs::get(cx, "irc.dock-layout"),
            Some(local_layout),
            "an automatic retirement leaves the saved Local tabs alone",
        );
    });
}

#[gpui_kit::test]
fn an_unresolved_fleet_keeps_autojoin_off_until_its_lookup_succeeds(cx: &mut TestAppContext) {
    const ID: &str = "99469809-1f67-4f56-9d66-47a052e0e92b";
    let initiative = |fleet: serde_json::Value| -> Vec<MemberInitiative> {
        serde_json::from_value(json!([
            {"id":ID,"name":"Observatory","channels":"#data","fleetId":"fleet-1","fleet":fleet}
        ]))
        .unwrap()
    };
    let (board, hub, cx) = mount(cx, settings());
    let catalog = hub.read_with(cx, |hub, _| hub.initiatives.catalog.clone());
    catalog.update(cx, |catalog, cx| {
        catalog.set_memberships_for_test(
            Some("fixture-user".into()),
            Ok(initiative(json!(null))),
            cx,
        );
        catalog.pick(ID, cx);
    });
    settle(cx);
    let joined = Command::SetAutojoin {
        network: network_id(),
        channels: Some(vec!["#data".into()]),
    };
    hub.read_with(cx, |hub, _| {
        assert!(
            !hub.sent().contains(&joined),
            "no fallback to the default server"
        );
        assert!(!hub.initiatives.selected_for_runtime());
        assert_eq!(
            hub.missing_initiative_network().as_deref(),
            Some("Fleet server unavailable. Retrying…")
        );
    });
    catalog.read_with(cx, |catalog, _| {
        assert!(catalog.fleet_retry_pending_for_test())
    });
    board.read_with(cx, |board, _| assert!(board.hub.is_some()));

    let fleet =
        json!({"id":"fleet-1","name":"Blue","ircHost":"127.0.0.1","ircPort":16667,"ircTls":false});
    catalog.update(cx, |catalog, cx| {
        catalog.set_memberships_for_test(Some("fixture-user".into()), Ok(initiative(fleet)), cx)
    });
    settle(cx);
    hub.read_with(cx, |hub, _| assert_eq!(hub.sent().last(), Some(&joined)));
    catalog.read_with(cx, |catalog, _| {
        assert!(!catalog.fleet_retry_pending_for_test())
    });
}
