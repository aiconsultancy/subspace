//! Designer preview lifetime follows the existing shell overlay.

use gpui_kit::{Context, Window};

use super::SubspaceApp;

/// Whether the theme designer shows. Only [`SubspaceApp::set_jig`] changes
/// it, so every close goes through `Jig::cancel` and its busy guard.
#[derive(Clone, Copy, Default)]
pub(super) struct JigOpen(bool);

impl JigOpen {
    pub(super) fn get(self) -> bool {
        self.0
    }
}

impl SubspaceApp {
    pub(super) fn set_jig(&mut self, open: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.settings.is_some() || (!open && !self.jig.read(cx).can_close()) {
            return;
        }
        if self.jig_open.0 == open {
            return;
        }
        self.jig_open = JigOpen(open);
        if open {
            self.jig.update(cx, |jig, cx| jig.begin(window, cx));
        } else {
            self.jig.update(cx, |jig, cx| jig.cancel(window, cx));
            self.focus_active(window, cx);
        }
        cx.notify();
    }

    /// Close the designer, restoring its opening appearance. Returns false
    /// when it stays open because a save is still running.
    pub(super) fn close_jig(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        self.set_jig(false, window, cx);
        !self.jig_open.get()
    }
}
