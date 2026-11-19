use super::{BoardDoc, help_tool};
use crate::help_image::{HelpImage, help_image_reply};
use crate::test_support;
use serde_json::json;

const PNG: &[u8] = b"\x89PNG\r\n\x1a\nsynthetic-test-image";
const IMAGES: &[HelpImage] = &[HelpImage::new(
    "dashboard",
    "Synthetic dashboard",
    "image/png",
    PNG,
)];

#[tokio::test]
async fn guides_list_image_metadata_and_only_the_selected_image_returns_binary() {
    let tool = help_tool(vec![BoardDoc {
        id: "lab",
        title: "Lab",
        markdown: "# Lab\n\n![Dashboard](subspace-help-image:dashboard)",
        images: IMAGES,
    }])
    .unwrap();
    let (_, invocation) = test_support::invocation(json!({"action":"get", "board_id":"lab"}), true);
    let guide = tool.call(invocation).await.unwrap();
    assert_eq!(guide["images"][0]["image_id"], "dashboard");
    assert_eq!(guide["images"][0]["media_type"], "image/png");
    assert!(!guide.to_string().contains("base64"));
    assert!(guide["images"][0].get("data").is_none());
    let (_, invocation) = test_support::invocation(
        json!({"action":"image", "board_id":"lab", "image_id":"dashboard"}),
        true,
    );
    let image = tool.call(invocation).await.unwrap();
    assert!(help_image_reply("board_help", &image).is_some());
    for (board_id, image_id) in [
        ("disabled", "dashboard"),
        ("lab", "../dashboard"),
        ("lab", "Dashboard"),
    ] {
        let (_, invocation) = test_support::invocation(
            json!({"action":"image", "board_id":board_id, "image_id":image_id}),
            true,
        );
        assert!(tool.call(invocation).await.is_err());
    }
}

#[test]
fn static_image_catalogs_reject_invalid_ids_mime_size_and_duplicates() {
    let oversized = Box::leak([PNG, &[0; 512 * 1024]].concat().into_boxed_slice());
    for image in [
        HelpImage::new("../image", "Test", "image/png", PNG),
        HelpImage::new("image", "Test", "image/svg+xml", PNG),
        HelpImage::new("image", "Test", "image/webp", PNG),
        HelpImage::new("image", "Test", "image/png", &[]),
        HelpImage::new("image", "Test", "image/png", oversized),
    ] {
        assert!(crate::help_image::validate_images(&[image]).is_err());
    }
    let four_images = ["one", "two", "three", "four"]
        .map(|id| HelpImage::new(id, "Synthetic image", "image/png", PNG));
    assert!(crate::help_image::validate_images(&four_images).is_err());
    assert!(crate::help_image::validate_images(&four_images[..3]).is_ok());
    assert!(crate::help_image::validate_images(&[IMAGES[0]; 2]).is_err());
}
