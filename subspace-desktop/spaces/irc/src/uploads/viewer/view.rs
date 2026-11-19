use super::{CONTEXT, CloseUpload, Viewer, close, open, save};
use crate::conversation::Conversation;
use crate::uploads::intake::{self, Preview};
use gpui_kit::component::{Disableable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, Context, Div, InteractiveElement as _, IntoElement as _, ObjectFit,
    ParentElement as _, StatefulInteractiveElement as _, Styled as _, StyledImage as _,
    TestSupportExt as _, img,
};
use subspace_api_client::chat_uploads::{Upload, UploadError};
use subspace_framework::dashboard::{
    Tokens,
    action::{Intent, action_button},
};

pub(crate) fn render(this: &Conversation, cx: &mut Context<Conversation>) -> AnyElement {
    let Some(viewer) = &this.upload_viewer else {
        return gpui_kit::div().into_any_element();
    };
    let tokens = Tokens::current(cx);
    let authorised = viewer
        .scope
        .as_ref()
        .is_some_and(|scope| this.hub.read(cx).uploads.session.matches(scope));
    let expired = viewer.metadata.as_ref().is_some_and(Upload::expired);
    let body = v_flex()
        .id("chat-upload-viewer")
        .test_support()
        .size_full()
        .min_h_0()
        .key_context(CONTEXT)
        .on_action(cx.listener(|this, _: &CloseUpload, window, cx| close(this, window, cx)))
        .bg(tokens.surface)
        .child(header(viewer, !authorised || expired, &tokens, cx));
    if !authorised {
        return body
            .child(UploadError::Access.to_string())
            .child(retry(cx))
            .into_any_element();
    }
    if expired {
        return body.child("Expired after 30 days.").into_any_element();
    }
    body.children(viewer.save_error.clone())
        .child(content(viewer, cx))
        .into_any_element()
}
fn header(viewer: &Viewer, disabled: bool, tokens: &Tokens, cx: &mut Context<Conversation>) -> Div {
    let name = viewer
        .metadata
        .as_ref()
        .map_or_else(|| viewer.key.clone(), |m| m.file_name.clone());
    h_flex()
        .gap_2()
        .px_3()
        .py_2()
        .border_b_1()
        .border_color(tokens.line)
        .child(
            action_button(
                "chat-upload-viewer-close",
                Intent::Quiet,
                Some(subspace_framework::lucide::IconName::ArrowLeft),
                "Close",
            )
            .track_focus(&viewer.close)
            .on_click(cx.listener(|this, _, window, cx| close(this, window, cx))),
        )
        .child(
            gpui_kit::div()
                .flex_1()
                .min_w_0()
                .text_ellipsis()
                .child(name),
        )
        .child(
            action_button(
                "chat-upload-save",
                Intent::Neutral,
                Some(subspace_framework::lucide::IconName::Download),
                if viewer.saving {
                    "Saving…"
                } else {
                    "Save file…"
                },
            )
            .disabled(viewer.metadata.is_none() || viewer.saving || disabled)
            .on_click(cx.listener(|this, _, _, cx| save(this, cx))),
        )
}
fn content(viewer: &Viewer, cx: &mut Context<Conversation>) -> AnyElement {
    match &viewer.content {
        Some(Preview::Image(image)) => gpui_kit::div()
            .flex_1()
            .min_h_0()
            .child(
                img(image.clone())
                    .size_full()
                    .object_fit(ObjectFit::Contain),
            )
            .into_any_element(),
        Some(Preview::Text(text)) => text_content(viewer, text),
        Some(Preview::File) => v_flex()
            .id("chat-upload-file-details")
            .test_support()
            .gap_2()
            .p_3()
            .children(
                viewer
                    .metadata
                    .as_ref()
                    .map(|meta| format!("{} bytes", meta.size_bytes)),
            )
            .child("Save this file to open it in another app.")
            .into_any_element(),
        None if viewer.error.is_some() => v_flex()
            .gap_2()
            .p_3()
            .child(
                viewer
                    .error
                    .as_ref()
                    .map(ToString::to_string)
                    .unwrap_or_default(),
            )
            .child(retry(cx))
            .into_any_element(),
        None => gpui_kit::div()
            .p_3()
            .child("Loading attachment…")
            .into_any_element(),
    }
}
fn text_content(viewer: &Viewer, text: &str) -> AnyElement {
    let (shown, shortened) = intake::preview_text(text);
    let text = if viewer
        .metadata
        .as_ref()
        .is_some_and(|m| m.content_type == "text/markdown")
    {
        subspace_framework::dashboard::rich::rich_text(
            "chat-upload-text",
            subspace_framework::dashboard::rich::Format::Markdown,
            shown,
        )
        .scrollable(true)
        .into_any_element()
    } else {
        subspace_framework::dashboard::find_in_page::scroll(
            viewer.scroll.clone(),
            gpui_kit::div()
                .id("chat-upload-literal-scroll")
                .size_full()
                .overflow_y_scroll()
                .track_scroll(&viewer.scroll)
                .child(subspace_framework::dashboard::rich::plain_text(
                    "chat-upload-text",
                    shown,
                )),
        )
        .into_any_element()
    };
    v_flex()
        .flex_1()
        .min_h_0()
        .p_3()
        .child(gpui_kit::div().flex_1().min_h_0().child(text))
        .children(shortened.then(|| {
            gpui_kit::div()
                .p_2()
                .child("Preview shortened. Save the file to read it in full.")
        }))
        .into_any_element()
}

fn retry(cx: &mut Context<Conversation>) -> impl gpui_kit::IntoElement {
    action_button(
        "chat-upload-retry",
        Intent::Neutral,
        Some(subspace_framework::lucide::IconName::RefreshCw),
        "Retry",
    )
    .on_click(cx.listener(|this, _, window, cx| {
        if let Some(viewer) = &this.upload_viewer {
            open(this, viewer.key.clone(), window, cx);
        }
    }))
}
