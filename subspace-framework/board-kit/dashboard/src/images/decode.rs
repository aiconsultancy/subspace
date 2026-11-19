use gpui_kit::RenderImage;
use image::{DynamicImage, Frame, ImageDecoder as _, ImageFormat, ImageReader, Limits};
use std::{io::Cursor, sync::Arc};

pub const MAX_ENCODED_BYTES: usize = 8 * 1024 * 1024;
const MAX_PIXELS: u64 = 8 * 1024 * 1024;
const MAX_DIMENSION: u32 = 8192;
const DECODE_WORKING_BYTES_PER_PIXEL: u64 = 5;
const MAX_DECODE_ALLOC: u64 = 64 * 1024 * 1024;

/// Decode only a bounded first frame. GPUI consumes BGRA pixels, not RGBA.
pub fn decode(bytes: &[u8], mime: &str) -> Result<Arc<RenderImage>, String> {
    decode_with_limits(bytes, mime, DecodeLimits::default())
}

/// Caller-owned service limits; the default preserves existing chat budgets.
#[derive(Clone, Copy)]
pub struct DecodeLimits {
    pub encoded_bytes: usize,
    pub dimension: u32,
    pub pixels: u64,
}
impl Default for DecodeLimits {
    fn default() -> Self {
        Self {
            encoded_bytes: MAX_ENCODED_BYTES,
            dimension: MAX_DIMENSION,
            pixels: MAX_PIXELS,
        }
    }
}
/// Decode a bounded first frame with an explicit service budget.
pub fn decode_with_limits(
    bytes: &[u8],
    mime: &str,
    limits: DecodeLimits,
) -> Result<Arc<RenderImage>, String> {
    Ok(render_image(raster(bytes, mime, limits)?))
}

fn raster(bytes: &[u8], mime: &str, budget: DecodeLimits) -> Result<DynamicImage, String> {
    if bytes.len() > budget.encoded_bytes {
        return Err("Image exceeds encoded byte limit".into());
    }
    let format = match mime {
        "image/png" => ImageFormat::Png,
        "image/jpeg" => ImageFormat::Jpeg,
        "image/gif" => ImageFormat::Gif,
        "image/webp" => ImageFormat::WebP,
        _ => return Err("Unsupported image type".into()),
    };
    let mut limits = Limits::default();
    limits.max_image_width = Some(budget.dimension);
    limits.max_image_height = Some(budget.dimension);
    limits.max_alloc =
        Some(MAX_DECODE_ALLOC.max(budget.pixels.saturating_mul(DECODE_WORKING_BYTES_PER_PIXEL)));
    let (width, height) = ImageReader::with_format(Cursor::new(bytes), format)
        .into_dimensions()
        .map_err(|_| "Cannot decode image")?;
    if width > budget.dimension || height > budget.dimension {
        return Err("Image exceeds dimension limit".into());
    }
    if u64::from(width) * u64::from(height) > budget.pixels {
        return Err("Image exceeds pixel limit".into());
    }
    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
    reader.limits(limits);
    let decoder = reader.into_decoder().map_err(|_| "Cannot decode image")?;
    let (width, height) = decoder.dimensions();
    if u64::from(width) * u64::from(height) > budget.pixels {
        return Err("Image exceeds pixel limit".into());
    }
    DynamicImage::from_decoder(decoder).map_err(|_| "Cannot decode image".into())
}

fn render_image(image: DynamicImage) -> Arc<RenderImage> {
    let mut rgba = image.into_rgba8();
    for pixel in rgba.pixels_mut() {
        pixel.0.swap(0, 2);
    }
    Arc::new(RenderImage::new(vec![Frame::new(rgba)]))
}

#[cfg(test)]
mod tests;
