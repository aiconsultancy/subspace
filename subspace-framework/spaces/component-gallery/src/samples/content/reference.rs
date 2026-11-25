//! Live reference-card treatments use the same constructors as Channels.
use crate::{
    ComponentGallery,
    samples::{Sample, sample},
};
use gpui_kit::{Context, Div, ParentElement as _, Styled as _, Window, component::h_flex};
use subspace_dashboard::{
    Tokens, Tone,
    reference_card::{self, Treatment},
};
use subspace_lucide::IconName;
pub(in crate::samples) fn reference_cards(
    _: &ComponentGallery,
    tokens: &Tokens,
    _: &mut Window,
    cx: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    use crate::samples::ignore_click;
    use subspace_dashboard::action::card_button;
    [
        ("Issue", Treatment::Outline, issue_content(false, tokens)),
        (
            "Pull request",
            Treatment::Outline,
            pull_request_content(tokens),
        ),
        ("Issue chip", Treatment::Chip, issue_content(true, tokens)),
    ]
    .into_iter()
    .map(|(name, treatment, content)| {
        let button = card_button(
            gpui_kit::SharedString::from(format!("gallery-reference:{name}")),
            format!("Open fixture {name}"),
            content,
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
    [
        ("Issue hover frame", issue_content(false, tokens)),
        ("Pull request hover frame", pull_request_content(tokens)),
    ]
    .into_iter()
    .map(|(name, content)| {
        sample(
            name,
            reference_card::frame(content, Treatment::Outline, tokens, cx),
        )
    })
    .collect()
}

fn issue_content(chip: bool, tokens: &Tokens) -> Div {
    h_flex()
        .gap_2()
        .child(reference_card::avatar("Rosa Fixture", false, chip, tokens))
        .child("NAV-7")
        .child(reference_card::pill(
            "In progress",
            IconName::Contrast,
            Tone::Yellow,
            tokens,
        ))
}

fn pull_request_content(tokens: &Tokens) -> Div {
    h_flex()
        .gap_2()
        .child("relay#42")
        .child(reference_card::pill(
            "Merged",
            IconName::GitMerge,
            Tone::Magenta,
            tokens,
        ))
}
