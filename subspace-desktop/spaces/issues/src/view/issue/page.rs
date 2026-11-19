//! The issue page's column: a capped reading width, centred in the window,
//! with the board's shared content edge. The path bar and the page body use
//! the same column, so Back, the title and every section share one leading
//! edge at any window width.

use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{Div, Styled as _, px, rems};

/// The widest the page grows, in rem so it follows the interface size. Wide
/// enough for the two columns at their comfortable widths; beyond it the
/// page centres rather than stretching prose and rows across the window.
const MAX_WIDTH_REM: f32 = 80.;

/// A full-width band that centres the page column.
pub(super) fn band() -> Div {
    h_flex().w_full().justify_center()
}

/// The page column: capped width and the board's shared content edge, the
/// same edge the issue list uses.
pub(super) fn column() -> Div {
    v_flex()
        .w_full()
        .min_w_0()
        .max_w(rems(MAX_WIDTH_REM))
        .px(px(crate::view::SIDE_INSET_PX))
}
