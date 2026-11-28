//! Recovery for replies whose original message has dropped from history.

use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{
    App, ClipboardItem, Entity, InteractiveElement as _, IntoElement, ParentElement as _,
    SharedString, Styled as _, TestSupportExt as _, WeakEntity, div,
};
use subspace_framework::dashboard::action::{Intent, action_button};
use subspace_framework::dashboard::{Tokens, truncated, workspace};
use subspace_framework::lucide::IconName;

use crate::compose::reply::{GONE, ReplyDraft};
use crate::conversation::Conversation;

pub(super) fn missing(
    drafts: Vec<Entity<ReplyDraft>>,
    conversation: &Entity<Conversation>,
    cx: &App,
) -> Option<impl IntoElement> {
    if drafts.is_empty() {
        return None;
    }
    let tokens = Tokens::current(cx);
    Some(
        workspace::section("Unsent replies", cx)
            .id("irc-unavailable-replies")
            .test_support()
            .child(
                div()
                    .text_size(tokens.secondary_size())
                    .text_color(tokens.muted)
                    .child(GONE),
            )
            .children(
                drafts
                    .into_iter()
                    .map(|draft| recovery(draft, conversation.downgrade(), &tokens, cx)),
            ),
    )
}

fn recovery(
    draft: Entity<ReplyDraft>,
    conversation: WeakEntity<Conversation>,
    tokens: &Tokens,
    cx: &App,
) -> impl IntoElement {
    let root = draft.read(cx).root().to_owned();
    let text = draft.read(cx).text(cx);
    let copy = action_button(
        SharedString::from(format!("irc-copy-reply:{root}")),
        Intent::Quiet,
        Some(IconName::Copy),
        "Copy draft",
    )
    .on_click(move |_, _, cx| {
        cx.write_to_clipboard(ClipboardItem::new_string(draft.read(cx).text(cx)))
    });
    let discard = action_button(
        SharedString::from(format!("irc-discard-reply:{root}")),
        Intent::Quiet,
        Some(IconName::X),
        "Discard draft",
    )
    .on_click(move |_, window, cx| {
        _ = conversation.update(cx, |conversation, _| {
            conversation.reply_drafts.remove(&root)
        });
        window.refresh();
    });
    v_flex()
        .min_w_0()
        .gap_1()
        .child(truncated::text("irc-unsent-reply-text", text).text_size(tokens.secondary_size()))
        .child(h_flex().gap_1().child(copy).child(discard))
}
