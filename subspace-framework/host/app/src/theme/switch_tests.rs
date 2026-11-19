//! Every bundled theme meets the switch contrast rule after Subspace applies it.

use std::rc::Rc;

use gpui_kit::component::{Theme, ThemeConfig, ThemeSet};
use gpui_kit::{Background, Hsla, Rgba, TestAppContext};
use subspace_dashboard::contrast::{composite, ratio_over};
use subspace_dashboard::toggle::track::{self, SWITCH_CONTRAST, switch_colors, tracks_on_surfaces};

use super::preview_tests::init_app;
use super::{FontChoices, GPUI_KIT_THEMES, SUBSPACE_THEMES, apply_document, document, initialize};

/// GPUI Kit 0.9.1 fades a disabled track, on or off, to half its alpha. The
/// knob stays opaque.
const DISABLED_TRACK_OPACITY: f32 = 0.5;
/// A disabled switch is exempt from WCAG 1.4.11, but it stays identifiable.
const DISABLED_SWITCH_CONTRAST: f64 = 1.5;

/// One measured pair: what, the surface or track, its ratio and minimum.
type Pair = (&'static str, &'static str, f64, f64);
/// One pair below its minimum, with the theme name.
type Failure = (String, &'static str, &'static str, f64);

fn bundled_variants() -> Vec<ThemeConfig> {
    SUBSPACE_THEMES
        .iter()
        .chain(GPUI_KIT_THEMES)
        .flat_map(|(_, content)| {
            serde_json::from_str::<ThemeSet>(content)
                .expect("bundled themes parse")
                .themes
        })
        .collect()
}

fn applied(config: ThemeConfig) -> Theme {
    let mut theme = Theme::default();
    theme.apply_config(&Rc::new(config));
    track::apply(&mut theme);
    theme
}

fn faded(color: Hsla) -> Rgba {
    color.opacity(DISABLED_TRACK_OPACITY).into()
}

/// Enabled tracks against one surface, and the knob on each of them.
fn enabled_pairs(theme: &Theme, on: Hsla, surface: &'static str, under: Rgba) -> [Pair; 4] {
    let knob: Rgba = theme.switch_thumb.into();
    let (off_track, on_track) = (
        composite(theme.switch.into(), under),
        composite(on.into(), under),
    );
    [
        (
            "off track",
            surface,
            ratio_over(theme.switch.into(), under),
            SWITCH_CONTRAST,
        ),
        (
            "on track",
            surface,
            ratio_over(on.into(), under),
            SWITCH_CONTRAST,
        ),
        (
            "knob on off track",
            surface,
            ratio_over(knob, off_track),
            SWITCH_CONTRAST,
        ),
        (
            "knob on on track",
            surface,
            ratio_over(knob, on_track),
            SWITCH_CONTRAST,
        ),
    ]
}

/// Faded tracks against one surface, and the opaque knob on each of them.
fn disabled_pairs(theme: &Theme, on: Hsla, surface: &'static str, under: Rgba) -> [Pair; 4] {
    let knob: Rgba = theme.switch_thumb.into();
    let off_track = composite(faded(theme.switch), under);
    let on_track = composite(faded(on), under);
    [
        (
            "disabled off track",
            surface,
            ratio_over(off_track, under),
            DISABLED_SWITCH_CONTRAST,
        ),
        (
            "disabled on track",
            surface,
            ratio_over(on_track, under),
            DISABLED_SWITCH_CONTRAST,
        ),
        (
            "knob on disabled off track",
            surface,
            ratio_over(knob, off_track),
            DISABLED_SWITCH_CONTRAST,
        ),
        (
            "knob on disabled on track",
            surface,
            ratio_over(knob, on_track),
            DISABLED_SWITCH_CONTRAST,
        ),
    ]
}

/// Every pair a switch draws, on every surface.
fn measured(theme: &Theme) -> Vec<Pair> {
    let on = switch_colors(theme).on;
    track::surfaces(theme)
        .into_iter()
        .flat_map(|(surface, under)| {
            enabled_pairs(theme, on, surface, under)
                .into_iter()
                .chain(disabled_pairs(theme, on, surface, under))
        })
        .collect()
}

fn failures(config: ThemeConfig) -> Vec<Failure> {
    let name = config.name.to_string();
    measured(&applied(config))
        .into_iter()
        .filter(|(_, _, ratio, minimum)| ratio < minimum)
        .map(|(what, surface, ratio, _)| (name.clone(), what, surface, ratio))
        .collect()
}

#[test]
fn every_bundled_theme_meets_the_switch_contrast_rule() {
    let variants = bundled_variants();
    assert_eq!(variants.len(), 42);
    let failures: Vec<Failure> = variants.into_iter().flat_map(failures).collect();
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn the_rule_finds_the_faint_tracks_it_fixes() {
    let unfixed = bundled_variants()
        .into_iter()
        .filter(|config| {
            let mut theme = Theme::default();
            theme.apply_config(&Rc::new(config.clone()));
            measured(&theme)
                .iter()
                .any(|(what, _, ratio, minimum)| *what == "off track" && ratio < minimum)
        })
        .count();
    assert_eq!(unfixed, 39);
}

#[test]
fn a_theme_whose_switch_already_passes_keeps_its_colours() {
    let one_dark = bundled_variants()
        .into_iter()
        .find(|config| config.name.as_ref() == "Subspace One Dark")
        .unwrap();
    let mut theme = applied(one_dark);
    let unchanged = theme.clone();
    track::apply(&mut theme);
    assert_eq!(theme.switch, unchanged.switch);
    assert_eq!(theme.switch_thumb, unchanged.switch_thumb);
}

#[test]
fn gradient_switch_tokens_become_the_measured_solid_colours() {
    let aurora = bundled_variants()
        .into_iter()
        .find(|config| config.name.as_ref() == "Aurora Light")
        .unwrap();
    let theme = applied(aurora);
    assert_eq!(
        theme.tokens.switch.background,
        Background::from(theme.switch)
    );
    assert_eq!(
        theme.tokens.switch_thumb.background,
        Background::from(theme.switch_thumb)
    );
}

#[test]
fn the_knob_is_chosen_against_both_tracks_on_every_surface() {
    let theme = applied(bundled_variants().remove(0));
    let colors = switch_colors(&theme);
    let surfaces = track::surfaces(&theme).map(|(_, surface)| surface);
    let tracks = tracks_on_surfaces([colors.off, colors.on], &surfaces);
    assert_eq!(tracks.len(), 2 * surfaces.len());
    assert!(
        tracks
            .iter()
            .all(|track| ratio_over(colors.thumb.into(), *track) >= SWITCH_CONTRAST)
    );
}

#[gpui_kit::test]
fn applying_a_theme_document_fixes_its_switch_tokens(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    cx.update(|cx| {
        init_app(cx, root.path(), root.path().join("state.json"));
        initialize(cx);
        let one_dark = document("Subspace One Dark", cx).unwrap();
        apply_document(&one_dark, &FontChoices::default(), cx).unwrap();
        let theme = Theme::global(cx);
        let colors = switch_colors(theme);
        assert_eq!(theme.switch, colors.off);
        assert_eq!(theme.tokens.switch.color, colors.off);
        assert_eq!(theme.switch_thumb, colors.thumb);
        for (surface, background) in track::surfaces(theme) {
            let measured = ratio_over(theme.switch.into(), background);
            assert!(measured >= SWITCH_CONTRAST, "{surface}: {measured:.2}");
        }
    });
}
