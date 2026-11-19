//! The context pills above the input and the approval or question card.
//! The transcript rows live in the sibling `transcript` module.

use super::logic::context_snapshot;
use super::{
    ChatPanel, Context, Div, FULL_FRACTION, PENDING_HEIGHT, Pending, Stateful, Tokens, h_flex, px,
    relative, v_flex,
};
use crate::action::{Intent, action_button, icon_button};
use crate::truncated;
use gpui_kit::component::Sizable as _;
use gpui_kit::component::attachment::{Attachment, AttachmentContent, AttachmentTitle};
use gpui_kit::{
    InteractiveElement as _, ParentElement as _, StatefulInteractiveElement as _, Styled as _,
};
use subspace_lucide::IconName;

impl ChatPanel {
    pub(super) fn render_context_pills(&self, cx: &mut Context<Self>) -> Div {
        let displayed_context =
            context_snapshot(&self.tab().context, self.current.clone(), self.auto_context);
        let pills = displayed_context
            .iter()
            .enumerate()
            .map(|(index, context)| {
                let key = context.key.clone();
                h_flex()
                    .min_w_0()
                    .max_w(relative(FULL_FRACTION))
                    .gap_1()
                    .child(
                        truncated::text(("chat-context-label", index), context.label.clone())
                            .flex_1(),
                    )
                    .child(
                        icon_button(
                            ("chat-context-remove", index),
                            Intent::Quiet,
                            IconName::X,
                            format!("Remove context: {}", context.label),
                        )
                        .compact()
                        .flex_shrink_0()
                        .on_click(cx.listener(move |chat, _, _, cx| chat.remove_context(&key, cx))),
                    )
            })
            .collect::<Vec<_>>();
        let image_pills = self
            .tab()
            .images
            .iter()
            .map(|image| {
                let label = image.display_name.clone();
                let id = image.id;
                Attachment::new()
                    .id(("chat-image", id as usize))
                    .xsmall()
                    .content(AttachmentContent::new().title(AttachmentTitle::new(label)))
                    .on_remove(cx.listener(move |chat, _, _, cx| {
                        chat.remove_image_draft(id);
                        cx.notify();
                    }))
            })
            .collect::<Vec<_>>();
        h_flex()
            .px_2()
            .flex_wrap()
            .children(pills)
            .children(image_pills)
    }

    /// The card body for an approval: the summary, Deny and Allow.
    fn pending_approval(
        card: Stateful<Div>,
        id: String,
        summary: String,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let deny_id = id.clone();
        card.child(crate::rich::plain_text(
            format!("chat-approval-title-{id}"),
            "Approve this exact action?",
        ))
        .child(crate::rich::plain_text(
            format!("chat-approval-{id}"),
            &summary,
        ))
        .child(
            h_flex()
                .gap_2()
                .child(
                    action_button("chat-deny", Intent::Reject, Some(IconName::X), "Deny").on_click(
                        cx.listener(move |chat, _, _, cx| chat.approve(deny_id.clone(), false, cx)),
                    ),
                )
                .child(
                    action_button("chat-allow", Intent::Accept, Some(IconName::Check), "Allow")
                        .on_click(
                            cx.listener(move |chat, _, _, cx| chat.approve(id.clone(), true, cx)),
                        ),
                ),
        )
    }

    /// The card body for a question: the text and one button per choice.
    fn pending_question(
        card: Stateful<Div>,
        id: String,
        text: String,
        choices: Vec<String>,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        card.child(crate::rich::plain_text(
            format!("chat-question-{id}"),
            &text,
        ))
        .child(crate::rich::plain_text(
            format!("chat-question-help-{id}"),
            "Choose an answer or type it in Message.",
        ))
        .children(choices.into_iter().enumerate().map(|(index, answer)| {
            let id = id.clone();
            action_button(
                ("chat-answer", index),
                Intent::Neutral,
                None,
                answer.clone(),
            )
            .on_click(cx.listener(move |chat, _, _, cx| {
                chat.answer(id.clone(), answer.clone(), cx);
            }))
        }))
    }

    pub(super) fn render_pending(
        &self,
        tokens: &Tokens,
        cx: &mut Context<Self>,
    ) -> Option<Stateful<Div>> {
        self.tab().pending.first().cloned().map(|pending| {
            let card = v_flex()
                .id("chat-pending")
                .max_h(px(PENDING_HEIGHT))
                .overflow_y_scroll()
                .flex_shrink_0()
                .gap_2()
                .p_2()
                .border_1()
                .border_color(tokens.yellow);
            match pending {
                Pending::Approval { id, summary } => Self::pending_approval(card, id, summary, cx),
                Pending::Question { id, text, choices } => {
                    Self::pending_question(card, id, text, choices, cx)
                }
            }
        })
    }
}
