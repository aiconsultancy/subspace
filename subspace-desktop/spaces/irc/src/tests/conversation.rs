//! The conversation's message rows and member list on the board, and a
//! conversation shown without the board.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AppContext as _, Bounds, Entity, Pixels, Role, TestAppContext, VisualTestContext, Window,
};
use subspace_client_irc::{Command, Event};
use subspace_framework::dashboard::workspace::{SpaceDock, SpaceDocks};
use subspace_framework::dashboard::{ChatInputEvent, Dashboard as _};

use super::{channel_events, inject, mount, settings};
use crate::fixtures::{at, channel, member, message};
use crate::hub::{self, Hub};
use crate::{Irc, view};

fn open_channel(
    cx: &mut TestAppContext,
    events: Vec<Event>,
) -> (Entity<Irc>, Entity<Hub>, &mut VisualTestContext) {
    let (board, hub, cx) = mount(cx, settings());
    inject(&hub, events, cx);
    let room = channel("#subspace");
    board.update(cx, |board, cx| view::reveal(board, &room, cx));
    cx.run_until_parked();
    cx.update(|window, cx| window.render_frame(cx));
    (board, hub, cx)
}

#[gpui_kit::test]
fn a_sender_s_following_messages_hide_their_avatar_and_name(cx: &mut TestAppContext) {
    let room = channel("#subspace");
    let mut events = channel_events();
    events.push(Event::Line(message(&room, "bob", "and one more thing", 3)));
    let (_, _, cx) = open_channel(cx, events);
    cx.update(|window, _| {
        assert!(
            window.try_find("irc-line:0:sender").is_some(),
            "alice starts a group"
        );
        assert!(
            window.try_find("irc-line:1:sender").is_some(),
            "bob starts a group"
        );
        assert!(
            window.try_find("irc-line:2:sender").is_none(),
            "bob's next message continues his group"
        );
        assert_eq!(
            window.find("irc-line:2:row").label(),
            Some("bob: and one more thing")
        );
        assert!(
            window.try_find("irc-day:2026-11-13").is_some(),
            "the day divider"
        );
    });
}

#[gpui_kit::test]
fn the_member_list_starts_inside_its_panel(cx: &mut TestAppContext) {
    let room = channel("#subspace");
    let mut events = channel_events();
    events.push(Event::Members {
        buffer: room.clone(),
        members: vec![
            member("galactic-captain-with-a-long-nick", "~"),
            member("alice", "@"),
            member("bob", ""),
        ],
    });
    let (_, _, cx) = open_channel(cx, events);
    cx.update(|window, _| {
        let panel = window.find("irc-members").bounds();
        for id in [
            "irc-member:galactic-captain-with-a-long-nick",
            "irc-member:alice",
            "irc-member:bob",
        ] {
            let row = window.find(id).bounds();
            assert!(
                row.left() >= panel.left() && row.right() <= panel.right(),
                "{id} {row:?} lies inside the member list {panel:?}"
            );
        }
    });
}

#[gpui_kit::test]
fn a_conversation_stands_alone_without_the_board(cx: &mut TestAppContext) {
    use crate::conversation::Conversation;

    cx.update(gpui_kit::init);
    cx.update(|cx| subspace_framework::dashboard::prefs::init_file(None, cx));
    let hub = cx.update(|cx| hub::install_for_test(&settings(), cx));
    hub.update(cx, |hub, cx| hub.ingest(channel_events(), at(30), cx));
    let room = channel("#subspace");
    let (window, conversation) = cx.update(|cx| {
        let room = room.clone();
        let hub = hub.clone();
        gpui_kit::open_window(gpui_kit::WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| Conversation::new(room, hub, window, cx))
        })
        .unwrap()
    });
    let cx = VisualTestContext::from_window(window, cx).into_mut();
    cx.update(|window, cx| window.render_frame(cx));
    cx.update(|window, _| {
        assert_eq!(
            window.find("irc-header").label(),
            Some("#subspace, Local Ergo, topic: Fixture channel · https://example.org")
        );
        assert_eq!(
            window.find("irc-line:1:row").label(),
            Some("bob: rocket ready?")
        );
        assert!(window.try_find("irc-members-count").is_none());
    });
    let input = conversation.read_with(cx, |conversation, _| conversation.composer().clone());
    input.update(cx, |_, cx| cx.emit(ChatInputEvent::Submit("hi all".into())));
    cx.run_until_parked();
    hub.read_with(cx, |hub, _| {
        assert!(hub.sent().contains(&Command::Input {
            buffer: room.clone(),
            text: "hi all".into()
        }));
    });
}

#[gpui_kit::test]
fn the_toolbar_sits_over_the_tabs_and_the_docks_run_beside_the_pane(cx: &mut TestAppContext) {
    let (board, _, cx) = open_channel(cx, channel_events());
    cx.update(|window, _| {
        let tab = window
            .within("dock-resource-tabs")
            .find_all_by_role(Role::Tab)
            .into_iter()
            .find(|tab| tab.label() == Some("#subspace"))
            .expect("the open channel has a tab");
        assert_workspace_alignment(window, tab.bounds());
    });
    cx.update(|window, cx| {
        board.update(cx, |board, cx| {
            let server = crate::state::server_key(&channel("#subspace").network);
            crate::dock::close(board, &server, window, cx);
        });
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        let title = window.find("conversation-tab-title");
        assert_eq!(title.label(), Some("#subspace"));
        assert_workspace_alignment(window, title.bounds());
    });
}

/// The workspace's zones around the conversation whose tab is at `title`.
fn assert_workspace_alignment(window: &mut Window, title: Bounds<Pixels>) {
    let toolbar = window.find("irc-toolbar").bounds();
    let transcript = window.find("irc-transcript-pane").bounds();
    let viewport = window.find("irc-transcript-viewport").bounds();
    let dock_top = window.find("dock-area").bounds().top();
    let navigator = window.find("workspace-navigator").bounds();
    let members = window.find("irc-inspector").bounds();
    let group = crate::rows::group_key(&channel("#subspace").network);
    let network_status = window
        .find(gpui_kit::SharedString::from(format!(
            "list-group-count:{group}"
        )))
        .bounds();
    assert!(
        toolbar.bottom() <= dock_top && title.top() >= dock_top,
        "the toolbar {toolbar:?} is over the tabs {title:?}"
    );
    assert!(
        transcript.top() >= title.bottom(),
        "the transcript {transcript:?} starts below the title {title:?}"
    );
    assert!(
        members.left() >= transcript.right(),
        "the member list is beside the conversation"
    );
    assert!(
        network_status.right() <= navigator.right() && navigator.right() <= transcript.left(),
        "the network row {network_status:?} is in the navigator {navigator:?}"
    );
    assert_eq!(
        (navigator.top(), members.top()),
        (toolbar.top(), toolbar.top()),
        "the docks and the pane start on one line"
    );
    assert!(
        viewport.top() > transcript.top(),
        "the scrollable messages start below the spacer"
    );
    assert_workspace_words(window);
}

/// What the navigator and the toolbar say, in place of the Space's heading.
fn assert_workspace_words(window: &mut Window) {
    assert_eq!(
        window.within("irc-toolbar").find("irc-header").label(),
        Some("#subspace, Local Ergo, topic: Fixture channel · https://example.org"),
        "the toolbar names the conversation in front, its network and topic"
    );
    assert_eq!(
        window.find("workspace-navigator-title").label(),
        Some("Channels")
    );
    assert!(window.try_find("irc-heading").is_none());
    assert!(window.try_find("irc-members-count").is_none());
}

#[gpui_kit::test]
fn the_inspector_switch_and_close_show_and_hide_the_members(cx: &mut TestAppContext) {
    let (board, _, cx) = open_channel(cx, channel_events());
    let shown =
        |cx: &mut VisualTestContext| board.read_with(cx, |board, _| board.dock.members_open());
    let listed = |cx: &mut VisualTestContext| {
        cx.update(|window, _| window.try_find("irc-member:alice").is_some())
    };
    assert!(shown(cx) && listed(cx));
    let docks = board.read_with(cx, |board, cx| board.docks(cx));
    assert_eq!(
        docks,
        SpaceDocks::both(true, true),
        "both status bar switches"
    );
    // The status bar's switch reaches the Space through its dock contract.
    let switch = |cx: &mut VisualTestContext| {
        board.update_in(cx, |board, window, cx| {
            board.toggle_dock(SpaceDock::Inspector, window, cx)
        });
        cx.run_until_parked();
        cx.update(|window, cx| window.render_frame(cx));
    };
    switch(cx);
    assert!(!shown(cx) && !listed(cx), "hidden");
    switch(cx);
    assert!(shown(cx) && listed(cx), "shown again");
    cx.update(|window, cx| window.click("workspace-inspector-close", cx));
    cx.run_until_parked();
    cx.update(|window, cx| window.render_frame(cx));
    assert!(!shown(cx) && !listed(cx), "closed from the inspector");
}

#[gpui_kit::test]
fn conversation_tabs_keep_short_names_and_describe_unread_and_topic(cx: &mut TestAppContext) {
    use gpui_kit::component::dock::Panel as _;
    let (board, hub, cx) = open_channel(cx, channel_events());
    let room = channel("#subspace");
    let other = channel("#ops");
    inject(
        &hub,
        vec![Event::Joined {
            buffer: other.clone(),
        }],
        cx,
    );
    board.update(cx, |board, cx| view::reveal(board, &other, cx));
    cx.run_until_parked();
    inject(
        &hub,
        vec![Event::Line(message(
            &room,
            "bob",
            "rocket needs attention",
            3,
        ))],
        cx,
    );
    board.read_with(cx, |board, cx| {
        let conversation = board.dock.open[&room].entity.read(cx);
        assert_eq!(conversation.tab_name(cx).as_deref(), Some("#subspace"));
    });
    cx.update(|window, cx| {
        window.render_frame(cx);
        let mut bar = window.within("dock-resource-tabs");
        let target = bar
            .find_all_by_role(gpui_kit::Role::Tab)
            .into_iter()
            .find(|tab| tab.label() == Some("#subspace"))
            .unwrap();
        let id = target.path().last().unwrap().clone();
        assert_eq!(bar.find(id.clone()).label(), Some("#subspace"));
        let title = bar.within(id.clone()).find("conversation-tab-title");
        assert_eq!(title.label(), Some("#subspace"));
        assert_eq!(
            title.description(),
            Some("#subspace, 1 unread item, mentions you, Local Ergo · Fixture channel · https://example.org")
        );
        assert_eq!(
            bar.within(id).find("close-tab").label(),
            Some("Close #subspace")
        );
    });
}
