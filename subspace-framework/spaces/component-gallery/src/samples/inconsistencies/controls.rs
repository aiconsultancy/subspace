//! Closest shared controls for the hand-rolled one-off controls.

use gpui_kit::{Context, Window};
use subspace_dashboard::segmented::{self, Segment};
use subspace_dashboard::{Tokens, links};

use crate::ComponentGallery;
use crate::samples::{FIXTURE_URL, Sample, ignore_index, sample};

fn shared_segmented(
    id: &'static str,
    labels: [&str; 3],
    selected: usize,
    tokens: &Tokens,
) -> Sample {
    let segments = labels.map(Segment::new);
    sample(
        "Shared: segmented::control",
        segmented::control(id, segments, Some(selected), tokens, ignore_index),
    )
}

pub(in crate::samples) fn mode_switch(
    _: &ComponentGallery,
    tokens: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let labels = ["Approve", "Request changes", "Comment"];
    vec![shared_segmented(
        "gallery-sample:github-mode-switch",
        labels,
        0,
        tokens,
    )]
}

pub(in crate::samples) fn selected_button_row(
    _: &ComponentGallery,
    tokens: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let labels = ["Keep", "Replace", "Clear"];
    vec![shared_segmented(
        "gallery-sample:board-lab-button-row",
        labels,
        0,
        tokens,
    )]
}

pub(in crate::samples) fn check_run_link(
    _: &ComponentGallery,
    tokens: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let link = links::external(
        "gallery-sample:github-check-link",
        "Check run",
        FIXTURE_URL,
        tokens,
    );
    vec![sample("Shared: links::external", link)]
}
