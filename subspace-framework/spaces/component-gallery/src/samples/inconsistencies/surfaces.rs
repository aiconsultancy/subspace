//! Closest shared surfaces for hand-rolled confirmations.

use gpui_kit::{Context, ParentElement as _, Styled as _, Window, div};
use subspace_dashboard::Tokens;
use subspace_dashboard::action::Intent;
use subspace_dashboard::dialog::{self, DialogWidth};
use subspace_lucide::IconName;

use crate::ComponentGallery;
use crate::samples::dialogs::{Action, footer};
use crate::samples::{Sample, sample};

const KEEP: Action = ("Keep board", Intent::Neutral, IconName::X);
const REMOVE: Action = ("Remove board", Intent::Reject, IconName::Trash);

pub(in crate::samples) fn inline_confirmation(
    _: &ComponentGallery,
    tokens: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let frame = dialog::frame(tokens, DialogWidth::D540, "Remove the fixture board?")
        .child(
            div()
                .text_color(tokens.muted)
                .child("This removes the fixture only."),
        )
        .child(footer("gallery-sample:inline-confirmation", KEEP, REMOVE));
    vec![sample("Shared: dialog::frame with dialog::footer", frame)]
}
