//! Live reference-card treatments use the same constructors as Channels.
use crate::{
    ComponentGallery,
    samples::{Sample, sample},
};
use gpui_kit::{Context, ParentElement as _, Styled as _, Window};
use subspace_dashboard::Tokens;
use subspace_lucide::IconName;
pub(in crate::samples) fn reference_cards(
    _: &ComponentGallery,
    tokens: &Tokens,
    _: &mut Window,
    cx: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    use crate::samples::ignore_click;
    use subspace_dashboard::{
        Tone,
        action::card_button,
        reference_card::{self, Treatment},
    };
    [
        ("Outline", Treatment::Outline, false),
        ("State edge", Treatment::Accent(Tone::Yellow), true),
        ("Chip", Treatment::Chip, false),
    ]
    .into_iter()
    .map(|(name, treatment, agent)| {
        let button = card_button(
            gpui_kit::SharedString::from(format!("gallery-reference:{name}")),
            format!("Open fixture {name}"),
            content(treatment, agent, tokens),
        )
        .on_click(ignore_click);
        sample(name, reference_card::surface(button, treatment, tokens, cx))
    })
    .collect()
}

/// Hover frames have their own card, so its catalogue name can be copied.
pub(in crate::samples) fn reference_frames(
    _: &ComponentGallery,
    tokens: &Tokens,
    _: &mut Window,
    cx: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    use subspace_dashboard::reference_card::{self, Treatment};
    [
        ("Tracker hover frame", Treatment::Outline),
        (
            "Issue hover frame",
            Treatment::Accent(subspace_dashboard::Tone::Yellow),
        ),
    ]
    .into_iter()
    .map(|(name, treatment)| {
        sample(
            name,
            reference_card::frame(content(treatment, false, tokens), treatment, tokens, cx),
        )
    })
    .collect()
}

fn content(
    treatment: subspace_dashboard::reference_card::Treatment,
    agent: bool,
    tokens: &Tokens,
) -> gpui_kit::Div {
    use gpui_kit::component::h_flex;
    use subspace_dashboard::{
        Tone,
        reference_card::{self, Treatment},
    };
    h_flex()
        .gap_2()
        .child(reference_card::avatar(
            "Rosa Fixture",
            agent,
            matches!(treatment, Treatment::Chip),
            tokens,
        ))
        .child("SUB-28")
        .child(reference_card::pill(
            "In progress",
            IconName::Contrast,
            Tone::Yellow,
            tokens,
        ))
}
