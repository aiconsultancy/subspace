//! An optional image preview, assembled on demand without the GPUI asset
//! cache. A raw, paged read remains available for every image.

use std::io::Cursor;
use std::sync::Arc;

use gpui_kit::{
    AnyElement, App, AppContext as _, Context, IntoElement as _, ParentElement as _, RenderImage,
    Styled as _, Window, div,
};
use image::{Frame, ImageDecoder as _, ImageFormat, ImageReader, Limits};
use subspace_client_fleet::{Error, diff::DiffContent};
use subspace_framework::client_github::model::image_type;
use subspace_framework::dashboard::action::{Intent, action_button};
use subspace_framework::dashboard::panel::IconName;
use subspace_framework::dashboard::{Toast, Tokens};

use super::{DiffView, Position, Retry, read::Blob};
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
    #[cfg(test)]
    pub(super) fn image(&self) -> Arc<RenderImage> {
        self.image.clone()
    }

    pub(super) fn evict(self, window: Option<&mut Window>, cx: &mut App) {
        let image = self.image;
        if let Some(window) = window {
            cx.drop_image(image, Some(window));
        } else {
            // An app-only callback may be running inside a window update.
            cx.defer(move |cx| cx.drop_image(image, None));
        }
    }

    pub(super) fn body(&self, tokens: &Tokens) -> AnyElement {
        div()
            .min_w_0()
            .px_3()
            .child(ready_image(self.image.clone(), self.size, tokens))
            .into_any_element()
    }
}

pub(super) enum PreviewError {
    Fleet(Error),
    NotRenderable,
}

impl DiffView {
    pub(super) fn preview_binary(
        &mut self,
        format: ImageFormat,
        mut progress: PreviewRead,
        cx: &mut Context<Self>,
    ) {
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
        let guard = closed.clone();
        let task = cx.spawn(async move |view, cx| {
            let (result, progress) = cx
                .background_spawn(async move {
                    let blob = Blob {
                        path: &path,
                        at: &at,
                        side,
                    };
                    let result = progress
                        .read(|position| {
                            blob.read(&client, source.request(mode), position, &closed)
                                .map_err(PreviewError::Fleet)
                        })
                        .and_then(|()| {
                            closed.check().map_err(PreviewError::Fleet)?;
                            decode(format, &progress.bytes).ok_or(PreviewError::NotRenderable)
                        });
                    (result, progress)
                })
                .await;
            _ = view.update(cx, |view, cx| {
                if guard.check().is_ok() {
                    view.finish_preview(result, format, progress, cx);
                }
            });
        });
        if let Some(binary) = self.binary.as_mut() {
            binary.retry = None;
            binary.read = Some(task);
        }
        cx.notify();
    }

    fn finish_preview(
        &mut self,
        result: Result<Preview, PreviewError>,
        format: ImageFormat,
        progress: PreviewRead,
        cx: &mut Context<Self>,
    ) {
        let Some(binary) = self.binary.as_mut() else {
            return;
        };
        binary.read = None;
        self.fleet.unstable |= progress.unstable;
        match result {
            Ok(preview) => {
                binary.replace_preview(Some(preview), None, cx);
            }
            Err(PreviewError::Fleet(Error::Busy(wait))) => {
                binary.retry = Some((Retry::Preview(format, progress), wait));
            }
            Err(PreviewError::Fleet(error)) => self.fleet_page_error(error, cx),
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
    .on_click(
        cx.listener(move |view, _, _, cx| view.preview_binary(format, PreviewRead::default(), cx)),
    )
    .into_any_element()
}

pub(super) fn bytes_button(cx: &Context<DiffView>) -> AnyElement {
    action_button(
        "crew-binary-return",
        Intent::Quiet,
        Some(IconName::FileCode),
        "Show bytes",
    )
    .on_click(cx.listener(|view, _, window, cx| {
        if let Some(binary) = view.binary.as_mut() {
            binary.replace_preview(None, Some(window), cx);
        }
        cx.notify();
    }))
    .into_any_element()
}

/// A busy continuation retains its bounded prefix and exact pending cursor.
#[derive(Default)]
pub(super) struct PreviewRead {
    bytes: Vec<u8>,
    position: Position,
    pages: usize,
    unstable: bool,
}

impl PreviewRead {
    pub(super) fn read(
        &mut self,
        mut read: impl FnMut(&Position) -> Result<(DiffContent, bool), PreviewError>,
    ) -> Result<(), PreviewError> {
        while self.pages < MAX_PREVIEW_PAGES {
            let (page, changed) = read(&self.position)?;
            self.unstable |= changed;
            self.pages += 1;
            if page.size > MAX_PREVIEW_BYTES as u64
                || self.bytes.len() + page.bytes.len() > MAX_PREVIEW_BYTES
            {
                return Err(PreviewError::NotRenderable);
            }
            let empty = page.bytes.is_empty();
            self.bytes.extend(page.bytes);
            let Some(cursor) = page.next_page.filter(|_| !empty) else {
                return Ok(());
            };
            self.position = Position {
                cursor: Some(cursor),
                offset: self.bytes.len() as u64,
            };
        }
        Err(PreviewError::NotRenderable)
    }
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

    use super::{DiffContent, PreviewRead, decode};

    #[test]
    fn renders_the_pixels_of_an_image_assembled_from_multiple_pages() {
        let mut progress = PreviewRead::default();
        progress.read(
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
        )
        .ok()
        .unwrap();
        assert_eq!(progress.bytes.len(), 70);
        assert!(progress.unstable);
        let preview = decode(ImageFormat::Png, &progress.bytes).unwrap();
        assert_eq!(preview.image.as_bytes(0), Some(&[3, 2, 1, 255][..]));
    }
}
