//! The settings dialog: opening it, saving from it and restoring the
//! previewed appearance when it closes.

use gpui_kit::component::WindowExt as _;
use gpui_kit::{AppContext as _, Context, Entity, ParentElement as _, Styled as _, Window, px};
use subspace_dashboard::{Modal, Toast};

mod footer;
#[cfg(test)]
mod interaction_tests;
mod layout;
mod lifecycle;

use lifecycle::Lifecycle;

use super::{OpenSettings, SubspaceApp};
use crate::settings::Settings;

/// The settings dialog's size, in pixels.
const SETTINGS_MAX_WIDTH: f32 = 1080.;
const SETTINGS_MARGIN: f32 = 32.;
const SETTINGS_MIN_WIDTH: f32 = 320.;
const ZERO_WIDTH: f32 = 0.;

fn settings_width(viewport_width: f32) -> f32 {
    let available = (viewport_width - SETTINGS_MARGIN).max(ZERO_WIDTH);
    available
        .min(SETTINGS_MAX_WIDTH)
        .max(available.min(SETTINGS_MIN_WIDTH))
}

impl SubspaceApp {
    pub(super) fn configure_board(
        &mut self,
        _: &subspace_dashboard::ConfigureBoard,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.settings.is_some() {
            self.open_settings(&OpenSettings, window, cx);
            return;
        }
        if let Some(board) = self.dashboard().map(|dashboard| dashboard.id) {
            self.open_board_settings(board, window, cx);
        } else {
            self.open_settings(&OpenSettings, window, cx);
        }
    }

    /// Opens Settings at Space `board`'s page.
    pub(super) fn open_board_settings(
        &mut self,
        board: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_settings(&OpenSettings, window, cx);
        if let Some(settings) = &self.settings {
            settings.update(cx, |settings, cx| settings.select_initial_board(board, cx));
        }
    }

    /// Developer settings… opens Settings at the Developer page.
    #[cfg(debug_assertions)]
    pub(super) fn open_developer_settings(
        &mut self,
        _: &crate::inspector::OpenDeveloperSettings,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_settings(&OpenSettings, window, cx);
        if let Some(settings) = &self.settings {
            settings.update(cx, |settings, cx| settings.select_developer_page(cx));
        }
    }

    pub(super) fn open_settings(
        &mut self,
        _: &OpenSettings,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.settings_shown(window, cx) || !self.ready_for_settings(window, cx) {
            return;
        }
        if !self.jig.read(cx).can_close() {
            return;
        }
        self.dismiss_shortcuts(window, cx);
        self.set_jig(false, window, cx);
        self.open_new_settings(window, cx);
    }

    /// Whether settings are already open; focuses them if they are.
    fn settings_shown(&self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(settings) = &self.settings else {
            return false;
        };
        settings.update(cx, |settings, cx| settings.focus(window, cx));
        true
    }

    /// Dismiss the dashboard's own overlay; a dialog of its own wins.
    fn ready_for_settings(&self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(dashboard) = self.dashboard() else {
            return true;
        };
        if dashboard.modal(cx) == Some(Modal::Dialog) {
            return false;
        }
        dashboard.dismiss(window, cx);
        true
    }

    /// Read the settings file, then show the dialog over the shell.
    fn open_new_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let settings = match Settings::open(window, cx) {
            Ok(settings) => settings,
            Err(error) => {
                self.push_toast(Toast::error(open_failure(&error)), cx);
                return;
            }
        };
        self.session = None;
        self.help_open = false;
        self.focus_active(window, cx);
        self.settings = Some(settings.clone());
        Self::show_settings_dialog(settings.clone(), window, cx);
        settings.update(cx, |settings, cx| settings.focus(window, cx));
        cx.notify();
    }

    pub(super) fn show_settings_dialog(
        settings: Entity<Settings>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let app = cx.weak_entity();
        let lifecycle = Lifecycle::new(settings.clone(), app);
        let restart = lifecycle.clone();
        let updates = super::about_dialog::update_panel::compact::settings_updates(
            move |window, cx| restart.restart(window, cx),
            cx,
        );
        let view =
            cx.new(|cx| layout::SettingsDialog::new(settings, updates, lifecycle.clone(), cx));
        window.open_dialog(cx, move |dialog, window, _| {
            let width = settings_width(f32::from(window.viewport_size().width));
            let submit = lifecycle.clone();
            let cancel = lifecycle.clone();
            let close = lifecycle.clone();
            dialog
                .p_0()
                .gap_0()
                .overflow_hidden()
                .width(px(width))
                .max_w(px(SETTINGS_MAX_WIDTH))
                .overlay_closable(false)
                .content({
                    let view = view.clone();
                    move |content, _, _| content.p_0().gap_0().child(view.clone())
                })
                .on_ok(move |_, window, cx| submit.confirm_main(window, cx))
                .on_cancel(move |_, window, cx| cancel.cancel(window, cx))
                .on_close(move |_, window, cx| {
                    close.close(window, cx);
                })
        });
    }

    pub(super) fn close_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(settings) = &self.settings
            && let Err(error) = settings.update(cx, |s, cx| s.restore_preview(window, cx))
        {
            self.push_toast(Toast::error(restore_failure(&error)), cx);
        }
        self.settings = None;
        self.focus_active(window, cx);
        cx.notify();
    }
}

fn open_failure(error: &str) -> String {
    format!("Cannot open settings: {error}")
}

fn restore_failure(error: &str) -> String {
    format!("Cannot restore appearance: {error}")
}

fn saved_message(restart: bool) -> &'static str {
    if restart {
        "Settings saved. Restart Subspace to apply service changes."
    } else {
        "Settings saved."
    }
}

#[cfg(test)]
mod tests {
    use super::{SETTINGS_MARGIN, open_failure, restore_failure, saved_message, settings_width};

    #[test]
    pub(super) fn the_messages_name_their_cause() {
        assert!(open_failure("no dir").ends_with("no dir"));
        assert!(restore_failure("no theme").ends_with("no theme"));
        assert!(saved_message(true).contains("Restart"));
        assert_eq!(saved_message(false), "Settings saved.");
        assert_eq!(settings_width(1000.), 968.);
        assert_eq!(settings_width(2200.), 1080.);
        assert_eq!(settings_width(300.), 268.);
        for viewport in [0., 300., 480., 1000., 2200.] {
            assert!(settings_width(viewport) <= (viewport - SETTINGS_MARGIN).max(0.));
        }
    }
}
