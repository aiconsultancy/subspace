//! The inspector's samples: a header action, Close, sections of facts and
//! footer actions, and a plain one with a title and body only.

use gpui_kit::component::h_flex;
use gpui_kit::{App, Context, Div, ParentElement as _, Stateful, Styled as _, Window};
use subspace_dashboard::Tokens;
use subspace_dashboard::action::{self, Intent};
use subspace_dashboard::workspace::{self, Inspector};
use subspace_lucide::IconName;

use crate::ComponentGallery;
use crate::catalog::Item;
use crate::samples::{Sample, bounded, ignore_click, sample};

const INSPECTOR_SOURCE: &str = "board-kit/dashboard/src/workspace/inspector.rs";
const INSPECTOR_BOX: (f32, f32) = (320., 340.);
const PLAIN_INSPECTOR_BOX: (f32, f32) = (320., 180.);

pub(in crate::samples) const INSPECTOR: Item = Item {
    name: "workspace::Inspector",
    source: INSPECTOR_SOURCE,
    used_in: "Every Space's trailing dock: the detail of the selection in workspace::section groups of workspace::fact rows, with the actions for it in the footer.",
    samples: inspectors,
    kit: None,
};

pub(in crate::samples) fn inspectors(
    _: &ComponentGallery,
    tokens: &Tokens,
    _: &mut Window,
    cx: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let cx = &*cx;
    vec![
        sample(
            "A header action, Close, two sections of facts and footer actions",
            bounded(INSPECTOR_BOX, tokens, issue_inspector(cx)),
        ),
        sample(
            "Title and body only",
            bounded(PLAIN_INSPECTOR_BOX, tokens, plain_inspector(cx)),
        ),
    ]
}

fn issue_inspector(cx: &App) -> Inspector {
    let open = action::icon_button(
        "gallery-sample:inspector-open",
        Intent::Quiet,
        IconName::ArrowUpRight,
        "Open as tab",
    )
    .on_click(ignore_click);
    Inspector::new("gallery-sample:inspector", "SUB-12")
        .action(open)
        .on_close("Close inspector", ignore_click)
        .child(properties(cx))
        .child(record(cx))
        .footer(inspector_actions())
}

fn plain_inspector(cx: &App) -> Inspector {
    Inspector::new("gallery-sample:inspector-plain", "Properties").child(properties(cx))
}

fn properties(cx: &App) -> Stateful<Div> {
    workspace::section("Properties", cx)
        .child(workspace::fact("Status", "In progress", cx))
        .child(workspace::fact("Assignee", "Ada Example", cx))
        .child(workspace::fact("Created by", "Sam Fixture", cx))
}

fn record(cx: &App) -> Stateful<Div> {
    workspace::section("Record", cx)
        .child(workspace::fact("Created", "3 days ago", cx))
        .child(workspace::fact("Updated", "2 hours ago", cx))
}

/// The footer's actions for the selection, which wrap in a narrow dock.
fn inspector_actions() -> Div {
    let edit = action::action_button(
        "gallery-sample:inspector-edit",
        Intent::Neutral,
        Some(IconName::Pencil),
        "Edit issue…",
    )
    .on_click(ignore_click);
    let copy = action::action_button(
        "gallery-sample:inspector-copy",
        Intent::Quiet,
        Some(IconName::Copy),
        "Copy link",
    )
    .on_click(ignore_click);
    h_flex().flex_wrap().gap_2().child(edit).child(copy)
}
