//! Reviewed synthetic screenshots shared by native Help and the board help tool.

use subspace_framework::dashboard::HelpImage;

pub(super) const IMAGES: &[HelpImage] = &[
    HelpImage::new(
        "dashboard",
        "Native starter dashboard with controls, chart, table and result metadata",
        "image/webp",
        include_bytes!("../help-assets/dashboard.webp"),
    ),
    HelpImage::new(
        "apply",
        "Working changes remain separate from the running dashboard until explicit apply",
        "image/webp",
        include_bytes!("../help-assets/apply.webp"),
    ),
    HelpImage::new(
        "settings",
        "Private settings show variable names and explicit keep, replace and clear choices",
        "image/webp",
        include_bytes!("../help-assets/settings.webp"),
    ),
];

#[cfg(test)]
mod tests {
    use super::IMAGES;

    #[test]
    fn guide_images_are_bounded_rasters_and_each_is_referenced_in_the_shared_guide() {
        subspace_framework::agent_tools::help_image::validate_images(IMAGES).unwrap();
        let guide = include_str!("../help.md");
        assert_eq!(IMAGES.len(), 3);
        for image in IMAGES {
            assert!(guide.contains(&format!("help-assets/{}.webp", image.id)));
            assert!(guide.contains(image.caption));
            assert!(!image.bytes.is_empty());
        }
        assert!(
            !guide.contains("data:image"),
            "guide reads must not inline binary context"
        );
    }
}
