//! External links, plain and marked.

use gpui_kit::{Context, Window};
use subspace_dashboard::{Tokens, links};

use crate::ComponentGallery;
use crate::catalog::Item;
use crate::samples::{FIXTURE_URL, Sample, sample};

const SOURCE: &str = "board-kit/dashboard/src/links.rs";

pub(super) const EXTERNAL: Item = Item {
    name: "links::external",
    source: SOURCE,
    used_in: "External links in the host.",
    samples: external,
    kit: None,
};

pub(super) const MARKED: Item = Item {
    name: "links::external_marked",
    source: SOURCE,
    used_in: "External links with a trailing mark.",
    samples: external_marked,
    kit: None,
};

pub(super) const MARKED_WITH_LABEL: Item = Item {
    name: "links::external_marked_with_label",
    source: SOURCE,
    used_in: "Decision evidence links show a host and announce the destination's purpose.",
    samples: external_marked_with_label,
    kit: None,
};

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

pub(in crate::samples) fn external_marked_with_label(
    _: &ComponentGallery,
    tokens: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    vec![sample(
        "Separate accessible name",
        links::external_marked_with_label(
            "gallery-sample:link-marked-with-label",
            "example.com",
            "Open project website: example.com",
            "https://example.com/",
            tokens,
        ),
    )]
}
