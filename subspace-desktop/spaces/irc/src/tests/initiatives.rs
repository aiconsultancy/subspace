use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{TestAppContext, VisualTestContext, px, size};
use serde_json::json;
use subspace_api_client::Error;
use subspace_api_client::model::MemberInitiative;
use subspace_client_irc::{Command, ConnectionStatus, Event};
use subspace_framework::dashboard::Dashboard as _;

use super::{inject, mount, settings};
use crate::fixtures::{channel, network_id};
use crate::view;

fn catalogue() -> Vec<MemberInitiative> {
    serde_json::from_value(json!([
        {"id":"dc94172e-aa0e-424a-8acd-8b7544b4ca0c","name":"Design Studio","channels":"#design, #releases"},
        {"id":"99469809-1f67-4f56-9d66-47a052e0e92b","name":"Observatory","channels":"#data, #forecast"}
    ])).unwrap()
}

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..3 {
        cx.run_until_parked();
        cx.update(|window, cx| window.render_frame(cx));
    }
}

#[gpui_kit::test]
fn local_network_actions_live_in_the_channels_header(cx: &mut TestAppContext) {
    let mut config = settings();
    config.networks[0].host = "irc.example.test".into();
    let (board, hub, cx) = mount(cx, config);
    hub.update(cx, |hub, cx| {
        hub.initiatives.catalog.clone().update(cx, |catalog, cx| {
            catalog.set_memberships_for_test(Some("fixture-user".into()), Ok(catalogue()), cx)
        })
    });
    settle(cx);
    cx.update(|window, cx| {
        assert!(
            board
                .update(cx, |board, cx| board.render_toolbar(window, cx))
                .is_none()
        );
        assert!(window.try_find("irc-initiative-button").is_none());
        let heading = window.find("irc-heading").bounds();
        let controls = window.find("irc-controls").bounds();
        let dock = window.find("dock-area").bounds();
        assert!(heading.bottom() <= controls.top());
        assert!(controls.bottom() <= dock.top());
        let row = window.within("irc-controls");
        for id in [
            "irc-connection",
            "irc-join",
            "irc-members-toggle",
            "irc-more",
        ] {
            assert!(row.find(id).visible(), "missing {id}");
        }
        assert_eq!(
            row.find("irc-more").bounds().right(),
            controls.right() - px(24.)
        );
        window.click("irc-connection", cx);
    });
    hub.read_with(cx, |hub, _| {
        assert!(hub.sent().contains(&Command::Connect(network_id())));
    });
}

#[gpui_kit::test]
fn narrow_header_keeps_picker_and_action_overflow_on_the_same_row(cx: &mut TestAppContext) {
    let (board, hub, cx) = mount(cx, settings());
    hub.update(cx, |hub, cx| {
        hub.initiatives.catalog.clone().update(cx, |catalog, cx| {
            catalog.set_memberships_for_test(Some("fixture-user".into()), Ok(catalogue()), cx)
        })
    });
    inject(
        &hub,
        vec![Event::Status {
            network: network_id(),
            status: ConnectionStatus::Connected {
                nick: "tester".into(),
            },
        }],
        cx,
    );
    cx.simulate_resize(size(px(250.), px(520.)));
    settle(cx);
    cx.update(|window, cx| {
        let row = window.within("irc-controls");
        let picker = row.find("irc-initiative-button").bounds();
        let more = row.find("overflow-toolbar-button").bounds();
        assert!(picker.right() < more.left());
        assert_eq!(picker.center().y, more.center().y);
        assert!(row.try_find("irc-join").is_none());
        assert!(row.try_find("irc-more").is_none());
        window.click("overflow-toolbar-button", cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        // At this width all four units use More. Join is the second native item.
        window.click(1usize, cx);
    });
    board.read_with(cx, |board, _| assert!(board.join.is_some()));
}

#[gpui_kit::test]
fn searchable_picker_switches_channels_and_survives_a_transient_api_error(cx: &mut TestAppContext) {
    let (board, hub, cx) = mount(cx, settings());
    cx.update(|_, cx| subspace_framework::dashboard::bind_keys(cx));
    cx.simulate_resize(size(px(720.), px(520.)));
    hub.update(cx, |hub, cx| {
        hub.initiatives.catalog.clone().update(cx, |catalog, cx| {
            catalog.set_memberships_for_test(Some("fixture-user".into()), Ok(catalogue()), cx)
        })
    });
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.find("irc-heading").visible());
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
