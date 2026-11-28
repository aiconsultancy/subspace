//! The Thread tab's pinned composer and its channel visibility.

use gpui_kit::component::{Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _, Role,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, div,
};
use subspace_client_irc::Target;
use subspace_framework::dashboard::panel::IconName;
use subspace_framework::dashboard::{Tokens, truncated};

use crate::Irc;

pub(crate) fn footer(board: &Irc, cx: &App) -> Option<AnyElement> {
    let buffer = board.dock.focused.as_ref()?;
    let Target::Channel(channel) = &buffer.target else {
        return None;
    };
    let thread = board.threads.shown(buffer)?;
    let root = thread.root.line.id.as_deref()?;
    let network = board.hub.as_ref()?.read(cx).state.network(&buffer.network);
    let refusal = crate::thread::reply_refusal(network);
    let conversation = board.dock.open.get(buffer)?.entity.read(cx);
    let draft = conversation.reply_drafts.get(root)?;
    let tokens = Tokens::current(cx);
    let show_editor = refusal.is_none() || draft.read(cx).has_text(cx);
    Some(
        v_flex()
            .w_full()
            .min_w_0()
            .gap_2()
            .children(show_editor.then(|| destination(channel, &tokens)))
            .children(refusal.map(|reason| refusal_line(reason, &tokens)))
            .children(show_editor.then_some(draft))
            .into_any_element(),
    )
}

fn destination(channel: &str, tokens: &Tokens) -> impl IntoElement {
    h_flex()
        .id("irc-reply-destination")
        .test_support()
        .w_full()
        .min_w_0()
        .gap_2()
        .text_size(tokens.secondary_size())
        .child(
            h_flex()
                .gap_1()
                .flex_shrink_0()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(tokens.fg_strong)
                .child(
                    div()
                        .text_color(tokens.accent)
                        .child(Icon::from(IconName::Reply).xsmall()),
                )
                .child("Reply in thread"),
        )
        .child(
            truncated::text("irc-reply-visibility", format!("Visible in {channel}"))
                .flex_1()
                .text_right()
                .text_size(tokens.caption_size())
                .text_color(tokens.muted),
        )
}

fn refusal_line(reason: &'static str, tokens: &Tokens) -> impl IntoElement {
    div()
        .id("irc-thread-reply-refused")
        .role(Role::Note)
        .aria_label(reason)
        .test_support()
        .min_w_0()
        .text_size(tokens.secondary_size())
        .text_color(tokens.muted)
        .child(reason)
}
