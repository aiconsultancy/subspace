use gpui_kit::Context;
use subspace_dashboard::{AddToChat, PaletteCommand, prefs};

use super::super::keys::{add_to_chat_key, chat_key, settings_key, shortcuts_key};
use super::super::{
    CloseLogView, MoveTerminalLeft, MoveTerminalRight, MoveTerminalToBottom, OpenAbout,
    OpenLogView, OpenSearchView, OpenSettings, Reload, ShowTerminalHelp, SubspaceApp, ToggleChat,
    ToggleHelp, ToggleJig, ToggleNavigationMode, ToggleShortcuts, ToggleTerminal,
    ToggleTerminalFullWidth, ToggleTerminalMaximized, ToggleWrap,
};
use crate::palette::RECENT_COMMANDS_KEY;

fn wrap_label(wrap: bool) -> &'static str {
    if wrap {
        "Truncate titles"
    } else {
        "Wrap titles"
    }
}

fn jig_label(open: bool) -> &'static str {
    if open {
        "Close theme designer"
    } else {
        "Open theme designer"
    }
}

fn search_key() -> &'static str {
    if cfg!(target_os = "macos") {
        "⌘⇧F"
    } else {
        "Ctrl+Shift+F"
    }
}

fn log_key() -> &'static str {
    if cfg!(target_os = "macos") {
        "⌘⇧L"
    } else {
        "Ctrl+Shift+L"
    }
}

/// While the log view is open the palette offers to close it instead.
fn log_command(logs_open: bool) -> PaletteCommand {
    if logs_open {
        return PaletteCommand::new("Close logs", CloseLogView)
            .keywords("log logs close hide viewer board")
            .shortcut("Esc");
    }
    PaletteCommand::new("Show logs", OpenLogView)
        .keywords("log logs diagnostics debug trace viewer")
        .shortcut(log_key())
}

pub(super) fn global(wrap: bool, jig_open: bool, logs_open: bool) -> Vec<PaletteCommand> {
    vec![
        PaletteCommand::new("Search Space content", OpenSearchView)
            .keywords("search full text cache history BM25")
            .shortcut(search_key()),
        log_command(logs_open),
        PaletteCommand::new("Reload", Reload)
            .keywords("refresh")
            .shortcut("r"),
        PaletteCommand::new(wrap_label(wrap), ToggleWrap)
            .keywords("wrap truncate long lines")
            .shortcut("w"),
        PaletteCommand::new("Toggle vertical navigation", ToggleNavigationMode)
            .keywords("navigation sidebar rail toolbar tabs vertical horizontal"),
        PaletteCommand::new(jig_label(jig_open), ToggleJig)
            .keywords("design jig density tokens")
            .shortcut("`"),
        PaletteCommand::new("Toggle chat", ToggleChat)
            .keywords("chat panel open close")
            .shortcut(chat_key()),
        PaletteCommand::new("Add to chat", AddToChat)
            .keywords("chat context reference attach row details")
            .shortcut(add_to_chat_key()),
        PaletteCommand::new("Help", ToggleHelp)
            .keywords("help keys shortcuts board documentation guide markdown manual setup")
            .shortcut("?"),
        PaletteCommand::new("Keyboard Shortcuts", ToggleShortcuts)
            .keywords("keyboard keys bindings help")
            .shortcut(shortcuts_key()),
        PaletteCommand::new("Open settings", OpenSettings)
            .keywords("preferences tokens credentials theme font appearance")
            .shortcut(settings_key()),
        PaletteCommand::new("ⓘ About", OpenAbout)
            .keywords("version build repository contact information"),
    ]
}

/// The terminal's commands; `open` and `maximized` say what happens next.
pub(super) fn terminal(open: bool, maximized: bool) -> Vec<PaletteCommand> {
    let (toggle, maximize) = (
        if open {
            "Hide terminal"
        } else {
            "Show terminal"
        },
        if open && maximized {
            "Restore terminal"
        } else {
            "Maximize terminal"
        },
    );
    vec![
        PaletteCommand::new(toggle, ToggleTerminal)
            .keywords("terminal shell console panel open close dock")
            .shortcut(crate::terminal::toggle_key()),
        PaletteCommand::new(maximize, ToggleTerminalMaximized)
            .keywords("terminal shell maximize restore zoom fill window"),
        PaletteCommand::new("Move terminal left", MoveTerminalLeft)
            .keywords("terminal shell dock left side position layout"),
        PaletteCommand::new("Move terminal to bottom", MoveTerminalToBottom)
            .keywords("terminal shell dock bottom position layout"),
        PaletteCommand::new("Move terminal right", MoveTerminalRight)
            .keywords("terminal shell dock right side position layout"),
        PaletteCommand::new("Toggle terminal full window width", ToggleTerminalFullWidth)
            .keywords("terminal shell bottom full width span beside chat layout"),
        PaletteCommand::new("Terminal help", ShowTerminalHelp)
            .keywords("terminal shell help guide ghostty keys"),
    ]
}

/// Puts `key`, from [`crate::palette::command_recent_key`], first in the
/// recent commands.
pub(super) fn remember(key: String, cx: &mut Context<SubspaceApp>) {
    let recent = prefs::get_list(cx, RECENT_COMMANDS_KEY);
    let recent = subspace_core::palette::push_recent(&recent, key, super::super::MAX_RECENT);
    prefs::set_list(cx, RECENT_COMMANDS_KEY, &recent);
}

#[cfg(test)]
mod tests {
    use super::{global, jig_label, log_key, search_key, terminal, wrap_label};
    use crate::app::keys::{add_to_chat_key, chat_key, settings_key};

    #[test]
    fn the_toggle_labels_say_what_happens_next() {
        assert_eq!(wrap_label(true), "Truncate titles");
        assert_eq!(wrap_label(false), "Wrap titles");
        assert_eq!(jig_label(true), "Close theme designer");
        assert_eq!(jig_label(false), "Open theme designer");
    }

    #[test]
    fn open_logs_offer_close_logs_instead_of_show_logs() {
        let labels = |logs_open| {
            global(false, false, logs_open)
                .into_iter()
                .map(|command| command.label.to_string())
                .collect::<Vec<_>>()
        };
        let closed = labels(false);
        let open = labels(true);
        assert!(closed.contains(&"Show logs".into()) && !closed.contains(&"Close logs".into()));
        assert!(open.contains(&"Close logs".into()) && !open.contains(&"Show logs".into()));
        assert_eq!(open.len(), closed.len());
    }

    #[test]
    fn terminal_commands_say_what_happens_next_and_show_the_toggle_key() {
        let labels = |open, maximized| {
            terminal(open, maximized)
                .into_iter()
                .map(|command| command.label.to_string())
                .collect::<Vec<_>>()
        };
        let closed = labels(false, false);
        assert_eq!(closed[..2], ["Show terminal", "Maximize terminal"]);
        assert!(closed.contains(&"Terminal help".to_owned()));
        assert!(closed.contains(&"Move terminal left".to_owned()));
        assert_eq!(
            labels(true, true)[..2],
            ["Hide terminal", "Restore terminal"]
        );
        assert_eq!(
            terminal(false, false)[0].shortcut.as_deref(),
            Some("Ctrl+`")
        );
    }

    #[test]
    fn global_commands_include_chat_and_show_platform_shortcuts() {
        let commands = global(false, false, false);
        let shortcut = |label: &str| {
            commands
                .iter()
                .find(|command| command.label == label)
                .and_then(|command| command.shortcut.as_deref())
        };
        assert_eq!(shortcut("Toggle chat"), Some(chat_key()));
        assert_eq!(shortcut("Add to chat"), Some(add_to_chat_key()));
        assert_eq!(shortcut("Search Space content"), Some(search_key()));
        assert_eq!(shortcut("Show logs"), Some(log_key()));
        assert_eq!(shortcut("Open settings"), Some(settings_key()));
        assert_eq!(
            shortcut("Keyboard Shortcuts"),
            Some(if cfg!(target_os = "macos") {
                "⌘⇧/"
            } else {
                "Ctrl+Shift+/"
            })
        );
    }
}
