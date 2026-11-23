//! Dock widths. Each dock is as wide as its density token, held inside the
//! window, and its inner edge drags to resize it. A double click on the edge
//! goes back to the default width. When the workspace is too narrow for both
//! docks and the pane's least width, the docks yield (see
//! [`super::PANE_MIN_WIDTH_PX`]).

use gpui_kit::{
    AnyElement, App, AppContext as _, BorrowAppContext as _, ClickEvent, Div, EmptyView,
    InteractiveElement as _, ParentElement as _, Stateful, StatefulInteractiveElement as _,
    Styled as _, TestSupportExt as _, Window, div, px, relative,
};

use crate::density::Density;
use crate::panel;
use crate::tokens::Tokens;

const NAVIGATOR_TOKEN: &str = "--nav-w";
const INSPECTOR_TOKEN: &str = "--inspector-w";
/// The narrowest a dock is drawn, whatever the window's width.
const NAVIGATOR_MIN: f32 = 180.;
const INSPECTOR_MIN: f32 = 260.;
/// The share of the window a dock may take at most.
const NAVIGATOR_MAX_SHARE: f32 = 0.3;
const INSPECTOR_MAX_SHARE: f32 = 0.4;
/// A dock never grows past the workspace it sits in.
const FULL_PARENT_WIDTH: f32 = 1.;
/// The handle straddles the dock's inner border.
const HANDLE_OFFSET_PX: f32 = -3.;
const HANDLE_WIDTH_PX: f32 = 6.;
const HANDLE_HOVER_OPACITY: f32 = 0.4;
/// A double click on a handle resets the width.
const RESET_CLICK_COUNT: usize = 2;

/// The drag value of the navigator's trailing edge.
pub struct NavigatorResize;

/// The drag value of the inspector's leading edge.
pub struct InspectorResize;

/// The navigator's width as drawn: its token, at least 180 px and at most
/// 30 % of the window, in whole pixels.
pub fn navigator_width(token_width: f32, window_width: f32) -> f32 {
    held(
        token_width,
        NAVIGATOR_MIN,
        window_width * NAVIGATOR_MAX_SHARE,
    )
}

/// The inspector's width as drawn: its token, at least 260 px and at most
/// 40 % of the window, in whole pixels.
pub fn inspector_width(token_width: f32, window_width: f32) -> f32 {
    held(
        token_width,
        INSPECTOR_MIN,
        window_width * INSPECTOR_MAX_SHARE,
    )
}

/// The minimum wins over the window's share, so a narrow window still gets a
/// usable dock.
fn held(width: f32, min: f32, max: f32) -> f32 {
    width.min(max).max(min).round()
}

pub(super) fn navigator_slot(dock: AnyElement, tokens: &Tokens, viewport: f32) -> Stateful<Div> {
    let width = navigator_width(f32::from(tokens.navigator_width), viewport);
    let handle = handle("workspace-navigator-resize", tokens)
        .right(px(HANDLE_OFFSET_PX))
        .aria_label("Resize navigator")
        .on_drag(NavigatorResize, |_, _, _, cx| cx.new(|_| EmptyView))
        .on_click(|event, window, cx| reset_on_double_click(NAVIGATOR_TOKEN, event, window, cx))
        .test_support();
    slot("workspace-navigator-slot", width)
        .child(clipped(dock))
        .child(handle)
        .on_drag_move::<NavigatorResize>(|event, window, cx| {
            let width = event.event.position.x - event.bounds.left();
            store(NAVIGATOR_TOKEN, f32::from(width), window, cx);
        })
}

pub(super) fn inspector_slot(dock: AnyElement, tokens: &Tokens, viewport: f32) -> Stateful<Div> {
    let width = inspector_width(f32::from(tokens.inspector_width), viewport);
    let handle = handle("workspace-inspector-resize", tokens)
        .left(px(HANDLE_OFFSET_PX))
        .aria_label("Resize inspector")
        .on_drag(InspectorResize, |_, _, _, cx| cx.new(|_| EmptyView))
        .on_click(|event, window, cx| reset_on_double_click(INSPECTOR_TOKEN, event, window, cx))
        .test_support();
    slot("workspace-inspector-slot", width)
        .child(clipped(dock))
        .child(handle)
        .on_drag_move::<InspectorResize>(|event, window, cx| {
            let width = event.bounds.right() - event.event.position.x;
            store(INSPECTOR_TOKEN, f32::from(width), window, cx);
        })
}

/// A dock's place in the workspace row: its width while there is room, and
/// narrower, down to nothing, when the pane would otherwise lose its least
/// width. Docks shrink in proportion to their widths.
fn slot(id: &'static str, width: f32) -> Stateful<Div> {
    div()
        .id(id)
        .relative()
        .flex_shrink_1()
        .min_w_0()
        .w(px(width))
        .max_w(relative(FULL_PARENT_WIDTH))
        .h_full()
}

/// A dock's content, clipped to its slot, so content that cannot shrink,
/// such as header buttons, never draws over the pane when the dock yields.
/// The resize handle sits outside the clip, so it keeps its whole width, and
/// menus and popovers draw deferred, so the clip never cuts them off.
fn clipped(dock: AnyElement) -> Div {
    div().size_full().overflow_hidden().child(dock)
}

/// A draggable edge; the caller places it on the dock's inner side.
fn handle(id: &'static str, tokens: &Tokens) -> Stateful<Div> {
    div()
        .id(id)
        .absolute()
        .top_0()
        .bottom_0()
        .w(px(HANDLE_WIDTH_PX))
        .cursor_col_resize()
        .hover(|style| style.bg(tokens.accent.opacity(HANDLE_HOVER_OPACITY)))
}

/// Stores a dock's width in the density settings and the state file. The
/// density control holds it to the token's limits.
fn store(token: &'static str, width: f32, window: &mut Window, cx: &mut App) {
    cx.update_default_global::<Density, _>(|density, _| density.set(token, width));
    panel::save_density(cx);
    window.refresh();
}

fn reset_on_double_click(
    token: &'static str,
    event: &ClickEvent,
    window: &mut Window,
    cx: &mut App,
) {
    if event.click_count() < RESET_CLICK_COUNT {
        return;
    }
    if cx.has_global::<Density>() {
        cx.global_mut::<Density>().remove(token);
    }
    panel::save_density(cx);
    window.refresh();
}
