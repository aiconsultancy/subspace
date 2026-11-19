//! The optional quit confirmation (REQ-34). Boards and the docked terminal
//! name their busy programs through `quit_warnings`; the shell names none. Quit and
//! restart, after an update installs, takes the same path.

use gpui_kit::{AnyElement, App, Context, IntoElement, ParentElement, SharedString, div};
use subspace_dashboard::action::{Intent, dialog_button};
use subspace_dashboard::{Tokens, dialog, prefs};
use subspace_lucide::IconName;

use super::SubspaceApp;
use super::keys::save_prefs;
use crate::settings::CONFIRM_QUIT_KEY;

/// What happens after the app quits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Exit {
    Quit,
    /// Open the app again, as after installing an update.
    Restart,
}

impl Exit {
    fn title(self) -> &'static str {
        match self {
            Self::Quit => "Quit",
            Self::Restart => "Quit and restart",
        }
    }
}

impl SubspaceApp {
    pub(super) fn request_quit(&mut self, cx: &mut Context<Self>) {
        self.request_exit(Exit::Quit, cx);
    }

    pub(super) fn request_restart(&mut self, cx: &mut Context<Self>) {
        self.request_exit(Exit::Restart, cx);
    }

    fn request_exit(&mut self, exit: Exit, cx: &mut Context<Self>) {
        if prefs::get(cx, CONFIRM_QUIT_KEY).as_deref() != Some("true") {
            exit_now(exit, cx);
            return;
        }
        self.quit_exit = exit;
        let warnings = self
            .dashboards
            .iter()
            .chain(self.parked.iter().map(|parked| &parked.handle))
            .map(|dashboard| dashboard.quit_warnings(cx))
            .chain([self.terminal_quit_warnings(cx)])
            .collect::<Vec<_>>();
        self.quit_warnings = Some(collect_quit_warnings(&warnings));
        cx.notify();
    }

    fn cancel_quit(&mut self, cx: &mut Context<Self>) {
        self.quit_warnings = None;
        cx.notify();
    }

    pub(super) fn render_quit_dialog(
        &self,
        tokens: &Tokens,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let warnings = self.quit_warnings.as_ref()?;
        let exit = self.quit_exit;
        let body = div().child(quit_heading(warnings)).children(
            warnings
                .iter()
                .map(|warning| div().child(warning.clone()).into_any_element()),
        );
        let footer = dialog::footer()
            .child(
                dialog_button("cancel-quit", Intent::Neutral, Some(IconName::X), "Cancel")
                    .on_click(cx.listener(|app, _, _, cx| app.cancel_quit(cx))),
            )
            .child(
                dialog_button(
                    "confirm-quit",
                    Intent::Primary,
                    Some(confirm_icon(exit)),
                    exit.title(),
                )
                .on_click(cx.listener(move |_, _, _, cx| exit_now(exit, cx))),
            );
        let frame = dialog::frame(tokens, dialog::DialogWidth::D540, exit.title())
            .child(body)
            .child(footer);
        Some(dialog::backdrop(tokens, frame).into_any_element())
    }
}

/// Save the preferences and quit, then open the app again for a restart.
/// Dropping the window drops every dashboard, so a board's own teardown (a
/// terminal closing its sessions) runs. GPUI's restart waits for this
/// process to end, then opens the app bundle or executable, which an
/// installed update has already replaced.
fn exit_now(exit: Exit, cx: &mut App) {
    save_prefs(cx);
    match exit {
        Exit::Quit => cx.quit(),
        Exit::Restart => cx.restart(),
    }
}

fn confirm_icon(exit: Exit) -> subspace_lucide::SvgIcon {
    match exit {
        Exit::Quit => IconName::LogOut,
        Exit::Restart => IconName::RotateCw,
    }
}

fn quit_heading(warnings: &[SharedString]) -> &'static str {
    if warnings.is_empty() {
        "Quit the app?"
    } else {
        "Quit the app? These programs are still running:"
    }
}

fn collect_quit_warnings(warnings: &[Vec<SharedString>]) -> Vec<SharedString> {
    warnings
        .iter()
        .flat_map(|board| board.iter().cloned())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{Exit, collect_quit_warnings, confirm_icon, quit_heading};
    use gpui_kit::SharedString;

    #[test]
    fn quit_warnings_keep_board_order() {
        let warnings = collect_quit_warnings(&[
            vec![SharedString::from("vim")],
            vec![SharedString::from("sleep"), SharedString::from("top")],
        ]);
        assert_eq!(warnings, ["vim", "sleep", "top"]);
    }

    #[test]
    fn a_restart_names_itself_in_the_confirmation() {
        assert_eq!(Exit::Quit.title(), "Quit");
        assert_eq!(Exit::Restart.title(), "Quit and restart");
        assert_ne!(confirm_icon(Exit::Quit), confirm_icon(Exit::Restart));
    }

    #[test]
    fn the_heading_names_running_programs_only_when_there_are_some() {
        assert_eq!(quit_heading(&[]), "Quit the app?");
        assert!(quit_heading(&[SharedString::from("vim")]).ends_with("still running:"));
    }
}
