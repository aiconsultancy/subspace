//! Quiet glyph buttons' finishes: flat or glass at rest and under the
//! pointer, chosen per theme; the status bar always flat.

use super::super::{Style, Surface, paint};
use super::IconButton;
use crate::action::{Intent, action_button};
use crate::density::Density;
use crate::tokens::{ButtonFinish, ButtonRole, Controls, Tokens, ToolbarLook};
use gpui_kit::component::button::ButtonVariant;
use gpui_kit::{Hsla, rgb};
use subspace_lucide::IconName;

const GLASS: ToolbarLook = ToolbarLook {
    rest: ButtonFinish::Glass,
    hover: ButtonFinish::Glass,
};

fn controls(json: &str) -> Controls {
    Controls::from_density(&Density::from_json(json))
}

fn glyph() -> Hsla {
    Hsla::from(rgb(0x808080))
}

fn surface(style: (bool, ToolbarLook, bool), role: ButtonRole, variant: ButtonVariant) -> Surface {
    let (glass, look, flat) = style;
    Style {
        glass,
        look,
        flat,
        role,
        variant,
        outline: false,
        has_icon: true,
    }
    .surface()
}

#[test]
fn every_theme_starts_flat_and_a_theme_opts_into_glass_per_state() {
    assert_eq!(Controls::default().toolbar_look, ToolbarLook::FLAT);
    let macos = controls(r#"{"--liquid-glass":"1"}"#);
    assert!(macos.liquid_glass);
    assert_eq!(macos.toolbar_look, ToolbarLook::FLAT, "glass is opt-in");
    let hover = controls(r#"{"--toolbar-button-hover":"glass"}"#).toolbar_look;
    assert_eq!(hover.rest, ButtonFinish::Flat);
    assert_eq!(hover.hover, ButtonFinish::Glass);
    let both = controls(r#"{"--toolbar-button-rest":"glass","--toolbar-button-hover":"glass"}"#);
    assert_eq!(both.toolbar_look, GLASS);
}

#[test]
fn quiet_toolbar_glyph_buttons_follow_the_theme_look_and_flat_ones_never_do() {
    let toolbar = ButtonRole::Toolbar;
    let ghost = ButtonVariant::Ghost;
    assert_eq!(
        surface((false, GLASS, false), toolbar, ghost),
        Surface::Quiet(GLASS),
        "the look is independent of Liquid glass"
    );
    assert_eq!(
        surface((true, ToolbarLook::FLAT, false), toolbar, ghost),
        Surface::Quiet(ToolbarLook::FLAT)
    );
    for role in [ButtonRole::Compact, ButtonRole::Toolbar] {
        assert_eq!(
            surface((true, GLASS, true), role, ghost),
            Surface::Quiet(ToolbarLook::FLAT),
            "{role:?}: a flat button ignores the theme"
        );
    }
    assert_eq!(
        surface((true, GLASS, false), ButtonRole::Compact, ghost),
        Surface::Native
    );
    assert_eq!(
        surface((true, GLASS, false), ButtonRole::Action, ghost),
        Surface::Native
    );
    assert_eq!(
        surface((true, GLASS, false), toolbar, ButtonVariant::Primary),
        Surface::Native
    );
    assert_eq!(
        surface(
            (true, GLASS, false),
            ButtonRole::Dialog,
            ButtonVariant::Danger
        ),
        Surface::Tinted
    );
    assert_eq!(
        surface(
            (false, GLASS, false),
            ButtonRole::Dialog,
            ButtonVariant::Danger
        ),
        Surface::Native
    );
    let text_only = Style {
        glass: true,
        look: GLASS,
        flat: false,
        role: toolbar,
        variant: ghost,
        outline: false,
        has_icon: false,
    };
    assert_eq!(text_only.surface(), Surface::Native);
}

#[test]
fn outlined_toolbar_buttons_keep_their_resting_glass_only_with_liquid_glass() {
    let outlined = |glass| Style {
        glass,
        look: ToolbarLook::FLAT,
        flat: false,
        role: ButtonRole::Toolbar,
        variant: ButtonVariant::Default,
        outline: true,
        has_icon: true,
    };
    assert_eq!(outlined(true).surface(), Surface::Glass);
    assert_eq!(outlined(false).surface(), Surface::Native);
}

#[test]
fn flat_rests_bare_and_answers_the_pointer_with_theme_fills() {
    let tokens = Tokens::one_dark();
    let off = paint(ToolbarLook::FLAT, false, &tokens, glyph());
    assert_eq!(off.rest.a, 0., "no fill at rest");
    assert_eq!(off.edge.a, 0., "no rim or outline at rest");
    assert!(!off.rims && !off.lit);
    assert_eq!(off.hover, tokens.button_hover);
    assert!(
        off.pressed.a > off.hover.a,
        "pressed is stronger than hover"
    );
    assert_eq!(off.glyph, glyph());
    assert_eq!(off.hover_glyph, tokens.fg_strong);
}

#[test]
fn flat_latched_buttons_keep_a_fill_and_an_accent_glyph() {
    let tokens = Tokens::one_dark();
    let off = paint(ToolbarLook::FLAT, false, &tokens, glyph());
    let on = paint(ToolbarLook::FLAT, true, &tokens, glyph());
    assert_eq!(on.rest, tokens.button_on, "visible without hovering");
    assert!(on.rest.a > off.hover.a, "distinct from hover");
    assert_eq!(on.edge.a, 0.);
    assert!(!on.rims && !on.lit);
    assert!(on.hover.a > on.rest.a, "deepens under the pointer");
    assert!(on.pressed.a > on.hover.a);
    assert_ne!(on.glyph, glyph(), "a second cue beside the fill");
    assert_ne!(on.glyph, tokens.accent, "tinted toward the strong text");
    assert_eq!(on.hover_glyph, on.glyph);
}

#[test]
fn glass_finishes_keep_their_glass_equivalents() {
    let tokens = Tokens::one_dark();
    let off = paint(GLASS, false, &tokens, glyph());
    assert!(off.rims && off.rest.a > 0. && off.edge.a > 0.);
    assert_eq!(off.hover, tokens.glass);
    assert!(paint(GLASS, true, &tokens, glyph()).lit);

    let glass_hover = ToolbarLook {
        rest: ButtonFinish::Flat,
        hover: ButtonFinish::Glass,
    };
    let bare = paint(glass_hover, false, &tokens, glyph());
    assert_eq!((bare.rest.a, bare.edge.a, bare.rims), (0., 0., false));
    assert_eq!(bare.hover, tokens.glass);
    assert!(paint(glass_hover, true, &tokens, glyph()).lit);

    let glass_rest = ToolbarLook {
        rest: ButtonFinish::Glass,
        hover: ButtonFinish::Flat,
    };
    let capsule = paint(glass_rest, false, &tokens, glyph());
    assert!(capsule.rims);
    assert_eq!(capsule.hover, tokens.button_hover);
    let on = paint(glass_rest, true, &tokens, glyph());
    assert!(!on.lit && !on.rims);
    assert_eq!(on.rest, tokens.button_on);
}

#[test]
fn a_flat_command_is_flat_even_when_the_theme_chooses_glass() {
    let glass = controls(
        r#"{"--liquid-glass":"1","--toolbar-button-rest":"glass","--toolbar-button-hover":"glass"}"#,
    );
    let command = action_button("status", Intent::Quiet, Some(IconName::Info), "About")
        .compact()
        .flat();
    assert_eq!(command.surface(&glass), Surface::Quiet(ToolbarLook::FLAT));
    let toolbar = IconButton::new("toolbar").icon(IconName::Search);
    assert_eq!(toolbar.surface(&glass), Surface::Quiet(GLASS));
}
