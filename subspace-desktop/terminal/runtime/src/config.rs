//! The split between settings that can change on a running session and launch
//! settings that are used only when a new session starts.

use crate::state::TerminalState;
use crate::style::TerminalStyle;

/// Apply the live part of a resolved configuration to one running session.
pub(crate) fn apply_config(state: &mut TerminalState, style: TerminalStyle) {
    state.clipboard.borrow_mut().restyle(style.clipboard);
    state.style = style;
    state.rebuild = true;
    state.apply_style();
}

#[cfg(test)]
mod tests {
    #[derive(Clone, Copy)]
    enum ConfigKey {
        Background,
        FontSize,
        WindowPadding,
        CursorDefaults,
        ScrollbackLimit,
        Clipboard,
        Keybind,
        ConfirmCloseSurface,
        Command,
        WorkingDirectory,
        Term,
    }

    fn applies_to_running_session(key: ConfigKey) -> bool {
        !matches!(
            key,
            ConfigKey::Command | ConfigKey::WorkingDirectory | ConfigKey::Term
        )
    }

    #[test]
    fn reload_applies_live_settings_but_not_launch_settings() {
        let cases = [
            (ConfigKey::Background, true),
            (ConfigKey::FontSize, true),
            (ConfigKey::WindowPadding, true),
            (ConfigKey::CursorDefaults, true),
            (ConfigKey::ScrollbackLimit, true),
            (ConfigKey::Clipboard, true),
            (ConfigKey::Keybind, true),
            (ConfigKey::ConfirmCloseSurface, true),
            (ConfigKey::Command, false),
            (ConfigKey::WorkingDirectory, false),
            (ConfigKey::Term, false),
        ];

        for (key, expected) in cases {
            assert_eq!(applies_to_running_session(key), expected);
        }
    }
}
