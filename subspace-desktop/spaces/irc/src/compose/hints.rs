//! The channel composer’s destination and attachment disclosure.

use crate::conversation::{Conversation, ConversationEvent};
use gpui_kit::component::h_flex;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, div,
};
use subspace_framework::dashboard::Tokens;
use subspace_framework::dashboard::action::{Intent, action_button};
use subspace_framework::lucide::IconName;

/// Who can see what is sent and for how long, as the hint row says it:
/// short enough for the slim row, which truncates it in a narrow pane; the
/// full sentence is its accessible name and its tooltip.
pub(crate) const RETENTION: &str = "Subspace users see it for 30 days. No credentials.";

/// The destination above the input and the existing attachment disclosure.
pub(super) fn destination(
    id: &'static str,
    words: String,
    uploads: bool,
    thread_draft: Option<&str>,
    cx: &mut Context<Conversation>,
) -> AnyElement {
    let tokens = Tokens::current(cx);
    h_flex()
        .id(id)
        .min_w_0()
        .w_full()
        .gap_2()
        .child(
            subspace_framework::dashboard::truncated::text("irc-destination-name", words)
                .text_size(tokens.secondary_size())
                .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                .text_color(tokens.fg_strong)
                .min_w_0(),
        )
        .children(thread_draft.map(|root| resume(root, cx)))
        .children(uploads.then(|| retention(&tokens)))
        .into_any_element()
}

fn resume(root: &str, cx: &Context<Conversation>) -> impl IntoElement {
    let root = root.to_owned();
    action_button(
        "irc-resume-reply",
        Intent::Quiet,
        Some(IconName::Reply),
        "Thread draft",
    )
    .on_click(
        cx.listener(move |_, _, _, cx| cx.emit(ConversationEvent::ReplyInThread(root.clone()))),
    )
}

/// The retention notice at the row's trailing end, named in full.
fn retention(tokens: &Tokens) -> AnyElement {
    div()
        .id("irc-retention")
        .aria_label(crate::uploads::outgoing::VISIBILITY)
        .test_support()
        .flex_1()
        .min_w_0()
        .flex()
        .justify_end()
        .text_size(tokens.caption_size())
        .text_color(tokens.muted)
        .child(div().truncate().child(RETENTION))
        .tooltip(|window, cx| Tooltip::new(crate::uploads::outgoing::VISIBILITY).build(window, cx))
        .into_any_element()
}
