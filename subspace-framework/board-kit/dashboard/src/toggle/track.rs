//! The switch contrast rule: the outline of an off track, an on track and the
//! knob each keep at least 3:1 (WCAG 1.4.11 non-text contrast) against every
//! surface a switch sits on.
//!
//! GPUI Kit's `Switch` draws a transparent border, so the track's fill is its
//! outline. The off track reads the theme's `switch` token, the knob reads
//! `switch_thumb` and the on track reads `primary` unless the switch sets its
//! own colour. A colour that already passes stays unchanged. One that fails
//! moves in small steps towards the theme's text colour (tracks) or window
//! background (knob), then towards black or white, until it passes. When no
//! step passes, the step with the highest worst-case contrast wins.

use gpui_kit::component::{Colorize as _, Theme};
use gpui_kit::{Hsla, Rgba, black, white};

use crate::contrast::{composite, ratio_over};

/// WCAG 1.4.11 non-text contrast for a control's identifying outline.
pub const SWITCH_CONTRAST: f64 = 3.0;
/// Mix steps from the theme colour (0) to each target colour (all steps).
const MIX_STEPS: u16 = 40;

/// Off track, on track and knob colours that meet [`SWITCH_CONTRAST`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SwitchColors {
    pub off: Hsla,
    pub on: Hsla,
    pub thumb: Hsla,
}

/// The surfaces a switch can sit on, each composited over the window
/// background: window, panel and dialog, secondary panel, sidebar, popover,
/// selected row and active list row.
pub fn surfaces(theme: &Theme) -> [(&'static str, Rgba); 7] {
    let base: Rgba = theme.background.into();
    [
        ("background", theme.background),
        ("muted", theme.muted),
        ("secondary", theme.secondary),
        ("sidebar", theme.sidebar),
        ("popover", theme.popover),
        ("accent", theme.accent),
        ("list_active", theme.list_active),
    ]
    .map(|(name, color)| (name, composite(color.into(), base)))
}

/// Each track drawn on each surface: the backgrounds the knob sits on.
pub fn tracks_on_surfaces(tracks: [Hsla; 2], surfaces: &[Rgba]) -> Vec<Rgba> {
    tracks
        .iter()
        .flat_map(|track| {
            surfaces
                .iter()
                .map(move |surface| composite((*track).into(), *surface))
        })
        .collect()
}

/// The switch colours for `theme` under the contrast rule.
pub fn switch_colors(theme: &Theme) -> SwitchColors {
    let backgrounds = surfaces(theme).map(|(_, surface)| surface);
    let off = legible(theme.switch, theme.foreground, &backgrounds);
    let on = legible(theme.primary, theme.foreground, &backgrounds);
    let tracks = tracks_on_surfaces([off, on], &backgrounds);
    let thumb = legible(theme.switch_thumb, theme.background, &tracks);
    SwitchColors { off, on, thumb }
}

/// Writes the off track and knob colours into `theme` as solid tokens, so
/// every GPUI Kit switch draws exactly the measured colours. The on colour
/// needs [`super::switch`].
pub fn apply(theme: &mut Theme) {
    let colors = switch_colors(theme);
    write(theme, colors);
}

/// Solid tokens replace any gradient, whose stops the rule cannot measure.
fn write(theme: &mut Theme, colors: SwitchColors) {
    theme.switch = colors.off;
    theme.tokens.switch = colors.off.into();
    theme.switch_thumb = colors.thumb;
    theme.tokens.switch_thumb = colors.thumb.into();
}

/// `color` when it passes on every background; otherwise the first passing
/// step towards `target`, black or white; otherwise the best step.
fn legible(color: Hsla, target: Hsla, backgrounds: &[Rgba]) -> Hsla {
    let candidates = candidates(color, target);
    candidates
        .iter()
        .copied()
        .find(|candidate| worst(*candidate, backgrounds) >= SWITCH_CONTRAST)
        .unwrap_or_else(|| best(&candidates, backgrounds))
}

/// `color`, then steps from it towards `target`, black and white in turn.
fn candidates(color: Hsla, target: Hsla) -> Vec<Hsla> {
    std::iter::once(color)
        .chain([target, black(), white()].into_iter().flat_map(|goal| {
            (1..=MIX_STEPS).map(move |step| {
                color.mix_oklab(goal, 1.0 - f32::from(step) / f32::from(MIX_STEPS))
            })
        }))
        .collect()
}

/// The lowest contrast of `color` against any background.
fn worst(color: Hsla, backgrounds: &[Rgba]) -> f64 {
    backgrounds
        .iter()
        .map(|background| ratio_over(color.into(), *background))
        .fold(f64::INFINITY, f64::min)
}

/// The candidate with the highest worst-case contrast.
fn best(candidates: &[Hsla], backgrounds: &[Rgba]) -> Hsla {
    candidates
        .iter()
        .copied()
        .max_by(|a, b| worst(*a, backgrounds).total_cmp(&worst(*b, backgrounds)))
        .unwrap_or(black())
}

#[cfg(test)]
mod tests {
    use super::{SWITCH_CONTRAST, legible, worst};
    use gpui_kit::{Hsla, Rgba, black, rgb, white};

    fn solid(hex: u32) -> Hsla {
        rgb(hex).into()
    }

    #[test]
    fn a_passing_colour_is_kept_exactly() {
        let page: Rgba = white().into();
        let track = solid(0x555555);
        assert_eq!(legible(track, black(), &[page]), track);
    }

    #[test]
    fn a_faint_track_moves_until_it_passes() {
        let page: Rgba = solid(0x292929).into();
        let track = legible(page.into(), white(), &[page]);
        assert!(worst(track, &[page]) >= SWITCH_CONTRAST);
    }

    #[test]
    fn an_impossible_target_still_moves_to_black_or_white() {
        let page: Rgba = white().into();
        let track = legible(white(), white(), &[page]);
        assert!(worst(track, &[page]) >= SWITCH_CONTRAST);
    }
}
