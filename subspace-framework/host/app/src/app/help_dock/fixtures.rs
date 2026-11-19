//! Host-owned raster fixtures. Solid rectangles exercise decoding and changing
//! layout without compiling any board source or copying its screenshots.

use subspace_dashboard::HelpImage;

pub(super) const GUIDE: &str = include_str!("fixtures/images.md");
pub(super) const IMAGES: &[HelpImage] = &[
    HelpImage::new(
        "first",
        "First rectangle",
        "image/png",
        include_bytes!("fixtures/first.png"),
    ),
    HelpImage::new(
        "second",
        "Second rectangle",
        "image/png",
        include_bytes!("fixtures/second.png"),
    ),
    HelpImage::new(
        "third",
        "Third rectangle",
        "image/png",
        include_bytes!("fixtures/third.png"),
    ),
];
