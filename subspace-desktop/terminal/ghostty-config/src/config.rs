//! The [`GhosttyConfig`] value type and its Ghostty-documented defaults.

use crate::types::{
    AdjustCellHeight, ClipboardAccess, ConfirmCloseSurface, CopyOnSelect, CursorStyle, Rgb,
    WorkingDirectory,
};
use std::collections::BTreeMap;

/// `scrollback-limit`'s default (REQ-16).
pub(crate) const DEFAULT_SCROLLBACK_LIMIT_BYTES: u64 = 10_000_000;
/// `window-padding-x` and `window-padding-y`'s default (Ghostty default).
pub(crate) const DEFAULT_WINDOW_PADDING: f32 = 2.0;
/// `mouse-scroll-multiplier`'s default.
pub(crate) const DEFAULT_MOUSE_SCROLL_MULTIPLIER: f32 = 1.0;

/// The Ghostty configuration values the terminal panel applies (REQ-31).
///
/// Every field here stays at Ghostty's documented default until a supported
/// key sets it, or falls back to that default again when the key's value
/// cannot be parsed (REQ-32). `None` on a colour, font or working-directory
/// field means the board should use its own appearance-derived default
/// instead of a Ghostty default.
#[derive(Debug, Clone, PartialEq)]
pub struct GhosttyConfig {
    pub background: Option<Rgb>,
    pub foreground: Option<Rgb>,
    pub palette: BTreeMap<u8, Rgb>,
    pub cursor_color: Option<Rgb>,
    pub cursor_text: Option<Rgb>,
    pub selection_background: Option<Rgb>,
    pub selection_foreground: Option<Rgb>,
    pub cursor_style: CursorStyle,
    pub cursor_style_blink: bool,
    pub font_family: Vec<String>,
    pub font_family_bold: Vec<String>,
    pub font_family_italic: Vec<String>,
    pub font_family_bold_italic: Vec<String>,
    pub font_size: Option<f32>,
    pub adjust_cell_height: Option<AdjustCellHeight>,
    pub scrollback_limit: u64,
    pub term: Option<String>,
    pub command: Option<String>,
    pub working_directory: Option<WorkingDirectory>,
    pub clipboard_read: ClipboardAccess,
    pub clipboard_write: ClipboardAccess,
    pub clipboard_paste_protection: bool,
    pub clipboard_paste_bracketed_safe: bool,
    pub copy_on_select: CopyOnSelect,
    pub confirm_close_surface: Option<ConfirmCloseSurface>,
    pub mouse_scroll_multiplier: f32,
    pub window_padding_x: f32,
    pub window_padding_y: f32,
    /// Raw `keybind` lines (`trigger=action`). Task 08 interprets them.
    pub keybind: Vec<String>,
    /// The raw `theme` value. [`crate::theme::resolve_theme`] interprets it.
    pub theme: Option<String>,
}

impl Default for GhosttyConfig {
    fn default() -> Self {
        Self {
            background: None,
            foreground: None,
            palette: BTreeMap::new(),
            cursor_color: None,
            cursor_text: None,
            selection_background: None,
            selection_foreground: None,
            cursor_style: CursorStyle::default(),
            cursor_style_blink: true,
            font_family: Vec::new(),
            font_family_bold: Vec::new(),
            font_family_italic: Vec::new(),
            font_family_bold_italic: Vec::new(),
            font_size: None,
            adjust_cell_height: None,
            scrollback_limit: DEFAULT_SCROLLBACK_LIMIT_BYTES,
            term: None,
            command: None,
            working_directory: None,
            clipboard_read: ClipboardAccess::Ask,
            clipboard_write: ClipboardAccess::Allow,
            clipboard_paste_protection: true,
            clipboard_paste_bracketed_safe: true,
            copy_on_select: CopyOnSelect::platform_default(),
            confirm_close_surface: None,
            mouse_scroll_multiplier: DEFAULT_MOUSE_SCROLL_MULTIPLIER,
            window_padding_x: DEFAULT_WINDOW_PADDING,
            window_padding_y: DEFAULT_WINDOW_PADDING,
            keybind: Vec::new(),
            theme: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::GhosttyConfig;

    #[test]
    fn default_scrollback_limit_is_ten_million_bytes() {
        assert_eq!(GhosttyConfig::default().scrollback_limit, 10_000_000);
    }
}
