//! The native IRC transcript viewport and its top edge treatment.

use gpui_kit::component::message_scroller::{MessageScroller, MessageScrollerState};
use gpui_kit::component::v_flex;
use gpui_kit::{
    AnyElement, App, Entity, Hsla, InteractiveElement as _, IntoElement, ParentElement as _,
    Pixels, StyleRefinement, Styled as _, TestSupportExt as _, Window, canvas, div, fill,
    linear_color_stop, linear_gradient, px,
};
use subspace_framework::dashboard::Tokens;

/// The native scroller over `state`, drawing row `index` with `row`.
pub(crate) fn scroller(
    state: Entity<MessageScrollerState>,
    tokens: &Tokens,
    row: impl Fn(usize, &mut Window, &mut App) -> AnyElement + 'static,
) -> AnyElement {
    let fade = top_fade(state.clone(), tokens.surface);
    v_flex()
        .size_full()
        .min_h_0()
        .pt_1()
        .bg(tokens.bg)
        .child(
            div()
                .id("irc-transcript-viewport")
                .test_support()
                .relative()
                .flex_1()
                .min_h_0()
                .overflow_hidden()
                .bg(tokens.surface)
                .child(
                    MessageScroller::new("irc-transcript", state, row)
                        .with_row_style(row_style())
                        .with_bottom_fade(tokens.surface)
                        .size_full(),
                )
                .child(fade),
        )
        .into_any_element()
}

/// Paint after the list lays out, when its offset reflects the visible rows.
/// Canvas adds no hit target, so links and selection still work at the edge.
fn top_fade(state: Entity<MessageScrollerState>, surface: Hsla) -> impl IntoElement {
    const HEIGHT_PX: f32 = 12.;
    const DOWNWARD: f32 = 180.;
    const START: f32 = 0.;
    const END: f32 = 1.;
    const TRANSPARENT: f32 = 0.;
    canvas(
        |_, _, _| (),
        move |bounds, (), window, cx| {
            let top = state.read(cx).logical_scroll_top();
            if top.item_ix > 0 || top.offset_in_item > Pixels::ZERO {
                let gradient = linear_gradient(
                    DOWNWARD,
                    linear_color_stop(surface, START),
                    linear_color_stop(surface.opacity(TRANSPARENT), END),
                );
                window.paint_quad(fill(bounds, gradient));
            }
        },
    )
    .absolute()
    .left_1()
    .right_1()
    .top_0()
    .h(px(HEIGHT_PX))
}

/// Message groups set their own spacing; rows add none of the chat gap.
fn row_style() -> StyleRefinement {
    StyleRefinement::default().px_1().pb_0()
}
