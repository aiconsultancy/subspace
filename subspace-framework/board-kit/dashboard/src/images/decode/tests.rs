use super::decode;
use image::{ImageBuffer, ImageFormat, Rgba};
use std::io::Cursor;

fn png(width: u32, height: u32) -> Vec<u8> {
    let image = ImageBuffer::from_pixel(width, height, Rgba([255_u8, 20, 10, 255]));
    let mut bytes = Cursor::new(Vec::new());
    image.write_to(&mut bytes, ImageFormat::Png).unwrap();
    bytes.into_inner()
}

#[test]
fn decoding_converts_to_gpui_bgra_and_rejects_unsafe_images() {
    let image = decode(&png(2, 1), "image/png").unwrap();
    assert_eq!(
        image.as_bytes(0).unwrap(),
        &[10, 20, 255, 255, 10, 20, 255, 255]
    );
    assert_eq!(image.frame_count(), 1);
    assert!(decode(b"<svg></svg>", "image/svg+xml").is_err());
    assert!(decode(b"not an image", "image/png").is_err());
    assert!(decode(&vec![0; 8 * 1024 * 1024 + 1], "image/png").is_err());
    assert!(
        decode(&png(8193, 1), "image/png")
            .unwrap_err()
            .contains("exceeds")
    );
    assert!(decode(&png(3000, 3000), "image/png").is_err());
}

#[test]
fn service_decode_limits_allow_larger_rasters_without_changing_default_budgets() {
    use super::{DecodeLimits, decode_with_limits};
    let bytes = png(3000, 3000);
    assert!(decode(&bytes, "image/png").is_err());
    let limits = DecodeLimits {
        encoded_bytes: 10 * 1024 * 1024,
        dimension: 4096,
        pixels: 16_777_216,
    };
    assert_eq!(
        decode_with_limits(&bytes, "image/png", limits)
            .unwrap()
            .size(0)
            .width
            .0,
        3000
    );
    assert!(decode_with_limits(&png(4097, 1), "image/png", limits).is_err());
}
