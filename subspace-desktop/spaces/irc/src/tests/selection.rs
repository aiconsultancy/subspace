//! Drag selection across transcript rows, and what Copy then holds. These
//! tests document GPUI Kit 0.9.1's behaviour: which bodies take
//! part in one window selection, and in what order their text is copied.

use gpui_kit::base::TextSelection;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    Bounds, Entity, Modifiers, MouseButton, Pixels, Point, TestAppContext, VisualTestContext,
    point, px,
};
use subspace_client_irc::{BufferKey, ConnectionStatus, Event, Line, LineKind};

use super::{inject, mount, settings};
use crate::fixtures::{channel, line, member, message, network_id};
use crate::hub::Hub;
use crate::view;

mod kinds;
mod kit;
mod links;
mod partial;
mod unread;

/// The channel joined, with alice and bob as members and no lines yet.
fn joined(room: &BufferKey) -> Vec<Event> {
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
        Event::Members {
            buffer: room.clone(),
            members: vec![member("alice", "@"), member("bob", "")],
        },
    ]
}

/// alice and bob taking turns, one line a minute from `first`, so every
/// message row starts its own group with a header.
fn turns(room: &BufferKey, first: i64, texts: &[(LineKind, &str)]) -> Vec<Line> {
    texts
        .iter()
        .enumerate()
        .map(|(index, (kind, text))| {
            let from = if index % 2 == 0 { "alice" } else { "bob" };
            let minute = first + i64::try_from(index).unwrap_or_default();
            line(room, *kind, from, text, minute)
        })
        .collect()
}

/// The board showing `room` with `live` lines, after which `older` arrives
/// as history and is drawn above them: the older rows' views are created
/// after the newer ones, against reading order.
fn shown<'a>(
    cx: &'a mut TestAppContext,
    room: &BufferKey,
    live: Vec<Line>,
    older: Vec<Line>,
) -> (Entity<Hub>, &'a mut VisualTestContext) {
    let (board, hub, cx) = mount(cx, settings());
    let mut events = joined(room);
    events.extend(live.into_iter().map(Event::Line));
    inject(&hub, events, cx);
    board.update(cx, |board, cx| view::reveal(board, room, cx));
    cx.run_until_parked();
    cx.update(|window, cx| window.render_frame(cx));
    if !older.is_empty() {
        let history = Event::History {
            buffer: room.clone(),
            lines: older,
            complete: true,
        };
        inject(&hub, vec![history], cx);
    }
    cx.update(|window, cx| window.render_frame(cx));
    (hub, cx)
}

/// Each row's identity, in reading order.
fn row_ids(hub: &Entity<Hub>, room: &BufferKey, cx: &mut VisualTestContext) -> Vec<String> {
    hub.read_with(cx, |hub, _| {
        let buffer = hub.state.buffer(room).expect("the fixture channel");
        buffer
            .entries
            .iter()
            .map(|entry| {
                let membership = matches!(
                    entry.line.kind,
                    LineKind::Join | LineKind::Part | LineKind::Quit | LineKind::Nick
                );
                if membership {
                    format!("irc-members:{}:row", entry.seq)
                } else {
                    format!("irc-line:{}:row", entry.seq)
                }
            })
            .collect()
    })
}

fn bounds(id: &str, cx: &mut VisualTestContext) -> Bounds<Pixels> {
    let id = id.to_owned();
    cx.update(|window, _| window.find(id).bounds())
}

/// Where a row's text starts: the left edge of the message column, which
/// compact rows share, on the row's last line.
fn text_start(row: Bounds<Pixels>, column: Pixels) -> Point<Pixels> {
    point(column + px(1.), row.bottom() - px(8.))
}

/// Past the end of a row's last line.
fn text_end(row: Bounds<Pixels>) -> Point<Pixels> {
    point(row.right() - px(4.), row.bottom() - px(8.))
}

/// The message column's left edge: where the first sender's name starts.
fn column(hub: &Entity<Hub>, room: &BufferKey, cx: &mut VisualTestContext) -> Pixels {
    let seq = hub.read_with(cx, |hub, _| {
        let buffer = hub.state.buffer(room).expect("the fixture channel");
        buffer
            .entries
            .iter()
            .find(|entry| entry.line.kind == LineKind::Privmsg)
            .map(|entry| entry.seq)
            .unwrap_or_default()
    });
    bounds(&format!("irc-line:{seq}:sender"), cx).left()
}

/// Drags from `from` to `to` and returns what Copy would put on the
/// clipboard.
fn drag(from: Point<Pixels>, to: Point<Pixels>, cx: &mut VisualTestContext) -> String {
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(to, Some(MouseButton::Left), Modifiers::default());
    cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::default());
    cx.update(|window, cx| {
        let _ = window.draw(cx);
        TextSelection::selected_text(window, cx)
    })
}

/// Drags from the start of row `first` to the end of row `last`, counted
/// in reading order.
fn drag_rows(
    hub: &Entity<Hub>,
    room: &BufferKey,
    (first, last): (usize, usize),
    cx: &mut VisualTestContext,
) -> String {
    let ids = row_ids(hub, room, cx);
    let column = column(hub, room, cx);
    let from = text_start(bounds(&ids[first], cx), column);
    let to = text_end(bounds(&ids[last], cx));
    drag(from, to, cx)
}

fn privmsgs(texts: &[&'static str]) -> Vec<(LineKind, &'static str)> {
    texts
        .iter()
        .map(|text| (LineKind::Privmsg, *text))
        .collect()
}

/// What Kit copies for whole rows: each text view's text ends its paragraph
/// with a line break, and Kit puts one more between participants, so rows
/// are separated by a blank line.
fn copied_rows(rows: &[&str]) -> String {
    format!("{}\n", rows.join("\n\n"))
}

/// Twelve Markdown bodies, the first six arriving last as history, so the
/// views' creation order differs from reading order. Every body is a
/// selectable `TextView`, which Kit numbers in paint order; a drag over all
/// of them copies them in reading order, never in `HashMap` order.
#[gpui_kit::test]
fn markdown_bodies_copy_in_reading_order(cx: &mut TestAppContext) {
    let room = channel("#subspace");
    let older = turns(
        &room,
        1,
        &privmsgs(&["*m01*", "*m02*", "*m03*", "*m04*", "*m05*", "*m06*"]),
    );
    let live = turns(
        &room,
        10,
        &privmsgs(&["*m07*", "*m08*", "*m09*", "*m10*", "*m11*", "*m12*"]),
    );
    let (hub, cx) = shown(cx, &room, live, older);
    let copied = drag_rows(&hub, &room, (0, 11), cx);
    assert_eq!(
        copied,
        copied_rows(&[
            "m01", "m02", "m03", "m04", "m05", "m06", "m07", "m08", "m09", "m10", "m11", "m12"
        ])
    );
}

/// The same drag upwards, from the end of the last row to the start of the
/// first, copies the same text in the same order.
#[gpui_kit::test]
fn an_upward_drag_copies_in_reading_order_too(cx: &mut TestAppContext) {
    let room = channel("#subspace");
    let older = turns(&room, 1, &privmsgs(&["*m01*", "*m02*", "*m03*"]));
    let live = turns(&room, 10, &privmsgs(&["*m04*", "*m05*", "*m06*"]));
    let (hub, cx) = shown(cx, &room, live, older);
    let ids = row_ids(&hub, &room, cx);
    let column = column(&hub, &room, cx);
    let from = text_end(bounds(&ids[5], cx));
    let to = text_start(bounds(&ids[0], cx), column);
    let copied = drag(from, to, cx);
    assert_eq!(
        copied,
        copied_rows(&["m01", "m02", "m03", "m04", "m05", "m06"])
    );
}

/// Markdown, plain, mIRC-coloured and membership rows interleaved.
fn mixed(room: &BufferKey) -> Vec<Line> {
    let mut lines = turns(
        room,
        1,
        &[
            (LineKind::Privmsg, "*md1*"),
            (LineKind::Privmsg, "plain2"),
            (LineKind::Privmsg, "\u{3}4mirc3\u{3}"),
        ],
    );
    lines.push(line(room, LineKind::Join, "carol", "", 4));
    lines.extend(turns(
        room,
        5,
        &[
            (LineKind::Privmsg, "*md5*"),
            (LineKind::Privmsg, "plain6"),
            (LineKind::Privmsg, "\u{2}mirc7\u{2}"),
        ],
    ));
    lines.push(line(room, LineKind::Quit, "dave", "", 8));
    lines.push(message(room, "alice", "*md9*", 9));
    lines
}

/// Every row, from the first Markdown body to the last, over every kind of
/// row: each body and membership line is a text view, so all of them copy,
/// in reading order. Names and times are labels and do not.
#[gpui_kit::test]
fn mixed_rows_copy_in_reading_order(cx: &mut TestAppContext) {
    let room = channel("#subspace");
    let (hub, cx) = shown(cx, &room, mixed(&room), Vec::new());
    let copied = drag_rows(&hub, &room, (0, 8), cx);
    assert_eq!(
        copied,
        copied_rows(&[
            "md1",
            "plain2",
            "mirc3",
            "carol joined",
            "md5",
            "plain6",
            "mirc7",
            "dave quit",
            "md9",
        ])
    );
}

/// A drag may start on a membership line, or a plain body, and end on an
/// mIRC body: only the rows between are copied, still in reading order.
#[gpui_kit::test]
fn a_drag_between_any_two_rows_copies_only_those_between(cx: &mut TestAppContext) {
    let room = channel("#subspace");
    let (hub, cx) = shown(cx, &room, mixed(&room), Vec::new());
    assert_eq!(
        drag_rows(&hub, &room, (3, 8), cx),
        copied_rows(&["carol joined", "md5", "plain6", "mirc7", "dave quit", "md9"])
    );
    assert_eq!(
        drag_rows(&hub, &room, (1, 6), cx),
        copied_rows(&["plain2", "mirc3", "carol joined", "md5", "plain6", "mirc7"])
    );
}
