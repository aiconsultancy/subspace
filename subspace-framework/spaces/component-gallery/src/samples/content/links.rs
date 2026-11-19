//! External links, plain and marked.

use gpui_kit::{Context, Window};
use subspace_dashboard::{Tokens, links};

use crate::ComponentGallery;
use crate::samples::{FIXTURE_URL, Sample, sample};

pub(in crate::samples) fn external(
    _: &ComponentGallery,
    tokens: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    vec![sample(
        "Default",
        links::external("gallery-sample:link", "Example link", FIXTURE_URL, tokens),
    )]
}

pub(in crate::samples) fn external_marked(
    _: &ComponentGallery,
    tokens: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    vec![sample(
        "Marked",
        links::external_marked(
            "gallery-sample:link-marked",
            "Example link",
            FIXTURE_URL,
            tokens,
        ),
    )]
}
