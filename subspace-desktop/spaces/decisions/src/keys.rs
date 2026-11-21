//! The Space's keys. Plain keys act only while no text field, menu or
//! dialog has focus; `r`, `?`, F-keys and `Cmd+K` stay the shell's. The
//! shared list keys move the selection, Enter opens a decision and Escape
//! closes it.

use gpui_kit::{App, Context, Div, Focusable as _, InteractiveElement as _, KeyBinding, actions};
use subspace_framework::dashboard::{CloseDetail, KeyHelp, view_keys};

use crate::Decisions;
use crate::model::View;
use crate::search;

actions!(
    decisions_board,
    [
        AllDecisions,
        Proposed,
        Accepted,
        Rejected,
        Superseded,
        FocusSearch,
        LeaveSearch,
        OpenDecision
    ]
);

/// The key context around the search field.
pub(crate) const SEARCH_CONTEXT: &str = "DecisionsSearch";

pub(crate) const HELP: &[KeyHelp] = &[
    KeyHelp::new("j / ↓", "Next row"),
    KeyHelp::new("k / ↑", "Previous row"),
    KeyHelp::new("Enter", "Open the decision, or fold or unfold a group"),
    KeyHelp::new("Esc", "Close the open decision"),
    KeyHelp::new(
        "1–5",
        "All decisions / Proposed / Accepted / Rejected / Superseded",
    ),
    KeyHelp::new("/", "Search decisions"),
];

pub(crate) fn bind(cx: &mut App) {
    let context = view_keys(crate::ID);
    let context = Some(context.as_str());
    let search = format!("{SEARCH_CONTEXT} > Input");
    cx.bind_keys([
        KeyBinding::new("1", AllDecisions, context),
        KeyBinding::new("2", Proposed, context),
        KeyBinding::new("3", Accepted, context),
        KeyBinding::new("4", Rejected, context),
        KeyBinding::new("5", Superseded, context),
        KeyBinding::new("/", FocusSearch, context),
        KeyBinding::new("escape", LeaveSearch, Some(search.as_str())),
        KeyBinding::new("enter", LeaveSearch, Some(search.as_str())),
    ]);
}

pub(crate) fn handlers(root: Div, cx: &mut Context<Decisions>) -> Div {
    let root = view_handlers(root, cx);
    root.on_action(cx.listener(|this, _: &FocusSearch, window, cx| {
        this.search.read(cx).focus_handle(cx).focus(window, cx);
    }))
    .on_action(cx.listener(|this, _: &LeaveSearch, window, cx| {
        search::commit(this, cx);
        this.focus.focus(window, cx);
    }))
    .on_action(cx.listener(|this, _: &OpenDecision, _, cx| this.open_selected(cx)))
    // The list handles Escape while it shows; with the page in its place,
    // Escape closes the page.
    .on_action(cx.listener(|this, _: &CloseDetail, window, cx| {
        if this.shown_page().is_some() {
            this.close_page(window, cx);
        }
    }))
}

fn view_handlers(root: Div, cx: &mut Context<Decisions>) -> Div {
    root.on_action(cx.listener(|this, _: &AllDecisions, _, cx| {
        this.choose_view(View::All, cx);
    }))
    .on_action(cx.listener(|this, _: &Proposed, _, cx| {
        this.choose_view(View::Proposed, cx);
    }))
    .on_action(cx.listener(|this, _: &Accepted, _, cx| {
        this.choose_view(View::Accepted, cx);
    }))
    .on_action(cx.listener(|this, _: &Rejected, _, cx| {
        this.choose_view(View::Rejected, cx);
    }))
    .on_action(cx.listener(|this, _: &Superseded, _, cx| {
        this.choose_view(View::Superseded, cx);
    }))
}
