//! Every menu bar item runs its own command, by the action's type.

use gpui_kit::component::input::{Copy, Cut, Paste, Redo, SelectAll, Undo};
use gpui_kit::{Action, Menu, MenuItem, OsAction};

use super::super::{
    MoveTerminalLeft, MoveTerminalRight, MoveTerminalToBottom, NextDashboard, OpenAbout,
    OpenLogView, OpenSearchView, OpenSettings, PreviousDashboard, Quit, Reload, SearchItems,
    ToggleChat, ToggleHelp, ToggleJig, ToggleNavigationMode, TogglePalette, ToggleShortcuts,
    ToggleSpaceInspector, ToggleSpaceNavigator, ToggleTerminal, ToggleTerminalFullWidth,
    ToggleTerminalMaximized, ToggleWrap,
};
use super::tests::bar;
use super::{
    HideApp, HideOtherApps, Layout, MinimizeWindow, ShowAllApps, ToggleFullScreen, ZoomWindow,
    menus,
};

/// An action item: its label, the command it runs and the system edit
/// action it is tied to, if any.
type Command = (String, &'static str, Option<&'static str>);

/// Every action item, submenus' included, in the menu bar's order.
fn commands(menus: &[Menu]) -> Vec<Command> {
    let mut found = Vec::new();
    for menu in menus {
        collect(&menu.items, &mut found);
    }
    found
}

fn collect(items: &[MenuItem], found: &mut Vec<Command>) {
    for item in items {
        match item {
            MenuItem::Action {
                name,
                action,
                os_action,
                ..
            } => found.push((name.to_string(), action.name(), os_action.map(os_name))),
            MenuItem::Submenu(menu) => collect(&menu.items, found),
            _ => {}
        }
    }
}

fn os_name(action: OsAction) -> &'static str {
    match action {
        OsAction::Cut => "Cut",
        OsAction::Copy => "Copy",
        OsAction::Paste => "Paste",
        OsAction::SelectAll => "Select All",
        OsAction::Undo => "Undo",
        OsAction::Redo => "Redo",
    }
}

/// The command `A` runs, by label, untied to a system edit action.
fn runs<A: Action>(label: &str) -> Command {
    (label.to_owned(), A::name_for_type(), None)
}

/// GPUI Kit's text command `A`, tied to the system's edit action of the
/// same name as its label.
fn edits<A: Action>(label: &'static str) -> Command {
    (label.to_owned(), A::name_for_type(), Some(label))
}

fn edit_commands() -> Vec<Command> {
    vec![
        edits::<Undo>("Undo"),
        edits::<Redo>("Redo"),
        edits::<Cut>("Cut"),
        edits::<Copy>("Copy"),
        edits::<Paste>("Paste"),
        edits::<SelectAll>("Select All"),
    ]
}

/// The View menu's commands with a terminal, as every layout lists them.
fn view_commands() -> Vec<Command> {
    vec![
        runs::<TogglePalette>("Command Palette"),
        runs::<SearchItems>("Search Items"),
        runs::<OpenSearchView>("Search Content"),
        runs::<ToggleChat>("Chat"),
        runs::<ToggleTerminal>("Terminal"),
        runs::<MoveTerminalLeft>("Move Left"),
        runs::<MoveTerminalToBottom>("Move to Bottom"),
        runs::<MoveTerminalRight>("Move Right"),
        runs::<ToggleTerminalFullWidth>("Full Window Width"),
        runs::<ToggleTerminalMaximized>("Maximize"),
        runs::<ToggleSpaceNavigator>("Navigator"),
        runs::<ToggleSpaceInspector>("Inspector"),
        runs::<ToggleNavigationMode>("Vertical Space Navigation"),
        runs::<NextDashboard>("Next Space"),
        runs::<PreviousDashboard>("Previous Space"),
        runs::<ToggleWrap>("Wrap Titles"),
        runs::<ToggleJig>("Theme Designer"),
        runs::<Reload>("Reload"),
    ]
}

fn help_commands() -> Vec<Command> {
    vec![
        runs::<ToggleHelp>("Example Help"),
        runs::<ToggleShortcuts>("Keyboard Shortcuts"),
        runs::<OpenLogView>("Show Logs"),
    ]
}

#[test]
fn every_mac_item_runs_its_own_command() {
    let app = vec![
        runs::<OpenAbout>("About Example"),
        runs::<OpenSettings>("Settings…"),
        runs::<HideApp>("Hide Example"),
        runs::<HideOtherApps>("Hide Others"),
        runs::<ShowAllApps>("Show All"),
        runs::<Quit>("Quit Example"),
    ];
    let window = vec![
        runs::<MinimizeWindow>("Minimize"),
        runs::<ZoomWindow>("Zoom"),
    ];
    let expected: Vec<Command> = [
        app,
        edit_commands(),
        view_commands(),
        window,
        help_commands(),
    ]
    .concat();
    assert_eq!(commands(&menus(bar(true, false), Layout::Mac)), expected);
}

#[test]
fn every_windows_and_linux_item_runs_its_own_command() {
    for (layout, quit) in [(Layout::Windows, "Exit"), (Layout::Linux, "Quit")] {
        let file = vec![runs::<OpenSettings>("Settings…"), runs::<Quit>(quit)];
        let full_screen = vec![runs::<ToggleFullScreen>("Full Screen")];
        let about = vec![runs::<OpenAbout>("About Example")];
        let expected: Vec<Command> = [
            file,
            edit_commands(),
            view_commands(),
            full_screen,
            help_commands(),
            about,
        ]
        .concat();
        assert_eq!(
            commands(&menus(bar(true, false), layout)),
            expected,
            "{layout:?}"
        );
    }
}

#[cfg(feature = "report-issue")]
#[test]
fn report_an_issue_runs_its_command() {
    let help = commands(&menus(bar(false, true), Layout::Mac));
    assert_eq!(
        help.last(),
        Some(&runs::<super::super::OpenReportIssue>("Report an Issue…"))
    );
}
