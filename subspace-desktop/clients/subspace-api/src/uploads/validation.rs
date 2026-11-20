use super::{MAX_FILE_BYTES, MAX_IMAGE_BYTES, MAX_IMAGE_EDGE, MAX_IMAGE_PIXELS, UploadError};
use std::{io::Cursor, path::Path, sync::LazyLock};
const MAX_NAME_UNITS: usize = 200;
const MAX_DECODE_ALLOC: u64 = 80 * 1024 * 1024;
static FORMAT_CHARACTERS: LazyLock<Option<regex::Regex>> =
    LazyLock::new(|| regex::Regex::new(r"\p{Cf}").ok());

pub(super) fn normalize_name(name: &str) -> Result<String, UploadError> {
    without_format_characters(name)
        .map(|name| name.trim().to_owned())
        .ok_or(UploadError::InvalidImage)
}

/// `name` without Unicode format characters, the bidirectional controls
/// among them, which can make a name read as something it is not. `None`
/// when they cannot be told apart.
pub fn without_format_characters(name: &str) -> Option<String> {
    FORMAT_CHARACTERS
        .as_ref()
        .map(|format| format.replace_all(name, "").into_owned())
}

pub fn file_mime(path: &Path) -> Option<&'static str> {
    match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        "pdf" => Some("application/pdf"),
        "txt" => Some("text/plain"),
        "md" => Some("text/markdown"),
        "docx" => Some("application/vnd.openxmlformats-officedocument.wordprocessingml.document"),
        "xlsx" => Some("application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"),
        "pptx" => Some("application/vnd.openxmlformats-officedocument.presentationml.presentation"),
        _ => None,
    }
}

/// The largest file of type `mime` an upload accepts: images up to
/// [`MAX_IMAGE_BYTES`], other files up to [`MAX_FILE_BYTES`].
pub fn max_bytes(mime: &str) -> usize {
    if crate::media::supported_image(mime) {
        MAX_IMAGE_BYTES
    } else {
        MAX_FILE_BYTES
    }
}

pub(super) fn validate(name: &str, mime: &str, bytes: &[u8]) -> Result<(), UploadError> {
    let image = crate::media::supported_image(mime);
    let limit = max_bytes(mime);
    if name.is_empty()
        || name.encode_utf16().count() > MAX_NAME_UNITS
        || name.chars().any(char::is_control)
        || name.contains(['/', '\\', '"'])
        || file_mime(Path::new(name)) != Some(mime)
        || bytes.is_empty()
        || bytes.len() > limit
    {
        return Err(UploadError::InvalidImage);
    }
    if image {
        raster(bytes, mime)
    } else {
        document(bytes, mime)
    }
}

fn raster(bytes: &[u8], mime: &str) -> Result<(), UploadError> {
    let format = match mime {
        "image/png" => image::ImageFormat::Png,
        "image/jpeg" => image::ImageFormat::Jpeg,
        "image/gif" => image::ImageFormat::Gif,
        "image/webp" => image::ImageFormat::WebP,
        _ => return Err(UploadError::InvalidImage),
    };
    if image::guess_format(bytes).ok() != Some(format) {
        return Err(UploadError::InvalidImage);
    }
    let (width, height) = image::ImageReader::with_format(Cursor::new(bytes), format)
        .into_dimensions()
        .map_err(|_| UploadError::InvalidImage)?;
    if width == 0
        || height == 0
        || width > MAX_IMAGE_EDGE
        || height > MAX_IMAGE_EDGE
        || u64::from(width) * u64::from(height) > MAX_IMAGE_PIXELS
    {
        return Err(UploadError::InvalidImage);
    }
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_IMAGE_EDGE);
    limits.max_image_height = Some(MAX_IMAGE_EDGE);
    limits.max_alloc = Some(MAX_DECODE_ALLOC);
    let mut reader = image::ImageReader::with_format(Cursor::new(bytes), format);
    reader.limits(limits);
    reader
        .decode()
        .map(|_| ())
        .map_err(|_| UploadError::InvalidImage)
}

pub(crate) fn document(bytes: &[u8], mime: &str) -> Result<(), UploadError> {
    let valid = match mime {
        "application/pdf" => bytes.starts_with(b"%PDF-"),
        "text/plain" | "text/markdown" => !bytes.contains(&0) && std::str::from_utf8(bytes).is_ok(),
        _ => bytes.starts_with(b"PK\x03\x04"),
    };
    valid.then_some(()).ok_or(UploadError::InvalidImage)
}
