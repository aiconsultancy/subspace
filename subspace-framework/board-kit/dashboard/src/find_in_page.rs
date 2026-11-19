//! Find within one Space. Native text keeps selection and paints its own matches.
mod model;
mod participant;
mod provider;
pub(crate) mod scope;
mod state;
mod view;

pub(crate) use scope::current;
/// Associate text with its nearest native scrolling container.
pub fn scroll(
    scroll: gpui_kit::ScrollHandle,
    child: impl gpui_kit::IntoElement,
) -> impl gpui_kit::IntoElement {
    scope::scroll_scope(scroll, child)
}
pub use provider::{Document, Provider};
pub(crate) use state::Session;
pub use view::{Close, FindInPage, Next, Open, Previous, pane, surface};

/// The currently focused content of a composite Space participates in local find.
pub fn identified(
    identity: impl Into<gpui_kit::SharedString>,
    enabled: bool,
    child: impl gpui_kit::IntoElement,
    cx: &gpui_kit::App,
) -> gpui_kit::AnyElement {
    scope::identified(identity.into(), enabled, child, cx)
}

mod commands;
pub(crate) use commands::commands;

pub use commands::{Shortcuts, shortcuts};

#[cfg(test)]
mod test_support;
