//! The private media previews Issues and Channels show: an image tile, a
//! file chip, attachment thumbnails and the unavailable-attachment chip.

use gpui_kit::component::h_flex;
use gpui_kit::{Context, ParentElement as _, Styled as _, Window};
use subspace_dashboard::action::{Intent, action_button};
use subspace_dashboard::{Tokens, media};
use subspace_lucide::IconName;

use super::super::{Sample, flag, sample, store_flag};
use crate::ComponentGallery;

pub(in crate::samples) fn media_tile(
    gallery: &ComponentGallery,
    tokens: &Tokens,
    _: &mut Window,
    cx: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let values = gallery.retained.values.clone();
    let opened = flag(&values, "media-preview", false, cx);
    vec![sample(
        "Open and close preview",
        media::tile("gallery-media", "Fixture sketch", tokens)
            .child(if opened {
                "Fixture image preview"
            } else {
                "Fixture sketch.png"
            })
            .child(
                action_button(
                    "gallery-media-open",
                    Intent::Neutral,
                    Some(IconName::Image),
                    if opened {
                        "Close preview"
                    } else {
                        "Open image"
                    },
                )
                .on_click(move |_, _, cx| store_flag(&values, "media-preview", !opened, cx)),
            ),
    )]
}

pub(in crate::samples) fn file_chip(
    _: &ComponentGallery,
    tokens: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    vec![sample(
        "File metadata",
        media::file_chip("Fixture notes.txt (text/plain, 14 bytes)", tokens),
    )]
}

pub(in crate::samples) fn attachment_thumbnails(
    _: &ComponentGallery,
    tokens: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let face = |name: &'static str| {
        (
            name.into(),
            format!("Open {name}, 2.4 KB, expires 17 Dec 2026").into(),
            format!("{name} · 2.4 KB · Expires 17 Dec 2026").into(),
        )
    };
    vec![
        sample(
            "Text chip",
            media::thumbnail(
                "gallery-thumb-text",
                media::Thumb::Text,
                face("fixture-notes.md"),
                (false, |_: &mut Window, _: &mut gpui_kit::App| {}),
                tokens,
            ),
        ),
        sample(
            "Row in sent order, keyboard on the second",
            h_flex()
                .flex_wrap()
                .gap_2()
                .child(media::thumbnail(
                    "gallery-thumb-row-1",
                    media::Thumb::Text,
                    face("fixture-plan.md"),
                    (false, |_: &mut Window, _: &mut gpui_kit::App| {}),
                    tokens,
                ))
                .child(media::thumbnail(
                    "gallery-thumb-row-2",
                    media::Thumb::File,
                    face("fixture-report.pdf"),
                    (true, |_: &mut Window, _: &mut gpui_kit::App| {}),
                    tokens,
                )),
        ),
        sample(
            "Loading and retry",
            h_flex()
                .gap_2()
                .child(media::thumbnail(
                    "gallery-thumb-loading",
                    media::Thumb::Loading,
                    face("Loading…"),
                    (false, |_: &mut Window, _: &mut gpui_kit::App| {}),
                    tokens,
                ))
                .child(media::thumbnail(
                    "gallery-thumb-retry",
                    media::Thumb::Retry,
                    face("Couldn’t load"),
                    (false, |_: &mut Window, _: &mut gpui_kit::App| {}),
                    tokens,
                )),
        ),
        sample(
            "Unavailable",
            media::unavailable_chip(
                "gallery-thumb-missing",
                "UPL-FixtureKey0000000000",
                false,
                tokens,
            ),
        ),
    ]
}
