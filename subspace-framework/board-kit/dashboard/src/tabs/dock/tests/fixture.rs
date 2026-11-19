//! Render the real dock adapter with offline panels, as the gallery does.

use gpui_kit::component::{
    button::Button,
    dock::{
        BasePanel, DockArea, DockLayout, DockPlacement, PaneRef, Panel, PanelControl, PanelEvent,
        panel_handle,
    },
    menu::{PopupMenu, PopupMenuItem},
};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    App, AppContext as _, Context, Entity, EventEmitter, FocusHandle, Focusable,
    InteractiveElement as _, IntoElement, ParentElement as _, Render,
    StatefulInteractiveElement as _, Styled as _, TestAppContext, TestSupportExt as _,
    VisualTestContext, Window, div,
};

pub(super) struct FixturePanel {
    name: &'static str,
    visible: bool,
    focus: FocusHandle,
    pub(super) invoked: usize,
    pub(super) control: Option<PanelControl>,
    pub(super) zoomable: bool,
}
impl FixturePanel {
    fn new(name: &'static str, visible: bool, cx: &mut Context<Self>) -> Self {
        Self {
            name,
            visible,
            focus: cx.focus_handle(),
            invoked: 0,
            control: Some(PanelControl::Both),
            zoomable: true,
        }
    }
}
impl EventEmitter<PanelEvent> for FixturePanel {}
impl Focusable for FixturePanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
impl BasePanel for FixturePanel {
    fn panel_name(&self) -> &'static str {
        "review.fixture"
    }
    fn visible(&self, _: &App) -> bool {
        self.visible
    }
    fn zoomable(&self, _: &App) -> bool {
        self.zoomable
    }
}
impl Panel for FixturePanel {
    fn tab_name(&self, _: &App) -> Option<gpui_kit::SharedString> {
        Some(self.name.into())
    }
    fn title(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().id("fixture-title").test_support().child(self.name)
    }
    fn title_suffix(&mut self, _: &mut Window, _: &mut Context<Self>) -> Option<impl IntoElement> {
        Some(
            div()
                .id("fixture-suffix")
                .test_support()
                .child("Panel details"),
        )
    }
    fn toolbar_buttons(&mut self, _: &mut Window, cx: &mut Context<Self>) -> Option<Vec<Button>> {
        Some(vec![
            Button::new("fixture-action")
                .label("Refresh")
                .on_click(cx.listener(|panel, _, _, cx| {
                    panel.invoked += 1;
                    cx.notify();
                })),
        ])
    }
    fn dropdown_menu(
        &mut self,
        menu: PopupMenu,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> PopupMenu {
        menu.item(
            PopupMenuItem::new("Fixture action").on_click(cx.listener(|panel, _, _, cx| {
                panel.invoked += 1;
                cx.notify();
            })),
        )
    }
    fn zoom_control(&self, _: &App) -> Option<PanelControl> {
        self.control
    }
}
impl Render for FixturePanel {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().child(self.name)
    }
}

struct Fixture {
    area: Entity<DockArea>,
}
impl Render for Fixture {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().relative().size_full().child(self.area.clone()).child(
            div()
                .id("external-drag")
                .test_support()
                .absolute()
                .bottom_0()
                .left_0()
                .size_8()
                .on_drag(
                    gpui_kit::component::dock::AnyDrag::new("fixture"),
                    |_, _, _, cx| cx.new(|_| ItemPreview),
                ),
        )
    }
}

struct ItemPreview;
impl Render for ItemPreview {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().child("Fixture item")
    }
}

pub(super) fn mount(
    cx: &mut TestAppContext,
    hidden: bool,
) -> (
    Entity<DockArea>,
    Vec<Entity<FixturePanel>>,
    &mut VisualTestContext,
) {
    mount_with(cx, hidden, false)
}

pub(super) fn mount_with(
    cx: &mut TestAppContext,
    hidden: bool,
    toggles: bool,
) -> (
    Entity<DockArea>,
    Vec<Entity<FixturePanel>>,
    &mut VisualTestContext,
) {
    cx.update(gpui_kit::init);
    let mut panels = Vec::new();
    let (view, cx) = cx.add_window_view(|window, cx| {
        let area =
            crate::tabs::dock::area_with_toggle_buttons("review-dock", None, window, cx, toggles);
        let mut layout = DockLayout::tabs();
        for (ix, name) in ["General", "Operations", "Design"].into_iter().enumerate() {
            let panel = cx.new(|cx| FixturePanel::new(name, !(hidden && ix == 0), cx));
            layout = layout.panel_view(panel_handle(panel.clone()), cx);
            panels.push(panel);
        }
        area.update(cx, |area, cx| area.set_center(layout, window, cx));
        if toggles {
            for placement in [DockPlacement::Left, DockPlacement::Right] {
                let panel = cx.new(|cx| FixturePanel::new("Inspector", true, cx));
                let layout = DockLayout::tabs().panel_view(panel_handle(panel), cx);
                area.update(cx, |area, cx| area.set_dock(placement, layout, window, cx));
            }
        }
        Fixture { area }
    });
    cx.update(|window, cx| {
        window.replace_root(cx, |window, cx| {
            gpui_kit::base::Root::new(view.clone(), window, cx)
        });
    });
    cx.run_until_parked();
    let area = view.read_with(cx, |view, _| view.area.clone());
    (area, panels, cx)
}

pub(super) fn active(area: &Entity<DockArea>, cx: &App) -> usize {
    fn active_in(node: &gpui_kit::component::dock::PaneNode) -> usize {
        match node.kind() {
            PaneRef::Tabs { active_ix, .. } => active_ix,
            PaneRef::Split { children, .. } => active_in(&children[0]),
        }
    }
    active_in(area.read(cx).layout(DockPlacement::Center).unwrap().root())
}

pub(super) fn settle(cx: &mut VisualTestContext) {
    cx.run_until_parked();
    cx.update(|window, cx| window.render_frame(cx));
}
