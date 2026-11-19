//! A dock panel standing in for the terminal in shell tests, so they run
//! without the Zig-built terminal crates.

use gpui_kit::{
    AnyView, App, Context, Entity, EventEmitter, FocusHandle, Focusable, InteractiveElement as _,
    IntoElement, ParentElement as _, Pixels, Render, SharedString, Size, Styled as _,
    TestSupportExt as _, Window, div,
};
use subspace_dashboard::dock_panel::{
    DockPanel, DockPanelEvent, DockPanelHandle, DockPanelRegistration, PanelLayout,
};

use super::SubspaceApp;

/// The fixture panel's root element id.
pub(crate) const PANEL_ID: &str = "terminal-panel";
pub(crate) const HELP: &str = "# Fixture terminal\n\n## Overview\n\nA fixture guide.\n\n## Keys\n\nNone.\n\n## Limits\n\nNone.\n";

pub(crate) struct Panel {
    focus: FocusHandle,
    pub(crate) shown: usize,
    pub(crate) layout: PanelLayout,
    pub(crate) minimum: Option<Size<Pixels>>,
    pub(crate) content: Option<AnyView>,
    pub(crate) warnings: Vec<SharedString>,
    pub(crate) reloads: usize,
}

impl EventEmitter<DockPanelEvent> for Panel {}

impl Focusable for Panel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for Panel {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .id(PANEL_ID)
            .test_support()
            .track_focus(&self.focus)
            .size_full()
            .children(self.content.clone())
    }
}

impl DockPanel for Panel {
    fn show(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.shown += 1;
        self.focus.focus(window, cx);
    }

    fn set_layout(&mut self, layout: PanelLayout, cx: &mut Context<Self>) {
        self.layout = layout;
        cx.notify();
    }

    fn quit_warnings(&self, _: &App) -> Vec<SharedString> {
        self.warnings.clone()
    }

    fn status(&self, _: &App) -> Option<SharedString> {
        Some("⚠ fixture warning".into())
    }

    fn minimum_size(&self, _: &App) -> Option<Size<Pixels>> {
        self.minimum
    }

    fn reload(&mut self, _: &mut Window, _: &mut Context<Self>) {
        self.reloads += 1;
    }
}

fn create(_: &mut Window, cx: &mut App) -> DockPanelHandle {
    DockPanelHandle::create(cx, |cx| Panel {
        focus: cx.focus_handle(),
        shown: 0,
        layout: PanelLayout::default(),
        minimum: None,
        content: None,
        warnings: Vec::new(),
        reloads: 0,
    })
}

pub(crate) fn registration() -> DockPanelRegistration {
    DockPanelRegistration {
        create,
        bind_keys: |_| {},
        help_markdown: HELP,
        help_images: &[],
        keys: &[],
        settings_editor: None,
    }
}

/// Installs the fixture as the product's terminal, before the window opens.
pub(crate) fn install(cx: &mut App) {
    crate::terminal::install(Some(registration()), cx);
}

/// The fixture panel the app created.
pub(crate) fn panel(app: &SubspaceApp) -> Entity<Panel> {
    app.terminal
        .as_ref()
        .and_then(|dock| dock.panel.view().downcast::<Panel>().ok())
        .unwrap_or_else(|| panic!("the app has the fixture terminal"))
}
