//! Keep oversized text editable, with an explicit local-file export.

use std::path::Path;

use gpui_kit::component::{Disableable as _, h_flex};
use gpui_kit::{AppContext as _, Context, IntoElement, ParentElement as _, Styled as _, div};
use subspace_framework::dashboard::Tokens;
use subspace_framework::dashboard::action::{Intent, action_button};
use subspace_framework::dashboard::panel::IconName;

use crate::conversation::Conversation;

pub(super) fn notice(saving: bool, cx: &mut Context<Conversation>) -> impl IntoElement {
    let tokens = Tokens::current(cx);
    h_flex()
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
                .child("Draft is too long to send in IRC."),
        )
        .child(
            action_button(
                "irc-save-draft",
                Intent::Quiet,
                Some(IconName::Save),
                "Save as file…",
            )
            .disabled(saving)
            .on_click(cx.listener(|this, _, _, cx| save(this, cx))),
        )
}

fn save(conversation: &mut Conversation, cx: &mut Context<Conversation>) {
    let draft = &mut conversation.draft;
    if !draft.oversized || draft.saving {
        return;
    }
    let text = draft.entity.read(cx).text(cx);
    draft.saving = true;
    cx.notify();
    let picker = cx.prompt_for_new_path(Path::new("."), Some("irc-draft.txt"));
    cx.spawn(async move |this, cx| {
        let result = match picker.await {
            Ok(Ok(Some(path))) => {
                cx.background_spawn(async move {
                    subspace_framework::core::state::write_atomic(&path, &text)
                })
                .await
            }
            Ok(Ok(None)) => Ok(()),
            Ok(Err(error)) => Err(std::io::Error::other(error)),
            Err(_) => Ok(()),
        };
        let _ = this.update(cx, |conversation, cx| finished(conversation, result, cx));
    })
    .detach();
}

fn finished(
    conversation: &mut Conversation,
    result: std::io::Result<()>,
    cx: &mut Context<Conversation>,
) {
    let draft = &mut conversation.draft;
    draft.saving = false;
    if let Err(error) = result {
        draft.entity.update(cx, |input, cx| {
            input.set_error(format!("Could not save draft. {error}"), cx);
        });
    }
    cx.notify();
}
