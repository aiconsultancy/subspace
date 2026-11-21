//! Liquid Glass navigation capsules. The shell's horizontal board tabs and the
//! vertical board rail draw each destination as a capsule; the current one is
//! a glass lozenge (a translucent fill brighter at the top, a specular rim and
//! a soft lifting shadow), the others are bare until hovered. Themes that set
//! `--liquid-glass` also put outlined toolbar buttons on glass with
//! [`control`], and quiet glyph buttons take glass for the finishes the
//! `--toolbar-button-rest` and `--toolbar-button-hover` tokens choose.
//!
//! GPUI paints no backdrop blur or refraction inside a window, so the glass is
//! translucency over the title bar or sidebar, not a lens.

use gpui_kit::component::button::{Button, ButtonCustomVariant, ButtonVariants as _};
use gpui_kit::component::{Selectable as _, Sizable as _};
use gpui_kit::{
    App, Background, BoxShadow, Div, ElementId, Hsla, IntoElement, ParentElement as _, Pixels,
    Styled as _, div, linear_color_stop, linear_gradient, px,
};

use crate::Tokens;

/// The fill's top and bottom alpha, relative to the `glass` tone: light
/// gathers at the top of the capsule.
const SHEEN_TOP: f32 = 1.5;
const SHEEN_BOTTOM: f32 = 0.85;
/// Fills for an unselected capsule under the pointer and while pressed.
const HOVER: f32 = 0.55;
const PRESSED: f32 = 0.8;
/// The hairline around the capsule and the fainter lower rim, relative to
/// the `glass-rim` tone. The upper rim takes the tone at full strength.
pub(crate) const EDGE: f32 = 0.45;
const LOWER_RIM: f32 = 0.3;
/// A resting glass control's fill and upper rim, and the shade while
/// pressed, relative to the `glass`, `glass-rim` and `glass-shadow` tones.
pub(crate) const CONTROL_REST: f32 = 0.7;
const CONTROL_RIM: f32 = 0.6;
pub(crate) const CONTROL_PRESSED: f32 = 0.3;
/// Physical shadow geometry: a one-pixel lift with a short falloff that stays
/// inside [`SHADOW_BLEED`].
const LIFT_Y: f32 = 1.;
const LIFT_BLUR: f32 = 3.;
const RIM: f32 = 1.;
/// Room the lifting shadow needs below and beside a capsule. Rows that clip
/// their content, such as an overflow toolbar, wrap each capsule with
/// [`bleed`].
pub const SHADOW_BLEED: f32 = LIFT_Y + LIFT_BLUR;
const SIDE_BLEED: f32 = LIFT_BLUR;
/// Gradient direction: top to bottom.
const DOWNWARD: f32 = 180.;
const HALF: f32 = 0.5;
/// The current destination in a theme without Liquid Glass: a faint accent
/// fill inside an accent ring, relative to the accent.
const ACCENT_FILL: f32 = 0.07;
const ACCENT_RING: f32 = 0.45;
/// `--nav-round` limits: square corners and a full capsule.
const SQUARE: f32 = 0.;
const CAPSULE: f32 = 1.;

/// What the current destination's capsule paints.
#[derive(Debug, Clone, PartialEq)]
pub struct Material {
    pub fill: Background,
    pub edge: Hsla,
    pub shadows: Vec<BoxShadow>,
}

/// The capsule's corner radius for an item `height` tall.
pub fn capsule_radius(height: Pixels, tokens: &Tokens) -> Pixels {
    height * HALF * tokens.nav_round.clamp(SQUARE, CAPSULE)
}

/// The glass of the current destination; `None` for every other one.
pub fn material(selected: bool, tokens: &Tokens) -> Option<Material> {
    selected.then(|| Material {
        fill: linear_gradient(
            DOWNWARD,
            linear_color_stop(tokens.glass.opacity(SHEEN_TOP), 0.),
            linear_color_stop(tokens.glass.opacity(SHEEN_BOTTOM), 1.),
        ),
        edge: tokens.glass_rim.opacity(EDGE),
        shadows: vec![
            BoxShadow::new(px(0.), px(LIFT_Y), tokens.glass_shadow).blur_radius(px(LIFT_BLUR)),
            BoxShadow::new(px(0.), px(RIM), tokens.glass_rim).inset(),
            BoxShadow::new(px(0.), px(-RIM), tokens.glass_rim.opacity(LOWER_RIM)).inset(),
        ],
    })
}

/// The current destination's surface in a theme without Liquid Glass: a
/// faint accent fill inside a one-pixel accent ring, with no shadow; `None`
/// for every other one.
pub fn accent_material(selected: bool, tokens: &Tokens) -> Option<Material> {
    selected.then(|| Material {
        fill: tokens.accent.opacity(ACCENT_FILL).into(),
        edge: tokens.accent.opacity(ACCENT_RING),
        shadows: Vec::new(),
    })
}

/// Label and glyph colour: the current destination reads stronger, in the
/// accent where the theme has no Liquid Glass.
pub fn label_color(selected: bool, tokens: &Tokens) -> Hsla {
    match (selected, tokens.controls.liquid_glass) {
        (true, true) => tokens.fg_strong,
        (true, false) => tokens.accent,
        (false, _) => tokens.fg,
    }
}

/// One navigation destination: a capsule `height` tall that is glass when
/// `selected` and the theme sets `--liquid-glass`, and otherwise a faint
/// accent fill in an accent ring. Callers add the glyph, label, accessible
/// label and click.
pub fn nav_button(
    id: impl Into<ElementId>,
    selected: bool,
    height: Pixels,
    tokens: &Tokens,
    cx: &App,
) -> Button {
    // Like a segmented choice, assistive technology hears the current
    // destination as pressed, not only sees the glass.
    let button = capsule(id, height, tokens, cx)
        .selected(selected)
        .toggled(selected);
    let color = label_color(selected, tokens);
    let surface = if tokens.controls.liquid_glass {
        material(selected, tokens)
    } else {
        accent_material(selected, tokens)
    };
    dress(button, surface).text_color(color)
}

/// The bare capsule: no fill at rest, a faint glass fill on hover and press.
/// The transparent hairline keeps every capsule the same size and gives the
/// keyboard focus ring a border to tint along the capsule.
fn capsule(id: impl Into<ElementId>, height: Pixels, tokens: &Tokens, cx: &App) -> Button {
    let variant = ButtonCustomVariant::new(cx)
        .color(gpui_kit::transparent_black())
        .foreground(tokens.fg)
        .hover(tokens.glass.opacity(HOVER))
        .active(tokens.glass.opacity(PRESSED))
        .shadow(false);
    Button::new(id)
        .custom(variant)
        .small()
        .rounded(capsule_radius(height, tokens))
        .h(height)
        .border_1()
        .border_color(gpui_kit::transparent_black())
}

/// A toolbar button on its own piece of glass, as macOS 26 draws toolbar
/// items: a resting translucent fill with the specular rim, brighter under
/// the pointer and shaded while pressed. A latched or selected control is the
/// lit glass of the current navigation capsule. The caller's radius and
/// geometry are kept; only the surface changes.
pub fn control(button: Button, selected: bool, tokens: &Tokens, cx: &App) -> Button {
    let variant = ButtonCustomVariant::new(cx)
        .color(tokens.glass.opacity(CONTROL_REST))
        .foreground(tokens.fg)
        .hover(tokens.glass)
        .active(tokens.glass_shadow.opacity(CONTROL_PRESSED))
        .shadow(false);
    let button = button
        .custom(variant)
        .border_1()
        .border_color(tokens.glass_rim.opacity(EDGE));
    match material(selected, tokens) {
        Some(glass) => dress(button, Some(glass)),
        None => button.shadow(rims(tokens)),
    }
}

/// The inset specular rim, stronger at the top, without the lifting shadow.
pub(crate) fn rims(tokens: &Tokens) -> Vec<BoxShadow> {
    vec![
        BoxShadow::new(px(0.), px(RIM), tokens.glass_rim.opacity(CONTROL_RIM)).inset(),
        BoxShadow::new(px(0.), px(-RIM), tokens.glass_rim.opacity(LOWER_RIM)).inset(),
    ]
}

/// Clear space around a capsule for its shadow inside a clipping row.
pub fn bleed(capsule: impl IntoElement) -> Div {
    div()
        .flex_shrink_0()
        .py(px(SHADOW_BLEED))
        .px(px(SIDE_BLEED))
        .child(capsule)
}

fn dress(button: Button, material: Option<Material>) -> Button {
    match material {
        Some(glass) => button
            .bg(glass.fill)
            .border_color(glass.edge)
            .shadow(glass.shadows),
        None => button,
    }
}

#[cfg(test)]
mod tests {
    use gpui_kit::px;

    use super::{accent_material, capsule_radius, label_color, material};
    use crate::Tokens;

    #[test]
    fn only_the_current_destination_is_glass() {
        let tokens = Tokens::one_dark();
        assert!(material(false, &tokens).is_none());
        let glass = material(true, &tokens).expect("the current item is glass");
        assert!(glass.edge.a > 0. && glass.edge.a < tokens.glass_rim.a);
        assert_eq!(
            glass.shadows.iter().filter(|shadow| shadow.inset).count(),
            2
        );
        let upper = glass
            .shadows
            .iter()
            .find(|shadow| shadow.inset && shadow.offset.y > px(0.))
            .expect("an upper rim");
        let lower = glass
            .shadows
            .iter()
            .find(|shadow| shadow.inset && shadow.offset.y < px(0.))
            .expect("a lower rim");
        assert!(
            upper.color.a > lower.color.a,
            "the rim is stronger at the top"
        );
        assert_ne!(label_color(true, &tokens), label_color(false, &tokens));
    }

    #[test]
    fn without_liquid_glass_the_current_destination_takes_the_accent() {
        let mut tokens = Tokens::one_dark();
        tokens.controls.liquid_glass = false;
        assert!(accent_material(false, &tokens).is_none());
        let current = accent_material(true, &tokens).expect("the current item is marked");
        assert!(
            current.edge.a > 0. && current.edge.a < 1.,
            "a faint accent ring"
        );
        assert!(current.shadows.is_empty(), "no glass shadow");
        assert_eq!(label_color(true, &tokens), tokens.accent);
        tokens.controls.liquid_glass = true;
        assert_eq!(label_color(true, &tokens), tokens.fg_strong);
    }

    #[test]
    fn the_capsule_is_round_ended_unless_the_theme_squares_it() {
        let mut tokens = Tokens::one_dark();
        assert_eq!(capsule_radius(px(28.), &tokens), px(14.));
        tokens.nav_round = 0.5;
        assert_eq!(capsule_radius(px(28.), &tokens), px(7.));
        tokens.nav_round = 3.;
        assert_eq!(capsule_radius(px(28.), &tokens), px(14.));
    }
}
