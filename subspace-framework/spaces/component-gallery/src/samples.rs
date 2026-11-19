//! Samples by section. Each sample calls the public constructor that the
//! boards call, with fixture data, and is captioned with the state it shows.

pub(crate) mod buttons;
pub(crate) mod choices;
pub(crate) mod content;
pub(crate) mod copy_details;
pub(crate) mod dialogs;
pub(crate) mod feedback;
pub(crate) mod github_sign_in;
pub(crate) mod inconsistencies;
pub(crate) mod inputs;
pub(crate) mod kit;
pub(crate) mod lists;
pub(crate) mod menus;
pub(crate) mod messaging;
pub(crate) mod navigation;
pub(crate) mod popover;
pub(crate) mod shell;

use std::collections::HashMap;

use gpui_kit::{
    AnyElement, App, ClickEvent, Div, Entity, IntoElement, ParentElement as _, SharedString,
    Styled as _, Window, div, px,
};
use subspace_dashboard::Tokens;

/// An obviously fake address for link samples.
pub(crate) const FIXTURE_URL: &str = "https://example.com/component-gallery";

/// One rendered state of an item.
pub(crate) struct Sample {
    pub(crate) state: SharedString,
    pub(crate) element: AnyElement,
}

pub(crate) fn sample(state: impl Into<SharedString>, element: impl IntoElement) -> Sample {
    Sample {
        state: state.into(),
        element: element.into_any_element(),
    }
}

/// A fixed-size clipped box for a sample that fills or pads its parent.
pub(crate) fn bounded(size: (f32, f32), tokens: &Tokens, element: impl IntoElement) -> Div {
    let (width, height) = size;
    div()
        .relative()
        .flex()
        .flex_col()
        .w(px(width))
        .h(px(height))
        .overflow_hidden()
        .border_1()
        .border_color(tokens.hair)
        .child(element)
}

/// A fixed-width box for a sample that takes its parent's width.
pub(crate) fn sized(width: f32, element: impl IntoElement) -> Div {
    div().w(px(width)).child(element)
}

/// Sample clicks do nothing outside the gallery.
pub(crate) fn ignore_click(_: &ClickEvent, _: &mut Window, _: &mut App) {}

/// Sample segmented controls keep their fixture choice.
pub(crate) fn ignore_index(_: usize, _: &ClickEvent, _: &mut Window, _: &mut App) {}

/// The values of controlled samples, such as a checkbox or a page number,
/// keyed by a name unique to the sample. A sample reads its value while it
/// renders and stores the value its control requests, so the control works
/// without each sample owning an entity.
#[derive(Debug, Default)]
pub(crate) struct Values {
    flags: HashMap<&'static str, bool>,
    numbers: HashMap<&'static str, usize>,
}

impl Values {
    pub(crate) fn flag(&self, key: &'static str, default: bool) -> bool {
        self.flags.get(key).copied().unwrap_or(default)
    }

    pub(crate) fn number(&self, key: &'static str, default: usize) -> usize {
        self.numbers.get(key).copied().unwrap_or(default)
    }

    pub(crate) fn set_flag(&mut self, key: &'static str, value: bool) {
        self.flags.insert(key, value);
    }

    pub(crate) fn set_number(&mut self, key: &'static str, value: usize) {
        self.numbers.insert(key, value);
    }
}

/// Reads the flag `key` of `values`.
pub(crate) fn flag(values: &Entity<Values>, key: &'static str, default: bool, cx: &App) -> bool {
    values.read(cx).flag(key, default)
}

/// Reads the number `key` of `values`.
pub(crate) fn number(
    values: &Entity<Values>,
    key: &'static str,
    default: usize,
    cx: &App,
) -> usize {
    values.read(cx).number(key, default)
}

/// Stores the flag a control requests and draws the gallery again.
pub(crate) fn store_flag(values: &Entity<Values>, key: &'static str, value: bool, cx: &mut App) {
    values.update(cx, |values, cx| {
        values.set_flag(key, value);
        cx.notify();
    });
}

/// Stores the number a control requests and draws the gallery again.
pub(crate) fn store_number(values: &Entity<Values>, key: &'static str, value: usize, cx: &mut App) {
    values.update(cx, |values, cx| {
        values.set_number(key, value);
        cx.notify();
    });
}

#[cfg(test)]
pub(crate) mod tests;
