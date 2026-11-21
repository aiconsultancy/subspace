//! The workspace shell every Space shares: a navigator dock on the leading
//! side, a pane with a tab strip and a toolbar above its content, and an
//! inspector dock on the trailing side.
//!
//! Panels tile to the window's edges and to each other, separated by
//! hairlines. The breathing room is inside each panel, on one text edge
//! ([`text_edge`]): a dock's header, its rows, the toolbar and the content all
//! start there. A Space decides which docks it shows; [`Workspace`] gives each
//! one its stored width and a draggable inner edge. The Space reports its
//! docks to the shell through `Dashboard::docks` ([`SpaceDocks`]), and the
//! shell's status bar draws a switch for each ([`dock_switch`]).

mod docks;
mod header;
mod inspector;
mod navigator;
mod pane;
mod resize;
mod row_parts;
#[cfg(test)]
mod tests;

use gpui_kit::component::h_flex;
use gpui_kit::{
    AnyElement, App, ElementId, InteractiveElement as _, IntoElement, ParentElement as _, Pixels,
    RenderOnce, Styled as _, TestSupportExt as _, Window, div,
};

pub use docks::{SpaceDock, SpaceDocks, dock_switch};
pub use inspector::{Inspector, fact, section};
pub use navigator::{NavRow, Navigator, nav_section};
pub use pane::{Pane, Toolbar, pane_head};
pub use resize::{InspectorResize, NavigatorResize, inspector_width, navigator_width};

use crate::tokens::Tokens;

/// Reading content, such as an issue's body or a conversation, is no wider
/// than this, so a wide window does not stretch its lines.
pub const READING_WIDTH_PX: f32 = 760.;

/// Where text starts inside every panel: the content inset plus the row
/// inset, the edge a shared list row's content already uses.
pub fn text_edge(tokens: &Tokens) -> Pixels {
    tokens.pad_x + tokens.row_inset
}

/// A Space's workspace: an optional navigator, the pane, and an optional
/// inspector, filling the Space's area.
#[derive(IntoElement)]
pub struct Workspace {
    id: ElementId,
    navigator: Option<AnyElement>,
    pane: Option<AnyElement>,
    inspector: Option<AnyElement>,
    detail: Option<AnyElement>,
}

// qual:allow(dry, boilerplate) reason: "Each dock is an independent optional slot on a GPUI element; a fluent builder keeps call sites readable."
impl Workspace {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            navigator: None,
            pane: None,
            inspector: None,
            detail: None,
        }
    }

    /// The leading dock, usually a [`Navigator`]. Leave it out to hide it.
    pub fn navigator(mut self, navigator: impl IntoElement) -> Self {
        self.navigator = Some(navigator.into_any_element());
        self
    }

    /// The work area, usually a [`Pane`]. It takes the width the docks leave.
    pub fn pane(mut self, pane: impl IntoElement) -> Self {
        self.pane = Some(pane.into_any_element());
        self
    }

    /// The trailing dock, usually an [`Inspector`]. Leave it out to hide it.
    pub fn inspector(mut self, inspector: impl IntoElement) -> Self {
        self.inspector = Some(inspector.into_any_element());
        self
    }

    /// A trailing panel that owns its own width and resize edge, such as the
    /// shared detail panel (`panel::frame`). It sits where the inspector
    /// does, at the workspace's full height, so its header lines up with the
    /// tab strip. Use it or [`Self::inspector`], not both.
    pub fn detail(mut self, detail: impl IntoElement) -> Self {
        self.detail = Some(detail.into_any_element());
        self
    }
}

impl RenderOnce for Workspace {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let tokens = Tokens::current(cx);
        let viewport = f32::from(window.viewport_size().width);
        let navigator = self
            .navigator
            .map(|dock| resize::navigator_slot(dock, &tokens, viewport).test_support());
        let inspector = self
            .inspector
            .map(|dock| resize::inspector_slot(dock, &tokens, viewport).test_support());
        h_flex()
            .id(self.id)
            .test_support()
            .size_full()
            .min_w_0()
            .min_h_0()
            .items_stretch()
            .bg(tokens.bg)
            .children(navigator)
            .child(
                div()
                    .id("workspace-pane")
                    .test_support()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .children(self.pane),
            )
            .children(inspector)
            .children(
                self.detail
                    .map(|panel| div().flex_shrink_0().min_w_0().h_full().child(panel)),
            )
    }
}
