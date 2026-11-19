//! The finite application icon-button scale. Text retains its native size.

use gpui_kit::{App, Global, Pixels, Rems, rems};

use crate::density::REFERENCE_REM;

/// A finite relative scale for application glyphs, targets and internal spacing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum IconButtonSize {
    #[default]
    Small,
    Medium,
    Large,
    ExtraLarge,
}

impl Global for IconButtonSize {}

struct SizeTokens {
    key: &'static str,
    label: &'static str,
    glyph: f32,
    target: f32,
    gap: f32,
    row_height: f32,
    overflow_width: f32,
    label_padding: f32,
}

const TOKENS: [SizeTokens; 4] = [
    SizeTokens {
        key: "small",
        label: "Small",
        glyph: 0.875,
        target: 1.5,
        gap: 0.25,
        row_height: 1.875,
        overflow_width: 1.75,
        label_padding: 0.5,
    },
    SizeTokens {
        key: "medium",
        label: "Medium",
        glyph: 1.25,
        target: 2.25,
        gap: 0.375,
        row_height: 2.8125,
        overflow_width: 2.625,
        label_padding: 0.75,
    },
    SizeTokens {
        key: "large",
        label: "Large",
        glyph: 1.75,
        target: 3.,
        gap: 0.5,
        row_height: 3.75,
        overflow_width: 3.5,
        label_padding: 1.,
    },
    SizeTokens {
        key: "extra-large",
        label: "Extra large",
        glyph: 2.5,
        target: 4.5,
        gap: 0.75,
        row_height: 5.625,
        overflow_width: 5.25,
        label_padding: 1.5,
    },
];

impl IconButtonSize {
    pub fn current(cx: &App) -> Self {
        cx.try_global::<Self>().copied().unwrap_or_default()
    }

    pub fn apply(self, cx: &mut App) {
        cx.set_global(self);
        cx.refresh_windows();
    }
    pub const ALL: [Self; 4] = [Self::Small, Self::Medium, Self::Large, Self::ExtraLarge];

    pub fn key(self) -> &'static str {
        TOKENS[self as usize].key
    }

    pub fn label(self) -> &'static str {
        TOKENS[self as usize].label
    }

    pub fn from_key(value: Option<&str>) -> Self {
        Self::ALL
            .into_iter()
            .find(|size| Some(TOKENS[*size as usize].key) == value)
            .unwrap_or_default()
    }

    /// This size at the default toolbar button height.
    pub fn metrics(self) -> Metrics {
        let (height, _) = Self::reference();
        Metrics::new(self, Pixels::from(height))
    }

    /// The nominal geometry of this size, in rems at the default role height.
    pub(super) fn nominal(self) -> Nominal {
        let tokens = &TOKENS[self as usize];
        Nominal {
            glyph: tokens.glyph,
            target: tokens.target,
            gap: tokens.gap,
            pad: tokens.label_padding,
        }
    }

    /// The Small toolbar target and label padding in token pixels: the role
    /// heights and paddings that leave every size at its nominal geometry.
    pub(super) fn reference() -> (f32, f32) {
        let small = &TOKENS[Self::Small as usize];
        (
            small.target * REFERENCE_REM,
            small.label_padding * REFERENCE_REM,
        )
    }
}

/// One size's glyph, target, gap and label padding, in rems.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Nominal {
    pub(super) glyph: f32,
    pub(super) target: f32,
    pub(super) gap: f32,
    pub(super) pad: f32,
}

/// The global icon-button size scaled to the theme's toolbar button height,
/// for layouts that reserve room for toolbar buttons: rows, gaps and the
/// overflow chevron.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Metrics {
    size: IconButtonSize,
    scale: f32,
}

impl Metrics {
    pub fn current(cx: &App) -> Self {
        let height = crate::Tokens::current(cx).controls.toolbar_button.height;
        Self::new(IconButtonSize::current(cx), height)
    }

    /// `height` is the toolbar role's height token in pixels.
    pub fn new(size: IconButtonSize, height: Pixels) -> Self {
        let (reference, _) = IconButtonSize::reference();
        Self {
            size,
            scale: f32::from(height) / reference,
        }
    }

    pub fn size(self) -> IconButtonSize {
        self.size
    }

    pub fn glyph(self) -> Rems {
        rems(TOKENS[self.size as usize].glyph * self.scale)
    }

    pub fn target(self) -> Rems {
        rems(TOKENS[self.size as usize].target * self.scale)
    }

    pub fn gap(self) -> Rems {
        rems(TOKENS[self.size as usize].gap * self.scale)
    }

    pub fn row_height(self) -> Rems {
        rems(TOKENS[self.size as usize].row_height * self.scale)
    }

    pub fn overflow_width(self) -> Rems {
        rems(TOKENS[self.size as usize].overflow_width * self.scale)
    }
}
