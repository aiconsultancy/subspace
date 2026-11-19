//! Loaded reply counts on the real transcript; replies remain readable inline.

use chrono::Local;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Pixels, ScrollDelta, TestAppContext, VisualTestContext, Window, point, px};
use subspace_client_irc::{BufferKey, Event, Line};

use super::{inject, mount, settings};
use crate::fixtures::{at, channel, message, query};
use crate::state::Buffer;
use crate::view;

fn tagged(buffer: &BufferKey, id: &str, parent: Option<&str>, minute: i64) -> Line {
    let mut line = message(buffer, "astra", id, minute);
    line.id = Some(id.into());
    line.reply_to = parent.map(str::to_owned);
    line
}

fn expected(count: &str, minute: i64) -> String {
    format!(
        "{count} · Last reply {}",
        at(minute).with_timezone(&Local).format("%H:%M")
    )
}

fn inline_rows_fit(held: &Buffer, window: &Window) {
    for entry in &held.entries {
        assert!(
            window
                .try_find(format!("irc-line:{}:row", entry.seq))
                .is_some()
        );
    }
    let root = window
        .find(format!("irc-line:{}:row", held.entries[0].seq))
        .bounds();
    let first = window
        .find(format!("irc-line:{}:row", held.entries[1].seq))
        .bounds();
    assert!(
        window.find("irc-replies:root").bounds().bottom() <= root.bottom(),
        "count fits in the root row"
    );
    assert!(
        root.bottom() <= first.top(),
        "growing a root does not overlap replies"
    );
}

fn scroll_past_root(cx: &mut VisualTestContext) -> (i64, Pixels) {
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.find("irc-line:0:row").visible());
        window.scroll(
            "irc-transcript-pane",
            ScrollDelta::Pixels(point(px(0.), px(-900.))),
            cx,
        );
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(
            window
                .try_find("irc-line:0:row")
                .is_none_or(|row| !row.visible())
        );
        (0..80)
            .find_map(|seq| {
                window
                    .try_find(format!("irc-line:{seq}:row"))
                    .filter(|row| row.visible())
                    .map(|row| (seq, row.bounds().top()))
            })
            .unwrap()
    })
}

#[gpui_kit::test]
fn a_previously_measured_offscreen_root_gets_its_count_when_revealed(cx: &mut TestAppContext) {
    let (board, hub, cx) = mount(cx, settings());
    let room = channel("#threads");
    let lines = (0..80)
        .map(|index| Event::Line(tagged(&room, &format!("message-{index}"), None, index)))
        .collect();
    inject(&hub, lines, cx);
    board.update(cx, |board, cx| view::reveal(board, &room, cx));
    let conversation = board.read_with(cx, |board, _| board.dock.open[&room].entity.clone());
    conversation.update(cx, |conversation, cx| assert!(conversation.reveal(0, cx)));
    cx.run_until_parked();
    let anchor = scroll_past_root(cx);
    inject(
        &hub,
        vec![Event::Line(tagged(
            &room,
            "new-reply",
            Some("message-0"),
            81,
        ))],
        cx,
    );
    cx.update(|window, _| {
        assert_eq!(
            window
                .find(format!("irc-line:{}:row", anchor.0))
                .bounds()
                .top(),
            anchor.1,
            "a reply above the viewport keeps the reader's place"
        )
    });
    conversation.update(cx, |conversation, cx| assert!(conversation.reveal(0, cx)));
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        let count = window.find("irc-replies:message-0");
        assert_eq!(count.label(), Some(expected("1 reply", 81).as_str()));
        let root = window.find("irc-line:0:row").bounds();
        let next = window.find("irc-line:1:row").bounds();
        assert!(count.bounds().bottom() <= root.bottom());
        assert!(root.bottom() <= next.top());
    });
}

#[gpui_kit::test]
fn loaded_reply_counts_update_in_channels_and_private_conversations(cx: &mut TestAppContext) {
    let (board, hub, cx) = mount(cx, settings());
    for buffer in [channel("#threads"), query("astra")] {
        inject(
            &hub,
            vec![Event::Line(tagged(&buffer, "root", None, 1))],
            cx,
        );
        board.update(cx, |board, cx| view::reveal(board, &buffer, cx));
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.render_frame(cx);
            assert!(window.try_find("irc-replies:root").is_none());
        });
        inject(
            &hub,
            vec![Event::Line(tagged(&buffer, "first", Some("root"), 2))],
            cx,
        );
        cx.update(|window, _| {
            assert_eq!(
                window.find("irc-replies:root").label(),
                Some(expected("1 reply", 2).as_str())
            );
        });
        inject(
            &hub,
            vec![Event::Line(tagged(&buffer, "second", Some("first"), 3))],
            cx,
        );
        cx.update(|window, cx| {
            let count = window.find("irc-replies:root");
            assert_eq!(count.label(), Some(expected("2 replies", 3).as_str()));
            assert!(count.bounds().size.height > px(0.));
            assert!(
                window.try_find("irc-replies:first").is_none(),
                "only the root gets a count"
            );
            // All three message bodies still render, including the reply to a reply.
            let held = hub.read(cx).state.buffer(&buffer).unwrap();
            inline_rows_fit(held, window);
            let before = hub.read(cx).sent().len();
            window.click("irc-replies:root", cx);
            assert_eq!(
                hub.read(cx).sent().len(),
                before,
                "a count sends no command"
            );
        });
    }
}

#[gpui_kit::test]
fn late_parents_restore_counts_without_crossing_buffers(cx: &mut TestAppContext) {
    let (board, hub, cx) = mount(cx, settings());
    let room = channel("#threads");
    let other = query("astra");
    inject(
        &hub,
        vec![
            Event::Line(tagged(&room, "leaf", Some("middle"), 3)),
            Event::Line(tagged(&other, "root", None, 0)),
            Event::Line(tagged(&other, "middle", Some("root"), 1)),
        ],
        cx,
    );
    board.update(cx, |board, cx| view::reveal(board, &room, cx));
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("irc-replies:root").is_none());
        assert!(window.try_find("irc-replies:middle").is_none());
    });
    hub.update(cx, |hub, cx| {
        hub.state
            .restore(&room, vec![tagged(&room, "middle", Some("root"), 2)]);
        cx.notify();
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(
            window.try_find("irc-replies:root").is_none(),
            "root is still missing"
        );
    });
    hub.update(cx, |hub, cx| {
        hub.state
            .restore(&room, vec![tagged(&room, "root", None, 1)]);
        cx.notify();
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find("irc-replies:root").label(),
            Some(expected("2 replies", 3).as_str())
        );
    });
    board.update(cx, |board, cx| view::reveal(board, &other, cx));
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find("irc-replies:root").label(),
            Some(expected("1 reply", 1).as_str())
        );
    });
}
