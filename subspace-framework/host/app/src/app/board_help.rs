//! Compatibility commands now open the same Help Dock as the toolbar and `?`.

use gpui_kit::{Context, Window};

use super::{CloseBoardHelp, CloseHelp, SubspaceApp, ToggleBoardHelp, ToggleHelp};

pub(super) fn missing_help(board: Option<&str>) -> String {
    match board {
        Some(board) => format!("The {board} Space has no bundled help yet."),
        None => "Open a Space to see its help.".into(),
    }
}

impl SubspaceApp {
    pub(super) fn toggle_board_help(
        &mut self,
        _: &ToggleBoardHelp,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_help(&ToggleHelp, window, cx);
    }

    pub(super) fn close_board_help(
        &mut self,
        _: &CloseBoardHelp,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_help(&CloseHelp, window, cx);
    }
}

pub(super) fn no_boards_message(config_file: &str) -> String {
    format!("All Spaces are turned off in {config_file}. Open Settings and enable a Space.")
}

#[cfg(test)]
mod tests {
    use super::{missing_help, no_boards_message};

    #[test]
    fn missing_help_names_only_the_current_board() {
        assert_eq!(
            missing_help(Some("Actions")),
            "The Actions Space has no bundled help yet."
        );
        assert_eq!(missing_help(None), "Open a Space to see its help.");
        assert!(no_boards_message("/fixture/config.json").contains("/fixture/config.json"));
    }
}
