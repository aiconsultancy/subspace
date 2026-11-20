//! Lucide icons that GPUI Kit controls accept wherever they take `Into<Icon>`:
//!
//! ```ignore
//! use subspace_lucide::IconName;
//! Button::new("send").icon(IconName::ArrowUp);
//! ```
//!
//! Each constant references only its own SVG, so the linker keeps just the icons
//! the application uses. No asset source registration is needed. Ported from
//! gupi's `gpui-lucide` (Apache-2.0,
//! https://github.com/suxiaoshao/gupi/tree/9fb1bf1a298e66425b691d74df6ec4a5cbb9c19e/crates/gpui-lucide).
use gpui_kit::component::Icon;
use gpui_kit::{App, Entity, IntoElement, RenderOnce, Window};

/// Static SVG bytes usable as a GPUI Kit icon.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, IntoElement)]
pub struct SvgIcon {
    bytes: &'static [u8],
}

/// The Lucide catalog bundled with GPUI Kit, as associated constants.
pub type IconName = SvgIcon;

impl SvgIcon {
    /// A custom SVG, such as `SvgIcon::new(include_bytes!("logo.svg"))`.
    pub const fn new(bytes: &'static [u8]) -> Self {
        Self { bytes }
    }

    pub const fn bytes(self) -> &'static [u8] {
        self.bytes
    }

    // qual:api (renders the icon as its own entity for callers that need one)
    pub fn view(self, cx: &mut App) -> Entity<Icon> {
        Icon::from(self).view(cx)
    }
}

impl From<SvgIcon> for Icon {
    fn from(icon: SvgIcon) -> Self {
        Self::default().data(icon.bytes)
    }
}

impl RenderOnce for SvgIcon {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        Icon::from(self)
    }
}

include!(concat!(env!("OUT_DIR"), "/icons.rs"));

#[cfg(test)]
mod tests {
    use super::{IconName, SvgIcon};
    use gpui_kit::component::Icon;
    use gpui_kit::component::button::Button;

    #[test]
    fn catalog_icons_are_distinct_svgs_that_controls_accept() {
        let icons = [IconName::ArrowUp, IconName::Cpu];
        for icon in icons {
            assert!(icon.bytes().starts_with(b"<svg"));
            let _: Icon = icon.into();
            let _ = Button::new("icon-test").icon(icon);
        }
        assert_ne!(icons[0].bytes(), icons[1].bytes());
        let custom = SvgIcon::new(b"<svg xmlns=\"http://www.w3.org/2000/svg\"/>");
        assert_eq!(
            custom.bytes(),
            b"<svg xmlns=\"http://www.w3.org/2000/svg\"/>"
        );
    }
}
