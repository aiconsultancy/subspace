//! Render tests: the real board over a hub that records commands instead
//! of connecting. Events are injected straight into the hub's state.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, Entity, TestAppContext, VisualTestContext};
use subspace_client_irc::{Command, ConnectionStatus, Event};
use subspace_framework::dashboard::{ChatInputEvent, Dashboard as _, ListDashboard as _};

use crate::fixtures::{at, channel, config, member, message, network_id};
use crate::hub::{self, Hub};
use crate::{Irc, config::Settings, rows, view};

mod back;
mod completion;
mod conversation;
mod conversation_menu;
mod counter;
mod decision_references;
mod focus;
mod formatting;
mod hidden_navigator;
mod initiatives;
mod issues;
mod items;
mod keys;
mod listing;
mod notifications;
mod pull_requests;
mod reactions;
mod remove;
mod replies;
mod selection;
mod settings;
mod statuses;
mod stored;
mod thread_status;
mod uploads;

pub(crate) fn settings() -> Settings {
    Settings {
        networks: vec![config()],
        highlights: vec!["rocket".into()],
        ..Settings::default()
    }
}

pub(crate) fn mount(
    cx: &mut TestAppContext,
    settings: Settings,
) -> (Entity<Irc>, Entity<Hub>, &mut VisualTestContext) {
    mount_hub(cx, move |cx| hub::install_for_test(&settings, cx))
}

/// The real board over the hub `install` creates, after the shell's setup.
pub(crate) fn mount_hub(
    cx: &mut TestAppContext,
    install: impl FnOnce(&mut gpui_kit::App) -> Entity<Hub>,
) -> (Entity<Irc>, Entity<Hub>, &mut VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| subspace_framework::dashboard::prefs::init_file(None, cx));
    let hub = cx.update(install);
    let (window, board) = cx.update(|cx| {
        gpui_kit::open_window(gpui_kit::WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| Irc::new(window, cx))
        })
        .unwrap()
    });
    let cx = VisualTestContext::from_window(window, cx).into_mut();
    cx.update(|window, cx| {
        window.activate_window();
        board.update(cx, |board, cx| board.activate(window, cx));
        window.render_frame(cx);
    });
    cx.run_until_parked();
    (board, hub, cx)
}

/// The board showing `#subspace` with bob's message `text` last. `install`
/// sets up the hub and whatever else a test routes, returned beside it.
pub(crate) fn open_with_message<'a, T>(
    cx: &'a mut TestAppContext,
    install: impl FnOnce(&mut gpui_kit::App) -> (Entity<Hub>, T),
    text: &str,
) -> (Entity<Irc>, Entity<Hub>, T, &'a mut VisualTestContext) {
    let mut installed = None;
    let (board, hub, cx) = mount_hub(cx, |cx| {
        let (hub, extra) = install(cx);
        installed = Some(extra);
        hub
    });
    let room = channel("#subspace");
    let mut events = channel_events();
    events.push(Event::Line(message(&room, "bob", text, 3)));
    inject(&hub, events, cx);
    board.update(cx, |board, cx| view::reveal(board, &room, cx));
    cx.run_until_parked();
    cx.update(|window, cx| window.render_frame(cx));
    cx.run_until_parked();
    let installed = installed.expect("the hub was installed");
    (board, hub, installed, cx)
}

/// The visible text drawn inside the tooltip `id`, joined with ` | `.
pub(crate) fn tooltip_text(id: &'static str, cx: &mut VisualTestContext) -> String {
    cx.update(|window, _| {
        assert!(window.find(id).visible());
        gpui_kit::base::test_support::texts(window)
            .into_iter()
            .filter(|text| text.path().contains(&id.into()) && text.visible())
            .map(|text| text.text().to_owned())
            .collect::<Vec<_>>()
            .join(" | ")
    })
}

pub(crate) fn inject(hub: &Entity<Hub>, events: Vec<Event>, cx: &mut VisualTestContext) {
    hub.update(cx, |hub, cx| hub.ingest(events, at(30), cx));
    cx.run_until_parked();
    cx.update(|window, cx| window.render_frame(cx));
}

pub(crate) fn channel_events() -> Vec<Event> {
    let room = channel("#subspace");
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
        Event::Topic {
            buffer: room.clone(),
            topic: "Fixture channel · https://example.org".into(),
            set_by: None,
        },
        Event::Members {
            buffer: room.clone(),
            members: vec![member("alice", "@"), member("bob", "")],
        },
        Event::Line(message(&room, "alice", "hello there", 1)),
        Event::Line(message(&room, "bob", "rocket ready?", 2)),
    ]
}

#[gpui_kit::test]
fn rows_show_networks_buffers_and_unread_state(cx: &mut TestAppContext) {
    let (board, hub, cx) = mount(cx, settings());
    inject(&hub, channel_events(), cx);
    let key = rows::row_key(&channel("#subspace"));
    cx.update(|_, cx| {
        let board = board.read(cx);
        let group = &board.list().groups()[0];
        assert_eq!(group.name.as_ref(), "Acme IRC");
        assert_eq!(group.count.as_deref(), Some("Connected"));
        let titles: Vec<&str> = group.rows.iter().map(|row| row.title.as_ref()).collect();
        assert_eq!(titles, ["Server", "#subspace"]);
        let row = &group.rows[1];
        assert_eq!(row.key, key);
        assert_eq!(
            board.row_accessible_label(row, cx).as_ref(),
            "#subspace, 2 unread, 1 mentioning you"
        );
        assert!(board.row_emphasis(row, cx), "a mention makes the row bold");
    });
}

#[gpui_kit::test]
fn opening_a_channel_shows_its_lines_members_and_reads_it(cx: &mut TestAppContext) {
    let (board, hub, cx) = mount(cx, settings());
    inject(&hub, channel_events(), cx);
    let room = channel("#subspace");
    board.update(cx, |board, cx| view::reveal(board, &room, cx));
    cx.run_until_parked();
    cx.update(|window, cx| window.render_frame(cx));
    cx.update(|window, _| {
        assert!(
            window.try_find("irc-member:alice").is_some(),
            "member column"
        );
        assert!(
            window
                .find_all("conversation-tab-title")
                .iter()
                .any(|title| {
                    title.label() == Some("#subspace")
                        && title.description().is_some_and(|description| {
                            description
                                .ends_with("Acme IRC · Fixture channel · https://example.org")
                        })
                })
        );
        assert_eq!(
            window.find("irc-line:0:row").label(),
            Some("alice: hello there"),
            "first message"
        );
        let unread = window.find("irc-unread:0").label().map(str::to_owned);
        assert_eq!(unread.as_deref(), Some("2 new messages since you left"));
        assert!(
            window.try_find("irc-history").is_some(),
            "older history control"
        );
    });
    hub.read_with(cx, |hub, _| {
        let buffer = hub.state.buffer(&room).unwrap();
        assert_eq!((buffer.unread, buffer.highlights), (0, 0));
        assert!(hub.sent().iter().any(|command| matches!(command, Command::MarkRead { buffer, .. } if *buffer == room)));
        assert!(hub.sent().iter().any(|command| matches!(command, Command::LoadHistory { buffer, limit: 100, .. } if *buffer == room)));
    });
}

#[gpui_kit::test]
fn the_composer_sends_input_and_choosing_a_member_opens_a_query(cx: &mut TestAppContext) {
    let (board, hub, cx) = mount(cx, settings());
    inject(&hub, channel_events(), cx);
    let room = channel("#subspace");
    board.update(cx, |board, cx| view::reveal(board, &room, cx));
    cx.update(|window, cx| window.render_frame(cx));
    let input = board.read_with(cx, |board, cx| {
        board.dock.open[&room].entity.read(cx).composer().clone()
    });
    input.update(cx, |_, cx| {
        cx.emit(ChatInputEvent::Submit("/me waves".into()))
    });
    cx.run_until_parked();
    hub.read_with(cx, |hub, _| {
        assert!(hub.sent().contains(&Command::Input {
            buffer: room.clone(),
            text: "/me waves".into()
        }));
    });
    cx.update(|window, cx| window.click("irc-member:bob", cx));
    cx.run_until_parked();
    hub.read_with(cx, |hub, _| {
        assert!(hub.sent().contains(&Command::Query {
            network: network_id(),
            nick: "bob".into()
        }));
    });
    board.read_with(cx, |board, _| {
        assert_eq!(
            board.dock.focused.as_ref(),
            Some(&crate::fixtures::query("bob"))
        );
    });
}

#[gpui_kit::test]
fn mentions_elsewhere_notify_and_the_notification_opens_the_buffer(cx: &mut TestAppContext) {
    struct Delivered(Vec<u64>);
    impl gpui_kit::Global for Delivered {}
    let (board, hub, cx) = mount(cx, settings());
    cx.update(|_, cx| {
        subspace_framework::dashboard::notifications::Preferences {
            mode: subspace_framework::dashboard::notifications::Mode::InApp,
            ..Default::default()
        }
        .save(crate::ID, cx);
        cx.set_global(Delivered(Vec::new()));
        cx.set_global(subspace_framework::dashboard::notifications::Sink(
            |_, target, _, cx| cx.global_mut::<Delivered>().0.push(target),
        ));
    });
    let private = crate::fixtures::query("carol");
    inject(
        &hub,
        vec![Event::Line(message(&private, "carol", "are you there?", 3))],
        cx,
    );
    let target = cx.update(|_, cx| cx.global::<Delivered>().0.clone());
    assert_eq!(target.len(), 1);
    cx.update(|window, cx| {
        board.update(cx, |board, cx| {
            board.open_notification(target[0], window, cx)
        });
    });
    board.read_with(cx, |board, _| {
        assert_eq!(board.dock.focused.as_ref(), Some(&private))
    });
    inject(
        &hub,
        vec![Event::Line(message(&private, "carol", "hello?", 4))],
        cx,
    );
    let delivered = cx.update(|_, cx| cx.global::<Delivered>().0.len());
    assert_eq!(
        delivered, 1,
        "the open, visible buffer raises no notification"
    );
}

#[gpui_kit::test]
fn a_query_opened_with_a_command_is_shown(cx: &mut TestAppContext) {
    let (board, hub, cx) = mount(cx, settings());
    let dave = crate::fixtures::query("dave");
    inject(
        &hub,
        vec![Event::Joined {
            buffer: dave.clone(),
        }],
        cx,
    );
    board.read_with(cx, |board, _| {
        assert_eq!(board.dock.focused.as_ref(), Some(&dave))
    });
}

mod find;
