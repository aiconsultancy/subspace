//! File metadata remains readable without opening an external download URL.
//! A text or Markdown file opens in the attachment panel beside the page.
use crate::Issues;
use gpui_kit::component::{Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::{AnyElement, Context, Div, IntoElement as _, ParentElement as _, Styled as _, div};
use subspace_api_client::{hub::IssueSnapshot, model::IssueMedia};
use subspace_framework::dashboard::{
    Tokens,
    action::{Intent, action_button, card_button},
};
use subspace_framework::lucide::IconName;

pub(crate) fn panel(open: &IssueSnapshot, cx: &mut Context<Issues>) -> Option<AnyElement> {
    let files = open.files();
    let rows = files
        .items()
        .iter()
        .filter(|media| media.kind == "file")
        .collect::<Vec<_>>();
    if rows.is_empty() && !files.is_failed() {
        return None;
    }
    let tokens = Tokens::current(cx);
    let mut panel = v_flex().gap_3().child(crate::view::section_heading(
        "Attachments",
        IconName::Paperclip,
        &tokens,
    ));
    for media in rows {
        let label = format!(
            "{} ({}, {})",
            media.file_name,
            media.content_type,
            crate::markdown::format_file_size(media.size_bytes.max(0) as u64)
        );
        panel = panel.child(if super::text::viewable(media) {
            open_button(media.clone(), label, "attachment-open", cx)
        } else {
            subspace_framework::dashboard::media::file_chip(label, &tokens).into_any_element()
        });
    }
    if files.is_failed() {
        panel = panel.child("Couldn’t load attachments").child(
            action_button(
                "retry-issue-attachments",
                Intent::Neutral,
                Some(IconName::RefreshCw),
                "Retry",
            )
            .on_click(cx.listener(|this, _, _, _| {
                if let Some(hub) = &this.hub {
                    _ = hub.load_attachments();
                }
            })),
        );
    }
    Some(panel.into_any_element())
}

/// The whole file row opens the attachment panel, with the chip's words. `place`
/// keeps the Attachments row and an inline reference to the same file apart.
pub(super) fn open_button(
    media: IssueMedia,
    label: String,
    place: &str,
    cx: &mut Context<Issues>,
) -> AnyElement {
    let tokens = Tokens::current(cx);
    let id = format!("{place}:{}", media.media_id);
    card_button(id, format!("Open {label}"), open_label(&label, &tokens))
        .min_w_0()
        .on_click(cx.listener(move |this, _, window, cx| {
            cx.stop_propagation();
            this.open_text(media.clone(), window, cx);
        }))
        .into_any_element()
}

fn open_label(label: &str, tokens: &Tokens) -> Div {
    h_flex()
        .flex_1()
        .min_w_0()
        .gap_2()
        .child(
            Icon::from(IconName::FileText)
                .small()
                .flex_shrink_0()
                .text_color(tokens.muted),
        )
        .child(
            div()
                .min_w_0()
                .text_ellipsis()
                .whitespace_nowrap()
                .child(label.to_owned()),
        )
}
