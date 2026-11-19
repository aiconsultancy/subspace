//! Why the last turn of the chat shown failed: a title, what to do next, a
//! Details disclosure with the facts the runtime reported, and Retry and
//! Copy details. A stopped or silent runtime also offers Reconnect, and Retry
//! waits until the new runtime has the chat ready.
//!
//! Debug builds have a fixture for screenshots: set
//! `SUBSPACE_CHAT_FAILURE_FIXTURE` to a category name (`no-response`,
//! `sign-in`, `limit`, `not-allowed`, `context-full`, `network`, `stopped`,
//! `not-responding` or `unknown`), optionally with `:details`, before the
//! chat panel opens. See `fixture.rs`. Release builds leave it out.

use gpui_kit::component::Disableable as _;
use gpui_kit::component::collapsible::Collapsible;
use gpui_kit::{
    ElementId, FontWeight, InteractiveElement as _, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _,
};
use subspace_agent::{FailureCategory, TurnFailure};
use subspace_core::diagnostics::ErrorDetails;
use subspace_dashboard::copy_details::CopyDetails;
use subspace_dashboard::icon_button::IconButton;
use subspace_lucide::IconName;

use super::tab_run::TabRun;
use super::tabs::Tab;
use super::{ChatPanel, Context, Div, Tokens, div, h_flex, px, v_flex};
use crate::action::{Intent, action_button};
use crate::rich;

pub(super) mod legacy;

#[cfg(all(feature = "test-support", debug_assertions))]
mod test_support;

/// The width of a label in the Details rows.
const LABEL_WIDTH: f32 = 128.;

/// Whether Retry can send the last message again now.
pub(super) fn retry_allowed(tab: &Tab, unavailable: bool) -> bool {
    matches!(tab.run, TabRun::Idle) && !tab.read_only && tab.last_turn.is_some() && !unavailable
}

/// Whether the failure offers its own Reconnect. A stopped runtime that the
/// panel already reports offers Reconnect with that report instead.
pub(super) fn offers_reconnect(tab: &Tab, unavailable: bool) -> bool {
    let stopped = tab
        .failure
        .as_ref()
        .is_some_and(|shown| shown.failure.category.needs_reconnect());
    stopped && !tab.is_ready() && !tab.read_only && !unavailable
}

impl ChatPanel {
    /// The failed turn of the chat shown, if its last turn failed.
    pub(super) fn render_failure(&self, tokens: &Tokens, cx: &mut Context<Self>) -> Option<Div> {
        let tab = self.tab();
        let shown = tab.failure.as_ref()?;
        let unavailable = self.is_unavailable();
        Some(
            div().child(
                v_flex()
                    .id("chat-failure")
                    .test_support()
                    .p_2()
                    .gap_1()
                    .child(summary(shown.failure.category, tokens))
                    .child(details(shown.details_open, &shown.failure, tokens, cx))
                    .child(buttons(tab, unavailable, cx)),
            ),
        )
    }

    /// Sends the last message again, with its context and images. With no
    /// runtime, as in the debug fixture, there is nothing to send to, so the
    /// failure stays.
    pub(super) fn retry(&mut self, cx: &mut Context<Self>) {
        let unavailable = self.is_unavailable();
        let tab = self.tab();
        let Some(turn) = tab
            .last_turn
            .clone()
            .filter(|_| self.handle.is_some() && retry_allowed(tab, unavailable))
        else {
            return;
        };
        if self.send_chat(turn.clone().into_command(), cx) {
            self.tab_mut().sent(turn);
        }
        self.sync_toolbar(cx);
    }

    fn toggle_failure_details(&mut self, cx: &mut Context<Self>) {
        if let Some(shown) = &mut self.tab_mut().failure {
            shown.details_open = !shown.details_open;
        }
        cx.notify();
    }
}

fn buttons(tab: &Tab, unavailable: bool, cx: &mut Context<ChatPanel>) -> Div {
    h_flex()
        .gap_2()
        .children(reconnect_button(tab, unavailable, cx))
        .child(retry_button(tab, unavailable, cx))
        .children(tab.failure.as_ref().map(|shown| {
            CopyDetails::new("chat-failure-copy", tab_error_details(&shown.failure, tab))
        }))
}

/// Reconnect, when only a new runtime can recover and nothing else offers it.
fn reconnect_button(
    tab: &Tab,
    unavailable: bool,
    cx: &mut Context<ChatPanel>,
) -> Option<IconButton> {
    offers_reconnect(tab, unavailable).then(|| {
        action_button(
            "chat-failure-reconnect",
            Intent::Primary,
            Some(IconName::RotateCw),
            "Reconnect",
        )
        .disabled(tab.is_busy())
        .on_click(cx.listener(|chat, _, window, cx| chat.reconnect(window, cx)))
    })
}

fn retry_button(tab: &Tab, unavailable: bool, cx: &mut Context<ChatPanel>) -> IconButton {
    action_button(
        "chat-failure-retry",
        Intent::Primary,
        Some(IconName::RotateCw),
        "Retry",
    )
    .disabled(!retry_allowed(tab, unavailable))
    .on_click(cx.listener(|chat, _, _, cx| chat.retry(cx)))
}

fn error_details(failure: &TurnFailure) -> ErrorDetails {
    ErrorDetails::new(
        failure.category.title(),
        failure.details_text(),
        "Copilot turn",
    )
    .with_code(failure.error_code.as_deref().unwrap_or(&failure.error_type))
    .with_recorded_at(failure.occurred_at.to_utc())
}

pub(super) fn tab_error_details(failure: &TurnFailure, tab: &Tab) -> ErrorDetails {
    error_details(failure).with_private_values(legacy::submitted_text(tab))
}

/// The title in the failure colour and the action under it.
fn summary(category: FailureCategory, tokens: &Tokens) -> Div {
    v_flex()
        .gap_1()
        .child(
            div()
                .id("chat-failure-title")
                .test_support()
                .text_color(tokens.red)
                .font_weight(FontWeight::MEDIUM)
                .child(rich::plain_text(
                    "chat-failure-title-text",
                    category.title(),
                )),
        )
        .child(
            div()
                .id("chat-failure-action")
                .test_support()
                .text_color(tokens.muted)
                .child(rich::plain_text(
                    "chat-failure-action-text",
                    category.action(),
                )),
        )
}

/// The facts the runtime reported, collapsed until asked for.
fn details(
    open: bool,
    failure: &TurnFailure,
    tokens: &Tokens,
    cx: &mut Context<ChatPanel>,
) -> Collapsible {
    Collapsible::new()
        .open(open)
        .child(toggle(open, tokens, cx))
        .content(
            v_flex()
                .w_full()
                .gap_1()
                .pt_1()
                .children(failure.details().into_iter().map(|(label, value)| {
                    h_flex()
                        .w_full()
                        .gap_2()
                        .items_start()
                        .child(
                            div()
                                .w(px(LABEL_WIDTH))
                                .flex_shrink_0()
                                .text_color(tokens.muted)
                                .child(label),
                        )
                        .child(div().flex_1().min_w_0().child(rich::plain_text(
                            ElementId::Name(format!("chat-failure-detail-{label}").into()),
                            &value,
                        )))
                })),
        )
}

/// The Details disclosure trigger: the shared button keeps native Enter/Space
/// and expanded-state semantics.
fn toggle(open: bool, tokens: &Tokens, cx: &mut Context<ChatPanel>) -> IconButton {
    IconButton::new("chat-failure-details-toggle")
        .focus_on_click()
        .justify_start()
        .text_color(tokens.muted)
        .rounded(tokens.control_radius)
        .icon(if open {
            IconName::ChevronDown
        } else {
            IconName::ChevronRight
        })
        .label("Details")
        .accessibility_label("Details")
        .aria_expanded(open)
        .on_click(cx.listener(|chat, _, _, cx| chat.toggle_failure_details(cx)))
}

#[cfg(debug_assertions)]
pub(super) mod fixture;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod copy_tests;

#[cfg(test)]
mod logging_tests;
