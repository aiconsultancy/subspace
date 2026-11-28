//! Message rendering in the Thread inspector.

use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, InteractiveElement as _, IntoElement, ParentElement as _, Role, SharedString,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _,
};

use super::{
    Entry, RowContext, Tokens, attachments, body, header, markup, shown_now, spoken, thread_facts,
};

/// A message as the inspector's Thread tab shows it: its sender, rank and
/// time unless it `continues` their group, then its text with its
/// references, as the transcript draws them. Replies that are hidden from the
/// channel also show their attachments, request status and reaction controls.
pub(crate) fn thread_message(
    original: &Entry,
    (continues, show_status): (bool, bool),
    context: &RowContext,
    cx: &App,
) -> AnyElement {
    let tokens = Tokens::current(cx);
    let shown = shown_now(original, context, cx);
    let entry = shown.as_ref().unwrap_or(original);
    let nick = entry.line.from.clone().unwrap_or_default();
    let rendered = markup::rich(&entry.line.text, &context.mentions);
    let rank = context
        .roster
        .verified(&nick, entry.line.account.as_deref());
    let thread_only = context.threads && entry.line.reply_to.is_some();
    v_flex()
        .id(SharedString::from(format!("irc-thread-line:{}", entry.seq)))
        .role(Role::Group)
        .aria_label(spoken(entry, rendered.as_ref()))
        .test_support()
        .w_full()
        .min_w_0()
        .gap_0p5()
        .when(!continues, |column| {
            column.child(header(entry, &nick, rank, &tokens, cx))
        })
        .when(!entry.line.text.is_empty(), |column| {
            column.child(body(entry, rendered.as_ref(), context, &tokens, cx))
        })
        .children(attachments(original, context, &tokens, cx))
        .when(thread_only && show_status, |column| {
            column.children(thread_facts(entry, context, &tokens, cx))
        })
        .children(if thread_only {
            crate::transcript::reactions::thread_reply(entry, context)
        } else {
            crate::transcript::reactions::read_only(entry, context)
        })
        .children(
            crate::transcript::actions::thread_stop(entry, context)
                .map(|stop| h_flex().child(stop)),
        )
        .into_any_element()
}
