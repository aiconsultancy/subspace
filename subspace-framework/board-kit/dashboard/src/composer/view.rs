//! How the composer draws itself: the label and Write/Preview tabs, the text
//! area or Markdown preview, and the hint and buttons under it.

use gpui_kit::component::Disableable as _;
use gpui_kit::component::input::Textarea;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Context, Div, FocusHandle, Focusable, InteractiveElement, IntoElement,
    KeyContext, OperatingSystem, ParentElement, Render, Stateful, StatefulInteractiveElement,
    Styled, TestSupportExt as _, Window, div, px,
};

use super::{
    COMPOSER_CONTEXT, Composer, ComposerEscape, ComposerSubmit, PREVIEW_MIN_HEIGHT_PX,
    TAB_PADDING_Y_PX, note_text,
};
use crate::MODAL_CONTEXT;
use crate::action::{Intent, action_button};
use crate::panel::IconName;
use crate::rich::{self, Format, rich_text};
use crate::tokens::Tokens;

impl Composer {
    /// The Write or Preview tab, highlighted while `on`.
    fn tab(tokens: &Tokens, id: &'static str, label: &'static str, on: bool) -> Stateful<Div> {
        div()
            .id(id)
            .px_2()
            .py(px(TAB_PADDING_Y_PX))
            .rounded(tokens.control_radius)
            .cursor_pointer()
            .text_color(if on { tokens.fg_strong } else { tokens.muted })
            .when(on, |tab| tab.bg(tokens.select))
            .child(label)
    }

    /// The text area, or the rendered Markdown while the preview tab is on.
    fn body(&self, tokens: &Tokens, text: &str, cx: &mut Context<Self>) -> AnyElement {
        match self.preview {
            false => self.textarea(cx),
            true => Self::preview_box(tokens, text),
        }
    }

    fn textarea(&self, cx: &mut Context<Self>) -> AnyElement {
        let owner = cx.entity().downgrade();
        let byte_limit = self
            .images
            .byte_limit
            .unwrap_or(crate::images::MAX_ENCODED_BYTES);
        let enabled = self.images.enabled && !self.state.readonly && !self.state.busy;
        Textarea::new(&self.input)
            .readonly(self.state.readonly || self.state.busy)
            .aria_label(self.label.clone())
            .on_paste(move |item, _, cx| paste_images(item, enabled, byte_limit, &owner, cx))
            .into_any_element()
    }

    /// The bordered box the Markdown preview sits in.
    fn preview_box(tokens: &Tokens, text: &str) -> AnyElement {
        div()
            .min_h(px(PREVIEW_MIN_HEIGHT_PX))
            .p_2()
            .border_1()
            .border_color(tokens.line)
            .rounded(tokens.control_radius)
            .child(Self::preview_content(tokens, text))
            .into_any_element()
    }

    fn preview_content(tokens: &Tokens, text: &str) -> AnyElement {
        if text.trim().is_empty() {
            return div()
                .text_color(tokens.muted)
                .child(rich::plain_text(
                    "composer-empty-preview",
                    "Nothing to preview.",
                ))
                .into_any_element();
        }
        rich_text("composer-preview", Format::Markdown, text).into_any_element()
    }

    /// The label and the Write/Preview tabs above the text.
    fn header(&self, tokens: &Tokens, cx: &mut Context<Self>) -> AnyElement {
        h_flex()
            .gap_1()
            .child(
                div()
                    .mr_2()
                    .text_color(tokens.fg_strong)
                    .child(self.label.clone()),
            )
            .child(
                Self::tab(tokens, "composer-write", "Write", !self.preview).on_click(
                    cx.listener(|composer, _, window, cx| composer.set_preview(false, window, cx)),
                ),
            )
            .child(
                Self::tab(tokens, "composer-preview-tab", "Preview", self.preview).on_click(
                    cx.listener(|composer, _, window, cx| composer.set_preview(true, window, cx)),
                ),
            )
            .child(div().flex_1())
            .child(
                div()
                    .text_size(tokens.caption_size())
                    .text_color(tokens.muted)
                    .child("Markdown"),
            )
            .into_any_element()
    }

    /// The hint under the text, in the warning colour while Escape has asked.
    fn note(&self, tokens: &Tokens, mac: bool) -> AnyElement {
        let note = if let Some(hint) = &self.hint {
            hint.to_string()
        } else if self.state.retained {
            if self.state.busy {
                "Posting…".to_owned()
            } else {
                format!(
                    "{}+Enter: {} · Enter: new line",
                    if mac { "Command" } else { "Control" },
                    self.submit_label.to_lowercase()
                )
            }
        } else {
            note_text(
                self.state.busy,
                self.state.asked,
                self.state.optional,
                &self.submit_label,
            )
        };
        let colour = match self.state.asked {
            true => tokens.yellow,
            false => tokens.muted,
        };
        div()
            .id("composer-hint")
            .test_support()
            .aria_label(note.clone())
            .flex_1()
            .min_w_0()
            .text_color(colour)
            .child(rich::plain_text("composer-note", &note))
            .into_any_element()
    }

    /// The hint and the Cancel and submit buttons under the text.
    fn footer(&self, tokens: &Tokens, cx: &mut Context<Self>) -> AnyElement {
        h_flex()
            .gap_2()
            .child(self.note(tokens, cx.platform_info().is_mac()))
            .child(
                action_button(
                    "composer-cancel",
                    Intent::Neutral,
                    Some(IconName::X),
                    if self.state.retained {
                        "Leave editor"
                    } else {
                        "Cancel"
                    },
                )
                .on_click(cx.listener(|composer, _, window, cx| {
                    composer.escape(&ComposerEscape, window, cx)
                })),
            )
            .child(
                action_button(
                    "composer-submit",
                    Intent::Primary,
                    Some(IconName::Check),
                    if self.state.busy && self.state.retained {
                        "Posting…".into()
                    } else {
                        self.submit_label.clone()
                    },
                )
                .disabled(
                    self.state.busy
                        || self.state.readonly
                        || !self.state.submit_enabled
                        || (!self.state.optional && self.text(cx).trim().is_empty()),
                )
                .on_click(cx.listener(|composer, _, window, cx| {
                    composer.submit(&ComposerSubmit, window, cx)
                })),
            )
            .into_any_element()
    }

    /// The key context: the composer's own keys, plus the modal context that
    /// holds the shell and dashboard keys off while the composer has focus.
    fn key_context(os: OperatingSystem) -> KeyContext {
        let mut context = KeyContext::new_with_defaults(os);
        context.add(COMPOSER_CONTEXT);
        context.add(MODAL_CONTEXT);
        context
    }
}

fn paste_images(
    item: &gpui_kit::ClipboardItem,
    enabled: bool,
    byte_limit: usize,
    owner: &gpui_kit::WeakEntity<Composer>,
    cx: &mut App,
) -> bool {
    if !enabled {
        return false;
    }
    let batch = crate::chat_input::attachments::clipboard_batch(
        item,
        crate::chat_input::attachments::AttachmentBudget::new(
            super::MAX_IMAGES,
            super::MAX_IMAGE_DRAFT_BYTES,
        )
        .with_image_limit(byte_limit),
    );
    let handled = !batch.sources.is_empty() || batch.notice.is_some();
    if handled {
        _ = owner.update(cx, |_, cx| cx.emit(super::ComposerImagePaste(batch)));
    }
    handled
        && !item
            .entries()
            .iter()
            .any(|entry| matches!(entry, gpui_kit::ClipboardEntry::String(_)))
}

impl Focusable for Composer {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for Composer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = Tokens::current(cx);
        let body = self.body(&tokens, &self.text(cx), cx);
        v_flex()
            .id("composer")
            .key_context(Self::key_context(cx.platform_info().os))
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::submit))
            .on_action(cx.listener(Self::escape))
            .gap_2()
            .text_size(tokens.row_font)
            .child(self.header(&tokens, cx))
            .child(body)
            .when_some(self.error.clone(), |el, error| {
                el.child(
                    div()
                        .text_color(tokens.red)
                        .child(rich::plain_text("composer-error", &error)),
                )
            })
            .when_some(self.images.accessory.clone(), |el, render| {
                el.child(render(window, cx))
            })
            .child(self.footer(&tokens, cx))
    }
}
