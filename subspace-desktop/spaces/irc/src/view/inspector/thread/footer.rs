//! The Thread tab's footer: **Reply in thread**, or why a reply can't be
//! sent on this connection.

use gpui_kit::{
    AnyElement, Context, InteractiveElement as _, IntoElement as _, ParentElement as _, Role,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, Window, div,
};
use subspace_client_irc::BufferKey;
use subspace_framework::dashboard::Tokens;
use subspace_framework::dashboard::action::{Intent, action_button};
use subspace_framework::dashboard::panel::IconName;

use crate::Irc;
use crate::compose::reply::{self, ReplyTo};

/// The tab's footer: **Reply in thread** while a thread is shown and the
/// network can carry the reply tag, else why not.
pub(crate) fn footer(board: &Irc, cx: &mut Context<Irc>) -> Option<AnyElement> {
    let buffer = board.dock.focused.clone()?;
    let thread = board.threads.shown(&buffer)?;
    let hub = board.hub.as_ref()?;
    let refusal = crate::thread::reply_refusal(hub.read(cx).state.network(&buffer.network));
    if let Some(reason) = refusal {
        let tokens = Tokens::current(cx);
        return Some(
            div()
                .id("irc-thread-reply-refused")
                .role(Role::Note)
                .aria_label(reason)
                .test_support()
                .flex_1()
                .min_w_0()
                .text_size(tokens.secondary_size())
                .text_color(tokens.muted)
                .child(reason)
                .into_any_element(),
        );
    }
    let root = &thread.root.line;
    let replying = ReplyTo::new(
        root.id.as_deref().unwrap_or_default(),
        root.from.as_deref().unwrap_or_default(),
        &root.text,
    );
    Some(
        action_button(
            "irc-thread-reply",
            Intent::Neutral,
            Some(IconName::Reply),
            "Reply in thread",
        )
        .on_click(cx.listener(move |board, _, window, cx| {
            start_reply(board, &buffer, replying.clone(), window, cx);
        }))
        .into_any_element(),
    )
}

/// Puts `buffer`'s composer into reply mode for `replying`.
fn start_reply(
    board: &Irc,
    buffer: &BufferKey,
    replying: ReplyTo,
    window: &mut Window,
    cx: &mut Context<Irc>,
) {
    if let Some(hosted) = board.dock.open.get(buffer) {
        hosted
            .entity
            .update(cx, |view, cx| reply::start(view, replying, window, cx));
    }
}
