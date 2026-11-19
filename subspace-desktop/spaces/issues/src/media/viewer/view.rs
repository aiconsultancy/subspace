use super::super::{Preview, key};
use super::{Position, Viewer};
use crate::Issues;
use gpui_kit::component::{Disableable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, Div, InteractiveElement as _, IntoElement as _, ObjectFit,
    ParentElement as _, StatefulInteractiveElement as _, Styled as _, StyledImage as _,
    TestSupportExt as _, Window, div, img,
};
use subspace_framework::dashboard::{
    Tokens,
    action::{Intent, action_button},
};
use subspace_framework::lucide::IconName;

pub(crate) fn render(issues: &Issues, cx: &mut Context<Issues>) -> Option<AnyElement> {
    let viewer = issues.media.viewer.as_ref()?;
    let tokens = Tokens::current(cx);
    let position = viewer.position(issues.issue_view.snapshot.as_ref()?);
    let counter = position.map(|position| {
        format!(
            "{} of {}{} on this issue",
            position.index + 1,
            position.count,
            if position.partial { "+" } else { "" }
        )
    });
    Some(
        backdrop(&tokens, cx)
            .child(header(viewer, counter, &tokens, cx))
            .child(stage(issues, position, &tokens, cx))
            .child(footer(issues, &tokens, cx))
            .into_any_element(),
    )
}

fn backdrop(
    tokens: &Tokens,
    cx: &mut Context<Issues>,
) -> impl gpui_kit::ParentElement + gpui_kit::IntoElement + use<> {
    let mut context = gpui_kit::KeyContext::new_with_defaults(cx.platform_info().os);
    context.add(subspace_framework::dashboard::MODAL_CONTEXT);
    context.add(crate::media::VIEWER_CONTEXT);
    v_flex()
        .id("image-viewer")
        .test_support()
        .absolute()
        .inset_0()
        .occlude()
        .key_context(context)
        .bg(tokens.media_stage)
        .text_color(tokens.fg)
        .on_action(cx.listener(|this, _: &crate::media::PreviousImage, _, cx| {
            this.step_image(false, cx);
            cx.stop_propagation();
        }))
        .on_action(cx.listener(|this, _: &crate::media::NextImage, _, cx| {
            this.step_image(true, cx);
            cx.stop_propagation();
        }))
        .on_action(
            cx.listener(|this, _: &crate::media::CloseImage, window, cx| {
                this.close_image(window, cx);
                cx.stop_propagation();
            }),
        )
        .on_action(
            cx.listener(|this, _: &crate::media::NextImageControl, window, cx| {
                cycle_focus(this, false, window, cx);
                cx.stop_propagation();
            }),
        )
        .on_action(
            cx.listener(|this, _: &crate::media::PreviousImageControl, window, cx| {
                cycle_focus(this, true, window, cx);
                cx.stop_propagation();
            }),
        )
}

fn header(
    viewer: &Viewer,
    counter: Option<String>,
    tokens: &Tokens,
    cx: &mut Context<Issues>,
) -> Div {
    h_flex()
        .gap_2()
        .px_4()
        .py_2()
        .border_b_1()
        .border_color(tokens.line)
        .child(viewer.media.file_name.clone())
        .when_some(counter, |header, counter| {
            header.child(
                div()
                    .id("image-viewer-count")
                    .aria_label(counter.clone())
                    .test_support()
                    .text_color(tokens.muted)
                    .child(counter),
            )
        })
        .child(div().flex_1())
        .child(
            action_button(
                "image-viewer-close",
                Intent::Quiet,
                Some(IconName::X),
                "Close",
            )
            .track_focus(&viewer.close)
            .on_click(cx.listener(|this, _, window, cx| this.close_image(window, cx))),
        )
}

fn stage(
    issues: &Issues,
    navigation: Option<Position>,
    tokens: &Tokens,
    cx: &mut Context<Issues>,
) -> Div {
    let viewer = issues.media.viewer.as_ref();
    let Some(viewer) = viewer else {
        return div();
    };
    h_flex()
        .flex_1()
        .min_h_0()
        .gap_4()
        .p_4()
        .items_center()
        .child(
            action_button(
                "image-viewer-previous",
                Intent::Neutral,
                Some(IconName::ChevronLeft),
                "Previous",
            )
            .track_focus(&viewer.previous)
            .disabled(viewer.alone || navigation.is_none_or(|position| position.index == 0))
            .on_click(cx.listener(|this, _, _, cx| this.step_image(false, cx))),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .h_full()
                .flex()
                .items_center()
                .justify_center()
                .child(viewer_image(issues, tokens)),
        )
        .child(
            action_button(
                "image-viewer-next",
                Intent::Neutral,
                Some(IconName::ChevronRight),
                "Next",
            )
            .track_focus(&viewer.next)
            .disabled(
                viewer.alone
                    || navigation.is_none_or(|position| {
                        position.index + 1 >= position.count && !position.more
                    }),
            )
            .on_click(cx.listener(|this, _, _, cx| this.step_image(true, cx))),
        )
}

fn footer(issues: &Issues, tokens: &Tokens, cx: &mut Context<Issues>) -> Div {
    let Some(viewer) = issues.media.viewer.as_ref() else {
        return div();
    };
    let mut footer = v_flex()
        .text_color(tokens.fg)
        .items_center()
        .gap_2()
        .p_3()
        .child(viewer.alt.clone());
    if revealable(issues, viewer) {
        let label = if viewer.comment.is_some() {
            "Show in the comment"
        } else {
            "Show in the description"
        };
        footer = footer.child(
            action_button(
                "image-viewer-show",
                Intent::Quiet,
                Some(IconName::FileText),
                label,
            )
            .track_focus(&viewer.show)
            .on_click(cx.listener(|this, _, window, cx| this.reveal_image(window, cx))),
        );
    }
    footer
}

fn viewer_image(issues: &Issues, tokens: &Tokens) -> AnyElement {
    let Some(viewer) = issues.media.viewer.as_ref() else {
        return div().into_any_element();
    };
    match issues.media.cache.get(&key(&viewer.media)) {
        Some(Preview::Ready(image)) => img(image.clone())
            .max_w_full()
            .max_h_full()
            .object_fit(ObjectFit::Contain)
            .into_any_element(),
        preview => super::super::view::preview_body(preview, &viewer.media, tokens),
    }
}

fn revealable(issues: &Issues, viewer: &Viewer) -> bool {
    let Some(issue) = issues.issue_view.snapshot.as_ref() else {
        return false;
    };
    match &viewer.comment {
        Some(id) => issue.timeline().page().is_some_and(|page| {
            page.items.iter().any(|entry| {
                entry
                    .comment
                    .as_ref()
                    .is_some_and(|comment| &comment.id == id && comment.deleted_at.is_none())
            })
        }),
        None => issue.detail().is_some_and(|detail| {
            detail
                .description
                .as_ref()
                .is_some_and(|text| text.contains(&format!("nett-media:{}", viewer.media.media_id)))
        }),
    }
}

fn cycle_focus(issues: &Issues, reverse: bool, window: &mut Window, cx: &mut Context<Issues>) {
    let Some(viewer) = issues.media.viewer.as_ref() else {
        return;
    };
    let position = issues
        .issue_view
        .snapshot
        .as_ref()
        .and_then(|issue| viewer.position(issue));
    let mut handles = vec![&viewer.close];
    if !viewer.alone && position.is_some_and(|position| position.index > 0) {
        handles.push(&viewer.previous);
    }
    if !viewer.alone
        && position.is_some_and(|position| position.index + 1 < position.count || position.more)
    {
        handles.push(&viewer.next);
    }
    if revealable(issues, viewer) {
        handles.push(&viewer.show);
    }
    let ix = handles
        .iter()
        .position(|handle| handle.is_focused(window))
        .unwrap_or(0);
    let next = if reverse {
        (ix + handles.len() - 1) % handles.len()
    } else {
        (ix + 1) % handles.len()
    };
    handles[next].focus(window, cx);
}
