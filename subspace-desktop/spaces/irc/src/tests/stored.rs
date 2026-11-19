//! The board over a hub that keeps messages in a real, temporary store:
//! restore before the runtime starts, catch-up anchors, writes and deletes,
//! and opening a search result.

use gpui_kit::{Entity, Global, TestAppContext, VisualTestContext};
use subspace_client_irc::{Command, Event, Line, Target};
use subspace_framework::dashboard::Dashboard as _;
use subspace_framework::dashboard::full_text::{
    Filters, Hit, RecordQuery, Service, Target as Found,
};

use super::{channel_events, inject, mount_hub, settings};
use crate::Irc;
use crate::fixtures::{channel, message, query};
use crate::hub::{self, Hub};
use crate::store::tests::{recent, service};
use crate::store::{self, Link, Op};

#[derive(Default)]
struct Delivered(usize);
impl Global for Delivered {}

/// The shell's services with the fixture network and `service` as search.
fn install_services(service: &Service, cx: &mut gpui_kit::App) {
    let mut config = subspace_framework::core::config::Config::default();
    config.board_settings.insert(
        crate::config::BOARD_KEY.into(),
        serde_json::json!({"networks": [crate::fixtures::config()]})
            .as_object()
            .cloned()
            .unwrap_or_default(),
    );
    cx.set_global(subspace_framework::dashboard::Services {
        credentials: std::sync::Arc::new(
            subspace_framework::core::credentials::Credentials::from_system(&config),
        ),
        config: std::sync::Arc::new(config),
        tools: subspace_framework::dashboard::Catalog::new(vec![]).unwrap(),
        search: Some(service.clone()),
        config_file: "/home/fixture/.config/subspace/config.json".into(),
    });
    cx.set_global(Delivered::default());
    cx.set_global(subspace_framework::dashboard::notifications::Sink(
        |_, _, _, cx| {
            cx.global_mut::<Delivered>().0 += 1;
        },
    ));
}

fn mount_stored<'a>(
    cx: &'a mut TestAppContext,
    service: &Service,
) -> (Entity<Irc>, Entity<Hub>, &'a mut VisualTestContext) {
    let service = service.clone();
    mount_hub(cx, move |cx| {
        install_services(&service, cx);
        hub::install_with_store_for_test(&settings(), service, cx)
    })
}

fn with_id(mut line: Line, id: &str) -> Line {
    line.id = Some(id.into());
    line
}

fn publish(service: &Service, lines: Vec<Line>) {
    let errors = Link::new(service.clone()).run(vec![Op::Publish(lines)], chrono::Utc::now());
    assert!(errors.is_empty(), "{errors:?}");
}

fn stored(service: &Service) -> usize {
    let query = RecordQuery {
        filters: Filters::default(),
        before: None,
        limit: 1000,
    };
    service
        .records(&store::scope(service.app_id()), &query)
        .map_or(0, |page| page.documents.len())
}

fn texts(
    hub: &Entity<Hub>,
    buffer: &subspace_client_irc::BufferKey,
    cx: &VisualTestContext,
) -> Vec<String> {
    hub.read_with(cx, |hub, _| {
        hub.state
            .buffer(buffer)
            .map(|held| {
                held.entries
                    .iter()
                    .map(|entry| entry.line.text.clone())
                    .collect()
            })
            .unwrap_or_default()
    })
}

#[gpui_kit::test]
fn stored_messages_restore_quietly_before_the_runtime_and_resume_after_the_newest(
    cx: &mut TestAppContext,
) {
    let (_root, service) = service();
    let room = channel("#subspace");
    let carol = query("carol");
    publish(
        &service,
        vec![
            recent(message(&room, "alice", "rocket launch at noon", 1)),
            recent(with_id(message(&room, "bob", "on it", 2), "m2")),
            recent(message(&carol, "carol", "are you there?", 3)),
        ],
    );
    let (board, hub, cx) = mount_stored(cx, &service);
    assert_eq!(texts(&hub, &room, cx), ["rocket launch at noon", "on it"]);
    assert_eq!(texts(&hub, &carol, cx), ["are you there?"]);
    hub.read_with(cx, |hub, _| {
        assert_eq!(
            hub.state.attention_count(),
            0,
            "restored lines are not news"
        );
        let held = hub.state.buffer(&room).unwrap();
        assert_eq!((held.unread, held.highlights), (0, 0));
        let sent = hub.sent();
        let configure = sent
            .iter()
            .position(|command| matches!(command, Command::Configure(_)));
        let anchors = sent
            .iter()
            .position(|command| matches!(command, Command::Anchors { .. }));
        assert!(configure < anchors && configure.is_some(), "{sent:?}");
        let Some(Command::Anchors { anchors, .. }) = anchors.map(|at| &sent[at]) else {
            panic!("no anchors");
        };
        let room_anchor = anchors
            .iter()
            .find(|anchor| anchor.target == Target::Channel("#subspace".into()))
            .unwrap();
        assert_eq!(room_anchor.id.as_deref(), Some("m2"));
        assert!(
            anchors
                .iter()
                .any(|anchor| anchor.target == Target::Query("carol".into()))
        );
    });
    cx.update(|_, cx| {
        assert_eq!(cx.global::<Delivered>().0, 0, "no notification");
        assert_eq!(board.read(cx).badge(cx), None);
    });
}

#[gpui_kit::test]
fn live_messages_are_stored_in_batches_and_removing_a_buffer_deletes_them(cx: &mut TestAppContext) {
    let (_root, service) = service();
    let (_board, hub, cx) = mount_stored(cx, &service);
    inject(&hub, channel_events(), cx);
    let room = channel("#subspace");
    let lines: Vec<Event> = (0..120)
        .map(|minute| {
            Event::Line(recent(message(
                &room,
                "alice",
                &format!("line {minute}"),
                minute,
            )))
        })
        .collect();
    inject(&hub, lines, cx);
    assert_eq!(stored(&service), 0, "writes wait for the debounce");
    hub.update(cx, |hub, cx| hub.flush_store(true, cx));
    cx.run_until_parked();
    assert_eq!(
        stored(&service),
        122,
        "every message, the setup's two included"
    );
    hub.update(cx, |hub, cx| {
        hub.remove(&room, cx);
        hub.flush_store(false, cx);
    });
    cx.run_until_parked();
    assert_eq!(stored(&service), 0);
}

/// The hit search would return for `line`, with its permit.
fn hit(
    service: &Service,
    line: &Line,
) -> (Hit, subspace_framework::dashboard::full_text::ReadPermit) {
    let target = Found {
        scope: store::scope(service.app_id()),
        id: store::document_id(line),
    };
    let cached = service.read_fenced(&target).unwrap().unwrap();
    let hit = Hit {
        target,
        document: cached.document,
        excerpt: String::new(),
        score: 0.0,
    };
    (hit, cached.permit)
}

fn open(
    board: &Entity<Irc>,
    hit: Hit,
    permit: subspace_framework::dashboard::full_text::ReadPermit,
    cx: &mut VisualTestContext,
) {
    cx.update(|window, cx| {
        board.update(cx, |board, cx| {
            board.open_indexed_result_fenced(&hit, permit, window, cx)
        });
    });
    cx.run_until_parked();
}

fn target(
    board: &Entity<Irc>,
    buffer: &subspace_client_irc::BufferKey,
    cx: &VisualTestContext,
) -> Option<i64> {
    board.read_with(cx, |board, cx| {
        board
            .dock
            .open
            .get(buffer)
            .and_then(|hosted| hosted.entity.read(cx).target())
    })
}

fn seq_of(hub: &Entity<Hub>, line: &Line, cx: &VisualTestContext) -> Option<i64> {
    hub.read_with(cx, |hub, _| {
        hub.state
            .buffer(&line.buffer)?
            .entries
            .iter()
            .find(|entry| entry.line.text == line.text)
            .map(|entry| entry.seq)
    })
}

#[gpui_kit::test]
fn a_result_the_buffer_holds_opens_its_panel_at_the_line(cx: &mut TestAppContext) {
    let (_root, service) = service();
    let room = channel("#subspace");
    let notes = recent(with_id(message(&room, "dave", "deploy notes", 1), "n1"));
    publish(&service, vec![notes.clone()]);
    let (board, hub, cx) = mount_stored(cx, &service);
    let (found, permit) = hit(&service, &notes);
    open(&board, found, permit, cx);
    assert!(target(&board, &room, cx).is_some());
    assert_eq!(target(&board, &room, cx), seq_of(&hub, &notes, cx));
    hub.read_with(cx, |hub, _| {
        assert!(
            !hub.sent()
                .iter()
                .any(|command| matches!(command, Command::LoadAround { .. }))
        );
    });
}

#[gpui_kit::test]
fn a_result_the_buffer_does_not_hold_asks_around_it_then_opens_it(cx: &mut TestAppContext) {
    let (_root, service) = service();
    let (board, hub, cx) = mount_stored(cx, &service);
    inject(&hub, channel_events(), cx);
    let room = channel("#subspace");
    let old = recent(with_id(message(&room, "dave", "deploy notes", -30), "old1"));
    publish(&service, vec![old.clone()]);
    let (found, permit) = hit(&service, &old);
    open(&board, found, permit, cx);
    hub.read_with(cx, |hub, _| {
        assert!(hub.sent().iter().any(|command| matches!(
            command,
            Command::LoadAround { buffer, id: Some(id), limit: 50, .. } if *buffer == room && id == "old1"
        )));
    });
    assert_eq!(target(&board, &room, cx), None);
    inject(
        &hub,
        vec![Event::Backfill {
            buffer: room.clone(),
            lines: vec![old.clone()],
        }],
        cx,
    );
    assert!(target(&board, &room, cx).is_some());
    assert_eq!(target(&board, &room, cx), seq_of(&hub, &old, cx));
    board.read_with(cx, |board, _| assert!(board.reveal.is_none()));
}
