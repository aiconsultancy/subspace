//! The list skeleton and the one-line loading and empty notes.

use gpui_kit::{Context, Window};
use subspace_dashboard::{Tokens, feedback};

use super::super::{Sample, bounded, sample};
use super::LINE_BOX;
use crate::ComponentGallery;

pub(in crate::samples) fn skeleton(
    _: &ComponentGallery,
    tokens: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    vec![sample(
        "Loading",
        bounded(LINE_BOX, tokens, feedback::skeleton(tokens)),
    )]
}

pub(in crate::samples) fn loading_line(
    _: &ComponentGallery,
    tokens: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    vec![sample(
        "Loading",
        feedback::loading_line(
            tokens,
            "gallery-sample:loading-line",
            "Loading fixture rows…",
        ),
    )]
}

pub(in crate::samples) fn empty_line(
    _: &ComponentGallery,
    tokens: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    vec![sample(
        "Empty",
        feedback::empty_line(
            tokens,
            "gallery-sample:empty-line",
            "No failed fixture rows",
        ),
    )]
}
