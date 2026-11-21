//! The issue page's reading column: no wider than the shared reading width,
//! centred in the pane, with the workspace's one text edge. The toolbar
//! above it holds Back and the path; the column holds the issue itself.

use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{Div, Styled as _, px};
use subspace_framework::dashboard::Tokens;
use subspace_framework::dashboard::workspace::{READING_WIDTH_PX, text_edge};

/// A full-width band that centres the page column.
pub(super) fn band() -> Div {
    h_flex().w_full().justify_center()
}

/// The page column: the reading width at most, on the workspace's text edge.
pub(super) fn column(tokens: &Tokens) -> Div {
    v_flex()
        .w_full()
        .min_w_0()
        .max_w(px(READING_WIDTH_PX))
        .px(text_edge(tokens))
}
