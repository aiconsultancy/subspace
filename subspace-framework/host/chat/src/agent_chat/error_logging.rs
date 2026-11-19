//! Accepted facts wait only for their owning effect callback, never for a render.

use gpui_kit::App;
use subspace_core::diagnostics::ErrorDetails;

use super::ChatPanel;
use super::failure_view::legacy::legacy_error_details;
use super::tabs::Tab;

#[derive(Default)]
pub(super) struct AcceptedErrors {
    pub(super) legacy: Option<ErrorDetails>,
    pending: Vec<(&'static str, ErrorDetails)>,
}

impl AcceptedErrors {
    fn accept_legacy(&mut self, details: ErrorDetails) {
        self.legacy = Some(details.clone());
        self.pending.push(("chat-error", details));
    }

    pub(super) fn accept_failure(&mut self, details: ErrorDetails) {
        self.pending.push(("chat-failure", details));
    }

    fn log(&mut self, cx: &App) {
        for (surface, details) in self.pending.drain(..) {
            subspace_dashboard::copy_details::log_error("agent", surface, &details, cx);
        }
    }
}

impl Tab {
    pub(super) fn set_error(&mut self, error: String) {
        let details = legacy_error_details(&error, self);
        self.errors.accept_legacy(details);
        self.error = Some(error);
    }

    pub(super) fn set_error_chain(&mut self, error: String, chain: String) {
        let details = legacy_error_details(&error, self).with_chain(chain);
        self.errors.accept_legacy(details);
        self.error = Some(error);
    }
}

impl ChatPanel {
    pub(super) fn reject_unavailable_copilot_controls(
        &mut self,
        cx: &mut super::Context<Self>,
    ) -> bool {
        if self.capabilities().copilot_controls {
            return false;
        }
        self.tab_mut()
            .set_error("YOLO is unavailable with Pi. Choose Copilot in Settings to use it.".into());
        self.log_accepted_errors(cx);
        cx.notify();
        true
    }

    pub(super) fn set_error(&mut self, error: String) {
        let details = legacy_error_details(&error, self.tab());
        self.errors.accept_legacy(details);
        self.error = Some(error);
    }

    pub(super) fn log_accepted_errors(&mut self, cx: &App) {
        self.errors.log(cx);
        for tab in &mut self.tabs {
            tab.errors.log(cx);
        }
    }
}
