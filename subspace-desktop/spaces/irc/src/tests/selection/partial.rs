//! Pointer regression coverage through the production Channels transcript.

use gpui_kit::base::TextSelection;
use gpui_kit::test::{ClickOptions, TestWindowExt as _};
use gpui_kit::{Pixels, Point, TestAppContext, VisualTestContext, point, px};

use super::{bounds, column, drag, privmsgs, row_ids, shown, text_end, text_start, turns};
use crate::fixtures::channel;

const CARET_PROBES: usize = 14;
const MESSAGE: &str = "alpha bravo charlie delta echo";

#[gpui_kit::test]
fn native_partial_ranges_on_plain_messages(cx: &mut TestAppContext) {
    assert_eq!(ranges(MESSAGE, cx), expected_ranges());
}

#[gpui_kit::test]
fn native_partial_ranges_on_markdown_messages(cx: &mut TestAppContext) {
    assert_eq!(
        ranges("alpha **bravo charlie delta** echo", cx),
        expected_ranges()
    );
}

#[gpui_kit::test]
fn native_partial_ranges_on_mirc_messages(cx: &mut TestAppContext) {
    assert_eq!(
        ranges("\x02alpha bravo charlie delta echo\x02", cx),
        expected_ranges()
    );
}

fn expected_ranges() -> [String; 3] {
    ["bravo charlie delta".into(), "bravo".into(), MESSAGE.into()]
}

fn ranges(source: &'static str, cx: &mut TestAppContext) -> [String; 3] {
    let room = channel("#selection");
    let live = turns(&room, 1, &privmsgs(&[source]));
    let (hub, cx) = shown(cx, &room, live, Vec::new());
    let (id, row, start, end) = message_extent(&hub, &room, cx);
    let from = caret_for_prefix(start, end, "alpha ", cx);
    let to = caret_for_prefix(start, end, "alpha bravo charlie delta", cx);
    // Native Markdown copy terminates its paragraph with a line break.
    let partial = drag(from, to, cx).trim_end_matches('\n').to_owned();
    let inside_word = caret_for_prefix(start, end, "alpha brav", cx);
    let offset = inside_word - row.origin;
    let clicked = cx.update(|window, cx| {
        window.click_with_options(
            id.clone(),
            ClickOptions::new().with_offset(offset).with_count(2),
            cx,
        );
        let word = TextSelection::selected_text(window, cx);
        window.click_with_options(
            id.clone(),
            ClickOptions::new().with_offset(offset).with_count(3),
            cx,
        );
        let line = TextSelection::selected_text(window, cx)
            .trim_end_matches('\n')
            .to_owned();
        (word, line)
    });
    [partial, clicked.0, clicked.1]
}

/// Locate a real native caret by observing prefix selection. This keeps the
/// pointer test independent of platform font metrics and display scaling.
fn caret_for_prefix(
    start: Point<Pixels>,
    end: Point<Pixels>,
    prefix: &str,
    cx: &mut VisualTestContext,
) -> Point<Pixels> {
    let (mut left, mut right) = (start.x, end.x);
    let mut position = start;
    let mut copied = String::new();
    for _ in 0..CARET_PROBES {
        position = point((left + right) / 2., start.y);
        copied = drag(start, position, cx).trim_end_matches('\n').to_owned();
        if copied == prefix {
            return position;
        }
        if copied.len() < prefix.len() {
            left = position.x;
        } else {
            right = position.x;
        }
        if right - left < px(0.01) {
            break;
        }
    }
    assert_eq!(
        copied, prefix,
        "the transcript must expose partial text carets"
    );
    position
}

#[gpui_kit::test]
fn native_middle_word_selection_survives_opening_and_reopening_find(cx: &mut TestAppContext) {
    let room = channel("#retained-selection");
    let (board, hub, cx) = crate::tests::mount(cx, crate::tests::settings());
    let mut events = super::joined(&room);
    events.extend(
        turns(&room, 1, &privmsgs(&[MESSAGE]))
            .into_iter()
            .map(subspace_client_irc::Event::Line),
    );
    crate::tests::inject(&hub, events, cx);
    board.update(cx, |board, cx| crate::view::reveal(board, &room, cx));
    cx.run_until_parked();
    crate::tests::find::attach_find(&board, cx);
    let (_, _, start, end) = message_extent(&hub, &room, cx);
    let from = caret_for_prefix(start, end, "alpha ", cx);
    let to = caret_for_prefix(start, end, "alpha bravo charlie delta", cx);
    assert_eq!(
        drag(from, to, cx).trim_end_matches('\n'),
        "bravo charlie delta"
    );
    for _ in 0..2 {
        cx.update(|window, cx| window.press(subspace_framework::dashboard::secondary!("f"), cx));
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.render_frame(cx);
            assert_eq!(
                TextSelection::selected_text(window, cx).trim_end_matches('\n'),
                "bravo charlie delta"
            );
            window.press("escape", cx);
        });
        cx.run_until_parked();
    }
}

fn message_extent(
    hub: &gpui_kit::Entity<crate::hub::Hub>,
    room: &subspace_client_irc::BufferKey,
    cx: &mut VisualTestContext,
) -> (
    String,
    gpui_kit::Bounds<Pixels>,
    Point<Pixels>,
    Point<Pixels>,
) {
    let id = row_ids(hub, room, cx).remove(0);
    let row = bounds(&id, cx);
    let start = text_start(row, column(hub, room, cx));
    (id, row, start, text_end(row))
}
