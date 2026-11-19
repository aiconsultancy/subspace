//! A real retained dock using the same shared skin as Channels.

use crate::{
    ComponentGallery,
    samples::{Sample, sample},
};
use gpui_kit::component::dock::{
    BasePanel, DockArea, DockLayout, Panel, PanelControl, PanelEvent, panel_handle,
};
use gpui_kit::{
    App, AppContext as _, Context, Entity, EventEmitter, FocusHandle, Focusable, IntoElement,
    ParentElement as _, Render, Styled as _, Window,
};
use subspace_dashboard::{Tokens, tabs};

pub(crate) fn fixture(window: &mut Window, cx: &mut App) -> Entity<DockArea> {
    let area = tabs::dock::area("gallery-resource-dock", None, window, cx);
    let mut layout = DockLayout::tabs();
    for name in ["General", "Operations", "Design"] {
        let panel = cx.new(|cx| FixturePanel {
            name,
            focus: cx.focus_handle(),
        });
        layout = layout.panel_view(panel_handle(panel), cx);
    }
    area.update(cx, |area, cx| area.set_center(layout, window, cx));
    area
}

pub(crate) fn samples(
    gallery: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    vec![sample(
        "Native dock tabs: select, close, zoom, open the panel menu or drag into a split",
        gpui_kit::component::v_flex()
            .w_96()
            .h_40()
            .child(gallery.retained.tab_dock.clone()),
    )]
}

struct FixturePanel {
    name: &'static str,
    focus: FocusHandle,
}
impl EventEmitter<PanelEvent> for FixturePanel {}
impl Focusable for FixturePanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
impl BasePanel for FixturePanel {
    fn panel_name(&self) -> &'static str {
        "gallery.resource"
    }
}
impl Panel for FixturePanel {
    fn zoom_control(&self, _: &App) -> Option<PanelControl> {
        Some(PanelControl::Both)
    }
    fn dropdown_menu(
        &mut self,
        menu: gpui_kit::component::menu::PopupMenu,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> gpui_kit::component::menu::PopupMenu {
        let focus = self.focus.clone();
        menu.item(
            gpui_kit::component::menu::PopupMenuItem::new("Focus panel")
                .on_click(move |_, window, cx| focus.focus(window, cx)),
        )
    }
    fn tab_name(&self, _: &App) -> Option<gpui_kit::SharedString> {
        Some(self.name.into())
    }
    fn title(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.name
    }
}
impl Render for FixturePanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        subspace_dashboard::rich::plain_text("gallery-tab-content", self.name)
            .text_color(Tokens::current(cx).fg)
            .p_3()
    }
}
