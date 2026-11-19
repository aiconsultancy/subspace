//! Layout defaults and the type scale: each text role's size from its density setting.

use gpui_kit::{Pixels, px};

use super::Tokens;
use crate::density::{
    BODY_FONT_DEFAULT, HEADING_FONT_DEFAULT, MONO_FONT_DEFAULT, TAB_FONT_DEFAULT,
};

pub(super) const ROW_HEIGHT_DEFAULT: f32 = 28.;
pub(super) const HORIZONTAL_PADDING_DEFAULT: f32 = 12.;
pub(super) const CHILD_INDENT_DEFAULT: f32 = 24.;
pub(super) const ROW_INSET_DEFAULT: f32 = 0.;
pub(super) const CHROME_HEIGHT_DEFAULT: f32 = 30.;
pub(super) const TOOLBAR_HEIGHT_DEFAULT: f32 = 36.;
pub(super) const PANEL_WIDTH_DEFAULT: f32 = 600.;
pub(super) const COLUMN_HEADER_HEIGHT_DEFAULT: f32 = 28.;
pub(super) const LANE_WIDTH_DEFAULT: f32 = 360.;
pub(super) const DONE_FRACTION_DEFAULT: f32 = 0.6;
pub(super) const TREE_WIDTH_DEFAULT: f32 = 360.;
pub(super) const MOTION_MILLIS_DEFAULT: u64 = 140;
pub(super) const CONTROL_RADIUS_DEFAULT: f32 = 4.;
pub(super) const SURFACE_RADIUS_DEFAULT: f32 = 6.;
pub(super) const NAV_ROUND_DEFAULT: f32 = 1.;

const CAPTION_FONT_DEFAULT: f32 = 11.;
const SECONDARY_FONT_DEFAULT: f32 = 12.;

impl Tokens {
    /// Scale an existing heading size without changing its default hierarchy.
    pub fn heading_size(&self, default: f32) -> Pixels {
        px(default) * (f32::from(self.heading_font) / HEADING_FONT_DEFAULT)
    }

    /// Scale a tab label size by the tab setting, as headings scale.
    pub fn tab_size(&self, default: f32) -> Pixels {
        px(default) * (f32::from(self.tab_font) / TAB_FONT_DEFAULT)
    }

    /// Small labels, key hints and badges, scaled from the row font.
    pub fn caption_size(&self) -> Pixels {
        self.row_font * (CAPTION_FONT_DEFAULT / BODY_FONT_DEFAULT)
    }

    /// Supporting text, scaled from the row font.
    pub fn secondary_size(&self) -> Pixels {
        self.row_font * (SECONDARY_FONT_DEFAULT / BODY_FONT_DEFAULT)
    }

    /// Scale an existing monospace size without changing its default hierarchy.
    pub fn mono_size(&self, default: f32) -> Pixels {
        px(default) * (f32::from(self.mono_font) / MONO_FONT_DEFAULT)
    }
}
