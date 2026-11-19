//! A quiet glyph button's finishes. The theme's `--toolbar-button-rest` and
//! `--toolbar-button-hover` tokens choose flat or glass for toolbar buttons;
//! the status bar is always flat.

use gpui_kit::component::button::{Button, ButtonCustomVariant, ButtonVariants as _};
use gpui_kit::component::{Colorize as _, Selectable as _};
use gpui_kit::{App, Hsla, Styled as _, transparent_black};

use crate::Tokens;
use crate::glass::{CONTROL_PRESSED, CONTROL_REST, EDGE};
use crate::tokens::{ButtonFinish, ToolbarLook};

/// A flat feedback fill relative to its tone: pressed is stronger than
/// hover, and a latched button's fill deepens under the pointer.
const PRESSED: f32 = 1.6;
const ON_HOVER: f32 = 1.3;
/// A latched flat button's glyph: the accent mixed halfway with the strong
/// text colour, so it reads as the accent and stays legible on the fill.
const ON_GLYPH_MIX: f32 = 0.5;
/// GPUI Kit draws a custom variant's resting colour at a fifth of its alpha;
/// the resting glass capsule keeps that look.
const KIT_REST_MIX: f32 = 0.2;

/// What a quiet glyph button paints in each state.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Paint {
    /// The fill and edge at rest: transparent when flat, the glass capsule
    /// when glass, the `button-on` tone when latched under a flat pointer.
    pub rest: Hsla,
    pub edge: Hsla,
    /// The glass capsule's inset specular rim at rest.
    pub rims: bool,
    /// The fills under the pointer and while pressed.
    pub hover: Hsla,
    pub pressed: Hsla,
    /// The glyph and text at rest and under the pointer.
    pub glyph: Hsla,
    pub hover_glyph: Hsla,
    /// Latched under a glass pointer finish: the lit glass of the current
    /// navigation destination instead of the fills above.
    pub lit: bool,
}

/// The paint for `look`. A latched (`on`) button follows the pointer
/// finish: flat shows the `button-on` fill and an accent-tinted glyph, so
/// the state reads without hovering and not by colour alone; glass is lit
/// glass.
pub fn paint(look: ToolbarLook, on: bool, tokens: &Tokens, glyph: Hsla) -> Paint {
    let stronger = |tone: Hsla, factor: f32| tone.alpha(tone.a * factor);
    let clear = transparent_black();
    let flat_on = on && look.hover == ButtonFinish::Flat;
    let on_glyph = tokens.accent.mix_oklab(tokens.fg_strong, ON_GLYPH_MIX);
    let (rest, edge) = match (flat_on, look.rest) {
        (true, _) => (tokens.button_on, clear),
        (false, ButtonFinish::Flat) => (clear, clear),
        (false, ButtonFinish::Glass) => (
            tokens.glass.opacity(CONTROL_REST * KIT_REST_MIX),
            tokens.glass_rim.opacity(EDGE),
        ),
    };
    let (hover, pressed) = match (flat_on, look.hover) {
        (true, _) => (
            stronger(tokens.button_on, ON_HOVER),
            stronger(tokens.button_on, PRESSED),
        ),
        (false, ButtonFinish::Flat) => {
            (tokens.button_hover, stronger(tokens.button_hover, PRESSED))
        }
        (false, ButtonFinish::Glass) => {
            (tokens.glass, tokens.glass_shadow.opacity(CONTROL_PRESSED))
        }
    };
    Paint {
        rest,
        edge,
        rims: !flat_on && look.rest == ButtonFinish::Glass,
        hover,
        pressed,
        glyph: if flat_on { on_glyph } else { glyph },
        hover_glyph: if flat_on { on_glyph } else { tokens.fg_strong },
        lit: on && look.hover == ButtonFinish::Glass,
    }
}

/// Dress a native button in `paint`. The transparent hairline of a flat
/// finish keeps every button the same size and gives the keyboard focus ring
/// an edge to follow. A latched button keeps its accessible pressed state;
/// its look comes from the paint, not GPUI Kit's selected style, so it still
/// answers the pointer.
pub(super) fn dress(button: Button, paint: &Paint, tokens: &Tokens, cx: &App) -> Button {
    if paint.lit {
        return crate::glass::control(button, true, tokens, cx);
    }
    let variant = ButtonCustomVariant::new(cx)
        .foreground(paint.glyph)
        .hover(paint.hover)
        .active(paint.pressed)
        .shadow(false);
    let button = button
        .custom(variant)
        .selected(false)
        .bg(paint.rest)
        .border_1()
        .border_color(paint.edge);
    if paint.rims {
        button.shadow(crate::glass::rims(tokens))
    } else {
        button
    }
}
