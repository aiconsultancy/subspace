//! Product conformance checks against the real host parser and image renderer.
//! Available only in tests or with the `test-support` feature.

use std::sync::Arc;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Bounds, Image, ImageFormat, ImageSource, TestAppContext,
    WindowBounds, WindowOptions, px, size,
};
use subspace_agent_tools::BoardDoc;

use super::{BoardHelp, HelpContent, HelpPanel, HelpSections};
use crate::app::help_dock::state::Side;

const DRAW_FRAMES: usize = 3;

/// Assert that a bundled guide has useful contents and loses no source text.
pub(crate) fn assert_help_contents(markdown: &str) {
    let document = HelpSections::parse(markdown);
    assert!(document.shows_contents(), "{markdown:.40}");
    assert_eq!(document.sections().concat(), markdown);
    assert!(
        document
            .headings()
            .iter()
            .all(|heading| { !heading.title.is_empty() && heading.title.trim() == heading.title })
    );
}

/// Render a registered guide and assert that its first embedded image reaches
/// the native cache without a file or network fetch.
pub(crate) fn check_help_image_cache(
    board: BoardDoc,
    cx: &mut TestAppContext,
) -> Result<(), String> {
    let image = board.images.first().ok_or("guide must supply an image")?;
    let format = ImageFormat::from_mime_type(image.media_type).ok_or("unsupported image type")?;
    let source = ImageSource::Image(Arc::new(Image::from_bytes(format, image.bytes.to_vec())));
    cx.update(gpui_kit::init);
    assert!(!cx.update(|cx| source.is_asset_cached(cx)));
    let window = open_help(board, cx)?;
    for _ in 0..DRAW_FRAMES {
        cx.update_window(window, |_, window, cx| window.render_frame(cx))
            .map_err(|error| error.to_string())?;
        cx.run_until_parked();
    }
    assert!(
        cx.update(|cx| source.is_asset_cached(cx)),
        "the bundled help image must render as embedded bytes, without a file or network fetch"
    );
    Ok(())
}

fn open_help(board: BoardDoc, cx: &mut TestAppContext) -> Result<AnyWindowHandle, String> {
    cx.update(|cx| {
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                    Default::default(),
                    size(px(900.), px(2_200.)),
                ))),
                ..Default::default()
            },
            cx,
            |window, cx| {
                cx.new(|cx| {
                    let mut panel = HelpPanel::new(Side::Right, false, window, cx);
                    panel.set_content(
                        HelpContent {
                            board: Some(BoardHelp {
                                id: board.id,
                                title: board.title.into(),
                                markdown: board.markdown,
                                images: board.images,
                            }),
                            ..HelpContent::default()
                        },
                        cx,
                    );
                    panel
                })
            },
        )
        .map(|(window, _)| window)
        .map_err(|error| error.to_string())
    })
}
