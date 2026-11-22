//! Status pills and count badges.

use gpui_kit::{Context, Window};
use subspace_dashboard::badge::{self, Pill, PillTone};
use subspace_dashboard::{Tokens, Tone};

use crate::ComponentGallery;
use crate::samples::{Sample, sample};

const BADGE_TINT: f32 = 0.18;

pub(crate) fn pills(
    _: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    [
        ("Muted", "2 / 100 lines", PillTone::Muted),
        ("Warning", "80 / 100 lines", PillTone::Warning),
        ("Danger", "101 / 100 lines", PillTone::Danger),
    ]
    .into_iter()
    .map(|(state, text, tone)| {
        sample(
            state,
            Pill::new(format!("gallery-sample:pill:{state}"), text).tone(tone),
        )
    })
    .chain(
        [
            ("Hue: blue, a captain", "CAPTAIN", Tone::Blue),
            (
                "Hue: accent, the rear admiral",
                "REAR ADMIRAL",
                Tone::Accent,
            ),
        ]
        .into_iter()
        .map(|(state, text, hue)| {
            sample(
                state,
                Pill::new(format!("gallery-sample:pill:{state}"), text).hue(hue),
            )
        }),
    )
    .collect()
}

pub(in crate::samples) fn badges(
    _: &ComponentGallery,
    tokens: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    vec![
        sample("Count", badge::badge("12", tokens.select, tokens.fg)),
        sample(
            "Accent",
            badge::badge("New", tokens.accent.opacity(BADGE_TINT), tokens.accent),
        ),
        sample("Key hint", badge::badge("3", tokens.select, tokens.muted)),
    ]
}
