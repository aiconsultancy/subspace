//! Board-local keys and the write commands. Product order owns the
//! function keys.

use gpui_kit::{App, Context, Div, InteractiveElement as _, KeyBinding, actions};
use subspace_framework::dashboard::{KeyHelp, PaletteCommand, view_keys};

use crate::{Actions, menus::Picker, model::Record, model::View, view, writes::Operation};

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
        RerunFailed,
        Cancel,
        Approve,
        Reject,
        Open,
        Copy
    ]
);

pub(crate) const HELP: &[KeyHelp] = &[
    KeyHelp::new("j / k", "Next / previous row"),
    KeyHelp::new("Enter", "Open the selected run in a tab"),
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
        .on_action(cx.listener(|this, _: &RerunFailed, window, cx| {
            this.begin_write(Operation::RerunFailed, window, cx)
        }))
        .on_action(cx.listener(|this, _: &Cancel, window, cx| {
            this.begin_write(Operation::Cancel, window, cx)
        }))
        .on_action(cx.listener(|this, _: &Approve, window, cx| {
            this.begin_write(Operation::Approve, window, cx)
        }))
        .on_action(cx.listener(|this, _: &Reject, window, cx| {
            this.begin_write(Operation::Reject, window, cx)
        }))
        .on_action(cx.listener(|this, _: &Open, _, cx| view::open_selected(this, cx, false)))
        .on_action(cx.listener(|this, _: &Copy, _, cx| view::open_selected(this, cx, true)))
}

/// `operation` as a Commands entry that dispatches its action, with its key
/// where it has one.
fn write_command(operation: Operation) -> PaletteCommand {
    let label = operation.prompt();
    match operation {
        Operation::Dispatch => PaletteCommand::new(label, Dispatch).shortcut("d"),
        Operation::Rerun => PaletteCommand::new(label, Rerun).shortcut("R"),
        Operation::RerunFailed => PaletteCommand::new(label, RerunFailed),
        Operation::Cancel => PaletteCommand::new(label, Cancel).shortcut("x"),
        Operation::Approve => PaletteCommand::new(label, Approve).shortcut("A"),
        Operation::Reject => PaletteCommand::new(label, Reject),
    }
}

/// The writes that apply to the selected item, for Commands. They keep
/// every write one step away when the inspector, which holds their
/// buttons, is hidden.
pub(crate) fn write_commands(selected: Option<&Record>) -> Vec<PaletteCommand> {
    Operation::ALL
        .into_iter()
        .filter(|operation| crate::writes::unavailable(*operation, selected).is_none())
        .map(write_command)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::write_commands;

    fn labels(selected: Option<&crate::model::Record>) -> Vec<String> {
        write_commands(selected)
            .into_iter()
            .map(|command| command.label.to_string())
            .collect()
    }

    #[test]
    fn commands_offer_only_the_writes_that_apply_to_the_selection() {
        assert!(labels(None).is_empty(), "nothing is selected");
        let mut run = crate::test_support::run("octo/app");
        let finished = labels(Some(&run));
        assert_eq!(finished, ["Trigger…", "Re-run…", "Re-run failed…"]);
        run.active = true;
        run.approval = Some((7, true));
        let every = [
            "Trigger…",
            "Re-run…",
            "Re-run failed…",
            "Cancel run…",
            "Approve…",
            "Reject…",
        ];
        assert_eq!(labels(Some(&run)), every);
    }

    #[test]
    fn a_write_command_shows_the_key_that_does_the_same() {
        let mut run = crate::test_support::run("octo/app");
        run.active = true;
        run.approval = Some((7, true));
        let keys = write_commands(Some(&run))
            .into_iter()
            .map(|command| command.shortcut.map(|key| key.to_string()))
            .collect::<Vec<_>>();
        let expected = [Some("d"), Some("R"), None, Some("x"), Some("A"), None];
        assert_eq!(keys, expected.map(|key| key.map(str::to_owned)));
    }
}
