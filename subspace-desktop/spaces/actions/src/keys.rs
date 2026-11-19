//! Board-local keys. Product order owns the function keys.

use gpui_kit::{App, Context, Div, InteractiveElement as _, KeyBinding, actions};
use subspace_framework::dashboard::{KeyHelp, view_keys};

use crate::{Actions, menus::Picker, model::View, view, writes::Operation};

actions!(
    actions_board,
    [
        Runs,
        Deployments,
        Releases,
        Flow,
        Status,
        Branch,
        Grouping,
        Account,
        Dispatch,
        Rerun,
        Cancel,
        Approve,
        Open,
        Copy
    ]
);

pub(crate) const HELP: &[KeyHelp] = &[
    KeyHelp::new("j / k", "Next / previous row"),
    KeyHelp::new("Enter", "Open details"),
    KeyHelp::new("← / →", "Fold / unfold group"),
    KeyHelp::new("1–4", "Runs / deployments / releases / flow"),
    KeyHelp::new("s / b / g / u", "Status / branch / grouping / account"),
    KeyHelp::new(
        "d / R / x / A",
        "Trigger / re-run / cancel / approve, after confirmation",
    ),
    KeyHelp::new("o / y", "Open GitHub / copy link"),
];

pub(crate) fn bind(cx: &mut App) {
    let context = view_keys("actions");
    let context = Some(context.as_str());
    cx.bind_keys([
        KeyBinding::new("1", Runs, context),
        KeyBinding::new("2", Deployments, context),
        KeyBinding::new("3", Releases, context),
        KeyBinding::new("4", Flow, context),
        KeyBinding::new("s", Status, context),
        KeyBinding::new("b", Branch, context),
        KeyBinding::new("g", Grouping, context),
        KeyBinding::new("u", Account, context),
        KeyBinding::new("d", Dispatch, context),
        KeyBinding::new("shift-r", Rerun, context),
        KeyBinding::new("x", Cancel, context),
        KeyBinding::new("shift-a", Approve, context),
        KeyBinding::new("o", Open, context),
        KeyBinding::new("y", Copy, context),
    ]);
}

pub(crate) fn handlers(root: Div, cx: &mut Context<Actions>) -> Div {
    root.on_action(cx.listener(|this, _: &Runs, _, cx| this.set_view(View::Runs, cx)))
        .on_action(cx.listener(|this, _: &Deployments, _, cx| this.set_view(View::Deployments, cx)))
        .on_action(cx.listener(|this, _: &Releases, _, cx| this.set_view(View::Releases, cx)))
        .on_action(cx.listener(|this, _: &Flow, _, cx| this.set_view(View::Flow, cx)))
        .on_action(cx.listener(|this, _: &Status, window, cx| {
            crate::filter_popup::open(this, Some(Picker::Status), window, cx);
        }))
        .on_action(cx.listener(|this, _: &Branch, window, cx| {
            crate::filter_popup::open(this, Some(Picker::Branch), window, cx);
        }))
        .on_action(cx.listener(|this, _: &Grouping, window, cx| {
            crate::filter_popup::open(this, Some(Picker::Grouping), window, cx);
        }))
        .on_action(cx.listener(|this, _: &Account, window, cx| {
            crate::menus::open_picker(this, Picker::Account, None, window, cx)
        }))
        .on_action(cx.listener(|this, _: &Dispatch, window, cx| {
            this.begin_write(Operation::Dispatch, window, cx)
        }))
        .on_action(
            cx.listener(|this, _: &Rerun, window, cx| {
                this.begin_write(Operation::Rerun, window, cx)
            }),
        )
        .on_action(cx.listener(|this, _: &Cancel, window, cx| {
            this.begin_write(Operation::Cancel, window, cx)
        }))
        .on_action(cx.listener(|this, _: &Approve, window, cx| {
            this.begin_write(Operation::Approve, window, cx)
        }))
        .on_action(cx.listener(|this, _: &Open, _, cx| view::open_selected(this, cx, false)))
        .on_action(cx.listener(|this, _: &Copy, _, cx| view::open_selected(this, cx, true)))
}
