//! Shared error actions and the error-free About diagnostics command.

use gpui_kit::component::WindowExt as _;
use gpui_kit::component::notification::{Notification, NotificationDelivery};
use gpui_kit::{
    App, ElementId, Global, IntoElement, ParentElement as _, RenderOnce, SharedString, Styled as _,
    Window,
};
use subspace_core::diagnostics::{ErrorDetails, LocalDiagnostics};
use subspace_lucide::IconName;

use crate::action::{Intent, action_button};
use crate::icon_button::IconButton;
use crate::tokens::ButtonRole;

/// Common diagnostics collected by the shell before rendering any error view.
pub struct CopyDiagnostics(LocalDiagnostics);

impl CopyDiagnostics {
    /// Install with `cx.set_global(CopyDiagnostics::new(diagnostics))`.
    pub fn new(diagnostics: LocalDiagnostics) -> Self {
        Self(diagnostics)
    }
}

impl Global for CopyDiagnostics {}

/// Native, keyboard-accessible copy action. The caller owns the error facts;
/// the shared control owns delivery and transient in-app confirmation.
#[derive(IntoElement)]
pub struct CopyDetails {
    id: ElementId,
    error: Option<ErrorDetails>,
    diagnostics: Option<LocalDiagnostics>,
    role: ButtonRole,
}

impl CopyDetails {
    /// Copy an error followed by the shell's local diagnostics.
    pub fn new(id: impl Into<ElementId>, error: ErrorDetails) -> Self {
        Self {
            id: id.into(),
            error: Some(error),
            diagnostics: None,
            role: ButtonRole::Action,
        }
    }

    /// Copy the same diagnostics block without an error, for About.
    pub fn diagnostics(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            error: None,
            diagnostics: None,
            role: ButtonRole::Action,
        }
    }

    /// Size the buttons for `role`, such as [`ButtonRole::Dialog`] in a footer.
    pub fn role(mut self, role: ButtonRole) -> Self {
        self.role = role;
        self
    }

    /// Supply fixture diagnostics without replacing the application's global.
    pub fn with_diagnostics(mut self, diagnostics: LocalDiagnostics) -> Self {
        self.diagnostics = Some(diagnostics);
        self
    }
}

impl RenderOnce for CopyDetails {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let show_log = self.error.is_some().then(|| {
            action_button(
                ElementId::NamedChild(std::sync::Arc::new(self.id.clone()), "show-log".into()),
                Intent::Neutral,
                Some(IconName::FileText),
                "Show log",
            )
            .role(self.role)
            .focus_on_click()
            .on_click(|_, window, cx| {
                window.dispatch_action(Box::new(crate::log_view::OpenLogView), cx)
            })
        });
        let (label, confirmation) = if self.error.is_some() {
            ("Copy details", "Details copied")
        } else {
            ("Copy diagnostics", "Diagnostics copied")
        };
        let copy = action_button(self.id, Intent::Neutral, Some(IconName::Copy), label)
            .role(self.role)
            .on_click(move |_, window, cx| {
                let diagnostics = self.diagnostics.clone().unwrap_or_else(|| common(cx));
                crate::links::copy(diagnostics.local_copy(self.error.as_ref()).markdown(), cx);
                window.push_notification(
                    Notification::new()
                        .id::<CopyDetails>()
                        .message(confirmation)
                        .delivery(NotificationDelivery::InApp),
                    cx,
                );
            });
        gpui_kit::component::h_flex()
            .gap_2()
            .flex_wrap()
            .child(copy)
            .children(show_log)
    }
}

/// A neutral Copy-glyph action that copies the text `text` builds when it is
/// activated, then confirms with the same temporary in-app notification.
/// The text is built on activation, not during render.
pub fn copy_text(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    confirmation: impl Into<SharedString>,
    text: impl Fn(&App) -> String + 'static,
) -> IconButton {
    let confirmation = confirmation.into();
    action_button(id, Intent::Neutral, Some(IconName::Copy), label).on_click(
        move |_, window, cx| {
            crate::links::copy(text(cx), cx);
            window.push_notification(
                Notification::new()
                    .id::<CopyDetails>()
                    .message(confirmation.clone())
                    .delivery(NotificationDelivery::InApp),
                cx,
            );
        },
    )
}

fn common(cx: &App) -> LocalDiagnostics {
    cx.try_global::<CopyDiagnostics>()
        .map(|diagnostics| diagnostics.0.clone())
        .unwrap_or_default()
}

/// Sanitize the explicitly offered report paths without enabling their inclusion.
pub fn issue_paths(config_path: Option<&str>, cx: &App) -> subspace_core::diagnostics::IssuePaths {
    common(cx).issue_paths(config_path)
}

/// Record an accepted error occurrence, never from a renderer or copy callback.
pub fn log_error(target: &str, surface: &str, error: &ErrorDetails, cx: &App) {
    common(cx).log_error(&subspace_core::log::Logger::new(target), surface, error);
}

#[cfg(test)]
mod tests;
