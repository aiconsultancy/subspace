//! Shared edge-to-edge frame; panels own the side and bottom dividers.

use super::attachments::clipboard_batch;
use super::{ChatInput, resize, sizing};
use crate::rich::{self, Format};
use gpui_kit::component::FocusableExt as _;
use gpui_kit::component::input::{InputGroup, InputGroupAddon, InputGroupAddonAlignment, Textarea};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    ClipboardEntry, ClipboardItem, Context, InteractiveElement as _, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, Window, div, px, rems,
};

pub(super) fn group(
    input: &ChatInput,
    window: &mut Window,
    cx: &mut Context<ChatInput>,
) -> InputGroup {
    let compact = input.compact();
    let mut group = InputGroup::new("chat-input").when(compact.is_none(), |group| {
        group
            .aria_label("Chat message")
            .rounded_none()
            .border_0()
            .focus_ring(false)
    });
    group = counter(input, group, cx);
    if input
        .message
        .as_ref()
        .is_some_and(|message| message.preview)
    {
        group = group.addon(
            InputGroupAddon::new("message-markdown-preview")
                .align(InputGroupAddonAlignment::BlockStart)
                .p_0()
                .child(preview(input, resize::editor_height(input, window, cx), cx)),
        );
    } else {
        let input_entity = cx.entity().downgrade();
        let budget = input.attachment_budget;
        group = group.input(
            Textarea::new(&input.state)
                .when_some(compact, |field, compact| {
                    field.aria_label(compact.input_name.clone())
                })
                .min_h_0()
                .disabled(input.disabled)
                .on_paste(move |item, _, cx| {
                    let has_attachment = has_attachment(item);
                    if has_attachment {
                        let batch = clipboard_batch(item, budget);
                        let _ = input_entity.update(cx, |_, cx| {
                            cx.emit(super::ChatInputEvent::PasteAttachments(
                                super::AttachmentPaste(batch),
                            ));
                        });
                    }
                    has_attachment
                        && !item
                            .entries()
                            .iter()
                            .any(|entry| matches!(entry, ClipboardEntry::String(_)))
                }),
        );
    }
    group.when_some(input.render_addon(window, cx), |group, addon| {
        group.addon(toolbar(addon, compact.is_some()))
    })
}

fn toolbar(addon: gpui_kit::AnyElement, compact: bool) -> InputGroupAddon {
    InputGroupAddon::new("chat-input-toolbar")
        .align(InputGroupAddonAlignment::BlockEnd)
        .when(compact, |addon| addon.py_1())
        .when(!compact, |addon| addon.py(px(sizing::TOOLBAR_PADDING_PX)))
        .child(
            div()
                .w_full()
                .min_w_0()
                .when(!compact, |row| row.min_h(rems(sizing::TOOLBAR_REM)))
                .child(addon),
        )
}

fn counter(input: &ChatInput, group: InputGroup, cx: &Context<ChatInput>) -> InputGroup {
    group.when_some(input.counter(cx), |group, counter| {
        group.addon(
            InputGroupAddon::new("chat-input-counter-row")
                .align(InputGroupAddonAlignment::BlockStart)
                .justify_end()
                .py_1()
                .child(
                    crate::badge::Pill::new("chat-input-counter", counter.text)
                        .tone(counter.tone)
                        .aria_label(counter.label),
                ),
        )
    })
}

fn has_attachment(item: &ClipboardItem) -> bool {
    item.entries().iter().any(|entry| {
        matches!(
            entry,
            ClipboardEntry::Image(_) | ClipboardEntry::ExternalPaths(_)
        )
    })
}

fn preview(
    input: &ChatInput,
    height: f32,
    cx: &Context<ChatInput>,
) -> gpui_kit::Stateful<gpui_kit::Div> {
    let text = input.text(cx);
    div()
        .id("message-preview-body")
        .w_full()
        .h(px(height))
        .overflow_y_scroll()
        .p_2()
        .text_color(crate::Tokens::current(cx).fg)
        .when(text.trim().is_empty(), |preview| {
            preview.child("Nothing to preview.")
        })
        .when(!text.trim().is_empty(), |preview| {
            preview.child(rich::rich_text(
                "message-preview-text",
                Format::Markdown,
                &text,
            ))
        })
}
