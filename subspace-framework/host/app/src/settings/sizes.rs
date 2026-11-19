//! Font-size number inputs use the same density values as the theme designer.
//! Valid in-range entries preview live; leaving a field clamps its value.

use gpui_kit::component::input::{
    InputEvent, InputState, NumberInput, NumberInputEvent, StepAction,
};
use gpui_kit::{
    App, AppContext as _, Context, Div, Entity, InteractiveElement as _, ParentElement as _, Role,
    Stateful, StatefulInteractiveElement as _, Styled as _, Subscription, Window, div, px,
};
use subspace_dashboard::density::{Control, Density, FONT_SIZES};

use super::Settings;
use crate::theme;

/// Fits the stepper buttons, a value such as `15.5` and the unit suffix.
const FONT_SIZE_INPUT_WIDTH: f32 = 136.;

pub(super) fn inputs(window: &mut Window, cx: &mut App) -> Vec<Entity<InputState>> {
    let density = cx.global::<Density>().clone();
    FONT_SIZES
        .iter()
        .map(|control| size_input(density.get(control.token), window, cx))
        .collect()
}

fn size_input(value: f32, window: &mut Window, cx: &mut App) -> Entity<InputState> {
    let text = size_text(value);
    cx.new(|cx| {
        let mut input = InputState::new(window, cx).validate(|text, _| numeric_text(text));
        // The board steps and clamps, so stepped values use `size_text`
        // (`15`, `14.5`) rather than the step's precision (`15.0`).
        input.set_step(None, window, cx);
        input.set_value(text, window, cx);
        input
    })
}

pub(super) fn subscribe(
    inputs: &[Entity<InputState>],
    window: &mut Window,
    cx: &mut Context<Settings>,
) -> Vec<Subscription> {
    inputs
        .iter()
        .enumerate()
        .flat_map(|(index, input)| {
            [
                cx.subscribe_in(
                    input,
                    window,
                    move |settings, _, event: &InputEvent, window, cx| {
                        settings.font_size_event(index, event, window, cx);
                    },
                ),
                cx.subscribe_in(
                    input,
                    window,
                    move |settings, _, event: &NumberInputEvent, window, cx| {
                        let NumberInputEvent::Step(action) = event;
                        settings.step_font_size(index, *action, window, cx);
                    },
                ),
            ]
        })
        .collect()
}

/// Previews every field that holds a valid in-range size; others keep the
/// last applied value until they are committed.
pub(super) fn apply(inputs: &[Entity<InputState>], cx: &mut App) {
    let values: Vec<_> = inputs
        .iter()
        .zip(FONT_SIZES)
        .map(|(input, control)| parse_size(&input.read(cx).value(), control))
        .collect();
    for (control, value) in FONT_SIZES.iter().zip(values) {
        if let Some(value) = value {
            cx.global_mut::<Density>().set(control.token, value);
        }
    }
    theme::sync_font_sizes(cx);
}

/// Clamps every field, shows the committed value and applies it.
pub(super) fn commit_all(inputs: &[Entity<InputState>], window: &mut Window, cx: &mut App) {
    for (input, control) in inputs.iter().zip(FONT_SIZES) {
        commit(input, control, window, cx);
    }
    theme::sync_font_sizes(cx);
}

fn commit(input: &Entity<InputState>, control: &Control, window: &mut Window, cx: &mut App) {
    let current = cx.global::<Density>().get(control.token);
    let value = committed_size(&input.read(cx).value(), control, current);
    let text = size_text(value);
    if input.read(cx).value() != text.as_str() {
        input.update(cx, |input, cx| input.set_value(text, window, cx));
    }
    cx.global_mut::<Density>().set(control.token, value);
}

/// The retained entry of each field, for the draft snapshot.
pub(super) fn values(inputs: &[Entity<InputState>], cx: &App) -> Vec<Option<f32>> {
    inputs
        .iter()
        .map(|input| input.read(cx).value().parse().ok())
        .collect()
}

/// Accepts the text states on the way to a size: empty, digits and one point.
fn numeric_text(text: &str) -> bool {
    text.chars()
        .all(|char| char.is_ascii_digit() || char == '.')
        && text.chars().filter(|char| *char == '.').count() <= 1
}

/// A size to preview while typing: parseable and within the role's range.
fn parse_size(text: &str, control: &Control) -> Option<f32> {
    text.trim()
        .parse::<f32>()
        .ok()
        .filter(|value| (control.min..=control.max).contains(value))
}

/// The value a field settles on: its entry clamped to range, or the last
/// applied value when the entry is not a number.
fn committed_size(text: &str, control: &Control, current: f32) -> f32 {
    text.trim()
        .parse::<f32>()
        .ok()
        .filter(|value| value.is_finite())
        .map_or(current, |value| value.clamp(control.min, control.max))
}

/// One step up or down from the field's settled value, kept in range.
fn stepped_size(text: &str, control: &Control, current: f32, action: StepAction) -> f32 {
    let base = text
        .trim()
        .parse::<f32>()
        .ok()
        .filter(|value| value.is_finite())
        .unwrap_or(current);
    let delta = match action {
        StepAction::Increment => control.step,
        StepAction::Decrement => -control.step,
    };
    (base + delta).clamp(control.min, control.max)
}

fn size_text(value: f32) -> String {
    value.to_string()
}

impl Settings {
    pub(super) fn font_size(&self, index: usize) -> Stateful<Div> {
        let control = &FONT_SIZES[index];
        div()
            .id(("font-size", index))
            .role(Role::Group)
            .aria_label(format!("{} font size", control.label))
            .flex_shrink_0()
            .w(px(FONT_SIZE_INPUT_WIDTH))
            .child(NumberInput::new(&self.font_sizes[index]).suffix(control.unit))
    }

    fn font_size_event(
        &self,
        index: usize,
        event: &InputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            InputEvent::Change => apply(&self.font_sizes, cx),
            InputEvent::Blur => {
                commit(&self.font_sizes[index], &FONT_SIZES[index], window, cx);
                theme::sync_font_sizes(cx);
            }
            _ => return,
        }
        window.refresh();
        cx.notify();
    }

    /// A stepper button or the up/down key: show and apply the next size.
    fn step_font_size(
        &self,
        index: usize,
        action: StepAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let control = &FONT_SIZES[index];
        let input = &self.font_sizes[index];
        let current = cx.global::<Density>().get(control.token);
        let value = stepped_size(&input.read(cx).value(), control, current, action);
        input.update(cx, |input, cx| {
            input.set_value(size_text(value), window, cx)
        });
        cx.global_mut::<Density>().set(control.token, value);
        theme::sync_font_sizes(cx);
        window.refresh();
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::{committed_size, numeric_text, parse_size, size_text, stepped_size};
    use gpui_kit::component::input::StepAction;
    use subspace_dashboard::density::FONT_SIZES;

    #[test]
    fn steps_move_by_the_role_step_stay_in_range_and_print_cleanly() {
        let body = &FONT_SIZES[1];
        let up = stepped_size("14", body, 13., StepAction::Increment);
        assert_eq!(up, 14. + body.step);
        assert_eq!(
            size_text(stepped_size(
                &size_text(up),
                body,
                13.,
                StepAction::Increment
            )),
            size_text(14. + 2. * body.step)
        );
        assert_eq!(
            stepped_size("", body, 13., StepAction::Decrement),
            13. - body.step
        );
        assert_eq!(
            stepped_size(&size_text(body.max), body, 13., StepAction::Increment),
            body.max
        );
        assert_eq!(
            stepped_size(&size_text(body.min), body, 13., StepAction::Decrement),
            body.min
        );
        assert!(!size_text(14. + 2. * body.step).ends_with(".0"));
    }

    #[test]
    fn size_entries_accept_only_unsigned_decimal_text() {
        for text in ["", "1", "15", "15.", "15.5"] {
            assert!(numeric_text(text), "{text}");
        }
        for text in ["-1", "1.2.3", "1e2", "15px", " 15"] {
            assert!(!numeric_text(text), "{text}");
        }
    }

    #[test]
    fn only_in_range_entries_preview_while_typing() {
        let heading = &FONT_SIZES[0];
        assert_eq!(parse_size("15.5", heading), Some(15.5));
        assert_eq!(
            parse_size(&size_text(heading.min), heading),
            Some(heading.min)
        );
        assert_eq!(
            parse_size(&size_text(heading.max), heading),
            Some(heading.max)
        );
        assert_eq!(parse_size("1", heading), None);
        assert_eq!(parse_size("99", heading), None);
        assert_eq!(parse_size("", heading), None);
    }

    #[test]
    fn committed_entries_clamp_or_keep_the_applied_value() {
        let body = &FONT_SIZES[1];
        assert_eq!(committed_size("5", body, 13.), body.min);
        assert_eq!(committed_size("100", body, 13.), body.max);
        assert_eq!(committed_size("14.5", body, 13.), 14.5);
        assert_eq!(committed_size("", body, 13.), 13.);
        assert_eq!(committed_size(".", body, 13.), 13.);
        assert_eq!(size_text(15.), "15");
        assert_eq!(size_text(15.5), "15.5");
    }
}
