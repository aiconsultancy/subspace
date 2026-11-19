//! Shared tab appearance over Kit's dock skin. Base still owns the layout.

use gpui_kit::component::dock::{
    BasePanelView, DockArea, DockAreaRenderer, DockContext, DockSkin, DropIndicator, NodeId,
    PanelState, TabGroupContext, TabGroupRenderer,
};
use gpui_kit::{
    AnyElement, AnyView, App, AppContext as _, Axis, Div, Entity, Stateful, WeakEntity, Window,
};
use std::{rc::Rc, sync::Arc};

mod drag;
mod group;
#[cfg(test)]
mod tests;
mod toggles;
mod toolbar;

struct Skin {
    native: Rc<DockSkin>,
    area: WeakEntity<DockArea>,
}

/// Native dock behaviour with the shared selected-tab treatment.
pub fn area(
    id: &'static str,
    version: Option<usize>,
    window: &mut Window,
    cx: &mut App,
) -> Entity<DockArea> {
    area_with_toggle_buttons(id, version, window, cx, false)
}

/// Create the shared dock with Kit's designated collapse controls visible or hidden.
pub fn area_with_toggle_buttons(
    id: &'static str,
    version: Option<usize>,
    window: &mut Window,
    cx: &mut App,
    visible: bool,
) -> Entity<DockArea> {
    let mut skin = None;
    let area = cx.new(|cx| {
        let native = DockSkin::new(cx);
        skin = Some(native.clone());
        DockArea::new(id, version, window, cx).with_renderer(Rc::new(Skin {
            native,
            area: cx.weak_entity(),
        }))
    });
    if let Some(skin) = skin {
        skin.set_close_button_visible(true, cx);
        skin.set_toggle_button_visible(visible, cx);
    }
    area
}

impl DockAreaRenderer for Skin {
    fn frame(&self, window: &mut Window, cx: &mut App) -> Stateful<Div> {
        self.native.frame(window, cx)
    }
    fn center_frame(&self, window: &mut Window, cx: &mut App) -> Stateful<Div> {
        self.native.center_frame(window, cx)
    }
    fn split_frame(
        &self,
        node: NodeId,
        axis: Axis,
        window: &mut Window,
        cx: &mut App,
    ) -> Stateful<Div> {
        self.native.split_frame(node, axis, window, cx)
    }
    fn render_split_handle(
        &self,
        handle: &gpui_kit::base::ResizeHandleContext,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<AnyElement> {
        self.native.render_split_handle(handle, window, cx)
    }
    fn render_dock(
        &self,
        dock: &DockContext,
        content: AnyElement,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        self.native.render_dock(dock, content, window, cx)
    }
    fn build_placeholder(
        &self,
        state: &PanelState,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Arc<dyn BasePanelView>> {
        self.native.build_placeholder(state, window, cx)
    }
    fn tab_group_renderer(&self) -> Rc<dyn TabGroupRenderer> {
        Rc::new(Group {
            native: self.native.tab_group_renderer(),
            skin: self.native.clone(),
            area: self.area.clone(),
            state: group::BarState::default(),
        })
    }
}

struct Group {
    native: Rc<dyn TabGroupRenderer>,
    skin: Rc<DockSkin>,
    area: WeakEntity<DockArea>,
    state: group::BarState,
}

impl TabGroupRenderer for Group {
    fn frame(&self, group: &TabGroupContext, window: &mut Window, cx: &mut App) -> Stateful<Div> {
        self.native.frame(group, window, cx)
    }
    fn content_frame(
        &self,
        group: &TabGroupContext,
        window: &mut Window,
        cx: &mut App,
    ) -> Stateful<Div> {
        self.native.content_frame(group, window, cx)
    }
    fn render_tab_bar(
        &self,
        group: &TabGroupContext,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        let visible = group
            .panels()
            .iter()
            .filter(|panel| panel.visible(cx))
            .count();
        if visible <= 1 || group.is_collapsed() {
            return self.native.render_tab_bar(group, window, cx);
        }
        group::bar(group, self, window, cx)
    }
    fn render_active_panel(
        &self,
        panel: AnyView,
        group: &TabGroupContext,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        self.native.render_active_panel(panel, group, window, cx)
    }
    fn render_drop_indicator(
        &self,
        indicator: DropIndicator,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<AnyElement> {
        self.native.render_drop_indicator(indicator, window, cx)
    }
}
