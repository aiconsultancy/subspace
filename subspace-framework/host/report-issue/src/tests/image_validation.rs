use std::io::Cursor;

use super::multiple_images::{pick, pixel_file};
use super::support::{client, setup};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, ClipboardEntry, TestAppContext};
use image::{ExtendedColorType, ImageEncoder as _, ImageFormat, codecs::png::PngEncoder};
use subspace_dashboard::images::MAX_ENCODED_BYTES;

const PRIVATE_METADATA: &[u8] = b"test-only-source-metadata-never-retained";

fn png_with_metadata() -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut encoder = PngEncoder::new(&mut bytes);
    encoder
        .set_exif_metadata(PRIVATE_METADATA.to_vec())
        .unwrap();
    encoder
        .write_image(&[20, 80, 160], 1, 1, ExtendedColorType::Rgb8)
        .unwrap();
    assert!(
        bytes
            .windows(PRIVATE_METADATA.len())
            .any(|part| part == PRIVATE_METADATA)
    );
    bytes
}

fn rejected_files(directory: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut gif = Cursor::new(Vec::new());
    image::DynamicImage::new_rgb8(2, 3)
        .write_to(&mut gif, ImageFormat::Gif)
        .unwrap();
    let mut paths = Vec::new();
    for (name, bytes) in [
        ("unsupported.png", gif.into_inner()),
        ("corrupt.png", b"\x89PNG\r\n\x1a\ninvalid".to_vec()),
        ("too-large.png", vec![0; MAX_ENCODED_BYTES + 1]),
    ] {
        let path = directory.join(name);
        std::fs::write(&path, bytes).unwrap();
        paths.push(path);
    }
    for (name, width, height) in [
        ("too-wide.png", 8193, 1),
        ("too-many-pixels.png", 3000, 3000),
    ] {
        let path = directory.join(name);
        image::RgbImage::new(width, height).save(&path).unwrap();
        paths.push(path);
    }
    paths
}

#[gpui_kit::test]
fn mixed_selection_validates_each_file_and_strips_png_metadata(cx: &mut TestAppContext) {
    let (handle, report) = setup(cx, client("http://127.0.0.1:1", false));
    let directory = tempfile::tempdir().unwrap();
    let png = directory.path().join("test-only-private-source-name.png");
    std::fs::write(&png, png_with_metadata()).unwrap();
    let mut paths = vec![png];
    paths.extend(rejected_files(directory.path()));
    paths.push(pixel_file(
        directory.path(),
        "last-valid.jpg",
        [160, 80, 20],
    ));
    pick(cx, handle, paths);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find("report-image-count").label(),
            Some("2 of 5 images")
        );
        let error = report.read(cx).state.error.as_deref().unwrap();
        assert!(error.contains("Cannot decode this image"));
        assert!(!error.contains(&directory.path().to_string_lossy().to_string()));
        for _ in 0..2 {
            window.click("report-copy-image", cx);
            let clipboard = cx.read_from_clipboard().unwrap();
            let [ClipboardEntry::Image(image)] = clipboard.entries() else {
                panic!("validated image")
            };
            assert_eq!(image.format, gpui_kit::ImageFormat::Png);
            assert!(
                !image
                    .bytes
                    .windows(PRIVATE_METADATA.len())
                    .any(|part| part == PRIVATE_METADATA)
            );
        }
    })
    .unwrap();
}

#[gpui_kit::test]
fn adding_and_removing_zero_through_five_keeps_the_other_ids(cx: &mut TestAppContext) {
    let (handle, report) = setup(cx, client("http://127.0.0.1:1", false));
    let directory = tempfile::tempdir().unwrap();
    let path = pixel_file(directory.path(), "synthetic.png", [20, 80, 160]);
    cx.update_window(handle, |_, window, _| {
        assert_eq!(
            window.find("report-image-count").label(),
            Some("0 of 5 images")
        );
        assert!(window.try_find("report-copy-image").is_none());
    })
    .unwrap();
    for count in 1..=5 {
        pick(cx, handle, vec![path.clone()]);
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            assert_eq!(
                window.find("report-image-count").label(),
                Some(format!("{count} of 5 images").as_str())
            );
            assert_eq!(report.read(cx).state.attachments.len(), count);
        })
        .unwrap();
    }
    for id in (0usize..5).rev() {
        cx.update_window(handle, |_, window, cx| {
            window.click(("report-remove-image", id), cx);
            assert_eq!(report.read(cx).state.attachments.len(), id);
            assert_eq!(
                window.find("report-image-count").label(),
                Some(format!("{id} of 5 images").as_str())
            );
        })
        .unwrap();
    }
    pick(cx, handle, vec![path]);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find(("report-remove-image", 5usize)).visible());
        assert!(window.try_find(("report-remove-image", 0usize)).is_none());
    })
    .unwrap();
}
