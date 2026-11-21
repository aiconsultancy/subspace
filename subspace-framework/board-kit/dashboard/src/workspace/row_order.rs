//! The navigator's focusable rows in the order they were drawn, so the list
//! keys move between those rows only and stop at the first and the last, as
//! a native sidebar outline does, instead of walking the window's whole tab
//! order into the toolbar or the status bar.
//!
//! [`super::Navigator`] starts its window's order afresh as it draws, and each
//! focusable [`super::NavRow`] drawn after it adds itself. A window shows one
//! navigator at a time.

use std::collections::HashMap;

use gpui_kit::{AnyWindowHandle, App, FocusHandle, Global, Window};

/// Each window's focusable navigator rows, in the order last drawn.
#[derive(Default)]
struct RowOrder(HashMap<AnyWindowHandle, Vec<FocusHandle>>);

impl Global for RowOrder {}

/// Which way a list key moves.
#[derive(Clone, Copy)]
pub(super) enum Step {
    Next,
    Previous,
}

/// Starts the window's row order afresh; the navigator's rows draw after it.
/// It also drops the order of any window that has closed.
pub(super) fn begin(window: &Window, cx: &mut App) {
    let open = cx.windows();
    let order = &mut cx.default_global::<RowOrder>().0;
    order.retain(|handle, _| open.contains(handle));
    order.insert(window.window_handle(), Vec::new());
}

/// Adds a focusable row after the rows drawn before it.
pub(super) fn push(focus: &FocusHandle, window: &Window, cx: &mut App) {
    let rows = cx
        .default_global::<RowOrder>()
        .0
        .entry(window.window_handle())
        .or_default();
    if !rows.contains(focus) {
        rows.push(focus.clone());
    }
}

/// Moves the keyboard focus from the row `from` to the next or the previous
/// row. At the last row, or the first, the focus stays where it is.
pub(super) fn step(from: &FocusHandle, direction: Step, window: &mut Window, cx: &mut App) {
    let Some(rows) = cx
        .try_global::<RowOrder>()
        .and_then(|order| order.0.get(&window.window_handle()))
    else {
        return;
    };
    let Some(index) = rows.iter().position(|row| row == from) else {
        return;
    };
    let target = match direction {
        Step::Next => index.checked_add(1),
        Step::Previous => index.checked_sub(1),
    }
    .and_then(|index| rows.get(index))
    .cloned();
    if let Some(target) = target {
        window.focus(&target, cx);
    }
}
