use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{TestAppContext, px, size};
use subspace_client_irc::{Command, ConnectionStatus, Event};
use subspace_framework::dashboard::Dashboard as _;

use super::super::{inject, mount, settings};
use super::{catalogue, settle};
use crate::fixtures::network_id;

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
