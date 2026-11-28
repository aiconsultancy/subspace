use gpui_kit::TestAppContext;
use gpui_kit::test::TestWindowExt as _;
use subspace_client_irc::{Command, ConnectionStatus, Event};

use super::{inject, mount, settings};
use crate::fixtures::{channel, message};

#[gpui_kit::test]
fn selecting_comms_opens_a_thread_without_reading_the_channel(cx: &mut TestAppContext) {
    let (board, hub, cx) = mount(cx, settings());
    let room = channel("#example-ship");
    let mut root = message(&room, "subspace", "Check the build", 1);
    root.id = Some("request".into());
    root.own = true;
    let mut first = message(&room, "captain", "@subspace The build passed", 2);
    first.id = Some("first".into());
    first.reply_to = Some("request".into());
    first.highlight = true;
    let mut second = message(&room, "crew", "The report is on the PR", 3);
    second.id = Some("second".into());
    second.reply_to = Some("first".into());
    inject(
        &hub,
        vec![
            Event::Status {
                network: room.network.clone(),
                status: ConnectionStatus::Connected {
                    nick: "subspace".into(),
                },
            },
            Event::Joined {
                buffer: room.clone(),
            },
            Event::Line(root),
            Event::Line(first),
            Event::Line(second),
        ],
        cx,
    );
    cx.run_until_parked();
    hub.read_with(cx, |hub, _| {
        assert_eq!(hub.comms.unread(), (2, true));
        assert_eq!(hub.comms.summaries().len(), 1);
        assert_eq!(hub.state.buffer(&room).unwrap().unread, 2);
    });
    cx.update(|window, cx| window.click("irc-comms-nav", cx));
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.find("irc-comms-pane").visible());
        window.click(format!("irc-comms:{}:request", room.network.0), cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.find("irc-comms-pane").visible());
        assert!(window.find("irc-thread").visible());
    });
    hub.read_with(cx, |hub, _| {
        assert_eq!(hub.comms.unread(), (0, false));
        assert_eq!(hub.state.buffer(&room).unwrap().unread, 2);
    });

    let destination = channel("#example-command");
    board.update_in(cx, |board, window, cx| {
        crate::reveal::open_thread(
            board,
            (&destination.network, "#example-command"),
            "decision",
            window,
            cx,
        );
    });
    hub.read_with(cx, |hub, _| {
        assert!(hub.sent().iter().any(|command| matches!(
            command, Command::LoadAround { buffer, id: Some(id), .. }
                if buffer == &destination && id == "decision"
        )));
    });
    let mut linked = message(&destination, "admiral", "Decision recorded", 4);
    linked.id = Some("decision".into());
    inject(
        &hub,
        vec![Event::Backfill {
            buffer: destination.clone(),
            lines: vec![linked],
        }],
        cx,
    );
    board.read_with(cx, |board, _| {
        assert!(board.comms.active);
        assert_eq!(board.threads.chosen(&destination), Some("decision"));
    });
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.find("irc-thread").visible());
    });
}

#[gpui_kit::test]
fn removing_the_network_clears_comms_without_waiting_for_irc(cx: &mut TestAppContext) {
    let (_, hub, cx) = mount(cx, settings());
    let room = channel("#example-ship");
    let mut mention = message(&room, "captain", "@subspace See this", 1);
    mention.id = Some("mention".into());
    mention.highlight = true;
    inject(&hub, vec![Event::Line(mention)], cx);
    hub.read_with(cx, |hub, _| assert_eq!(hub.comms.unread(), (1, true)));

    hub.update(cx, |hub, cx| hub.follow(Default::default(), cx));
    hub.read_with(cx, |hub, _| {
        assert!(hub.comms.summaries().is_empty());
        assert_eq!(hub.comms.unread(), (0, false));
    });
}
