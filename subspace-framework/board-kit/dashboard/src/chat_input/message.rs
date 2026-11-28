//! The same chat input with Markdown Write/Preview instead of agent settings.

use super::{ChatInput, Context, IntoElement, Window};
use crate::{
    action::{Intent, action_button, icon_button},
    panel::IconName,
    tokens::Tokens,
};
use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Selectable as _, Sizable as _, h_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, FocusHandle, InteractiveElement as _, ParentElement as _, SharedString,
    Styled as _, TestSupportExt as _, div,
};

pub(super) struct CompactMessage {
    pub label: SharedString,
    pub input_name: SharedString,
}

pub(super) struct MessageState {
    /// Write/Preview for Markdown; off for plain-text protocols.
    pub markdown: bool,
    pub compact: Option<CompactMessage>,
    pub busy: bool,
    pub submit_blocked: bool,
    pub error: Option<String>,
    pub preview: bool,
    pub height: Option<f32>,
    pub available: f32,
    pub resize_focus: FocusHandle,
}

impl ChatInput {
    /// A Markdown message input, sharing Copilot's minimum size and Enter bindings.
    pub fn message(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::with_message(None, window, cx)
    }

    /// An inset plain-text composer with a labelled send action.
    ///
    /// An addon set on this composer appears before the shared send controls.
    pub fn compact_message(
        label: impl Into<SharedString>,
        input_name: impl Into<SharedString>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let compact = CompactMessage {
            label: label.into(),
            input_name: input_name.into(),
        };
        Self::with_message(Some(compact), window, cx)
    }

    fn with_message(
        compact: Option<CompactMessage>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            message: Some(MessageState {
                markdown: compact.is_none(),
                compact,
                busy: false,
                submit_blocked: false,
                error: None,
                preview: false,
                height: None,
                available: 0.0,
                resize_focus: cx.focus_handle(),
            }),
            ..Self::with_placeholder("Write a message…".into(), window, cx)
        }
    }

    pub(super) fn compact(&self) -> Option<&CompactMessage> {
        self.message.as_ref()?.compact.as_ref()
    }

    /// A plain-text message input for a protocol without Markdown, such as
    /// IRC: no Write/Preview and no Markdown hint.
    pub fn set_plain_text(&mut self, cx: &mut Context<Self>) {
        if let Some(message) = &mut self.message {
            message.markdown = false;
            message.preview = false;
        }
        cx.notify();
    }

    // qual:api - controlled by a messaging board's asynchronous send.
    pub fn set_busy(&mut self, busy: bool, cx: &mut Context<Self>) {
        if let Some(message) = &mut self.message {
            message.busy = busy;
            message.error = None;
        }
        self.set_disabled(busy, cx);
    }

    // qual:api - read by a messaging board's tests of its send errors.
    #[cfg(any(test, feature = "test-support"))]
    pub fn error(&self) -> Option<&str> {
        self.message.as_ref()?.error.as_deref()
    }

    // qual:api - controlled by a messaging board's asynchronous send.
    pub fn set_error(&mut self, error: String, cx: &mut Context<Self>) {
        self.set_busy(false, cx);
        if let Some(message) = &mut self.message {
            message.error = Some(error);
        }
        cx.notify();
    }

    /// Prevents submitting the message while leaving its draft editable.
    // qual:api
    pub fn set_submit_blocked(&mut self, blocked: bool, cx: &mut Context<Self>) {
        if let Some(message) = &mut self.message
            && message.submit_blocked != blocked
        {
            message.submit_blocked = blocked;
            cx.notify();
        }
    }

    /// The actual board height, excluding neighbouring panes and window chrome.
    // qual:api
    pub fn set_available_height(&mut self, available: f32, cx: &mut Context<Self>) {
        if let Some(message) = &mut self.message
            && crate::measure::changed(message.available, available)
        {
            message.available = available;
            cx.notify();
        }
    }

    fn set_preview(&mut self, preview: bool, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(message) = &mut self.message {
            message.preview = preview;
        }
        if !preview {
            self.focus(window, cx);
        }
        cx.notify();
    }

    pub(super) fn render_addon(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if let Some(compact) = self.compact() {
            return self.compact_toolbar(&compact.label, window, cx);
        }
        if let Some(addon) = &self.addon {
            return Some(addon.clone().into_any_element());
        }
        let message = self.message.as_ref()?;
        Some(
            h_flex()
                .w_full()
                .gap_1()
                .when(message.markdown, |toolbar| {
                    toolbar.children(Self::markdown_modes(message, cx))
                })
                .child(self.message_status(message, window, cx))
                .child(self.message_send(message, cx))
                .into_any_element(),
        )
    }

    fn compact_toolbar(
        &self,
        label: &SharedString,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let message = self.message.as_ref()?;
        let send = action_button(
            "message-send",
            Intent::Primary,
            Some(IconName::CornerDownLeft),
            label.clone(),
        )
        .loading(message.busy)
        .disabled(!self.can_send(message, cx))
        .on_click(cx.listener(|this, _, _, cx| this.enter(cx)));
        Some(
            h_flex()
                .w_full()
                .min_w_0()
                .gap_2()
                .children(self.addon.clone())
                .child(self.message_status(message, window, cx))
                .child(send)
                .into_any_element(),
        )
    }

    fn can_send(&self, message: &MessageState, cx: &gpui_kit::App) -> bool {
        !self.disabled
            && !message.submit_blocked
            && (self.submit_empty || !self.text(cx).trim().is_empty())
    }

    fn message_status(
        &self,
        message: &MessageState,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let tokens = Tokens::current(cx);
        if let Some(error) = &message.error {
            return crate::truncated::text("message-status", error.clone())
                .flex_1()
                .min_w_0()
                .text_xs()
                .text_color(tokens.red)
                .into_any_element();
        }
        if message.compact.is_some() {
            let keys = if cx.platform_info().is_mac() {
                "⇧↵ New line"
            } else {
                "Shift+Enter · New line"
            };
            return crate::truncated::text("message-status", keys)
                .flex_1()
                .min_w_0()
                .text_right()
                .text_size(tokens.caption_size())
                .text_color(tokens.muted)
                .into_any_element();
        }
        let target = hint_opacity(!self.text(cx).is_empty(), message.submit_blocked);
        let opacity = gpui_kit::base::transition(
            "message-hint-opacity",
            target,
            gpui_kit::base::Transition::new(cx.theme().motion_tokens().duration_fast),
            window,
            cx,
        );
        div()
            .id("message-hint")
            .test_support()
            .flex_1()
            .min_w_0()
            .opacity(opacity)
            .when(opacity == 0.0, |hint| hint.invisible())
            .child(
                crate::truncated::text(
                    "message-status",
                    send_hint(message.markdown, cx.platform_info().is_mac()),
                )
                .text_xs()
                .text_color(tokens.muted),
            )
            .into_any_element()
    }

    /// The Write and Preview choices of a Markdown message.
    fn markdown_modes(message: &MessageState, cx: &mut Context<Self>) -> [AnyElement; 2] {
        [
            action_button(
                "message-write",
                Intent::Quiet,
                Some(IconName::Pencil),
                "Write",
            )
            .selected(!message.preview)
            .on_click(cx.listener(|this, _, window, cx| this.set_preview(false, window, cx)))
            .into_any_element(),
            action_button(
                "message-preview",
                Intent::Quiet,
                Some(IconName::Eye),
                "Preview",
            )
            .selected(message.preview)
            .on_click(cx.listener(|this, _, window, cx| this.set_preview(true, window, cx)))
            .into_any_element(),
        ]
    }

    fn message_send(
        &self,
        message: &MessageState,
        cx: &Context<Self>,
    ) -> crate::icon_button::IconButton {
        icon_button(
            "message-send",
            Intent::Primary,
            IconName::ArrowUp,
            "Send message",
        )
        .primary()
        .small()
        .rounded_full()
        .loading(message.busy)
        .disabled(self.disabled || message.submit_blocked || self.text(cx).trim().is_empty())
        .on_click(cx.listener(|this, _, _, cx| this.enter(cx)))
    }
}

fn hint_opacity(has_text: bool, blocked: bool) -> f32 {
    if has_text || blocked { 0.0 } else { 1.0 }
}

/// The toolbar's key hint: Return (Enter off macOS) sends and Shift with it
/// starts a new line, after `Markdown` when the draft takes Markdown.
fn send_hint(markdown: bool, mac: bool) -> String {
    let key = if mac { "Return" } else { "Enter" };
    let keys = format!("{key} to send · Shift+{key} for a new line");
    if markdown {
        format!("Markdown · {keys}")
    } else {
        keys
    }
}

#[cfg(test)]
#[path = "message_hint_tests.rs"]
mod hint_tests;
#[cfg(test)]
#[path = "message_tests.rs"]
mod tests;

impl super::ChatInput {
    /// The owner service's intake budget. File reads remain the owner's responsibility.
    pub fn set_attachment_budget(
        &mut self,
        budget: super::attachments::AttachmentBudget,
        cx: &mut Context<Self>,
    ) {
        self.attachment_budget = budget;
        cx.notify();
    }
}
