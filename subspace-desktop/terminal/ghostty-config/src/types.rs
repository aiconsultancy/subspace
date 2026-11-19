//! Typed values the terminal panel reads from Ghostty keys (REQ-31).

use std::collections::BTreeMap;
use std::path::PathBuf;

/// An RGB colour, as Ghostty's `#RRGGBB` (or bare `RRGGBB`) syntax.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    /// Parses `#RRGGBB` or `RRGGBB`. Named colours are not supported; see
    /// the crate's module documentation for this deviation from Ghostty.
    pub(crate) fn parse(input: &str) -> Option<Self> {
        const HEX_DIGITS: usize = 6;
        const RADIX: u32 = 16;
        let hex = input.strip_prefix('#').unwrap_or(input);
        if hex.len() != HEX_DIGITS || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        let r = u8::from_str_radix(&hex[0..2], RADIX).ok()?;
        let g = u8::from_str_radix(&hex[2..4], RADIX).ok()?;
        let b = u8::from_str_radix(&hex[4..6], RADIX).ok()?;
        Some(Self { r, g, b })
    }
}

/// `cursor-style` (REQ-15). The default is `Block`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CursorStyle {
    #[default]
    Block,
    Bar,
    Underline,
    BlockHollow,
}

impl CursorStyle {
    pub(crate) fn parse(input: &str) -> Option<Self> {
        match input {
            "block" => Some(Self::Block),
            "bar" => Some(Self::Bar),
            "underline" => Some(Self::Underline),
            "block_hollow" => Some(Self::BlockHollow),
            _ => None,
        }
    }
}

/// `clipboard-read` and `clipboard-write` (REQ-24).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardAccess {
    Allow,
    Deny,
    Ask,
}

impl ClipboardAccess {
    pub(crate) fn parse(input: &str) -> Option<Self> {
        match input {
            "allow" => Some(Self::Allow),
            "deny" => Some(Self::Deny),
            "ask" => Some(Self::Ask),
            _ => None,
        }
    }
}

/// `copy-on-select` (REQ-23). `true` resolves to the platform default
/// (`Primary` on Linux, `Clipboard` on macOS) at parse time, matching
/// Ghostty. Windows, which Ghostty does not ship on, follows macOS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopyOnSelect {
    None,
    Primary,
    Clipboard,
    Both,
}

impl CopyOnSelect {
    pub(crate) fn parse(input: &str) -> Option<Self> {
        match input {
            "none" | "false" => Some(Self::None),
            "primary" => Some(Self::Primary),
            "clipboard" => Some(Self::Clipboard),
            "both" => Some(Self::Both),
            "true" => Some(Self::platform_true()),
            _ => None,
        }
    }

    /// macOS and Windows have no primary selection, so `true` means the
    /// system clipboard there.
    #[cfg(any(target_os = "macos", windows))]
    fn platform_true() -> Self {
        Self::Clipboard
    }

    #[cfg(not(any(target_os = "macos", windows)))]
    fn platform_true() -> Self {
        Self::Primary
    }

    /// The platform default when the key is not set (REQ-23). Platforms
    /// without a primary selection copy nothing on select by default.
    #[cfg(any(target_os = "macos", windows))]
    pub fn platform_default() -> Self {
        Self::None
    }

    /// The platform default when the key is not set (REQ-23).
    #[cfg(not(any(target_os = "macos", windows)))]
    pub fn platform_default() -> Self {
        Self::Primary
    }
}

/// `confirm-close-surface` (REQ-10). `None` means the key is unset, so the
/// board's own busy-tab rule applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmCloseSurface {
    Never,
    Always,
}

impl ConfirmCloseSurface {
    pub(crate) fn parse(input: &str) -> Option<Self> {
        match input {
            "false" => Some(Self::Never),
            "true" | "always" => Some(Self::Always),
            _ => None,
        }
    }
}

/// `working-directory` (REQ-08).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkingDirectory {
    Home,
    Path(PathBuf),
}

impl WorkingDirectory {
    pub(crate) fn parse(input: &str) -> Option<Self> {
        if input.is_empty() {
            return None;
        }
        if input == "home" {
            Some(Self::Home)
        } else {
            Some(Self::Path(PathBuf::from(input)))
        }
    }
}

/// `adjust-cell-height` (REQ-14): a percentage delta such as `10%` or
/// `-5%`, or an absolute point delta.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AdjustCellHeight {
    Percent(f32),
    Points(f32),
}

impl AdjustCellHeight {
    pub(crate) fn parse(input: &str) -> Option<Self> {
        if let Some(percent) = input.strip_suffix('%') {
            percent.trim().parse::<f32>().ok().map(Self::Percent)
        } else {
            input.trim().parse::<f32>().ok().map(Self::Points)
        }
    }
}

/// The active Subspace appearance, used to pick a `light:`/`dark:` theme
/// pair (REQ-30). This crate has no `gpui-kit` dependency, so the caller
/// passes this in rather than reading a GPUI theme global.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Appearance {
    Light,
    Dark,
}

/// The directories searched for a named theme, in REQ-30 priority order.
#[derive(Debug, Clone)]
pub struct ThemeSearchPaths {
    pub subspace_layer_themes: PathBuf,
    pub xdg_themes: PathBuf,
    pub resources_themes: Option<PathBuf>,
}

/// The colour keys a theme (and the configuration on top of it) can set
/// (REQ-12, REQ-30).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ThemeValues {
    pub background: Option<Rgb>,
    pub foreground: Option<Rgb>,
    pub palette: BTreeMap<u8, Rgb>,
    pub cursor_color: Option<Rgb>,
    pub cursor_text: Option<Rgb>,
    pub selection_background: Option<Rgb>,
    pub selection_foreground: Option<Rgb>,
}

#[cfg(test)]
mod tests {
    use super::{
        AdjustCellHeight, ClipboardAccess, ConfirmCloseSurface, CopyOnSelect, CursorStyle, Rgb,
        WorkingDirectory,
    };
    use std::path::PathBuf;

    #[test]
    fn rgb_parses_hash_prefixed_hex() {
        assert_eq!(
            Rgb::parse("#112233"),
            Some(Rgb {
                r: 0x11,
                g: 0x22,
                b: 0x33
            })
        );
    }

    #[test]
    fn rgb_parses_bare_hex() {
        assert_eq!(
            Rgb::parse("abcdef"),
            Some(Rgb {
                r: 0xab,
                g: 0xcd,
                b: 0xef
            })
        );
    }

    #[test]
    fn rgb_rejects_invalid_hex() {
        assert_eq!(Rgb::parse("not-a-color"), None);
    }

    #[test]
    fn cursor_style_rejects_unknown_values() {
        assert_eq!(CursorStyle::parse("triangle"), None);
    }

    #[test]
    fn clipboard_access_parses_known_values() {
        assert_eq!(
            ClipboardAccess::parse("allow"),
            Some(ClipboardAccess::Allow)
        );
        assert_eq!(ClipboardAccess::parse("maybe"), None);
    }

    #[test]
    fn copy_on_select_resolves_true_to_a_concrete_value() {
        let resolved = CopyOnSelect::parse("true").unwrap();
        assert_ne!(resolved, CopyOnSelect::None);
    }

    #[test]
    fn confirm_close_surface_parses_false_and_always() {
        assert_eq!(
            ConfirmCloseSurface::parse("false"),
            Some(ConfirmCloseSurface::Never)
        );
        assert_eq!(
            ConfirmCloseSurface::parse("always"),
            Some(ConfirmCloseSurface::Always)
        );
    }

    #[test]
    fn working_directory_recognises_home() {
        assert_eq!(
            WorkingDirectory::parse("home"),
            Some(WorkingDirectory::Home)
        );
        assert_eq!(
            WorkingDirectory::parse("~/code"),
            Some(WorkingDirectory::Path(PathBuf::from("~/code")))
        );
    }

    #[test]
    fn adjust_cell_height_parses_a_percentage() {
        assert_eq!(
            AdjustCellHeight::parse("10%"),
            Some(AdjustCellHeight::Percent(10.0))
        );
        assert_eq!(
            AdjustCellHeight::parse("3"),
            Some(AdjustCellHeight::Points(3.0))
        );
    }
}
