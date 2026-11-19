use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{Entity, ParentElement as _, Styled as _};
use subspace_framework::dashboard::action::Intent;
use subspace_framework::dashboard::{Tokens, feedback};
use subspace_framework::lucide::IconName;
use subspace_terminal::TerminalView;

use crate::keys::{NewTab, Restart, Retry};

pub(crate) fn running(view: &Entity<TerminalView>, exited: bool) -> gpui_kit::Div {
    v_flex()
        .size_full()
        .child(view.clone())
        .when(exited, |element| {
            element.child(
                feedback::recovery_action(
                    "terminal-restart",
                    Intent::Primary,
                    IconName::RotateCcw,
                    "Restart",
                    |_, window, cx| window.dispatch_action(Box::new(Restart), cx),
                )
                .tooltip("Restart terminal")
                .accessibility_label("Restart this terminal session"),
            )
        })
}

pub(crate) fn startup_error(error: &str, tokens: &Tokens) -> gpui_kit::Div {
    feedback::error_box_with_actions(
        tokens,
        "terminal-startup",
        "Terminal failed to start",
        error,
        [feedback::recovery_action(
            "terminal-retry",
            Intent::Primary,
            IconName::RefreshCw,
            "Retry",
            |_, window, cx| window.dispatch_action(Box::new(Retry), cx),
        )
        .accessibility_label("Retry starting this terminal session")],
    )
}

pub(crate) fn no_session(tokens: &Tokens) -> gpui_kit::Div {
    v_flex()
        .size_full()
        .child(feedback::empty(
            tokens,
            "terminal-no-session",
            "No terminal session",
            "Open a new tab to start a terminal session.",
        ))
        .child(
            h_flex().justify_center().child(
                feedback::recovery_action(
                    "terminal-new-tab-empty",
                    Intent::Primary,
                    IconName::Plus,
                    "New tab",
                    |_, window, cx| window.dispatch_action(Box::new(NewTab), cx),
                )
                .tooltip(new_tab_tooltip())
                .accessibility_label("Open a new terminal tab"),
            ),
        )
}

#[cfg(target_os = "macos")]
fn new_tab_tooltip() -> &'static str {
    "New terminal tab (Cmd+T)"
}

#[cfg(not(target_os = "macos"))]
fn new_tab_tooltip() -> &'static str {
    "New terminal tab (Ctrl+Shift+T)"
}
