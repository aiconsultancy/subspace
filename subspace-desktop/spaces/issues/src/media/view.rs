use super::{Preview, key};
use crate::Issues;
use gpui_kit::component::{Sizable as _, button::ButtonVariants as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Context, Div, InteractiveElement as _, IntoElement as _, ObjectFit,
    ParentElement as _, StatefulInteractiveElement as _, Styled as _, StyledImage as _,
    TestSupportExt as _, canvas, img, rems,
};
use subspace_api_client::{hub::IssueSnapshot, model::IssueMedia};
use subspace_framework::dashboard::{
    Tokens,
    action::{ActionState, Intent, action_button},
    icon_button::IconButton,
};
use subspace_framework::lucide::IconName;
const PREVIEW_HEIGHT_REM: f32 = 7.;
const VIEWPORT_PREFETCH_SCREENS: f32 = 2.;

pub(crate) fn descriptor(issues: &Issues, id: &str) -> Option<IssueMedia> {
    let open = issues.issue_view.snapshot.as_ref()?;
    open.files()
        .items()
        .iter()
        .chain(
            open.timeline()
                .page()
                .into_iter()
                .flat_map(|page| page.items.iter())
                .filter_map(|entry| entry.comment.as_ref())
                .filter(|comment| comment.deleted_at.is_none())
                .flat_map(|comment| comment.media.iter()),
        )
        .chain(open.images().items().iter().map(|image| &image.media))
        .chain(
            open.timeline()
                .projected()
                .iter()
                .filter(|comment| comment.deleted_at.is_none())
                .flat_map(|comment| &comment.media),
        )
        .find(|media| media.media_id == id)
        .cloned()
}

pub(crate) fn markdown(
    text: subspace_framework::dashboard::rich::RichText,
    issues: &Issues,
) -> subspace_framework::dashboard::rich::RichText {
    super::plugin::install(text, &issues.weak)
}

pub(crate) fn file_chip(label: &str, cx: &App) -> Div {
    let tokens = Tokens::current(cx);
    subspace_framework::dashboard::media::file_chip(label.to_owned(), &tokens)
}

pub(crate) fn tile(
    issues: &Issues,
    media: IssueMedia,
    alt: &str,
    inline: bool,
    cx: &mut Context<Issues>,
) -> AnyElement {
    let tokens = Tokens::current(cx);
    let preview = issues.media.cache.get(&key(&media));
    let mut tile = subspace_framework::dashboard::media::tile(
        format!(
            "image-{}:{}",
            if inline { "inline" } else { "tile" },
            media.media_id
        ),
        alt.to_owned(),
        &tokens,
    )
    .test_support();
    if inline && let Some(anchor) = issues.media.anchors.get(&media.media_id) {
        tile = tile.anchor_scroll(Some(anchor.clone()));
    }
    let ready = matches!(preview, Some(Preview::Ready(_)));
    tile = tile
        .when(ready && !inline, |tile| tile.min_h_0())
        .child(preview_body(preview, &media, &tokens))
        .when(ready && inline, |tile| tile.child(media.file_name.clone()));
    let focus_key = if inline {
        media.media_id.clone()
    } else {
        format!("panel:{}", media.media_id)
    };
    let focus = issues.media.focus.get(&focus_key).cloned();
    let opener = focus.clone();
    tile = tile.child(tile_action(
        media.clone(),
        alt.to_owned(),
        preview,
        (inline, focus),
        cx,
    ));
    let open_media = media.clone();
    let alt = alt.to_owned();
    tile = tile.on_click(cx.listener(move |this, _, window, cx| {
        cx.stop_propagation();
        if let Some(focus) = &opener {
            focus.focus(window, cx);
        }
        this.open_image(open_media.clone(), alt.clone(), window, cx);
    }));
    if preview.is_none() {
        tile = tile.child(visible_request(issues, media));
    }
    tile.into_any_element()
}

fn visible_request(issues: &Issues, media: IssueMedia) -> impl gpui_kit::IntoElement {
    let weak = issues.weak.clone();
    let scope = issues.media.scope;
    let issue = issues.media.issue.clone();
    let revision = issues.media.revision;
    canvas(
        move |bounds, window, cx| {
            let height = window.viewport_size().height;
            if bounds.origin.y < height * VIEWPORT_PREFETCH_SCREENS && bounds.bottom() > -height {
                cx.defer(move |cx| {
                    let _ = weak.update(cx, |this, cx| {
                        if this.media.scope == scope
                            && this.media.issue == issue
                            && this.media.revision == revision
                        {
                            this.ensure_image(media, cx);
                        }
                    });
                });
            }
        },
        |_, (), _, _| {},
    )
    .absolute()
    .size_full()
}

pub(super) fn preview_body(
    preview: Option<&Preview>,
    media: &IssueMedia,
    tokens: &Tokens,
) -> AnyElement {
    match preview {
        Some(Preview::Ready(image)) => img(image.clone())
            .w_full()
            .h(rems(PREVIEW_HEIGHT_REM))
            .object_fit(ObjectFit::Contain)
            .into_any_element(),
        _ => {
            let label = match preview {
                Some(Preview::TooLarge) => "Too large to preview here",
                Some(Preview::Failed) => "Couldn’t load image",
                Some(Preview::Evicted) => "Preview cleared to save memory",
                _ => "Loading image…",
            };
            v_flex()
                .min_h(rems(PREVIEW_HEIGHT_REM))
                .justify_center()
                .text_color(tokens.muted)
                .gap_1()
                .child(label)
                .child(media.file_name.clone())
                .child(crate::markdown::format_file_size(
                    media.size_bytes.max(0) as u64
                ))
                .into_any_element()
        }
    }
}

pub(crate) fn panel(
    issues: &Issues,
    open: &IssueSnapshot,
    cx: &mut Context<Issues>,
) -> Option<AnyElement> {
    let images = open.images();
    if !images.is_visible() {
        return None;
    }
    let tokens = Tokens::current(cx);
    let mut panel = v_flex()
        .id("issue-images")
        .test_support()
        .gap_3()
        .min_w_0()
        .child(crate::view::section_heading(
            format!(
                "Images · {}{}",
                images.items().len(),
                if images.is_partial() { "+" } else { "" }
            ),
            IconName::Images,
            &tokens,
        ));
    panel = panel.child(
        h_flex().items_start().gap_2().flex_wrap().children(
            images
                .items()
                .iter()
                .map(|image| tile(issues, image.media.clone(), &image.alt_text, false, cx)),
        ),
    );
    if images.is_loading() || images.is_failed() || images.has_more() {
        let label = if images.is_failed() {
            "Retry"
        } else if images.is_loading() {
            "Loading images…"
        } else {
            "Look further for images"
        };
        let button = action_button(
            "issue-images-more",
            Intent::Neutral,
            Some(IconName::Images),
            label,
        )
        .small()
        .on_click(cx.listener(|this, _, _, _| {
            if let Some(hub) = &this.hub {
                let _ = hub.load_images(None);
            }
        }));
        panel = panel.child(if images.is_loading() {
            ActionState::Busy.apply(button)
        } else {
            button
        });
    }
    Some(panel.into_any_element())
}

pub(crate) use super::viewer::render as viewer;

fn tile_action(
    media: IssueMedia,
    alt: String,
    preview: Option<&Preview>,
    target: (bool, Option<gpui_kit::FocusHandle>),
    cx: &mut Context<Issues>,
) -> IconButton {
    let (inline, focus) = target;
    let failed = matches!(preview, Some(Preview::Failed));
    let id = format!(
        "image-{}-open:{}",
        if inline { "inline" } else { "panel" },
        media.media_id
    );
    let button = match preview {
        Some(Preview::Ready(_)) if !inline => {
            let name = format!("Open {}", media.file_name);
            IconButton::new(id)
                .secondary()
                .icon(IconName::Expand)
                .accessibility_label(name.clone())
                .tooltip(name)
                .absolute()
                .top_2()
                .right_2()
        }
        Some(Preview::Failed) => {
            action_button(id, Intent::Neutral, Some(IconName::RotateCw), "Retry")
        }
        _ => action_button(id, Intent::Neutral, Some(IconName::Expand), "Open image"),
    };
    let opener = focus.clone();
    button
        .small()
        .map(|button| match focus {
            Some(focus) => button.track_focus(&focus),
            None => button,
        })
        .on_click(cx.listener(move |this, _, window, cx| {
            cx.stop_propagation();
            if failed {
                this.retry_image(media.clone(), cx);
            } else {
                if let Some(focus) = &opener {
                    focus.focus(window, cx);
                }
                this.open_image(media.clone(), alt.clone(), window, cx);
            }
        }))
}
