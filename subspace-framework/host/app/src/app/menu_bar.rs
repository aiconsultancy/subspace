//! The native menu bar, laid out for each platform's conventions, and the
//! window and application commands only its menus offer.
//!
//! macOS shows the menu bar. Windows and Linux keep the same model, laid out
//! as their apps lay out a menu bar, although GPUI does not draw it there.

#[cfg(target_os = "macos")]
use gpui_kit::KeyBinding;
use gpui_kit::component::input::{Copy, Cut, Paste, Redo, SelectAll, Undo};
use gpui_kit::{App, Menu, MenuItem, OsAction, SystemMenuType, Window, actions};

use super::{
    MoveTerminalLeft, MoveTerminalRight, MoveTerminalToBottom, NextDashboard, OpenAbout,
    OpenLogView, OpenSearchView, OpenSettings, PreviousDashboard, Quit, Reload, SearchItems,
    ToggleChat, ToggleHelp, ToggleJig, ToggleNavigationMode, TogglePalette, ToggleShortcuts,
    ToggleSpaceInspector, ToggleSpaceNavigator, ToggleTerminal, ToggleTerminalFullWidth,
    ToggleTerminalMaximized, ToggleWrap,
};

// Every platform declares the macOS commands, so the macOS layout can be
// built and tested anywhere; only macOS handles them or binds their keys.
actions!(
    subspace_app,
    [
        /// Enter or leave full screen.
        ToggleFullScreen,
        /// Hide the app, as the app menu's Hide item does.
        HideApp,
        /// Hide every other app.
        HideOtherApps,
        /// Show every hidden app.
        ShowAllApps,
        /// Minimise the window into the Dock.
        MinimizeWindow,
        /// Zoom the window, as a double-click on its title bar does.
        ZoomWindow
    ]
);

/// What the menu bar holds beyond its fixed items.
pub(crate) struct MenuBar {
    /// The app's name, for the app menu and its items.
    pub name: String,
    /// The product docks a terminal.
    pub terminal: bool,
    /// The app names a repository for Report an Issue.
    pub report_issue: bool,
    /// The debug build's Developer menu, placed after View as Safari places
    /// its Develop menu.
    pub developer: Option<Menu>,
}

/// The platforms whose menu bars differ in layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Layout {
    Mac,
    Windows,
    Linux,
}

impl Layout {
    pub(crate) const fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::Mac
        } else if cfg!(windows) {
            Self::Windows
        } else {
            Self::Linux
        }
    }
}

/// The native menu bar, with the Developer menu in debug builds while
/// Developer mode is on. Keys are bound first, so each item shows its key.
pub(crate) fn install_menus(cx: &mut App) {
    if cx
        .try_global::<subspace_dashboard::CurrentProduct>()
        .is_none()
    {
        return;
    }
    let bar = MenuBar {
        name: crate::product::desktop_name(cx).to_owned(),
        terminal: crate::terminal::registration(cx).is_some(),
        report_issue: report_issue_available(cx),
        developer: developer_menu(cx),
    };
    cx.set_menus(menus(bar, Layout::current()));
}

#[cfg(feature = "report-issue")]
fn report_issue_available(cx: &App) -> bool {
    super::report_issue::available(cx)
}

#[cfg(not(feature = "report-issue"))]
fn report_issue_available(_: &App) -> bool {
    false
}

#[cfg(debug_assertions)]
fn developer_menu(cx: &App) -> Option<Menu> {
    crate::settings::developer_mode(cx).then(crate::inspector::menu)
}

#[cfg(not(debug_assertions))]
fn developer_menu(_: &App) -> Option<Menu> {
    None
}

/// The menu bar for `layout`; each item runs the same action as its key.
pub(crate) fn menus(bar: MenuBar, layout: Layout) -> Vec<Menu> {
    let (first, window) = framing_menus(&bar.name, layout);
    let help = help_menu(&bar.name, bar.report_issue, layout);
    [first, edit_menu(), view_menu(bar.terminal, layout)]
        .into_iter()
        .chain(bar.developer)
        .chain(window)
        .chain([help])
        .collect()
}

/// The first menu and the Window menu, which only macOS has: Windows and
/// Linux put its commands on the title bar.
fn framing_menus(name: &str, layout: Layout) -> (Menu, Option<Menu>) {
    match layout {
        Layout::Mac => (app_menu(name), Some(window_menu())),
        Layout::Windows => (file_menu("Exit"), None),
        Layout::Linux => (file_menu("Quit"), None),
    }
}

/// The macOS app menu, in the order Apple's apps use.
fn app_menu(name: &str) -> Menu {
    Menu::new(name.to_owned()).items([
        MenuItem::action(format!("About {name}"), OpenAbout),
        MenuItem::separator(),
        MenuItem::action("Settings…", OpenSettings),
        MenuItem::separator(),
        MenuItem::os_submenu("Services", SystemMenuType::Services),
        MenuItem::separator(),
        MenuItem::action(format!("Hide {name}"), HideApp),
        MenuItem::action("Hide Others", HideOtherApps),
        MenuItem::action("Show All", ShowAllApps),
        MenuItem::separator(),
        MenuItem::action(format!("Quit {name}"), Quit),
    ])
}

/// The File menu on Windows and Linux, which hold Settings and the way out
/// there. `quit` is Exit on Windows and Quit on Linux.
fn file_menu(quit: &'static str) -> Menu {
    Menu::new("File").items([
        MenuItem::action("Settings…", OpenSettings),
        MenuItem::separator(),
        MenuItem::action(quit, Quit),
    ])
}

/// GPUI Kit's text commands, tied to the system's edit actions so the
/// focused field or web page handles them.
fn edit_menu() -> Menu {
    Menu::new("Edit").items([
        MenuItem::os_action("Undo", Undo, OsAction::Undo),
        MenuItem::os_action("Redo", Redo, OsAction::Redo),
        MenuItem::separator(),
        MenuItem::os_action("Cut", Cut, OsAction::Cut),
        MenuItem::os_action("Copy", Copy, OsAction::Copy),
        MenuItem::os_action("Paste", Paste, OsAction::Paste),
        MenuItem::os_action("Select All", SelectAll, OsAction::SelectAll),
    ])
}

/// AppKit adds Enter Full Screen to the View menu itself, with the label for
/// the window's state, so only Windows and Linux list it here.
fn view_menu(terminal: bool, layout: Layout) -> Menu {
    let full_screen = (layout != Layout::Mac).then(|| {
        [
            MenuItem::separator(),
            MenuItem::action("Full Screen", ToggleFullScreen),
        ]
    });
    Menu::new("View").items(
        [
            MenuItem::action("Command Palette", TogglePalette),
            MenuItem::action("Search Items", SearchItems),
            MenuItem::action("Search Content", OpenSearchView),
            MenuItem::separator(),
            MenuItem::action("Chat", ToggleChat),
        ]
        .into_iter()
        .chain(terminal.then(terminal_items).into_iter().flatten())
        .chain([
            MenuItem::action("Navigator", ToggleSpaceNavigator),
            MenuItem::action("Inspector", ToggleSpaceInspector),
            MenuItem::action("Vertical Space Navigation", ToggleNavigationMode),
            MenuItem::separator(),
            MenuItem::action("Next Space", NextDashboard),
            MenuItem::action("Previous Space", PreviousDashboard),
            MenuItem::separator(),
            MenuItem::action("Wrap Titles", ToggleWrap),
            MenuItem::action("Theme Designer", ToggleJig),
            MenuItem::action("Reload", Reload),
        ])
        .chain(full_screen.into_iter().flatten()),
    )
}

fn terminal_items() -> [MenuItem; 2] {
    [
        MenuItem::action("Terminal", ToggleTerminal),
        MenuItem::submenu(Menu::new("Terminal Layout").items([
            MenuItem::action("Move Left", MoveTerminalLeft),
            MenuItem::action("Move to Bottom", MoveTerminalToBottom),
            MenuItem::action("Move Right", MoveTerminalRight),
            MenuItem::action("Full Window Width", ToggleTerminalFullWidth),
            MenuItem::action("Maximize", ToggleTerminalMaximized),
        ])),
    ]
}

/// macOS's Window menu. AppKit adds the window list and, from macOS 15, the
/// tiling commands to the menu titled Window.
fn window_menu() -> Menu {
    Menu::new("Window").items([
        MenuItem::action("Minimize", MinimizeWindow),
        MenuItem::action("Zoom", ZoomWindow),
    ])
}

/// Help comes last. Windows and Linux put About here; macOS puts it in the
/// app menu.
fn help_menu(name: &str, report_issue: bool, layout: Layout) -> Menu {
    let about = (layout != Layout::Mac).then(|| {
        [
            MenuItem::separator(),
            MenuItem::action(format!("About {name}"), OpenAbout),
        ]
    });
    Menu::new("Help").items(
        [
            MenuItem::action(format!("{name} Help"), ToggleHelp),
            MenuItem::action("Keyboard Shortcuts", ToggleShortcuts),
            MenuItem::separator(),
            MenuItem::action("Show Logs", OpenLogView),
        ]
        .into_iter()
        .chain(report_issue.then(report_issue_item).flatten())
        .chain(about.into_iter().flatten()),
    )
}

#[cfg(feature = "report-issue")]
fn report_issue_item() -> Option<MenuItem> {
    Some(MenuItem::action("Report an Issue…", super::OpenReportIssue))
}

#[cfg(not(feature = "report-issue"))]
fn report_issue_item() -> Option<MenuItem> {
    None
}

/// The menu bar's own keys, bound on macOS only: elsewhere these keys belong
/// to the window manager or reach a focused terminal.
#[cfg(target_os = "macos")]
pub(super) fn bind_keys(cx: &mut App) {
    // GPUI ranks a binding without a context as the deepest, so the Space
    // keys stay out of Chat, whose own tab keys are the same.
    let outside_chat = format!("!{}", subspace_dashboard::chat_input::CHAT_PANEL_CONTEXT);
    let outside_chat = Some(outside_chat.as_str());
    cx.bind_keys([
        KeyBinding::new("cmd-h", HideApp, None),
        KeyBinding::new("cmd-alt-h", HideOtherApps, None),
        KeyBinding::new("cmd-m", MinimizeWindow, None),
        KeyBinding::new("cmd-ctrl-f", ToggleFullScreen, None),
        // macOS reports shifted punctuation as the resulting character, so
        // each key is bound both ways; the first spelling labels the menu.
        KeyBinding::new("cmd-shift-]", NextDashboard, outside_chat),
        KeyBinding::new("cmd-}", NextDashboard, outside_chat),
        KeyBinding::new("cmd-shift-[", PreviousDashboard, outside_chat),
        KeyBinding::new("cmd-{", PreviousDashboard, outside_chat),
    ]);
}

#[cfg(not(target_os = "macos"))]
pub(super) fn bind_keys(_: &mut App) {}

/// The menu bar's keys for the shortcut list; the rest are the system's.
pub(super) fn help_keys() -> &'static [(&'static str, &'static str)] {
    if cfg!(target_os = "macos") {
        &[
            ("⌘⇧[ / ⌘⇧]", "Previous / next Space"),
            ("⌃⌘F", "Enter or leave full screen"),
        ]
    } else {
        &[]
    }
}

/// The window and app commands, registered once for the whole app so they
/// act whatever has focus, a dialog included, and Hide works with no window
/// open. Only macOS handles the Hide and window commands: GPUI panics on
/// Windows when asked to hide other apps.
pub(super) fn register_actions(cx: &mut App) {
    cx.on_action(|_: &ToggleFullScreen, cx| {
        with_active_window(cx, Window::toggle_fullscreen);
    });
    register_mac_actions(cx);
}

#[cfg(target_os = "macos")]
fn register_mac_actions(cx: &mut App) {
    cx.on_action(|_: &HideApp, cx| cx.hide());
    cx.on_action(|_: &HideOtherApps, cx| cx.hide_other_apps());
    cx.on_action(|_: &ShowAllApps, cx| cx.unhide_other_apps());
    cx.on_action(|_: &MinimizeWindow, cx| {
        with_active_window(cx, Window::minimize_window);
    });
    cx.on_action(|_: &ZoomWindow, cx| with_active_window(cx, Window::zoom_window));
}

#[cfg(not(target_os = "macos"))]
fn register_mac_actions(_: &mut App) {}

/// Runs `act` on the active window, if there is one. GPUI runs app-wide
/// handlers while it is updating that window, so the change waits until
/// the update ends, as the inspector's toggle does.
fn with_active_window(cx: &mut App, act: fn(&Window)) {
    if let Some(window) = cx.active_window() {
        cx.defer(move |cx| {
            _ = window.update(cx, |_, window, _| act(window));
        });
    }
}

#[cfg(test)]
#[path = "menu_bar_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "menu_bar_key_tests.rs"]
mod key_tests;
