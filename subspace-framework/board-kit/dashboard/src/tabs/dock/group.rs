//! Tab presentation and native dock callback wiring.

use crate::tabs::{Tab, TabSpec, tab, tab_bar};
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
        .last_empty_space(trailing_drop(group, &tokens))
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

fn trailing_drop(group: &TabGroupContext, tokens: &crate::Tokens) -> impl gpui_kit::IntoElement {
    let target = group.clone();
    let items = group.clone();
    let fill = tokens.select;
    let count = group.panels().len();
    gpui_kit::div()
        .id("dock-trailing-drop")
        .test_support()
        .h_full()
        .flex_grow_1()
        .min_w_16()
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
