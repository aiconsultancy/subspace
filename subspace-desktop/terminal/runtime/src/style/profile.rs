//! Validate a platform terminal profile against the text system.

use gpui_kit::{FontStyle, FontWeight, TextSystem, font};
use subspace_framework::dashboard::typography;

/// A platform terminal profile. Its face is validated by the text system.
#[derive(Clone, Debug, Default)]
pub struct ProfileFont {
    pub(crate) family: Option<String>,
    pub(crate) size: Option<f32>,
    pub(crate) weight: FontWeight,
    pub(crate) style: FontStyle,
}

impl ProfileFont {
    pub(super) fn validated(&self, installed: &[String], text: &TextSystem) -> Self {
        let Some(family) = self
            .family
            .as_deref()
            .and_then(|name| typography::installed_family(name, installed))
        else {
            return Self::default();
        };
        let face = gpui_kit::Font {
            weight: self.weight,
            style: self.style,
            ..font(family.clone())
        };
        if crate::font::fixed_width(text, &face) {
            Self {
                family: Some(family),
                ..self.clone()
            }
        } else {
            Self::default()
        }
    }
}
