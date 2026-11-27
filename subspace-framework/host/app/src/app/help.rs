//! Unified Help commands and the active board's shortcut snapshot.

use gpui_kit::{App, Context, SharedString, Window};
use subspace_dashboard::KeyHelp;

use super::keys::{add_to_chat_key, chat_key, palette_key, settings_key, shortcuts_key};
use super::{CloseHelp, SubspaceApp, ToggleHelp};

fn shortcut(mac: &'static str, other: &'static str) -> &'static str {
    if cfg!(target_os = "macos") {
        mac
    } else {
        other
    }
}

fn shell_keys() -> [(&'static str, &'static str); 24] {
    let find = subspace_dashboard::find_in_page::shortcuts();
    [
        (find.open, "Find in this Space"),
        (find.next, "Next find match"),
        (find.previous, "Previous find match"),
        ("Esc", "Close find before closing content"),
        (
            shortcut("⌘⌥F", "Ctrl+Alt+F"),
            "In Help: focus the shortcut filter",
        ),
        ("w", "Wrap long titles"),
        (shortcut("r / ⌘R", "r / Ctrl+R"), "Reload"),
        ("`", "Theme designer"),
        (settings_key(), "Settings"),
        (shortcut("⌘⇧L", "Ctrl+Shift+L"), "Show the log viewer"),
        ("F1–F12", "Spaces, in tab order, as Secondary+1–9 do"),
        (shortcut("⌘1–⌘9", "Ctrl+1–Ctrl+9"), "Spaces, in tab order"),
        (chat_key(), "Show or hide Chat"),
        (add_to_chat_key(), "Add the row and its details to Chat"),
        (
            shortcut("⌘T / ⌘W", "Ctrl+T / Ctrl+W"),
            "In Chat: new / close tab",
        ),
        (
            shortcut("⌘⇧[ / ⌘⇧]", "Ctrl+Shift+[ / Ctrl+Shift+]"),
            "In Chat: previous / next tab",
        ),
        (
            shortcut("⌘⌥G", "Ctrl+Alt+G"),
            "In Chat: open / close the canvas gallery (Esc closes, ← / → step)",
        ),
        (
            "Panel edge ← / →",
            "Widen / narrow the detail panel (Shift for bigger steps)",
        ),
        ("1 – 9", "In a menu: pick the numbered option"),
        ("Ctrl+N / Ctrl+P", "In a menu: next / previous option"),
        ("Tab / Esc", "Close a menu"),
        ("?", "Show or hide Help"),
        (shortcuts_key(), "Keyboard Shortcuts"),
        (shortcut("⌘Q", "Ctrl+Q"), "Quit"),
    ]
}

impl SubspaceApp {
    /// Whether Help is zoomed and so covers the Space.
    pub(super) fn help_zoomed(&self, cx: &App) -> bool {
        self.help_dock
            .as_ref()
            .is_some_and(|host| host.read(cx).workspace_is_zoomed(cx))
    }

    pub(super) fn toggle_help(
        &mut self,
        _: &ToggleHelp,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.settings.is_some() {
            return;
        }
        // Help opened from a focused terminal shows the terminal's guide.
        self.help_terminal = !self.help_open && self.terminal_focused(window, cx);
        self.set_help_open(!self.help_open, window, cx);
    }

    pub(super) fn set_help_open(
        &mut self,
        open: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.session = None;
        self.ensure_help_dock(window, cx);
        self.help_open = open;
        if let Some(host) = &self.help_dock {
            host.update(cx, |host, cx| host.set_open(open, window, cx));
        }
        cx.notify();
    }

    pub(super) fn close_help(
        &mut self,
        _: &CloseHelp,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.help_open = false;
        if let Some(host) = &self.help_dock {
            host.update(cx, |host, cx| host.close(window, cx));
        }
        cx.notify();
    }

    pub(super) fn help_keys(&self, cx: &App) -> Vec<(SharedString, SharedString)> {
        let mut keys = self.board_keys(cx);
        keys.extend(terminal_keys(cx));
        keys.extend(
            shell_keys()
                .into_iter()
                .chain(super::menu_bar::help_keys().iter().copied())
                .map(|(key, meaning)| (key.into(), meaning.into())),
        );
        keys
    }

    fn board_keys(&self, cx: &App) -> Vec<(SharedString, SharedString)> {
        let Some(board) = self.dashboard() else {
            return Vec::new();
        };
        let search = board
            .search(cx)
            .map_or(SharedString::from("Search"), |search| search.placeholder);
        let mut keys = vec![
            (
                palette_key().into(),
                "Command palette: commands and items".into(),
            ),
            ("/".into(), search),
        ];
        keys.extend(
            board
                .keys(cx)
                .iter()
                .map(|KeyHelp { key, meaning }| ((*key).into(), (*meaning).into())),
        );
        keys
    }
}

/// The terminal's toggle and its own keys, for a product with a terminal.
fn terminal_keys(cx: &App) -> Vec<(SharedString, SharedString)> {
    crate::terminal::registration(cx).map_or_else(Vec::new, |registration| {
        std::iter::once((crate::terminal::toggle_key(), "Show or hide the terminal"))
            .chain(
                registration
                    .keys
                    .iter()
                    .map(|KeyHelp { key, meaning }| (*key, *meaning)),
            )
            .map(|(key, meaning)| (key.into(), meaning.into()))
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::shell_keys;

    #[test]
    fn shortcuts_include_help_and_shell_commands() {
        let keys = shell_keys();
        assert!(keys.contains(&("?", "Show or hide Help")));
        assert!(
            keys.iter()
                .any(|(_, meaning)| meaning.contains("canvas gallery"))
        );
        assert!(keys.iter().any(|(_, meaning)| *meaning == "Quit"));
        assert!(
            keys.iter()
                .all(|(key, meaning)| !key.is_empty() && !meaning.is_empty())
        );
    }
}
