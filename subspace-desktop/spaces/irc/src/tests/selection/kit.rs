//! What GPUI Kit 0.9.1 copies from a drag over its three kinds of text,
//! interleaved, outside the board: a selectable `TextView` (every
//! transcript row), `InteractiveText` (plain and mIRC bodies
//! and server lines before it) and `SelectableText` (membership lines
//! before it). Kit numbers text views in paint order and gives every
//! `SelectableText` order 0 unless it is told otherwise; `InteractiveText`
//! takes no part in selection. A drag covers the participants whose order
//! lies between its two ends' and whose text lies between its two points,
//! and Copy sorts them by order, ties in `HashMap` order. See
//! `docs/changes/irc-transcript-drag-selection/findings.md`.

use gpui_kit::base::TextSelectionLayer;
use gpui_kit::{
    AnyElement, Context, InteractiveText, IntoElement, ParentElement as _, Render, Styled as _,
    StyledText, TestAppContext, Window, div, point, px,
};
use subspace_framework::dashboard::rich;

use super::drag;

const ROW_PX: f32 = 32.;
const WIDTH_PX: f32 = 600.;

#[derive(Clone, Copy)]
enum Kind {
    View,
    Interactive,
    Selectable,
}

/// The rows: a text view, interactive text and selectable text, twice,
/// then a text view.
const ROWS: [(Kind, &str); 7] = [
    (Kind::View, "view0"),
    (Kind::Interactive, "interactive1"),
    (Kind::Selectable, "selectable2"),
    (Kind::View, "view3"),
    (Kind::Interactive, "interactive4"),
    (Kind::Selectable, "selectable5"),
    (Kind::View, "view6"),
];

fn element(kind: Kind, text: &'static str) -> AnyElement {
    match kind {
        Kind::View => rich::rich_text_prepared(text, text, |_, _, _| false).into_any_element(),
        Kind::Interactive => InteractiveText::new(text, StyledText::new(text)).into_any_element(),
        Kind::Selectable => rich::plain_text(text, text).into_any_element(),
    }
}

struct Mixed;

impl Render for Mixed {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .child(TextSelectionLayer)
            .children(ROWS.map(|(kind, text)| {
                div()
                    .w(px(WIDTH_PX))
                    .h(px(ROW_PX))
                    .child(element(kind, text))
            }))
    }
}

/// Drags from the start of row `first` to the end of row `last`.
fn drag_rows(cx: &mut TestAppContext, (first, last): (usize, usize)) -> String {
    cx.update(gpui_kit::init);
    let (_, cx) = cx.add_window_view(|_, _| Mixed);
    cx.update(|window, cx| {
        let _ = window.draw(cx);
    });
    let row_y = |row: usize| px(ROW_PX * row as f32 + 8.);
    let from = point(px(1.), row_y(first));
    let to = point(px(WIDTH_PX - 4.), row_y(last));
    drag(from, to, cx)
}

/// Between two text views every text view between copies, in order;
/// selectable text (order 0) and interactive text do not.
#[gpui_kit::test]
fn text_views_copy_in_paint_order_and_skip_the_others(cx: &mut TestAppContext) {
    assert_eq!(drag_rows(cx, (0, 6)), "view0\n\nview3\n\nview6\n");
}

/// From selectable text, the range of orders starts at 0: every selectable
/// text between the two points copies first, then the text views, out of
/// reading order.
#[gpui_kit::test]
fn selectable_text_copies_before_text_views(cx: &mut TestAppContext) {
    let copied = drag_rows(cx, (2, 6));
    let (selectable, views) = split(&copied);
    assert_eq!(selectable, ["selectable2", "selectable5"]);
    assert_eq!(views, "view3\n\nview6\n");
}

/// From a text view down to selectable text, the orders run from 0 to the
/// first view's: the later view between them is left out, and the
/// selectable text copies before the view above it.
#[gpui_kit::test]
fn a_drag_ending_on_selectable_text_leaves_out_later_text_views(cx: &mut TestAppContext) {
    let copied = drag_rows(cx, (0, 5));
    let (selectable, views) = split(&copied);
    assert_eq!(selectable, ["selectable2", "selectable5"]);
    assert_eq!(views, "view0\n", "view3 is left out");
}

/// Copied text split where the text views start: the selectable texts
/// before them, sorted, as they tie in `HashMap` order, and the rest.
fn split(copied: &str) -> (Vec<&str>, &str) {
    let (selectable, views) = copied.split_at(copied.find("view").unwrap_or(copied.len()));
    let mut selectable: Vec<&str> = selectable.lines().collect();
    selectable.sort_unstable();
    (selectable, views)
}

/// A drag that starts on interactive text selects nothing.
#[gpui_kit::test]
fn interactive_text_takes_no_part(cx: &mut TestAppContext) {
    assert_eq!(drag_rows(cx, (1, 4)), "");
}
