//! The brief notice above the composer after a refusal or a stop, with
//! Dismiss.

use gpui_kit::component::h_flex;
use gpui_kit::{
    AnyElement, Context, InteractiveElement as _, IntoElement as _, ParentElement as _, Role,
    SharedString, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, div,
};
use subspace_framework::dashboard::Tokens;
use subspace_framework::dashboard::action::{Intent, icon_button};
use subspace_framework::dashboard::panel::IconName;

use crate::conversation::Conversation;

/// The brief notice above the composer, with Dismiss.
pub(in crate::conversation) fn notice(
    this: &Conversation,
    cx: &mut Context<Conversation>,
) -> Option<AnyElement> {
    let text = this.reacting.notice.clone()?;
    let tokens = Tokens::current(cx);
    Some(
        h_flex()
            .id("irc-reaction-notice")
            .role(Role::Status)
            .aria_label(SharedString::from(text.clone()))
            .w_full()
            .gap_2()
            .px_2()
            .py_1()
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_sm()
                    .text_color(tokens.fg)
                    .child(text),
            )
            .child(
                icon_button(
                    "irc-reaction-notice-dismiss",
                    Intent::Quiet,
                    IconName::X,
                    "Dismiss",
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.reacting.notice = None;
                    cx.notify();
                })),
            )
            .test_support()
            .into_any_element(),
    )
}
