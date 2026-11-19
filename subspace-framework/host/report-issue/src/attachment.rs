//! User-selected raster images stay in memory and never enter an issue body.

use std::fs::{File, OpenOptions};
use std::io::{Read as _, Write as _};
use std::path::Path;
use std::sync::Arc;

use gpui_kit::{ClipboardItem, Image, ImageFormat, RenderImage};
use image::{ExtendedColorType, ImageEncoder as _, codecs::png::PngEncoder};
use subspace_dashboard::images::{MAX_ENCODED_BYTES, decode};

const IMAGE_REQUIRED: &str = "Choose a PNG or JPEG image.";
const IMAGE_UNREADABLE: &str = "Cannot read this image. Choose another PNG or JPEG file.";
const IMAGE_TOO_LARGE: &str = "This image is too large. Choose an image smaller than 8 MiB.";
const MAX_READ_BYTES: u64 = MAX_ENCODED_BYTES as u64 + 1;
const JPEG_SIGNATURE: &[u8] = b"\xff\xd8\xff";
const CHANNELS_PER_PIXEL: usize = 4;

/// One explicitly selected, validated PNG or JPEG. No source path is retained.
#[derive(Clone)]
pub struct Attachment {
    image: Image,
    preview: Arc<RenderImage>,
}

impl Attachment {
    /// Read a selected file off the UI thread under the shared raster limits.
    pub fn read(path: &Path) -> Result<Self, String> {
        let file = File::open(path).map_err(|_| IMAGE_UNREADABLE)?;
        let mut bytes = Vec::new();
        file.take(MAX_READ_BYTES)
            .read_to_end(&mut bytes)
            .map_err(|_| IMAGE_UNREADABLE)?;
        Self::from_bytes(bytes)
    }

    /// Validate file content, rather than trusting its extension or filename.
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, String> {
        check_encoded_size(&bytes)?;
        let mime = image_type(&bytes).ok_or(IMAGE_REQUIRED)?;
        let preview = decode(&bytes, mime)
            .map_err(|_| "Cannot decode this image. Choose a PNG or JPEG within 8 megapixels.")?;
        let bytes = pixels_only(&preview)?;
        Ok(Self {
            image: Image::from_bytes(ImageFormat::Png, bytes),
            preview,
        })
    }

    pub fn preview(&self) -> Arc<RenderImage> {
        self.preview.clone()
    }

    /// Called only after the user commits the reviewed report or chooses Copy.
    pub fn clipboard_item(&self) -> ClipboardItem {
        ClipboardItem::new_image(&self.image)
    }

    /// Save only to a user-selected new file. Existing files are never replaced.
    pub fn save_new(&self, path: &Path) -> Result<(), String> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|_| "Cannot save this image. Choose a new file in a writable folder.")?;
        file.write_all(&self.image.bytes)
            .and_then(|()| file.sync_all())
            .map_err(|_| {
                "Cannot finish saving the image. Choose another file and try again.".into()
            })
    }
}

fn image_type(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if bytes.starts_with(JPEG_SIGNATURE) {
        Some("image/jpeg")
    } else {
        None
    }
}

fn check_encoded_size(bytes: &[u8]) -> Result<(), String> {
    if bytes.len() > MAX_ENCODED_BYTES {
        Err(IMAGE_TOO_LARGE.into())
    } else {
        Ok(())
    }
}

/// Re-encode pixels so clipboard and saved images carry no filename or EXIF data.
fn pixels_only(preview: &RenderImage) -> Result<Vec<u8>, String> {
    let mut rgba = preview.as_bytes(0).ok_or(IMAGE_UNREADABLE)?.to_vec();
    for pixel in rgba.as_chunks_mut::<CHANNELS_PER_PIXEL>().0 {
        pixel.swap(0, 2);
    }
    let size = preview.size(0);
    let width = u32::try_from(size.width.0).map_err(|_| IMAGE_UNREADABLE)?;
    let height = u32::try_from(size.height.0).map_err(|_| IMAGE_UNREADABLE)?;
    let mut encoded = Vec::new();
    PngEncoder::new(&mut encoded)
        .write_image(&rgba, width, height, ExtendedColorType::Rgba8)
        .map_err(|_| IMAGE_UNREADABLE)?;
    if encoded.len() > MAX_ENCODED_BYTES {
        return Err(IMAGE_TOO_LARGE.into());
    }
    Ok(encoded)
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use gpui_kit::ClipboardEntry;
    use image::{DynamicImage, ImageFormat, Rgb, RgbImage};

    use super::{Attachment, IMAGE_TOO_LARGE, MAX_ENCODED_BYTES};

    fn fixture(format: ImageFormat) -> Vec<u8> {
        let mut bytes = Cursor::new(Vec::new());
        DynamicImage::ImageRgb8(RgbImage::from_pixel(2, 3, Rgb([20, 80, 160])))
            .write_to(&mut bytes, format)
            .unwrap();
        bytes.into_inner()
    }

    #[test]
    fn accepts_only_decodable_png_and_jpeg_content() {
        for format in [ImageFormat::Png, ImageFormat::Jpeg] {
            let attachment = Attachment::from_bytes(fixture(format)).unwrap();
            let size = attachment.preview().size(0);
            assert_eq!((size.width.0, size.height.0), (2, 3));
            let clipboard = attachment.clipboard_item();
            assert!(
                matches!(clipboard.entries(), [ClipboardEntry::Image(image)] if image.format == gpui_kit::ImageFormat::Png)
            );
            let pasted = image::load_from_memory(&attachment.image.bytes)
                .unwrap()
                .to_rgb8();
            let pixel = pasted.get_pixel(0, 0).0;
            for (actual, expected) in pixel.into_iter().zip([20u8, 80, 160]) {
                assert!(
                    actual.abs_diff(expected) <= 2,
                    "PNG and JPEG preserve pixel colour"
                );
            }
        }
        assert!(Attachment::from_bytes(fixture(ImageFormat::Gif)).is_err());
        assert!(Attachment::from_bytes(b"\x89PNG\r\n\x1a\ninvalid".to_vec()).is_err());
    }

    #[test]
    fn rejects_oversize_images_and_hides_file_errors() {
        let error = Attachment::from_bytes(vec![0; MAX_ENCODED_BYTES + 1])
            .err()
            .unwrap();
        assert_eq!(error, IMAGE_TOO_LARGE);
        let temp = tempfile::tempdir().unwrap();
        let sensitive = temp.path().join("test-only-private-hostname.png");
        let error = Attachment::read(&sensitive).err().unwrap();
        assert!(!error.contains("test-only-private-hostname"));
        assert!(!error.contains(&temp.path().to_string_lossy().to_string()));
    }

    #[test]
    fn selected_file_is_read_without_retaining_its_name() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("test-only-user-selected.png");
        std::fs::write(&file, fixture(ImageFormat::Png)).unwrap();
        let attachment = Attachment::read(&file).unwrap();
        std::fs::remove_file(file).unwrap();
        assert_eq!(attachment.preview().size(0).width.0, 2);
    }

    #[test]
    fn clipboard_and_saved_pixels_exclude_source_metadata() {
        let private = b"test-only-private-location";
        let jpeg = fixture(ImageFormat::Jpeg);
        let length = u16::try_from(private.len() + 2).unwrap().to_be_bytes();
        let mut bytes = vec![0xff, 0xd8, 0xff, 0xfe, length[0], length[1]];
        bytes.extend_from_slice(private);
        bytes.extend_from_slice(&jpeg[2..]);
        assert!(bytes.windows(private.len()).any(|window| window == private));
        let attachment = Attachment::from_bytes(bytes).unwrap();
        assert_eq!(attachment.image.format, gpui_kit::ImageFormat::Png);
        assert!(
            !attachment
                .image
                .bytes
                .windows(private.len())
                .any(|window| window == private)
        );
        assert_eq!(attachment.preview().size(0).height.0, 3);
    }

    #[test]
    fn explicit_save_never_overwrites_an_existing_file() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("chosen-image.png");
        let bytes = fixture(ImageFormat::Png);
        let attachment = Attachment::from_bytes(bytes.clone()).unwrap();
        attachment.save_new(&file).unwrap();
        let saved = std::fs::read(&file).unwrap();
        assert_eq!(saved, attachment.image.bytes);
        assert!(attachment.save_new(&file).is_err());
        assert_eq!(std::fs::read(&file).unwrap(), saved);
    }
}
