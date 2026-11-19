//! Feedback recovery actions and the panel service link, in the Buttons section.

use gpui_kit::component::Disableable as _;
use gpui_kit::{Context, Window};
use subspace_dashboard::action::Intent;
use subspace_dashboard::{ServiceLink, Tokens, feedback, panel};
use subspace_lucide::IconName;

use crate::ComponentGallery;
use crate::samples::{FIXTURE_URL, Sample, ignore_click, sample};

pub(in crate::samples) fn retry_action(
    _: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    vec![
        sample(
            "Default",
            feedback::retry_action("gallery-sample:retry-action", ignore_click),
        ),
        sample(
            "Disabled",
            feedback::retry_action("gallery-sample:retry-action-disabled", ignore_click)
                .disabled(true),
        ),
    ]
}

pub(in crate::samples) fn recovery_action(
    _: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let restart = |id: &'static str| {
        feedback::recovery_action(
            id,
            Intent::Caution,
            IconName::RotateCcw,
            "Restart session",
            ignore_click,
        )
    };
    vec![
        sample("Caution", restart("gallery-sample:recovery-action")),
        sample(
            "Disabled",
            restart("gallery-sample:recovery-action-disabled").disabled(true),
        ),
    ]
}

pub(in crate::samples) fn configure_action(
    _: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    vec![
        sample(
            "Default",
            feedback::configure_action("gallery-sample:configure-action"),
        ),
        sample(
            "Disabled",
            feedback::configure_action("gallery-sample:configure-action-disabled").disabled(true),
        ),
    ]
}

/// `panel::service_link` returns `impl IntoElement`, so it has no disabled state.
pub(in crate::samples) fn service_link(
    _: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let link = ServiceLink::new("Open in service", FIXTURE_URL);
    vec![sample(
        "Default",
        panel::service_link("gallery-sample:service-link", link),
    )]
}
