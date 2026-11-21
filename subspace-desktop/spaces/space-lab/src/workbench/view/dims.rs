//! Named layout dimensions for the workbench views, so each width/height is
//! self-documenting instead of a magic literal repeated at each call site.

use gpui_kit::{Pixels, px};

pub(super) const COMPOSER_SIDEBAR_WIDTH: Pixels = px(360.);
pub(super) const COMPOSER_SIDEBAR_MIN_WIDTH: Pixels = px(260.);
pub(super) const COMPACT_RESULT_HEIGHT: Pixels = px(380.);
pub(super) const ENVIRONMENT_CARD_WIDTH: Pixels = px(340.);
pub(super) const VERSION_FIELD_WIDTH: Pixels = px(180.);
pub(super) const PACKAGE_FIELD_WIDTH: Pixels = px(240.);
pub(super) const GALLERY_CARD_WIDTH_WIDE: Pixels = px(470.);
pub(super) const GALLERY_CARD_WIDTH_NARROW: Pixels = px(310.);
pub(super) const GALLERY_CHART_HEIGHT_WIDE: Pixels = px(230.);
pub(super) const GALLERY_CHART_HEIGHT_NARROW: Pixels = px(180.);
