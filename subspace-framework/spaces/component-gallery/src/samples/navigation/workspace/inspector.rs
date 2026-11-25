//! The inspector's samples: a header action, Close, sections of facts and
//! footer actions, and a plain one with a title and body only; and its tabs,
//! the one approved exception, in place of the title.

use gpui_kit::component::h_flex;
use gpui_kit::{App, Context, Div, FocusHandle, ParentElement as _, Stateful, Styled as _, Window};
use subspace_dashboard::Tokens;
use subspace_dashboard::action::{self, Intent};
use subspace_dashboard::workspace::{self, Inspector, InspectorTab};
use subspace_lucide::IconName;

use crate::ComponentGallery;
use crate::catalog::Item;
use crate::samples::{Sample, bounded, ignore_click, ignore_index, sample};

const INSPECTOR_SOURCE: &str = "board-kit/dashboard/src/workspace/inspector.rs";
const INSPECTOR_BOX: (f32, f32) = (320., 340.);
const PLAIN_INSPECTOR_BOX: (f32, f32) = (320., 180.);
const TABBED_INSPECTOR_BOX: (f32, f32) = (320., 220.);
/// The least width the inspector dock keeps, to show the tabs still fit.
const NARROW_INSPECTOR_BOX: (f32, f32) = (260., 220.);

pub(in crate::samples) const INSPECTOR: Item = Item {
    name: "workspace::Inspector",
    source: INSPECTOR_SOURCE,
    used_in: "Every Space's trailing dock: the detail of the selection in workspace::section groups of workspace::fact rows, with the actions for it in the footer.",
    samples: inspectors,
    kit: None,
};

pub(in crate::samples) const INSPECTOR_TAB: Item = Item {
    name: "workspace::InspectorTab",
    source: "board-kit/dashboard/src/workspace/inspector/tabs.rs",
    used_in: "Inspector::tabs: at most three subjects of the same selection, such as a channel's ship, its thread and its people, in place of the inspector's title. Not a second navigator.",
    samples: inspector_tabs,
    kit: None,
};

pub(in crate::samples) fn inspector_tabs(
    gallery: &ComponentGallery,
    tokens: &Tokens,
    _: &mut Window,
    cx: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    vec![
        sample(
            "Three tabs, one with a count, the second open, and Close",
            bounded(
                TABBED_INSPECTOR_BOX,
                tokens,
                tabbed_inspector(
                    "gallery-sample:inspector-tabs",
                    Some(gallery.inspector_sample_tab.clone()),
                    cx,
                ),
            ),
        ),
        sample(
            "At the dock's least width",
            bounded(
                NARROW_INSPECTOR_BOX,
                tokens,
                tabbed_inspector("gallery-sample:inspector-tabs-narrow", None, cx),
            ),
        ),
    ]
}

/// A channel's inspector: its ship, its thread and its people.
fn tabbed_inspector(id: &'static str, focus: Option<FocusHandle>, cx: &App) -> Inspector {
    let tabs = [
        InspectorTab::new("Ship"),
        InspectorTab::new("Thread"),
        InspectorTab::new("People").count("5"),
    ];
    let inspector = Inspector::new(id, "Channel details")
        .tabs(tabs, 1, ignore_index)
        .on_close("Close inspector", ignore_click)
        .child(
            workspace::section("Thread", cx)
                .child(workspace::fact("Started by", "Ada Example", cx))
                .child(workspace::fact("Replies", "12", cx)),
        );
    match focus {
        Some(focus) => inspector.track_selected_tab(&focus).footer(
            action::action_button(
                "gallery-focus-thread",
                Intent::Quiet,
                None,
                "Focus Thread tab",
            )
            .on_click(move |_, window, cx| focus.focus(window, cx)),
        ),
        None => inspector,
    }
}

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
