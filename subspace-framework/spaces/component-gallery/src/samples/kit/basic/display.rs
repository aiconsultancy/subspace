//! Read-only GPUI Kit basics: alerts, badges, breadcrumbs, empty states,
//! icons, images and keys.

use std::sync::Arc;

use gpui_kit::component::alert::Alert;
use gpui_kit::component::badge::Badge;
use gpui_kit::component::breadcrumb::{Breadcrumb, BreadcrumbItem};
use gpui_kit::component::button::Button;
use gpui_kit::component::empty::{
    Empty, EmptyContent, EmptyDescription, EmptyHeader, EmptyMedia, EmptyTitle,
};
use gpui_kit::component::kbd::Kbd;
use gpui_kit::component::{Icon, Sizable as _, h_flex};
use gpui_kit::{
    Context, Image, ImageFormat, Keystroke, ParentElement as _, Styled as _, Window, img, px,
};
use subspace_dashboard::Tokens;
use subspace_lucide::IconName;

use crate::ComponentGallery;
use crate::samples::{Sample, ignore_click, sample, sized};

const WIDE: f32 = 320.;
const IMAGE_SIZE: f32 = 48.;
const BADGE_COUNT: usize = 8;
const BADGE_OVERFLOW: usize = 120;
const BADGE_MAX: usize = 99;
const ARTWORK: &[u8] = include_bytes!("../../../../assets/component.svg");

pub(crate) fn alerts(
    _: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let alert = |state: &'static str, alert: Alert| sample(state, sized(WIDE, alert));
    vec![
        alert(
            "Info",
            Alert::info("gallery-kit:alert-info", "The fixture board is read only.")
                .title("Read only"),
        ),
        alert(
            "Success",
            Alert::success("gallery-kit:alert-success", "Fixture saved."),
        ),
        alert(
            "Warning",
            Alert::warning(
                "gallery-kit:alert-warning",
                "The fixture token expires soon.",
            ),
        ),
        alert(
            "Error",
            Alert::error("gallery-kit:alert-error", "Couldn’t reach example.com.")
                .title("Connection failed"),
        ),
        alert(
            "Banner",
            Alert::info("gallery-kit:alert-banner", "A banner spans its container.").banner(),
        ),
    ]
}

pub(crate) fn badges(
    _: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let bell = || Icon::from(IconName::Bell);
    vec![
        sample("Count", Badge::new().count(BADGE_COUNT).child(bell())),
        sample(
            "Over the maximum",
            Badge::new()
                .count(BADGE_OVERFLOW)
                .max(BADGE_MAX)
                .child(bell()),
        ),
        sample("Dot", Badge::new().dot().child(bell())),
        sample(
            "Icon",
            Badge::new()
                .icon(IconName::Check)
                .child(Icon::from(IconName::Inbox)),
        ),
    ]
}

pub(crate) fn breadcrumbs(
    _: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    vec![sample(
        "Path with a disabled step",
        Breadcrumb::new()
            .child(BreadcrumbItem::new("Boards"))
            .child(BreadcrumbItem::new("Fixtures").disabled(true))
            .child(BreadcrumbItem::new("Component gallery")),
    )]
}

pub(crate) fn empties(
    _: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let empty = Empty::new()
        .header(
            EmptyHeader::new()
                .media(EmptyMedia::new().child(Icon::from(IconName::Inbox)))
                .title(EmptyTitle::new().child("No fixtures"))
                .description(
                    EmptyDescription::new().child("Add a fixture to see it in this list."),
                ),
        )
        .content(
            EmptyContent::new().child(
                Button::new("gallery-kit:empty-action")
                    .label("Add fixture")
                    .small()
                    .on_click(ignore_click),
            ),
        );
    vec![sample(
        "Media, title, description and action",
        sized(WIDE, empty),
    )]
}

pub(crate) fn icons(
    _: &ComponentGallery,
    tokens: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    vec![
        sample(
            "Sizes",
            h_flex()
                .gap_2()
                .child(Icon::from(IconName::Component).xsmall())
                .child(Icon::from(IconName::Component).small())
                .child(Icon::from(IconName::Component))
                .child(Icon::from(IconName::Component).large()),
        ),
        sample(
            "Semantic colours",
            h_flex()
                .gap_2()
                .child(Icon::from(IconName::CircleCheck).text_color(tokens.green))
                .child(Icon::from(IconName::TriangleAlert).text_color(tokens.orange))
                .child(Icon::from(IconName::CircleX).text_color(tokens.red)),
        ),
    ]
}

pub(crate) fn images(
    _: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let source = Arc::new(Image::from_bytes(ImageFormat::Svg, ARTWORK.to_vec()));
    vec![sample("Embedded SVG", img(source).size(px(IMAGE_SIZE)))]
}

pub(crate) fn keys(
    _: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let key = |text: &'static str| Keystroke::parse(text).ok().map(Kbd::new);
    vec![
        sample(
            "Shortcuts",
            h_flex()
                .gap_2()
                .children(key("cmd-k"))
                .children(key("cmd-shift-p"))
                .children(key("escape")),
        ),
        sample("Outline", h_flex().children(key("cmd-s").map(Kbd::outline))),
    ]
}
