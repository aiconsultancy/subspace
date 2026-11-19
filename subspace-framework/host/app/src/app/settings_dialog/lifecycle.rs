//! The cancel veto owns the draft until a separate native dialog resolves it.

use std::{cell::Cell, rc::Rc};

use gpui_kit::component::WindowExt as _;
use gpui_kit::{App, Entity, WeakEntity, Window};
use subspace_dashboard::Toast;

use super::{Settings, SubspaceApp, footer, saved_message};
use crate::app::OpenLogView;

#[derive(Clone, Copy, PartialEq)]
enum Phase {
    Editing,
    Confirming,
    Exiting,
    Closed,
}

#[derive(Clone, Copy)]
enum AfterClose {
    ShowLog,
    Restart,
}

#[derive(Clone)]
pub(super) struct Lifecycle {
    settings: Entity<Settings>,
    app: WeakEntity<SubspaceApp>,
    phase: Rc<Cell<Phase>>,
    after_close: Rc<Cell<Option<AfterClose>>>,
}

impl Lifecycle {
    pub(super) fn new(settings: Entity<Settings>, app: WeakEntity<SubspaceApp>) -> Self {
        Self {
            settings,
            app,
            phase: Rc::new(Cell::new(Phase::Editing)),
            after_close: Rc::new(Cell::new(None)),
        }
    }

    fn save(&self, window: &mut Window, cx: &mut App) -> bool {
        match self
            .settings
            .update(cx, |settings, cx| settings.save(window, cx))
        {
            Ok(restart) => {
                _ = self.app.update(cx, |app, cx| {
                    app.sync_navigation_preferences(window, cx);
                    app.sync_terminal_preferences(window, cx);
                    app.push_toast(Toast::info(saved_message(restart)), cx);
                });
                true
            }
            Err(error) => {
                self.settings
                    .update(cx, |settings, cx| settings.show_error(error, cx));
                false
            }
        }
    }

    pub(super) fn cancel(&self, window: &mut Window, cx: &mut App) -> bool {
        if self.phase.get() != Phase::Editing {
            return false;
        }
        if !self.settings.read(cx).has_unsaved_changes(cx) {
            return true;
        }
        self.phase.set(Phase::Confirming);
        let lifecycle = self.clone();
        // Return the veto before adding another Root dialog or changing focus.
        window.defer(cx, move |window, cx| lifecycle.confirm(window, cx));
        false
    }

    pub(super) fn dismiss(&self, window: &mut Window, cx: &mut App) {
        if self.cancel(window, cx) {
            window.close_dialog(cx);
            self.close(window, cx);
        }
    }

    pub(super) fn show_log(&self, window: &mut Window, cx: &mut App) {
        self.leave_for(AfterClose::ShowLog, window, cx);
    }

    pub(super) fn restart(&self, window: &mut Window, cx: &mut App) {
        self.leave_for(AfterClose::Restart, window, cx);
    }

    fn leave_for(&self, action: AfterClose, window: &mut Window, cx: &mut App) {
        if self.phase.get() == Phase::Editing {
            self.after_close.set(Some(action));
            self.dismiss(window, cx);
        }
    }

    pub(super) fn save_and_close(&self, window: &mut Window, cx: &mut App) {
        if self.confirm_main(window, cx) {
            window.close_dialog(cx);
            self.close(window, cx);
        }
    }

    pub(super) fn confirm_main(&self, window: &mut Window, cx: &mut App) -> bool {
        self.phase.get() == Phase::Editing && self.save(window, cx)
    }

    pub(super) fn close(&self, window: &mut Window, cx: &mut App) {
        self.phase.set(Phase::Closed);
        let after_close = self.after_close.take();
        _ = self.app.update(cx, |app, cx| {
            if app.settings.as_ref() == Some(&self.settings) {
                app.close_settings(window, cx);
                match after_close {
                    Some(AfterClose::ShowLog) => app.open_log_view(&OpenLogView, window, cx),
                    Some(AfterClose::Restart) => app.request_restart(cx),
                    None => {}
                }
            }
        });
    }

    fn confirm(&self, window: &mut Window, cx: &mut App) {
        if self.phase.get() != Phase::Confirming {
            return;
        }
        let lifecycle = self.clone();
        window.open_alert_dialog(cx, move |dialog, _, _| {
            let save = lifecycle.clone();
            let close = lifecycle.clone();
            dialog
                .title("Unsaved settings")
                .description("Save your changes before leaving Settings, or exit without saving.")
                .close_button(true)
                .footer(footer::confirmation(lifecycle.clone()))
                .on_ok(move |_, window, cx| save.confirm_save(window, cx))
                .on_close(move |_, window, cx| close.confirmation_closed(window, cx))
        });
    }

    fn confirm_save(&self, window: &mut Window, cx: &mut App) -> bool {
        if self.phase.get() != Phase::Confirming {
            return false;
        }
        if self.save(window, cx) {
            self.phase.set(Phase::Exiting);
        }
        // A failed save returns to the editor so its existing error is visible.
        true
    }

    pub(super) fn save_and_exit(&self, window: &mut Window, cx: &mut App) {
        if self.phase.get() == Phase::Confirming {
            self.confirm_save(window, cx);
            window.close_dialog(cx);
            self.confirmation_closed(window, cx);
        }
    }

    pub(super) fn discard(&self, window: &mut Window, cx: &mut App) {
        if self.phase.get() == Phase::Confirming {
            self.phase.set(Phase::Exiting);
            window.close_dialog(cx);
            self.confirmation_closed(window, cx);
        }
    }

    fn confirmation_closed(&self, window: &mut Window, cx: &mut App) {
        match self.phase.replace(Phase::Editing) {
            Phase::Exiting => {
                // Pop the underlying dialog explicitly, never dispatch from the
                // confirmation's old action anchor into the Settings dialog.
                window.close_dialog(cx);
                self.close(window, cx);
            }
            Phase::Confirming => {
                self.after_close.set(None);
                self.settings
                    .update(cx, |settings, cx| settings.focus(window, cx));
                window.refresh();
            }
            phase => self.phase.set(phase),
        }
    }
}
