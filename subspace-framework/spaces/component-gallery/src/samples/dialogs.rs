//! Dialogs: the shared frame, footer and backdrop, drawn inline.

use gpui_kit::{Context, Div, ParentElement as _, Styled as _, Window, div};
use subspace_dashboard::Tokens;
use subspace_dashboard::action::{self, Intent};
use subspace_dashboard::dialog::{self, DialogWidth};
use subspace_lucide::{IconName, SvgIcon};

use super::{Sample, bounded, ignore_click, sample, sized};
use crate::ComponentGallery;
use crate::catalog::Item;

const DIALOG: &str = "board-kit/dashboard/src/dialog.rs";
const FOOTER_WIDTH: f32 = 320.;
const BACKDROP_SIZE: (f32, f32) = (640., 220.);

/// One footer action: label, intent and glyph.
pub(crate) type Action = (&'static str, Intent, SvgIcon);

/// Confirmation pairs in the shared order: secondary first, then the primary
/// or destructive action at the trailing edge.
const PAIRS: [(Action, Action); 3] = [
    (
        ("Cancel", Intent::Neutral, IconName::X),
        ("Save", Intent::Primary, IconName::Check),
    ),
    (
        ("Cancel", Intent::Neutral, IconName::X),
        ("Delete", Intent::Reject, IconName::Trash),
    ),
    (
        ("Keep editing", Intent::Neutral, IconName::Pencil),
        ("Discard", Intent::Reject, IconName::Eraser),
    ),
];

pub(crate) const ITEMS: &[Item] = &[
    Item {
        name: "dialog::frame",
        source: DIALOG,
        used_in: "Board dialogs in Actions, IRC, Terminal and the host.",
        samples: frames,
        kit: None,
    },
    Item {
        name: "dialog::footer",
        source: DIALOG,
        used_in: "Dialog action rows in the same boards as dialog::frame.",
        samples: footers,
        kit: None,
    },
    Item {
        name: "dialog::backdrop",
        source: DIALOG,
        used_in: "The dimmed modal layer behind dialog frames, drawn here inside a bounded box.",
        samples: backdrop,
        kit: None,
    },
];

/// A shared footer with two actions, in the order given.
pub(crate) fn footer(id: &'static str, secondary: Action, primary: Action) -> Div {
    let button = |index: usize, (label, intent, icon): Action| {
        action::dialog_button((id, index), intent, Some(icon), label).on_click(ignore_click)
    };
    dialog::footer()
        .child(button(0, secondary))
        .child(button(1, primary))
}

/// A D540 frame with fixture body text and a Cancel → Save footer.
pub(crate) fn sample_frame(id: &'static str, tokens: &Tokens, title: &'static str) -> Div {
    let (secondary, primary) = PAIRS[0];
    dialog::frame(tokens, DialogWidth::D540, title)
        .child(
            div()
                .text_color(tokens.muted)
                .child("Fixture body text of a dashboard dialog."),
        )
        .child(footer(id, secondary, primary))
}

pub(super) fn frames(
    _: &ComponentGallery,
    tokens: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    vec![sample(
        "D540 with footer",
        sample_frame("gallery-sample:dialog-frame", tokens, "Save the changes?"),
    )]
}

pub(super) fn footers(
    _: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    const IDS: [&str; 3] = [
        "gallery-sample:footer-save",
        "gallery-sample:footer-delete",
        "gallery-sample:footer-discard",
    ];
    IDS.iter()
        .zip(PAIRS)
        .map(|(id, (secondary, primary))| {
            let caption = format!("{} → {}", secondary.0, primary.0);
            sample(caption, sized(FOOTER_WIDTH, footer(id, secondary, primary)))
        })
        .collect()
}

pub(super) fn backdrop(
    _: &ComponentGallery,
    tokens: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let frame = sample_frame(
        "gallery-sample:backdrop-frame",
        tokens,
        "Dialog over a backdrop",
    );
    vec![sample(
        "Bounded",
        bounded(BACKDROP_SIZE, tokens, dialog::backdrop(tokens, frame)),
    )]
}
