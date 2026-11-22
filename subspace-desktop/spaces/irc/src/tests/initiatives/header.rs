use gpui_kit::TestAppContext;
use gpui_kit::test::TestWindowExt as _;
use subspace_client_irc::{Command, ConnectionStatus, Event};
use subspace_framework::dashboard::Dashboard as _;

use super::super::{inject, mount, settings};
use super::{member_of_the_catalogue, settle};
use crate::fixtures::network_id;

#[gpui_kit::test]
fn local_network_actions_live_in_the_pane_toolbar(cx: &mut TestAppContext) {
    let mut config = settings();
    config.networks[0].host = "irc.example.test".into();
    let (board, hub, cx) = mount(cx, config);
    member_of_the_catalogue(&hub, cx);
    settle(cx);
    cx.update(|window, cx| {
        assert!(
            board
                .update(cx, |board, cx| board.render_toolbar(window, cx))
                .is_none()
        );
        assert!(window.try_find("irc-initiative-button").is_none());
        assert!(window.try_find("irc-heading").is_none());
        let toolbar = window.find("irc-toolbar").bounds();
        let dock = window.find("dock-area").bounds();
        assert!(toolbar.bottom() <= dock.top());
        let row = window.within("irc-toolbar");
        for id in ["irc-find", "irc-more"] {
            assert!(row.find(id).visible(), "missing {id}");
        }
        for id in ["irc-connection", "irc-join"] {
            assert!(row.try_find(id).is_none(), "{id} is in the More menu");
        }
        assert!(
            row.try_find("irc-members-toggle").is_none(),
            "the status bar's inspector switch shows and hides the members"
        );
        assert!(
            row.try_find("workspace-inspector-toggle").is_none(),
            "the dock switches are the status bar's"
        );
        assert!(
            window.find("irc-navigator-join").visible(),
            "the navigator's header repeats Join"
        );
        window.click("irc-more", cx);
    });
    crate::tests::remove::settle(cx);
    crate::tests::remove::choose("Connect", cx);
    hub.read_with(cx, |hub, _| {
        assert!(hub.sent().contains(&Command::Connect(network_id())));
    });
}

#[gpui_kit::test]
fn the_toolbar_leads_with_the_picker_and_trails_with_the_actions(cx: &mut TestAppContext) {
    let (board, hub, cx) = mount(cx, settings());
    member_of_the_catalogue(&hub, cx);
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
    settle(cx);
    cx.update(|window, cx| {
        let row = window.within("irc-toolbar");
        let picker = row.find("irc-initiative-button").bounds();
        let heading = row.find("irc-header").bounds();
        let more = row.find("irc-more").bounds();
        assert!(
            picker.right() <= heading.left() && heading.right() <= more.left(),
            "the picker {picker:?}, the heading {heading:?}, then More {more:?}"
        );
        window.click("irc-more", cx);
    });
    crate::tests::remove::settle(cx);
    crate::tests::remove::choose("Join…", cx);
    board.read_with(cx, |board, _| assert!(board.join.is_some()));
}
