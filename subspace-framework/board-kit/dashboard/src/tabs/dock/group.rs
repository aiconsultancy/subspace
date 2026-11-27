//! Tab presentation and native dock callback wiring.

use crate::tabs::{Tab, TabSpec, tab, tab_bar};
use gpui_kit::base::ElementExt as _;
use gpui_kit::component::Sizable as _;
use gpui_kit::component::dock::{AnyDrag, DragPanel, PanelHandle, TabGroupContext};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, InteractiveElement as _, IntoElement as _, ScrollHandle, Styled as _,
    TestSupportExt as _, Window,
};

#[derive(Default)]
pub(super) struct BarState {
    scroll: ScrollHandle,
    active: std::rc::Rc<std::cell::Cell<Option<usize>>>,
}

pub(super) fn bar(
    group: &TabGroupContext,
    renderer: &super::Group,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let state = &renderer.state;
    let tokens = crate::Tokens::current(cx);
    let leading = super::toggles::leading(renderer, group, cx);
    let trailing = super::toolbar::trailing(renderer, group, window, cx);
    let visible: Vec<_> = group
        .panels()
        .iter()
        .enumerate()
        .filter(|(_, panel)| panel.visible(cx))
        .collect();
    let active = group.active_panel().map(|panel| panel.panel_id(cx));
    let selected = visible
        .iter()
        .position(|(_, panel)| Some(panel.panel_id(cx)) == active);
    reveal(state, selected);
    let tabs = visible
        .into_iter()
        .enumerate()
        .map(|(visible_ix, (ix, panel))| {
            let handle = PanelHandle::of(panel);
            let label = handle
                .and_then(|handle| handle.tab_name(cx))
                .unwrap_or_else(|| panel.panel_name(cx).into());
            let title = handle.map(|handle| handle.title(window, cx));
            let spec = TabSpec::plain(label).map(|spec| match title {
                Some(title) => spec.content(title),
                None => spec,
            });
            wire(tab(spec), group, (ix, visible_ix), state, cx)
        })
        .collect::<Vec<_>>();
    let tabs_len = tabs.len();
    let free = free_width(&state.scroll, tabs_len);
    tab_bar("dock-resource-tabs")
        .with_size(gpui_kit::component::Size::Medium)
        .surface(tokens.surface)
        .w_full()
        .track_scroll(&state.scroll)
        .map(|bar| match selected {
            Some(ix) => bar.selected_index(ix),
            None => bar,
        })
        .map(|bar| match leading {
            Some(leading) => bar.prefix(leading),
            None => bar,
        })
        .children(tabs)
        .last_empty_space(trailing_drop(
            group,
            &tokens,
            (&state.scroll, tabs_len, free),
        ))
        .suffix(trailing)
        .into_any_element()
}

fn wire(
    tab: Tab,
    group: &TabGroupContext,
    indices: (usize, usize),
    state: &BarState,
    cx: &App,
) -> Tab {
    let (ix, visible_ix) = indices;
    let active = state.active.clone();
    let panel = &group.panels()[ix];
    let id = panel.panel_id(cx);
    let select = group.clone();
    let close = group.clone();
    tab.on_click(move |_, window, cx| {
        active.set(Some(visible_ix));
        select.select_tab(ix, window, cx);
    })
    .when(group.is_panel_closable(id, cx), |tab| {
        tab.on_close(move |_, window, cx| close.close(id, window, cx))
    })
    .when(group.is_draggable() && !group.is_locked(), |tab| {
        tab.when_some(group.drag_panel(ix, cx), |tab, drag| {
            super::drag::wire(tab, drag, panel.clone())
        })
    })
    .when(group.is_droppable(), |tab| {
        drops(tab, group, ix, &crate::Tokens::current(cx))
    })
}

fn drops(tab: Tab, group: &TabGroupContext, ix: usize, tokens: &crate::Tokens) -> Tab {
    let panels = group.clone();
    let items = group.clone();
    let edge = tokens.accent;
    tab.drag_over::<DragPanel>(move |style, _, _, _| {
        style
            .rounded_l_none()
            .border_l_2()
            .border_r_0()
            .border_color(edge)
    })
    .drag_over::<AnyDrag>(move |style, _, _, _| {
        style
            .rounded_l_none()
            .border_l_2()
            .border_r_0()
            .border_color(edge)
    })
    .on_drop(move |drag: &DragPanel, window, cx| {
        panels.drop_panel(drag.clone(), Some(ix), true, window, cx)
    })
    .on_drop(move |item: &AnyDrag, window, cx| items.drop_item(item.clone(), None, window, cx))
}

/// The strip's width after its last tab, as the last frame laid it out.
/// GPUI Kit 0.11 wraps the last empty space in a box that does not grow, so
/// the drop target sizes itself to the free width to cover the rest of the
/// strip.
fn free_width(scroll: &ScrollHandle, tabs: usize) -> gpui_kit::Pixels {
    let strip = scroll.bounds();
    let end = tabs
        .checked_sub(1)
        .and_then(|last| scroll.bounds_for_item(last))
        .map_or(strip.left(), |tab| tab.right());
    (strip.right() - end).max(gpui_kit::Pixels::ZERO)
}

/// The change in free width, in pixels, that draws the strip again.
const FREE_WIDTH_STEP: f32 = 1.;

/// Whether the free width laid out now differs from the width drawn.
fn free_width_moved(drawn: gpui_kit::Pixels, now: gpui_kit::Pixels) -> bool {
    (f32::from(now) - f32::from(drawn)).abs() >= FREE_WIDTH_STEP
}

fn trailing_drop(
    group: &TabGroupContext,
    tokens: &crate::Tokens,
    (scroll, tabs, free): (&ScrollHandle, usize, gpui_kit::Pixels),
) -> impl gpui_kit::IntoElement {
    let scroll = scroll.clone();
    let target = group.clone();
    let items = group.clone();
    let fill = tokens.select;
    let count = group.panels().len();
    gpui_kit::div()
        .id("dock-trailing-drop")
        .test_support()
        // GPUI Kit 0.11 wraps the last empty space in a plain box, so the
        // target takes the tab height and the strip's free width itself.
        .h(gpui_kit::px(crate::tabs::paint::DEFAULT_HEIGHT))
        .min_w_16()
        .w(free)
        // The width comes from the frame before. When this frame's layout
        // moved the end of the tabs, such as after a resize, ask for another
        // frame: a refresh while drawing is ignored, so it waits until this
        // frame is drawn.
        .on_prepaint(move |_, window, _| {
            if free_width_moved(free, free_width(&scroll, tabs)) {
                window.on_next_frame(|window, _| window.refresh());
            }
        })
        .when(group.is_droppable(), |space| {
            space
                .drag_over::<DragPanel>(move |style, _, _, _| style.bg(fill))
                .drag_over::<AnyDrag>(move |style, _, _, _| style.bg(fill))
                .on_drop(move |drag: &DragPanel, window, cx| {
                    let index = (drag.source() == target.node()).then_some(count.saturating_sub(1));
                    target.drop_panel(drag.clone(), index, false, window, cx);
                })
                .on_drop(move |item: &AnyDrag, window, cx| {
                    items.drop_item(item.clone(), None, window, cx)
                })
        })
}

fn reveal(state: &BarState, selected: Option<usize>) {
    if state.active.replace(selected) != selected
        && let Some(selected) = selected
    {
        state.scroll.scroll_to_item(selected);
    }
}
