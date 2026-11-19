//! Bundled help images are resolved only in the board-help document.

use crate::HelpImage;
use base64::Engine as _;

/// Expand exact bundled image targets before applying the unchanged rich sanitizer.
/// Unknown ids, paths and URLs never become image sources.
pub fn prepare_markdown(markdown: &str, images: &[HelpImage]) -> String {
    let expanded = expand_images(markdown, images);
    crate::rich::prepare_markdown(&expanded)
}

fn expand_images(markdown: &str, images: &[HelpImage]) -> String {
    if subspace_agent_tools::help_image::validate_images(images).is_err() {
        return markdown.to_owned();
    }
    let mut out = String::with_capacity(markdown.len());
    let mut rest = markdown;
    while let Some(start) = rest.find("![") {
        out.push_str(&rest[..start]);
        rest = &rest[start..];
        if let Some((source, end)) = image_target(rest) {
            out.push_str(&rest[..source]);
            out.push_str(&bundled_target(&rest[source..end], images));
            rest = &rest[end..];
        } else {
            out.push_str("![");
            rest = &rest[2..];
        }
    }
    out.push_str(rest);
    out
}

fn image_target(markdown: &str) -> Option<(usize, usize)> {
    let alt_end = markdown.find("](")?;
    let source = alt_end + 2;
    let end = source + markdown[source..].find(')')?;
    Some((source, end))
}

fn bundled_target(target: &str, images: &[HelpImage]) -> String {
    let image = if let Some(id) = target.strip_prefix("subspace-help-image:") {
        let Some(image) = images.iter().find(|image| image.id == id) else {
            return String::new();
        };
        Some(image)
    } else {
        images.iter().find(|image| {
            let extension = match image.media_type {
                "image/png" => "png",
                "image/webp" => "webp",
                _ => return false,
            };
            target == format!("help-assets/{}.{extension}", image.id)
        })
    };
    let Some(image) = image else {
        return target.to_owned();
    };
    format!(
        "data:{};base64,{}",
        image.media_type,
        base64::engine::general_purpose::STANDARD.encode(image.bytes)
    )
}

#[cfg(test)]
mod tests {
    use super::prepare_markdown;
    use crate::HelpImage;
    use base64::Engine as _;

    const IMAGES: &[HelpImage] = &[
        HelpImage::new(
            "dashboard",
            "Synthetic dashboard",
            "image/png",
            b"\x89PNG\r\n\x1a\ntest",
        ),
        HelpImage::new(
            "apply",
            "Synthetic apply",
            "image/webp",
            b"RIFF0000WEBPtest",
        ),
    ];

    #[test]
    fn only_exact_bundled_image_placeholders_expand_in_place() {
        let markdown = "Before\n\n![Dashboard](subspace-help-image:dashboard)\n\nAfter";
        let prepared = prepare_markdown(markdown, IMAGES);
        assert!(prepared.starts_with("Before\n\n![Dashboard](data:image/png;base64,"));
        assert!(prepared.ends_with(")\n\nAfter"));
        for target in ["unknown", "../dashboard", "dashboard/extra", "Dashboard"] {
            let prepared =
                prepare_markdown(&format!("![Test](subspace-help-image:{target})"), IMAGES);
            assert!(!prepared.contains("data:image"), "{prepared}");
        }
        for target in [
            "file:///private/test.png",
            "https://example.invalid/test.png",
        ] {
            let prepared = prepare_markdown(&format!("![Test]({target})"), IMAGES);
            assert!(!prepared.contains("!["), "{prepared}");
        }
    }

    #[test]
    fn relative_targets_resolve_only_registered_images_with_matching_extensions() {
        let prepared = prepare_markdown("![Dashboard](help-assets/dashboard.png)", IMAGES);
        let encoded = base64::engine::general_purpose::STANDARD.encode(IMAGES[0].bytes);
        assert_eq!(
            prepared,
            format!("![Dashboard](data:image/png;base64,{encoded})")
        );
        let prepared = prepare_markdown("![Apply](help-assets/apply.webp)", IMAGES);
        let encoded = base64::engine::general_purpose::STANDARD.encode(IMAGES[1].bytes);
        assert_eq!(
            prepared,
            format!("![Apply](data:image/webp;base64,{encoded})")
        );

        for target in [
            "help-assets/unknown.png",
            "help-assets/../dashboard.png",
            "../help-assets/dashboard.png",
            "help-assets/dashboard.webp",
            "help-assets/dashboard.png/extra",
        ] {
            let prepared = prepare_markdown(&format!("![Test]({target})"), IMAGES);
            assert!(!prepared.contains("data:image"), "{prepared}");
            assert!(!prepared.contains("!["), "{prepared}");
        }
    }
}
