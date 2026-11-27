//! The app shell's key bindings and the state file it writes.

use gpui_kit::{Action, App, KeyBinding};
use subspace_dashboard::{AddToChat, app_keys, prefs, secondary, view_keys};

use super::{
    CloseBoardHelp, CloseHelp, CloseJig, OpenLogView, OpenSearchView, OpenSettings,
    PaletteBackspace, PaletteEscape, Quit, Reload, SearchItems, ToggleChat, ToggleHelp, ToggleJig,
    TogglePalette, ToggleShortcuts, ToggleWrap,
};

/// Show the dashboard at `index`, in switcher order. F1 to F12 send it with
/// index 0 to 11; an index with no dashboard does nothing.
#[derive(Action, Clone, PartialEq)]
#[action(namespace = subspace_app, no_json)]
pub struct ShowDashboard {
    pub index: usize,
}

/// The keystrokes that show the dashboard at each index: F1 to F12 all switch
/// dashboards, so none of them may take another action.
const FUNCTION_KEYS: [&str; 12] = [
    "f1", "f2", "f3", "f4", "f5", "f6", "f7", "f8", "f9", "f10", "f11", "f12",
];
/// Secondary+1 to Secondary+9 for the first nine dashboards.
const BOARD_KEYS: [&str; 9] = [
    secondary!("1"),
    secondary!("2"),
    secondary!("3"),
    secondary!("4"),
    secondary!("5"),
    secondary!("6"),
    secondary!("7"),
    secondary!("8"),
    secondary!("9"),
];

/// Key context of the app shell.
pub const APP_CONTEXT: &str = "Subspace";
pub const HELP_CONTEXT: &str = "SubspaceHelp";
/// The board's bundled Markdown help.
pub const BOARD_HELP_CONTEXT: &str = "SubspaceBoardHelp";
pub const PALETTE_CONTEXT: &str = "SubspacePalette";
/// The palette field, ahead of the field's own bindings.
const PALETTE_INPUT_CONTEXT: &str = "SubspacePalette > Input";

fn bind_reference_keys(cx: &mut App) {
    let shell = view_keys(APP_CONTEXT);
    let shell = Some(shell.as_str());
    cx.bind_keys([
        // macOS normalizes shifted punctuation to the resulting character.
        KeyBinding::new(secondary!("?"), ToggleShortcuts, None),
        KeyBinding::new(secondary!("shift-/"), ToggleShortcuts, None),
        KeyBinding::new("w", ToggleWrap, shell),
        KeyBinding::new("r", Reload, shell),
        KeyBinding::new("?", ToggleHelp, shell),
        KeyBinding::new("`", ToggleJig, shell),
        KeyBinding::new("/", SearchItems, shell),
        KeyBinding::new("escape", CloseHelp, Some(HELP_CONTEXT)),
        KeyBinding::new("escape", CloseHelp, Some("SubspaceHelp > Input")),
        KeyBinding::new(
            "escape",
            subspace_dashboard::find_in_page::Close,
            Some("SubspaceHelp && !SubspaceModal && !SubspaceRawKeys"),
        ),
        KeyBinding::new(
            "escape",
            subspace_dashboard::find_in_page::Close,
            Some("(SubspaceHelp > Input) && !SubspaceModal && !SubspaceRawKeys"),
        ),
        KeyBinding::new(
            secondary!("alt-f"),
            super::help_dock::FocusShortcutFilter,
            Some(HELP_CONTEXT),
        ),
        KeyBinding::new("escape", CloseBoardHelp, Some(BOARD_HELP_CONTEXT)),
        KeyBinding::new("escape", CloseJig, Some(crate::jig::JIG_CONTEXT)),
        KeyBinding::new("escape", PaletteEscape, Some(PALETTE_INPUT_CONTEXT)),
        KeyBinding::new("backspace", PaletteBackspace, Some(PALETTE_INPUT_CONTEXT)),
    ]);
}

/// App keys. `secondary` is Cmd on macOS and Ctrl elsewhere. Plain keys act
/// only while no text field, menu or dialog has focus (REQ-06).
pub fn bind_keys(cx: &mut App) {
    bind_reference_keys(cx);
    // Linux app keys use Ctrl+Shift so plain Ctrl keys reach a focused
    // terminal (REQ-20). They still act there, so they take no context.
    #[cfg(target_os = "linux")]
    cx.bind_keys([
        KeyBinding::new("ctrl-shift-q", Quit, None),
        KeyBinding::new("ctrl-shift-k", TogglePalette, None),
        KeyBinding::new("ctrl-shift-r", Reload, None),
        KeyBinding::new("ctrl-shift-,", OpenSettings, None),
        KeyBinding::new("super-shift-c", ToggleChat, None),
        KeyBinding::new("ctrl-shift-a", AddToChat, None),
        KeyBinding::new("ctrl-shift-f", OpenSearchView, None),
        KeyBinding::new("ctrl-shift-l", OpenLogView, None),
    ]);
    #[cfg(not(target_os = "linux"))]
    cx.bind_keys([
        KeyBinding::new(secondary!("q"), Quit, None),
        KeyBinding::new(secondary!("k"), TogglePalette, None),
        KeyBinding::new(secondary!("r"), Reload, None),
        KeyBinding::new(secondary!(","), OpenSettings, None),
        KeyBinding::new(secondary!("shift-c"), ToggleChat, None),
        KeyBinding::new(secondary!("shift-a"), AddToChat, None),
        KeyBinding::new(secondary!("shift-f"), OpenSearchView, None),
        KeyBinding::new(secondary!("shift-l"), OpenLogView, None),
    ]);
    // Secondary+1 to Secondary+9 switch boards even from a raw-key region;
    // plain F1 to F12 go to that region instead (REQ-21).
    cx.bind_keys(
        BOARD_KEYS
            .into_iter()
            .enumerate()
            .map(|(index, keys)| KeyBinding::new(keys, ShowDashboard { index }, None)),
    );
    let app_context = app_keys();
    let app_context = Some(app_context.as_str());
    cx.bind_keys(
        FUNCTION_KEYS
            .into_iter()
            .enumerate()
            .map(|(index, keys)| KeyBinding::new(keys, ShowDashboard { index }, app_context)),
    );
    super::menu_bar::bind_keys(cx);
    super::menu_bar::register_actions(cx);
}

pub fn save_prefs(cx: &mut App) {
    if let Err(error) = prefs::save(cx) {
        eprintln!("subspace: could not save the state file: {error}");
    }
}

/// The palette key as the help and the toolbar show it.
pub fn palette_key() -> &'static str {
    if cfg!(target_os = "macos") {
        "⌘K"
    } else if cfg!(target_os = "linux") {
        "Ctrl+Shift+K"
    } else {
        "Ctrl K"
    }
}

pub fn shortcuts_key() -> &'static str {
    if cfg!(target_os = "macos") {
        "⌘⇧/"
    } else {
        "Ctrl+Shift+/"
    }
}

/// The settings key as the help shows it.
pub fn settings_key() -> &'static str {
    if cfg!(target_os = "macos") {
        "⌘,"
    } else if cfg!(target_os = "linux") {
        "Ctrl+Shift+,"
    } else {
        "Ctrl+,"
    }
}

/// The add-to-chat key as the help shows it.
pub fn add_to_chat_key() -> &'static str {
    if cfg!(target_os = "macos") {
        "⌘⇧A"
    } else {
        "Ctrl+Shift+A"
    }
}

/// The chat toggle key as the help and toolbar show it.
pub fn chat_key() -> &'static str {
    if cfg!(target_os = "macos") {
        "⌘⇧C"
    } else if cfg!(target_os = "linux") {
        "Super+Shift+C"
    } else {
        "Ctrl+Shift+C"
    }
}

#[cfg(test)]
mod tests {
    use super::{ShowDashboard, add_to_chat_key, bind_keys, chat_key, palette_key, settings_key};
    use gpui_kit::{Keystroke, TestAppContext};

    #[gpui_kit::test]
    fn digit_and_function_keys_show_dashboards_in_switcher_order(cx: &mut TestAppContext) {
        cx.update(|cx| {
            bind_keys(cx);
            let os = cx.platform_info().os;
            for (keys, index) in [
                ("secondary-1", 0),
                ("secondary-9", 8),
                ("f1", 0),
                ("f12", 11),
            ] {
                let input = [Keystroke::parse_with_os(keys, os).unwrap()];
                let bindings = cx.all_bindings_for_input(&input);
                assert!(
                    bindings
                        .iter()
                        .any(|binding| binding.action().partial_eq(&ShowDashboard { index })),
                    "{keys} shows Space {index}"
                );
            }
        });
    }

    #[test]
    pub(super) fn the_shown_keys_follow_the_platform() {
        let mac = cfg!(target_os = "macos");
        assert_eq!(palette_key() == "⌘K", mac);
        assert_eq!(settings_key() == "⌘,", mac);
        assert_eq!(add_to_chat_key() == "⌘⇧A", mac);
        assert_eq!(chat_key() == "⌘⇧C", mac);
        if cfg!(target_os = "linux") {
            assert_eq!(chat_key(), "Super+Shift+C");
            assert_eq!(palette_key(), "Ctrl+Shift+K");
        }
    }
}
