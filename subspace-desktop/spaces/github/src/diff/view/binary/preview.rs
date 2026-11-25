//! An optional image preview, assembled on demand without the GPUI asset
//! cache. A raw, paged read remains available for every image.

use std::io::Cursor;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use gpui_kit::{
    AnyElement, AppContext as _, Context, IntoElement as _, ParentElement as _, RenderImage,
    Styled as _, div,
};
use image::{Frame, ImageDecoder as _, ImageFormat, ImageReader, Limits};
use subspace_client_fleet::{Error, diff::DiffContent};
use subspace_framework::client_github::model::image_type;
use subspace_framework::dashboard::action::{Intent, action_button};
use subspace_framework::dashboard::panel::IconName;
use subspace_framework::dashboard::{Toast, Tokens};

use super::{DiffView, Position, read::Blob};
use crate::diff::view::{DiffEvent, DiffSource, image::ready_image};

const MAX_PREVIEW_BYTES: usize = 10 * 1024 * 1024;
const MAX_PREVIEW_PAGES: usize = 16;
const MAX_DIMENSION: u32 = 4096;
const MAX_DECODE_BYTES: u64 = 128 * 1024 * 1024;
const CHANNELS_PER_PIXEL: usize = 4;

pub(super) struct Preview {
    image: Arc<RenderImage>,
    size: usize,
}

impl Preview {
    pub(super) fn body(&self, tokens: &Tokens) -> AnyElement {
        div()
            .min_w_0()
            .px_3()
            .child(ready_image(self.image.clone(), self.size, tokens))
            .into_any_element()
    }
}

enum PreviewError {
    Fleet(Error),
    NotRenderable,
    Closed,
}

impl DiffView {
    fn preview_binary(&mut self, format: ImageFormat, cx: &mut Context<Self>) {
        let (DiffSource::Fleet(source), Some(at), Some(binary)) = (
            self.source.clone(),
            self.fleet.fingerprint.clone(),
            self.binary.as_ref(),
        ) else {
            return;
        };
        if self.fleet.changed() || binary.read.is_some() {
            return;
        }
        let client = match source.client(cx) {
            Ok(client) => client,
            Err(help) => {
                cx.emit(DiffEvent::Toast(Toast::error(help)));
                return;
            }
        };
        let path = self.files[binary.file].file.path.clone();
        let (mode, side) = (self.fleet.mode, binary.side);
        let closed = binary.closed.clone();
        let task = cx.spawn(async move |view, cx| {
            let (result, unstable) = cx
                .background_spawn(async move {
                    let blob = Blob {
                        path: &path,
                        at: &at,
                        side,
                    };
                    let mut unstable = false;
                    let result = read_preview(
                        |position| {
                            if closed.load(Ordering::Acquire) {
                                return Err(PreviewError::Closed);
                            }
                            blob.read(&client, source.request(mode), position)
                                .map_err(PreviewError::Fleet)
                        },
                        &mut unstable,
                    )
                    .and_then(|bytes| {
                        if closed.load(Ordering::Acquire) {
                            return Err(PreviewError::Closed);
                        }
                        decode(format, &bytes).ok_or(PreviewError::NotRenderable)
                    });
                    (result, unstable)
                })
                .await;
            _ = view.update(cx, |view, cx| view.finish_preview(result, unstable, cx));
        });
        if let Some(binary) = self.binary.as_mut() {
            binary.read = Some(task);
        }
        cx.notify();
    }

    fn finish_preview(
        &mut self,
        result: Result<Preview, PreviewError>,
        unstable: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(binary) = self.binary.as_mut() else {
            return;
        };
        binary.read = None;
        self.fleet.unstable |= unstable;
        match result {
            Ok(preview) => {
                binary.preview = Some(preview);
                self.fleet.busy = None;
            }
            Err(PreviewError::Fleet(error)) => self.fleet_page_error(error, cx),
            Err(PreviewError::Closed) => {}
            Err(PreviewError::NotRenderable) => cx.emit(DiffEvent::Toast(Toast::error(
                "Cannot preview this image. You can still read its bytes.",
            ))),
        }
        cx.notify();
    }
}

pub(super) fn format(path: &str) -> Option<ImageFormat> {
    match image_type(path)? {
        "image/png" => Some(ImageFormat::Png),
        "image/jpeg" => Some(ImageFormat::Jpeg),
        "image/gif" => Some(ImageFormat::Gif),
        "image/webp" => Some(ImageFormat::WebP),
        _ => None,
    }
}

pub(super) fn open_button(format: ImageFormat, cx: &Context<DiffView>) -> AnyElement {
    action_button(
        "crew-binary-preview",
        Intent::Quiet,
        Some(IconName::Image),
        "Preview image",
    )
    .on_click(cx.listener(move |view, _, _, cx| view.preview_binary(format, cx)))
    .into_any_element()
}

pub(super) fn bytes_button(cx: &Context<DiffView>) -> AnyElement {
    action_button(
        "crew-binary-return",
        Intent::Quiet,
        Some(IconName::FileCode),
        "Show bytes",
    )
    .on_click(cx.listener(|view, _, _, cx| {
        if let Some(binary) = view.binary.as_mut() {
            binary.preview = None;
        }
        cx.notify();
    }))
    .into_any_element()
}

fn read_preview(
    mut read: impl FnMut(&Position) -> Result<(DiffContent, bool), PreviewError>,
    unstable: &mut bool,
) -> Result<Vec<u8>, PreviewError> {
    let mut bytes = Vec::new();
    let mut position = Position::default();
    for _ in 0..MAX_PREVIEW_PAGES {
        let (page, changed) = read(&position)?;
        *unstable |= changed;
        if page.size > MAX_PREVIEW_BYTES as u64
            || bytes.len() + page.bytes.len() > MAX_PREVIEW_BYTES
        {
            return Err(PreviewError::NotRenderable);
        }
        bytes.extend(page.bytes);
        let Some(cursor) = page.next_page else {
            return Ok(bytes);
        };
        position = Position {
            cursor: Some(cursor),
            offset: bytes.len() as u64,
        };
    }
    Err(PreviewError::NotRenderable)
}

fn decode(format: ImageFormat, bytes: &[u8]) -> Option<Preview> {
    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_DIMENSION);
    limits.max_image_height = Some(MAX_DIMENSION);
    limits.max_alloc = Some(MAX_DECODE_BYTES);
    reader.limits(limits);
    let mut decoder = reader.into_decoder().ok()?;
    let orientation = decoder.orientation().ok()?;
    let mut image = image::DynamicImage::from_decoder(decoder).ok()?;
    image.apply_orientation(orientation);
    let mut pixels = image.into_rgba8();
    for pixel in pixels.as_chunks_mut::<CHANNELS_PER_PIXEL>().0 {
        pixel.swap(0, 2);
    } // GPUI expects BGRA.
    Some(Preview {
        image: Arc::new(RenderImage::new([Frame::new(pixels)])),
        size: bytes.len(),
    })
}

#[cfg(test)]
mod tests {
    use image::ImageFormat;
    use serde_json::json;

    use super::{DiffContent, decode, read_preview};

    #[test]
    fn renders_the_pixels_of_an_image_assembled_from_multiple_pages() {
        let mut unstable = false;
        let bytes = read_preview(
            |position| {
                let (data, next_page) = if position.offset == 0 {
                    ("iVBO", Some("next+1"))
                } else {
                    assert_eq!(position.cursor.as_deref(), Some("next+1"));
                    assert_eq!(position.offset, 3);
                    ("Rw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGNgZGL+DwABFAEG1rmmRQAAAABJRU5ErkJggg==", None)
                };
                let page: DiffContent = serde_json::from_value(json!({
                    "side":"new", "encoding":"base64", "offset":position.offset,
                    "size":70, "data":data, "nextPage":next_page
                }))
                .unwrap();
                Ok((page, next_page.is_none()))
            },
            &mut unstable,
        )
        .ok()
        .unwrap();
        assert_eq!(bytes.len(), 70);
        assert!(unstable);
        let preview = decode(ImageFormat::Png, &bytes).unwrap();
        assert_eq!(preview.image.as_bytes(0), Some(&[3, 2, 1, 255][..]));
    }
}
