//! Drag and keyboard resizing of the shared message input.

use super::{ChatInput, sizing};
use crate::Tokens;
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, DragMoveEvent, EmptyView, EntityId,
    InteractiveElement as _, IntoElement as _, KeyBinding, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, Window, actions, div, px,
};

const CONTEXT: &str = "SubspaceMessageResize";
const HANDLE_HEIGHT_PX: f32 = 6.0;
const STEP_ROWS: f32 = 1.0;
const GRIP_WIDTH_PX: f32 = 24.0;
const GRIP_HEIGHT_PX: f32 = 2.0;
const HANDLE_OFFSET_PX: f32 = -HANDLE_HEIGHT_PX * 0.5;
const HOVER_OPACITY: f32 = 0.25;
const RESET_CLICK_COUNT: usize = 2;
const PADDING_EDGES: f32 = 2.0;

fn editor_padding() -> f32 {
    f32::from(gpui_kit::component::Size::default().input_py()) * PADDING_EDGES
}

actions!(
    subspace_message_resize,
    [InputTaller, InputShorter, InputReset]
);

pub(super) fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("up", InputTaller, Some(CONTEXT)),
        KeyBinding::new("down", InputShorter, Some(CONTEXT)),
        KeyBinding::new("home", InputReset, Some(CONTEXT)),
    ]);
}

#[derive(Clone)]
pub(super) struct InputResize(EntityId);

fn available_height(input: &ChatInput, window: &Window) -> f32 {
    input
        .message
        .as_ref()
        .map(|message| message.available)
        .filter(|height| *height > 0.0)
        .unwrap_or_else(|| f32::from(window.viewport_size().height))
}

fn pill_height(input: &ChatInput, cx: &App) -> Option<f32> {
    input
        .counter(cx)
        .map(|_| f32::from(crate::badge::pill_height(&Tokens::current(cx))))
}

/// Change native row bounds only when geometry changes, never on every frame.
pub(super) fn sync_rows(input: &mut ChatInput, window: &Window, cx: &mut Context<ChatInput>) {
    let Some(message) = &input.message else {
        return;
    };
    let limits = sizing::row_limits(
        message.height,
        available_height(input, window),
        f32::from(window.rem_size()),
        pill_height(input, cx),
        editor_padding(),
    );
    if input.row_limits != limits {
        input.row_limits = limits;
        input
            .state
            .update(cx, |state, cx| state.set_auto_grow(limits.0, limits.1, cx));
    }
}

pub(super) fn editor_height(input: &ChatInput, window: &Window, cx: &App) -> f32 {
    let line = f32::from(window.rem_size()) * sizing::LINE_REM;
    let rows = input
        .row_limits
        .0
        .max(input.state.read(cx).display_map().wrap_row_count())
        .min(input.row_limits.1);
    rows as f32 * line + editor_padding()
}

fn set_height(input: &mut ChatInput, requested: Option<f32>, cx: &mut Context<ChatInput>) {
    if let Some(message) = &mut input.message {
        message.height = requested;
        cx.notify();
    }
}

fn step(input: &mut ChatInput, delta: f32, window: &Window, cx: &mut Context<ChatInput>) {
    let height = f32::from(input.state.read(cx).input_bounds().size.height)
        + sizing::chrome_height(f32::from(window.rem_size()), None)
        + editor_padding();
    set_height(
        input,
        Some(height + delta * f32::from(window.rem_size()) * sizing::LINE_REM),
        cx,
    );
}

pub(super) fn drag(
    input: &mut ChatInput,
    event: &DragMoveEvent<InputResize>,
    _: &mut Window,
    cx: &mut Context<ChatInput>,
) {
    if event.drag(cx).0 == cx.entity_id() {
        set_height(
            input,
            Some(f32::from(event.bounds.bottom() - event.event.position.y)),
            cx,
        );
    }
}

pub(super) fn handle(input: &ChatInput, cx: &Context<ChatInput>) -> Option<AnyElement> {
    let focus = input.message.as_ref()?.resize_focus.clone();
    let tokens = Tokens::current(cx);
    Some(
        div()
            .id("message-input-resize")
            .absolute()
            .left_0()
            .right_0()
            .top(px(HANDLE_OFFSET_PX))
            .h(px(HANDLE_HEIGHT_PX))
            .key_context(CONTEXT)
            .track_focus(&focus)
            .tab_stop(true)
            .role(gpui_kit::Role::Button)
            .aria_label("Resize message input. Up or Down adjusts height; Home resets.")
            .cursor_row_resize()
            .flex()
            .items_center()
            .justify_center()
            .hover(|style| style.bg(tokens.accent.opacity(HOVER_OPACITY)))
            .child(
                div()
                    .w(px(GRIP_WIDTH_PX))
                    .h(px(GRIP_HEIGHT_PX))
                    .bg(tokens.muted),
            )
            .on_drag(InputResize(cx.entity_id()), |_, _, _, cx| {
                cx.new(|_| EmptyView)
            })
            .on_click(
                cx.listener(move |this, event: &gpui_kit::ClickEvent, window, cx| {
                    focus.focus(window, cx);
                    if event.click_count() >= RESET_CLICK_COUNT {
                        set_height(this, None, cx);
                    }
                }),
            )
            .on_action(
                cx.listener(|this, _: &InputTaller, window, cx| step(this, STEP_ROWS, window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &InputShorter, window, cx| {
                    step(this, -STEP_ROWS, window, cx)
                }),
            )
            .on_action(cx.listener(|this, _: &InputReset, _, cx| set_height(this, None, cx)))
            .into_any_element(),
    )
}
