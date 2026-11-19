//! Button roles and their geometry tokens: the `controls` density group.

use gpui_kit::{Pixels, Rems, px, rems};

use crate::density::{Density, REFERENCE_REM};

/// `--liquid-glass` is a 0/1 switch stored as a number, and a finish is the
/// index of `flat` (0) or `glass` (1).
const LIQUID_GLASS_ON: f32 = 0.5;

/// What a button is for, which selects its geometry tokens.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ButtonRole {
    /// Toolbar, header and chrome buttons, usually icon-only.
    #[default]
    Toolbar,
    /// Labelled actions in board content, such as Retry or Allow.
    Action,
    /// The commit and dismiss buttons in a dialog footer.
    Dialog,
    /// The deliberate dense preset: segments, status bar commands.
    Compact,
}

/// What a quiet glyph button draws in one state: the stored value of the
/// `--toolbar-button-rest` and `--toolbar-button-hover` choice tokens.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ButtonFinish {
    /// No capsule or rim; a theme fill only for feedback.
    #[default]
    Flat,
    /// A Liquid Glass capsule.
    Glass,
}

impl ButtonFinish {
    fn from_index(value: f32) -> Self {
        if value >= LIQUID_GLASS_ON {
            Self::Glass
        } else {
            Self::Flat
        }
    }
}

/// A quiet glyph toolbar button's finish at rest and under the pointer. The
/// pointer finish also draws the pressed and latched states.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ToolbarLook {
    pub rest: ButtonFinish,
    pub hover: ButtonFinish,
}

impl ToolbarLook {
    /// Flat in every state: the default, and always the status bar's look.
    pub const FLAT: Self = Self {
        rest: ButtonFinish::Flat,
        hover: ButtonFinish::Flat,
    };
}

/// One button role's geometry, in pixels at [`crate::density::REFERENCE_REM`].
/// Resolve them with [`rem`] so they follow the interface base.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ButtonMetrics {
    pub height: Pixels,
    pub pad_x: Pixels,
    pub font: Pixels,
    pub radius: Pixels,
}

impl ButtonMetrics {
    fn from_density(density: &Density, role: &str) -> Self {
        let size = |part: &str| px(density.get(&format!("--{role}-button-{part}")));
        Self {
            height: size("h"),
            pad_x: size("pad-x"),
            font: size("font"),
            radius: size("radius"),
        }
    }
}

/// A control token, in pixels at [`REFERENCE_REM`], as rems of the interface
/// base, so it follows the body text size like the rest of the interface.
pub fn rem(value: Pixels) -> Rems {
    rems(f32::from(value) / REFERENCE_REM)
}

/// The `controls` density group.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Controls {
    /// `--nav-h`: the least height of a horizontal board tab.
    pub nav_height: Pixels,
    pub toolbar_button: ButtonMetrics,
    pub action_button: ButtonMetrics,
    pub dialog_button: ButtonMetrics,
    pub compact_button: ButtonMetrics,
    /// `--dialog-pad` and `--dialog-gap`, in pixels at the reference rem.
    pub dialog_pad: Pixels,
    pub dialog_gap: Pixels,
    /// `--liquid-glass`: outlined toolbar buttons, such as menu pickers, are
    /// glass capsules and destructive actions are tinted rather than filled,
    /// as in macOS 26.
    pub liquid_glass: bool,
    /// `--toolbar-button-rest` and `--toolbar-button-hover`: quiet glyph
    /// toolbar buttons' finish. Flat unless a theme opts into glass.
    pub toolbar_look: ToolbarLook,
}

impl Default for Controls {
    /// The density group's defaults: every theme without the tokens.
    fn default() -> Self {
        Self::from_density(&Density::default())
    }
}

impl Controls {
    pub fn from_density(density: &Density) -> Self {
        let size = |token: &str| px(density.get(token));
        Self {
            nav_height: size("--nav-h"),
            toolbar_button: ButtonMetrics::from_density(density, "toolbar"),
            action_button: ButtonMetrics::from_density(density, "action"),
            dialog_button: ButtonMetrics::from_density(density, "dialog"),
            compact_button: ButtonMetrics::from_density(density, "compact"),
            dialog_pad: size("--dialog-pad"),
            dialog_gap: size("--dialog-gap"),
            liquid_glass: density.get("--liquid-glass") >= LIQUID_GLASS_ON,
            toolbar_look: ToolbarLook {
                rest: ButtonFinish::from_index(density.get("--toolbar-button-rest")),
                hover: ButtonFinish::from_index(density.get("--toolbar-button-hover")),
            },
        }
    }

    /// The geometry of a button with `role`.
    pub fn button(&self, role: ButtonRole) -> ButtonMetrics {
        match role {
            ButtonRole::Toolbar => self.toolbar_button,
            ButtonRole::Action => self.action_button,
            ButtonRole::Dialog => self.dialog_button,
            ButtonRole::Compact => self.compact_button,
        }
    }
}
