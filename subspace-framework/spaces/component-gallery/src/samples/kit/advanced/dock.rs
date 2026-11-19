//! GPUI Kit's DockArea with three fixture panels: two share a tab group,
//! the third is split beside them. Drag a tab to rearrange.
//!
//! Follows GPUI Kit's `dock_story.rs` (Apache-2.0), cut down to fixture panels.

use gpui_kit::component::dock::{
    BasePanel, DockArea, DockLayout, DockSkin, Panel, PanelEvent, panel_handle,
};
use gpui_kit::{
    App, AppContext as _, Context, Entity, EventEmitter, FocusHandle, Focusable, IntoElement,
    ParentElement as _, Render, SharedString, Styled as _, Window, div, px,
};
use subspace_dashboard::Tokens;

use crate::ComponentGallery;
use crate::samples::{Sample, bounded, sample};

const AREA: (f32, f32) = (560., 260.);
const SIDE_WIDTH: f32 = 200.;
const DOCK_VERSION: usize = 1;
/// The fixture panels: their stable name, tab title and body.
const PANELS: [(&str, &str, &str); 3] = [
    (
        "gallery-kit.files",
        "Files",
        "Drag this tab into the other group.",
    ),
    (
        "gallery-kit.search",
        "Search",
        "Two panels share one tab group.",
    ),
    (
        "gallery-kit.editor",
        "Editor",
        "Drop a tab near an edge to split the group.",
    ),
];

/// One fixture panel.
pub(crate) struct FixturePanel {
    name: &'static str,
    title: SharedString,
    body: SharedString,
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
        self.name
    }

    fn closable(&self, _: &App) -> bool {
        false
    }
}

impl Panel for FixturePanel {
    fn title(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.title.clone()
    }
}

impl Render for FixturePanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = Tokens::current(cx);
        div()
            .size_full()
            .p(tokens.pad_x)
            .text_color(tokens.muted)
            .child(self.body.clone())
    }
}

fn panel(
    (name, title, body): (&'static str, &'static str, &'static str),
    cx: &mut App,
) -> Entity<FixturePanel> {
    cx.new(|cx| FixturePanel {
        name,
        title: title.into(),
        body: body.into(),
        focus: cx.focus_handle(),
    })
}

pub(crate) fn dock_area(window: &mut Window, cx: &mut App) -> Entity<DockArea> {
    let (area, _skin) = DockSkin::dock_area("gallery-kit:dock", Some(DOCK_VERSION), window, cx);
    let [files, search, editor] = PANELS.map(|fixture| panel(fixture, cx));
    area.update(cx, |area, cx| {
        let side = DockLayout::tabs()
            .panel_view(panel_handle(files), cx)
            .panel_view(panel_handle(search), cx);
        let main = DockLayout::tabs().panel_view(panel_handle(editor), cx);
        area.set_center(
            DockLayout::h_split()
                .child(side, Some(px(SIDE_WIDTH)))
                .child(main, None),
            window,
            cx,
        );
    });
    area
}

pub(crate) fn docks(
    gallery: &ComponentGallery,
    tokens: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    vec![sample(
        "Tabs and a split; drag a tab",
        bounded(AREA, tokens, gallery.retained.kit.advanced.dock.clone()),
    )]
}
