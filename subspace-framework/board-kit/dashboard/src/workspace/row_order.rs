//! The navigator's focusable rows in the order they were drawn, so the list
//! keys move between those rows only and stop at the first and the last, as
//! a native sidebar outline does, instead of walking the window's whole tab
//! order into the toolbar or the status bar.
//!
//! Each [`super::Navigator`] keeps its own row order as state of its element
//! and starts it afresh as it draws. Each focusable [`super::NavRow`] drawn
//! after it joins that order and moves only within it, so two navigators in
//! one window, such as two workspaces side by side, each keep their own.

use std::collections::{HashMap, HashSet};

use gpui_kit::{AnyWindowHandle, App, Entity, EntityId, FocusHandle, Global, WeakEntity, Window};

/// One navigator's focusable rows, in the order last drawn.
#[derive(Default)]
pub(super) struct NavigatorRows {
    order: Vec<FocusHandle>,
    /// The rows already in `order`, by their focus state, so a row drawn
    /// twice joins once without a search of the whole order.
    joined: HashSet<EntityId>,
}

/// The navigator each window is drawing, whose order the rows drawn after it
/// join.
#[derive(Default)]
struct Drawing(HashMap<AnyWindowHandle, WeakEntity<NavigatorRows>>);

impl Global for Drawing {}

/// Which way a list key moves.
#[derive(Clone, Copy)]
pub(super) enum Step {
    Next,
    Previous,
}

/// Starts this navigator's row order afresh; its rows draw after it. It also
/// forgets any window that has closed.
pub(super) fn begin(window: &mut Window, cx: &mut App) {
    let rows = window.use_keyed_state("workspace-navigator-rows", cx, |_, _| {
        NavigatorRows::default()
    });
    rows.update(cx, |rows, _| {
        rows.order.clear();
        rows.joined.clear();
    });
    let open = cx.windows();
    let drawing = &mut cx.default_global::<Drawing>().0;
    drawing.retain(|handle, _| open.contains(handle));
    drawing.insert(window.window_handle(), rows.downgrade());
}

/// Adds a focusable row, whose focus state is `state`, after the rows drawn
/// before it in the navigator being drawn, and returns that navigator's rows
/// for the row's list keys to move in.
pub(super) fn push(
    focus: &FocusHandle,
    state: EntityId,
    window: &Window,
    cx: &mut App,
) -> Option<Entity<NavigatorRows>> {
    let rows = cx
        .try_global::<Drawing>()?
        .0
        .get(&window.window_handle())?
        .upgrade()?;
    rows.update(cx, |rows, _| {
        if rows.joined.insert(state) {
            rows.order.push(focus.clone());
        }
    });
    Some(rows)
}

/// Moves the keyboard focus from the row `from` to the next or the previous
/// row of its navigator, `rows`. At the last row, or the first, the focus
/// stays where it is.
pub(super) fn step(
    rows: Option<&Entity<NavigatorRows>>,
    from: &FocusHandle,
    direction: Step,
    window: &mut Window,
    cx: &mut App,
) {
    let Some(rows) = rows else {
        return;
    };
    let order = &rows.read(cx).order;
    let Some(index) = order.iter().position(|row| row == from) else {
        return;
    };
    let target = match direction {
        Step::Next => index.checked_add(1),
        Step::Previous => index.checked_sub(1),
    }
    .and_then(|index| order.get(index))
    .cloned();
    if let Some(target) = target {
        window.focus(&target, cx);
    }
}
