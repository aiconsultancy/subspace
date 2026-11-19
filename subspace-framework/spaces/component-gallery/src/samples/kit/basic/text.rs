//! GPUI Kit text basics: labels, links, separators, shimmer, skeletons,
//! tags, text views and tooltips.

use gpui_kit::component::button::Button;
use gpui_kit::component::label::Label;
use gpui_kit::component::link::Link;
use gpui_kit::component::separator::Separator;
use gpui_kit::component::shimmer::ShimmerText;
use gpui_kit::component::skeleton::Skeleton;
use gpui_kit::component::tag::Tag;
use gpui_kit::component::text::TextView;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::{
    Context, InteractiveElement as _, ParentElement as _, StatefulInteractiveElement as _,
    Styled as _, Window, div, px,
};
use subspace_dashboard::Tokens;

use crate::ComponentGallery;
use crate::samples::{FIXTURE_URL, Sample, ignore_click, sample, sized};

const WIDE: f32 = 320.;
const NARROW: f32 = 200.;
const SKELETON_LINE: f32 = 160.;
const SKELETON_SHORT: f32 = 96.;
const SKELETON_HEIGHT: f32 = 12.;
const SKELETON_AVATAR: f32 = 32.;
const MARKDOWN: &str = "**GPUI Kit** `TextView` renders *Markdown*:\n\n- lists\n- `code`\n- [links](https://example.com/component-gallery)";
const HTML: &str = "<p><b>HTML</b> with <i>inline</i> <code>code</code>.</p>";

pub(crate) fn labels(
    _: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    vec![
        sample("Plain", Label::new("Fixture name")),
        sample(
            "With secondary text",
            Label::new("Fixture name").secondary("(optional)"),
        ),
        sample(
            "Highlighted match",
            Label::new("Component gallery").highlights("gall"),
        ),
        sample("Masked", Label::new("not-a-real-secret").masked(true)),
    ]
}

pub(crate) fn links(
    _: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    vec![
        sample(
            "External",
            Link::new("gallery-kit:link")
                .href(FIXTURE_URL)
                .child("example.com"),
        ),
        sample(
            "Disabled",
            Link::new("gallery-kit:link-disabled")
                .href(FIXTURE_URL)
                .disabled(true)
                .child("example.com"),
        ),
    ]
}

pub(crate) fn separators(
    _: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    vec![
        sample("Horizontal", sized(NARROW, Separator::horizontal())),
        sample(
            "Labelled",
            sized(NARROW, Separator::horizontal().label("or")),
        ),
        sample("Dashed", sized(NARROW, Separator::horizontal_dashed())),
        sample(
            "Vertical",
            h_flex()
                .h(px(SKELETON_AVATAR))
                .gap_2()
                .child("Left")
                .child(Separator::vertical())
                .child("Right"),
        ),
    ]
}

pub(crate) fn shimmers(
    _: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    vec![sample(
        "Streaming text",
        ShimmerText::new("Thinking about the fixture…").id("gallery-kit:shimmer"),
    )]
}

pub(crate) fn skeletons(
    _: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    vec![sample(
        "Row placeholder",
        h_flex()
            .gap_2()
            .child(Skeleton::new().size(px(SKELETON_AVATAR)).rounded_full())
            .child(
                v_flex()
                    .gap_1()
                    .child(Skeleton::new().w(px(SKELETON_LINE)).h(px(SKELETON_HEIGHT)))
                    .child(
                        Skeleton::new()
                            .secondary()
                            .w(px(SKELETON_SHORT))
                            .h(px(SKELETON_HEIGHT)),
                    ),
            ),
    )]
}

pub(crate) fn tags(
    _: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    vec![
        sample(
            "Variants",
            h_flex()
                .gap_1()
                .child(Tag::new().child("Default"))
                .child(Tag::primary().child("Primary"))
                .child(Tag::secondary().child("Secondary"))
                .child(Tag::success().child("Success"))
                .child(Tag::warning().child("Warning"))
                .child(Tag::danger().child("Danger"))
                .child(Tag::info().child("Info")),
        ),
        sample(
            "Outline and small",
            h_flex()
                .gap_1()
                .child(Tag::primary().outline().child("Outline"))
                .child(Tag::secondary().small().child("Small"))
                .child(Tag::info().rounded_full().child("Pill")),
        ),
    ]
}

pub(crate) fn text_views(
    _: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    vec![
        sample(
            "Markdown",
            sized(
                WIDE,
                TextView::markdown("gallery-kit:text-view-markdown", MARKDOWN),
            ),
        ),
        sample(
            "HTML",
            sized(WIDE, TextView::html("gallery-kit:text-view-html", HTML)),
        ),
    ]
}

pub(crate) fn tooltips(
    _: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    vec![
        sample(
            "Text; hover to show",
            Button::new("gallery-kit:tooltip-text")
                .label("Hover me")
                .small()
                .tooltip("A short explanation")
                .on_click(ignore_click),
        ),
        sample(
            "Custom element; hover to show",
            div()
                .id("gallery-kit:tooltip-element")
                .px_2()
                .child("Hover for details")
                .tooltip(|window, cx| {
                    Tooltip::element(|_, _| {
                        v_flex()
                            .child(div().child("Fixture details"))
                            .child(div().text_xs().child("Built with Tooltip::element"))
                    })
                    .build(window, cx)
                }),
        ),
    ]
}
