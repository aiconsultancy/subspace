//! Native slider presentation and input routing.

use std::{cell::Cell, rc::Rc};

use gpui_kit::base::{SliderIndicator, SliderThumb, SliderTrack, TestSupportExt as _};
use gpui_kit::component::slider::SliderState;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AccessibleAction, App, Axis, ElementId, Entity, FocusHandle, InteractiveElement as _,
    IntoElement, KeyDownEvent, MouseButton, ParentElement as _, RenderOnce, Role, SharedString,
    StatefulInteractiveElement, Styled as _, Window, div, relative, rems,
};

use super::NumericSliderState;
use crate::Tokens;

// These match the `size_4` thumb and `h_1` indicator in the native composition.
const THUMB_SIZE_REM: f32 = 1.;
const TRACK_HEIGHT_REM: f32 = 0.25;
const THUMB_LEFT_OFFSET_REM: f32 = -(THUMB_SIZE_REM / 2.);
const THUMB_TOP_OFFSET_REM: f32 = -((THUMB_SIZE_REM - TRACK_HEIGHT_REM) / 2.);

/// Compose a slider with a stable ID, visible label and retained numeric state.
pub fn control(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    state: &Entity<NumericSliderState>,
    tokens: &Tokens,
) -> SliderControl {
    SliderControl {
        id: id.into(),
        label: label.into(),
        state: state.clone(),
        tokens: *tokens,
    }
}

/// The value-like presentation returned by [`control`].
#[derive(IntoElement)]
pub struct SliderControl {
    id: ElementId,
    label: SharedString,
    state: Entity<NumericSliderState>,
    tokens: Tokens,
}

fn adjust(state: &Entity<NumericSliderState>, direction: i32, window: &mut Window, cx: &mut App) {
    state.update(cx, |state, cx| {
        let next = state
            .tick
            .saturating_add_signed(direction)
            .min(state.spec.ticks);
        state.user_tick(next, window, cx);
    });
}

fn key_handler(state: Entity<NumericSliderState>) -> impl Fn(&KeyDownEvent, &mut Window, &mut App) {
    move |event, window, cx| {
        if event.keystroke.modifiers.control
            || event.keystroke.modifiers.alt
            || event.keystroke.modifiers.platform
        {
            return;
        }
        match event.keystroke.key.as_str() {
            "left" | "down" => adjust(&state, -1, window, cx),
            "right" | "up" => adjust(&state, 1, window, cx),
            "home" => state.update(cx, |state, cx| state.user_tick(0, window, cx)),
            "end" => state.update(cx, |state, cx| {
                state.user_tick(state.spec.ticks, window, cx)
            }),
            _ => return,
        }
        cx.stop_propagation();
    }
}

fn accessibility<T: StatefulInteractiveElement + IntoElement>(
    root: T,
    state: &Entity<NumericSliderState>,
    disabled: bool,
) -> T {
    let increment = state.clone();
    let decrement = state.clone();
    root.when(disabled, |root| root.aria_description("Disabled"))
        .when(!disabled, |root| {
            root.on_a11y_action(AccessibleAction::Increment, move |_, window, cx| {
                adjust(&increment, 1, window, cx)
            })
            .on_a11y_action(AccessibleAction::Decrement, move |_, window, cx| {
                adjust(&decrement, -1, window, cx)
            })
        })
}

fn pointer<T: StatefulInteractiveElement + IntoElement>(
    root: T,
    inner: &Entity<SliderState>,
    focus: &FocusHandle,
    pressed: Rc<Cell<bool>>,
    disabled: bool,
) -> T {
    let focus = focus.clone();
    let click_pressed = pressed.clone();
    let release_pressed = pressed.clone();
    let release_state = inner.clone();
    let release_out_state = inner.clone();
    root.capture_any_mouse_down(move |event, window, cx| {
        if !disabled && event.button == MouseButton::Left {
            click_pressed.set(true);
            window.focus(&focus, cx);
        }
    })
    .when(!disabled, |root| {
        root.on_mouse_up(MouseButton::Left, move |_, _, cx| {
            if release_pressed.replace(false) {
                release_state.update(cx, |state, cx| state.handle_release(cx));
            }
        })
        .on_mouse_up_out(MouseButton::Left, move |_, _, cx| {
            if pressed.replace(false) {
                release_out_state.update(cx, |state, cx| state.handle_release(cx));
            }
        })
    })
}

impl SliderControl {
    fn root(&self, window: &mut Window, cx: &mut App) -> impl IntoElement + use<> {
        let focus = window
            .use_keyed_state(self.id.clone(), cx, |_, cx| cx.focus_handle())
            .read(cx)
            .clone();
        let state = self.state.read(cx);
        let focus = focus.tab_stop(!state.disabled);
        let disabled = state.disabled;
        let inner = state.inner.clone();
        let value = state.value();
        let spec = state.spec;
        let track = track(
            &inner,
            disabled,
            inner.read(cx).percentage().end,
            &self.tokens,
        );
        let pressed = window
            .use_keyed_state(("slider-pressed", inner.entity_id()), cx, |_, _| {
                Rc::new(Cell::new(false))
            })
            .read(cx)
            .clone();
        let focus_color = self.tokens.accent;
        let root = div()
            .id(self.id.clone())
            .test_support()
            .role(Role::Slider)
            .aria_label(self.label.clone())
            .aria_numeric_value(value)
            .aria_min_numeric_value(spec.min)
            .aria_max_numeric_value(spec.max)
            .aria_numeric_value_step(spec.step)
            .track_focus(&focus)
            .w_full()
            .rounded(self.tokens.control_radius)
            .focus_visible(move |style| style.border_1().border_color(focus_color))
            .on_key_down(key_handler(self.state.clone()));
        let root = accessibility(root, &self.state, disabled);
        pointer(root, &inner, &focus, pressed, disabled).child(track)
    }
}

impl RenderOnce for SliderControl {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let value = self.state.read(cx).value();
        let root = self.root(window, cx);
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .flex()
                    .justify_between()
                    .text_color(self.tokens.muted)
                    .child(self.label.clone())
                    .child(format!("{value}")),
            )
            .child(root)
    }
}

fn track(
    state: &Entity<SliderState>,
    disabled: bool,
    percentage: f32,
    tokens: &Tokens,
) -> impl IntoElement + use<> {
    let thumb = SliderThumb::new(state)
        .axis(Axis::Horizontal)
        .disabled(disabled)
        .absolute()
        .left(relative(percentage))
        .ml(rems(THUMB_LEFT_OFFSET_REM))
        .top(rems(THUMB_TOP_OFFSET_REM))
        .size_4()
        .rounded_full()
        .bg(if disabled {
            tokens.muted
        } else {
            tokens.fg_strong
        });
    SliderTrack::new(state)
        .axis(Axis::Horizontal)
        .disabled(disabled)
        .flex()
        .items_center()
        .w_full()
        .h_6()
        .child(
            SliderIndicator::new(state)
                .relative()
                .w_full()
                .h_1()
                .rounded_full()
                .bg(tokens.line)
                .child(
                    div()
                        .absolute()
                        .left_0()
                        .top_0()
                        .bottom_0()
                        .w(relative(percentage))
                        .rounded_full()
                        .bg(if disabled {
                            tokens.muted
                        } else {
                            tokens.accent
                        }),
                )
                .child(thumb),
        )
}
