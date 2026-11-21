//! The tray above the composer: one small thumbnail per attachment with ×,
//! "Check upload" for an upload that may have completed, the limit notice,
//! "Send as attachment…" and one muted line about who can see files.
use super::{Item, ItemState, Pending, Phase, Tray};
use crate::conversation::Conversation;
use crate::uploads::intake::Preview;
use gpui_kit::component::attachment::{
    Attachment, AttachmentActions, AttachmentContent, AttachmentMedia, AttachmentStatus,
    AttachmentTitle,
};
use gpui_kit::component::{Disableable as _, Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, Entity, InteractiveElement as _, IntoElement as _, ParentElement as _,
    SharedString, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, div,
};
use subspace_framework::dashboard::{
    ChatInput, Tokens,
    action::{Intent, action_button, icon_button},
    icon_button::IconButton,
    media::size_text,
};
use subspace_framework::lucide::IconName;

/// Who can read an attachment, said once.
pub(crate) const VISIBILITY: &str =
    "Visible to signed-in Subspace users for 30 days. Don’t paste credentials into chat.";

pub(crate) fn controls(
    input: &Entity<ChatInput>,
    tray: &Tray,
    cx: &mut Context<Conversation>,
) -> AnyElement {
    let tokens = Tokens::current(cx);
    let idle = tray.idle();
    let row = (!tray.is_empty()).then(|| {
        h_flex()
            .flex_wrap()
            .gap_2()
            .children(tray.items.iter().map(|item| thumbnail(item, idle, cx)))
    });
    let checks: Vec<AnyElement> = tray
        .items
        .iter()
        .filter_map(|item| match &item.state {
            ItemState::Ready(p) if p.uncertain => Some(check(p, idle, cx)),
            _ => None,
        })
        .collect();
    let empty = input.read(cx).text(cx).trim().is_empty();
    v_flex()
        .gap_1p5()
        .px_2()
        .py_1()
        .children(row)
        .children(status_line(tray, &tokens, cx))
        .children(
            tray.notice
                .map(|text| div().text_sm().text_color(tokens.orange).child(text)),
        )
        .children((!checks.is_empty()).then(|| h_flex().gap_2().children(checks)))
        .child(send_row(empty || !idle, &tokens, cx))
        .into_any_element()
}

/// The tray's status, with **Dismiss** once the line is queued.
fn status_line(
    tray: &Tray,
    tokens: &Tokens,
    cx: &mut Context<Conversation>,
) -> Option<gpui_kit::Div> {
    let dismiss = matches!(tray.phase, Phase::Queued { .. }).then(|| {
        action_button(
            "irc-dismiss-attachments",
            Intent::Quiet,
            Some(IconName::X),
            "Dismiss",
        )
        .on_click(cx.listener(|this, _, _, cx| super::dismiss(this, cx)))
    });
    status(tray).map(|text| {
        h_flex()
            .gap_2()
            .child(div().text_sm().text_color(tokens.muted).child(text))
            .children(dismiss)
    })
}

/// One quiet line above the composer: **Send as attachment…** behind its
/// paperclip, then who can see files.
fn send_row(disabled: bool, tokens: &Tokens, cx: &mut Context<Conversation>) -> gpui_kit::Div {
    h_flex()
        .w_full()
        .min_w_0()
        .gap_2()
        .child(
            icon_button(
                "irc-send-attachment",
                Intent::Quiet,
                IconName::Paperclip,
                "Send as attachment…",
            )
            .disabled(disabled)
            .on_click(cx.listener(|this, _, window, cx| super::text(this, window, cx))),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_size(tokens.caption_size())
                .text_color(tokens.muted)
                .child(VISIBILITY),
        )
}

/// What the tray is doing, while it is doing something.
fn status(tray: &Tray) -> Option<String> {
    match &tray.phase {
        Phase::Idle => None,
        Phase::Uploading { .. } => {
            let done = tray
                .items
                .iter()
                .filter(|item| matches!(&item.state, ItemState::Ready(p) if p.uploaded.is_some()))
                .count();
            Some(format!("Uploading {} of {}…", done + 1, tray.items.len()))
        }
        Phase::Queued { .. } => {
            Some("Sent. Check the conversation, or dismiss these attachments.".to_owned())
        }
    }
}

fn thumbnail(item: &Item, idle: bool, cx: &mut Context<Conversation>) -> AnyElement {
    let (media, name, tooltip, status) = face(item);
    let id = item.id;
    let label = SharedString::from(format!("{name}: {}", tooltip.replace('\n', ". ")));
    let remove = idle.then(|| remove_button(id, &name, cx));
    let card = Attachment::new()
        .id(SharedString::from(format!("irc-attachment-card:{id}")))
        .xsmall()
        .status(status)
        .media(media)
        .content(AttachmentContent::new().title(AttachmentTitle::new(name)))
        .tooltip(tooltip)
        .when_some(remove, |card, remove| {
            card.actions(AttachmentActions::new().child(remove))
        });
    div()
        .id(SharedString::from(format!("irc-attachment:{id}")))
        .aria_label(label)
        .child(card)
        .test_support()
        .into_any_element()
}

/// A visible, labelled ×, rather than the Kit's hover-only "Remove".
fn remove_button(id: usize, name: &str, cx: &mut Context<Conversation>) -> IconButton {
    IconButton::new(SharedString::from(format!("irc-attachment-remove:{id}")))
        .icon(IconName::X)
        .compact()
        .flat()
        .accessibility_label(format!("Remove {name}"))
        .on_click(cx.listener(move |this, _, window, cx| {
            super::remove(this, id, window, cx);
        }))
}

/// The thumbnail's media, name, tooltip and status.
fn face(item: &Item) -> (AttachmentMedia, String, String, AttachmentStatus) {
    match &item.state {
        ItemState::Preparing(_) => (
            AttachmentMedia::new().child(Icon::from(IconName::File)),
            "Preparing…".to_owned(),
            "Preparing attachment…".to_owned(),
            AttachmentStatus::Processing,
        ),
        ItemState::Failed(error) => (
            AttachmentMedia::new().child(Icon::from(IconName::FileX)),
            "Can’t attach".to_owned(),
            error.to_string(),
            AttachmentStatus::Failed,
        ),
        ItemState::Ready(p) => ready(p),
    }
}

fn ready(p: &Pending) -> (AttachmentMedia, String, String, AttachmentStatus) {
    let media = match &p.preview {
        Preview::Image(image) => AttachmentMedia::new().src(image.clone()),
        Preview::Text(_) => AttachmentMedia::new().child(Icon::from(IconName::FileText)),
        Preview::File => AttachmentMedia::new().child(Icon::from(IconName::File)),
    };
    let name = p.draft.name().to_owned();
    let mut tooltip = format!("{name} · {}", size_text(p.draft.bytes().len()));
    if let Some(error) = &p.error {
        tooltip = format!("{tooltip}\n{error}");
    }
    let status = if p.busy {
        AttachmentStatus::Uploading
    } else if p.error.is_some() {
        AttachmentStatus::Failed
    } else {
        AttachmentStatus::Complete
    };
    (media, name, tooltip, status)
}

fn check(p: &Pending, idle: bool, cx: &mut Context<Conversation>) -> AnyElement {
    let id = p.draft.id().to_owned();
    action_button(
        SharedString::from(format!("irc-check-upload:{id}")),
        Intent::Neutral,
        Some(IconName::RefreshCw),
        format!("Check upload: {}", p.draft.name()),
    )
    .disabled(p.busy || !idle)
    .on_click(cx.listener(move |this, _, _, cx| super::check(this, &id, cx)))
    .into_any_element()
}
