//! The attachment panel beside the issue page: the shared detail panel's
//! width, surface and edge, a title bar with Close, and the file in the
//! description's Markdown renderer or the theme's monospace text.
use super::{Content, TextViewer};
use crate::Issues;
use gpui_kit::component::{ActiveTheme as _, Icon, h_flex, v_flex};
use gpui_kit::{
    AnyElement, Context, Div, InteractiveElement as _, IntoElement as _, ParentElement as _,
    SharedString, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, Window, div,
    px, relative,
};
use subspace_api_client::media::MAX_TEXT_BYTES;
use subspace_framework::dashboard::action::{Intent, icon_button};
use subspace_framework::dashboard::feedback::{self, Words};
use subspace_framework::dashboard::panel::resizable_left_edge;
use subspace_framework::dashboard::rich::{plain_text_with, rich_text_prepared};
use subspace_framework::dashboard::row_menu::MenuChat;
use subspace_framework::dashboard::{Tokens, heading_font, links, truncated};
use subspace_framework::lucide::IconName;

/// The panel never grows past its parent pane, as the shared detail panel.
const FULL_PARENT_WIDTH: f32 = 1.;
const MIB: usize = 1024 * 1024;
/// The viewer's limit in whole MiB, the unit help uses.
const LIMIT_MIB: usize = MAX_TEXT_BYTES / MIB;
const DIGITS_PER_GROUP: usize = 3;

pub(crate) fn panel(
    issues: &Issues,
    tokens: &Tokens,
    window: &mut Window,
    cx: &mut Context<Issues>,
) -> Option<AnyElement> {
    let viewer = issues.media.text.as_ref()?;
    let width = super::panel_width(issues, tokens);
    let pane = v_flex()
        .id("attachment-viewer")
        .aria_label(format!("Attachment {}", viewer.media.file_name))
        .test_support()
        .relative()
        .flex_shrink_0()
        .w(px(width))
        .max_w(relative(FULL_PARENT_WIDTH))
        .h_full()
        .min_h_0()
        .bg(tokens.surface)
        .border_l_1()
        .border_color(tokens.line)
        .child(header(viewer, tokens, cx))
        .child(
            div()
                .id("attachment-viewer-body")
                .flex_1()
                .min_h_0()
                .child(body(issues, viewer, tokens, cx)),
        );
    Some(
        resizable_left_edge(pane, "attachment-viewer-resize", tokens, window, cx)
            .into_any_element(),
    )
}

/// The file's glyph, name and size, and Close.
fn header(viewer: &TextViewer, tokens: &Tokens, cx: &mut Context<Issues>) -> Div {
    let media = &viewer.media;
    let size = crate::markdown::format_file_size(media.size_bytes.max(0) as u64);
    h_flex()
        .flex_shrink_0()
        .min_h(tokens.toolbar_height)
        .gap_2()
        .px(tokens.pad_x)
        .py_1()
        .border_b_1()
        .border_color(tokens.line)
        .child(
            Icon::from(IconName::FileText)
                .flex_shrink_0()
                .size(tokens.heading_font)
                .text_color(tokens.muted),
        )
        .child(
            truncated::text("attachment-viewer-title", media.file_name.clone())
                .flex_1()
                .font_family(heading_font(cx))
                .text_size(tokens.heading_font)
                .text_color(tokens.fg_strong),
        )
        .child(
            div()
                .id("attachment-viewer-size")
                .aria_label(size.clone())
                .test_support()
                .flex_shrink_0()
                .text_size(tokens.secondary_size())
                .text_color(tokens.muted)
                .child(size),
        )
        .child(
            icon_button(
                "attachment-viewer-close",
                Intent::Quiet,
                IconName::X,
                "Close attachment",
            )
            .track_focus(&viewer.close)
            .on_click(cx.listener(|this, _, window, cx| {
                this.close_text(window, cx);
            })),
        )
}

fn body(
    issues: &Issues,
    viewer: &TextViewer,
    tokens: &Tokens,
    cx: &mut Context<Issues>,
) -> AnyElement {
    let name = &viewer.media.file_name;
    match &viewer.content {
        Content::Loading => div()
            .p_4()
            .child(feedback::loading_line(
                tokens,
                "attachment-viewer-loading",
                format!("Loading {name}…"),
            ))
            .into_any_element(),
        Content::Failed => feedback::error_box_words(
            tokens,
            "attachment-viewer-failed",
            words(
                format!("Couldn’t load {name}"),
                "Check the connection, then try again.",
            ),
            [feedback::retry_action(
                "attachment-viewer-retry",
                cx.listener(|this, _, _, cx| this.retry_text(cx)),
            )],
        )
        .into_any_element(),
        Content::TooLarge => notice(
            tokens,
            "attachment-viewer-large",
            words("Too large to preview here", too_large(&viewer.media)),
        ),
        Content::Empty => notice(
            tokens,
            "attachment-viewer-empty",
            words("Nothing to show", "This file is empty."),
        ),
        Content::Unreadable => notice(
            tokens,
            "attachment-viewer-unreadable",
            words(
                format!("Couldn’t show {name}"),
                "The file isn’t UTF-8 text.",
            ),
        ),
        Content::Markdown(text) => markdown(issues, viewer, text.clone(), tokens),
        Content::Plain(text) => plain(viewer, text, tokens, cx),
    }
}

fn words(title: impl Into<SharedString>, message: impl Into<SharedString>) -> Words {
    Words {
        chat: MenuChat::Off,
        ..Words::new(title, message)
    }
}

/// The file's size and the limit in exact bytes, and the limit in MiB as
/// help states it, so a file just over the limit never reads as within it.
pub(super) fn too_large(media: &subspace_api_client::model::IssueMedia) -> String {
    format!(
        "{} is {} bytes. The viewer shows files up to {LIMIT_MIB} MiB ({} bytes).",
        media.file_name,
        grouped(media.size_bytes.max(0) as u64),
        grouped(MAX_TEXT_BYTES as u64),
    )
}

/// A byte count with comma thousands separators, as "1,048,576".
fn grouped(count: u64) -> String {
    let digits = count.to_string();
    let groups: Vec<&str> = digits
        .as_bytes()
        .rchunks(DIGITS_PER_GROUP)
        .rev()
        .map(|group| std::str::from_utf8(group).unwrap_or_default())
        .collect();
    groups.join(",")
}

fn notice(tokens: &Tokens, instance: &'static str, words: Words) -> AnyElement {
    feedback::empty_words(tokens, instance, words, std::iter::empty::<AnyElement>())
        .into_any_element()
}

/// The description's renderer and link policy. Its own virtual list scrolls
/// a long file, so the panel edge carries the scrollbar.
fn markdown(
    issues: &Issues,
    viewer: &TextViewer,
    text: SharedString,
    tokens: &Tokens,
) -> AnyElement {
    let id = format!(
        "attachment-text:{}:{}",
        viewer.media.media_id, viewer.media.sha256
    );
    crate::media::view::markdown(
        rich_text_prepared(id, text, |url, _, _| {
            links::safe_url(url).is_none() || url.starts_with("subspace-search:")
        }),
        issues,
    )
    .scrollable(true)
    .size_full()
    .px(tokens.pad_x)
    .py_4()
    .into_any_element()
}

/// Plain text in the theme's monospace face, with native selection, copy and
/// an explicit Add to chat action. It never supplies automatic chat context.
fn plain(
    viewer: &TextViewer,
    text: &SharedString,
    tokens: &Tokens,
    cx: &Context<Issues>,
) -> AnyElement {
    div()
        .id("attachment-viewer-text")
        .test_support()
        .size_full()
        .overflow_y_scroll()
        .track_scroll(&viewer.scroll)
        .px(tokens.pad_x)
        .py_4()
        .font_family(cx.theme().mono_font_family.clone())
        .text_size(tokens.mono_font)
        .child(subspace_framework::dashboard::find_in_page::scroll(
            viewer.scroll.clone(),
            plain_text_with(
                format!("attachment-text:{}", viewer.media.media_id),
                text,
                MenuChat::Offered,
            ),
        ))
        .into_any_element()
}
