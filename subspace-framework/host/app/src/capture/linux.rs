//! Read only the native capture window on the script's private X server.
//! GPUI 0.3.12's Linux window has no `render_to_image` implementation.

use std::path::Path;
use std::time::Duration;

use gpui_kit::{AsyncApp, Window};
use image::RgbaImage;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{ConnectionExt, ImageFormat, ImageOrder};

use super::pixels::{PixelFormat, rgba};

type CaptureResult<T> = Result<T, Box<dyn std::error::Error>>;

pub(super) struct Reader {
    connection: x11rb::rust_connection::RustConnection,
}

impl Reader {
    pub(super) fn new() -> Result<Self, String> {
        x11rb::connect(None)
            .map(|(connection, _)| Self { connection })
            .map_err(|error| error.to_string())
    }

    pub(super) fn prepare(&self, window: &Window) -> Result<(), String> {
        window_id(window)
            .and_then(|id| self.raise_window(id))
            .map_err(|error| error.to_string())
    }

    pub(super) fn capture(&self, window: &Window) -> Result<RgbaImage, String> {
        window_id(window)
            .and_then(|id| self.read_window(id))
            .map_err(|error| error.to_string())
    }

    fn raise_window(&self, id: u32) -> CaptureResult<()> {
        use x11rb::protocol::xproto::{ConfigureWindowAux, StackMode};

        self.connection
            .configure_window(id, &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE))?
            .check()?;
        Ok(())
    }

    fn read_window(&self, id: u32) -> CaptureResult<RgbaImage> {
        let geometry = self.connection.get_geometry(id)?.reply()?;
        let attributes = self.connection.get_window_attributes(id)?.reply()?;
        let visual = self
            .connection
            .setup()
            .roots
            .iter()
            .flat_map(|screen| &screen.allowed_depths)
            .flat_map(|depth| &depth.visuals)
            .find(|visual| visual.visual_id == attributes.visual)
            .ok_or("capture window visual is unavailable")?;
        let format = self
            .connection
            .setup()
            .pixmap_formats
            .iter()
            .find(|format| format.depth == geometry.depth)
            .ok_or("capture window pixel format is unavailable")?;
        let reply = self
            .connection
            .get_image(
                ImageFormat::Z_PIXMAP,
                id,
                0,
                0,
                geometry.width,
                geometry.height,
                u32::MAX,
            )?
            .reply()?;
        let format = PixelFormat {
            bits: format.bits_per_pixel,
            pad: format.scanline_pad,
            little_endian: self.connection.setup().image_byte_order == ImageOrder::LSB_FIRST,
            masks: [visual.red_mask, visual.green_mask, visual.blue_mask],
        };
        let width = u32::from(geometry.width);
        let height = u32::from(geometry.height);
        let data = rgba(&reply.data, width, height, format)?;
        RgbaImage::from_raw(width, height, data)
            .ok_or_else(|| "invalid capture image dimensions".into())
    }
}

fn window_id(window: &Window) -> CaptureResult<u32> {
    let handle = HasWindowHandle::window_handle(window).map_err(|error| error.to_string())?;
    match handle.as_raw() {
        RawWindowHandle::Xcb(handle) => Ok(handle.window.get()),
        RawWindowHandle::Xlib(handle) => Ok(u32::try_from(handle.window)?),
        _ => Err("Linux capture requires the private X11 display from make capture".into()),
    }
}

pub(super) async fn capture_shot(name: &str, dir: &Path, cx: &mut AsyncApp) -> Result<(), String> {
    let reader = Reader::new()?;
    for (index, handle) in cx.update(|cx| cx.windows()).into_iter().enumerate() {
        // Hidden X11 window pixels are undefined. Raise this owned window on
        // the private display, then let GPUI repaint before reading its image.
        cx.update(|cx| {
            handle
                .update(cx, |_, window, _| reader.prepare(window))
                .map_err(|error| error.to_string())?
        })?;
        present_capture_frames(handle, cx).await?;
        let image = cx.update(|cx| {
            handle
                .update(cx, |_, window, _| reader.capture(window))
                .map_err(|error| error.to_string())?
        })?;
        super::save_image(name, dir, index, &image)?;
    }
    Ok(())
}

async fn present_capture_frames(
    handle: gpui_kit::AnyWindowHandle,
    cx: &mut AsyncApp,
) -> Result<(), String> {
    // One frame presents the latest state; the next lets the X server consume
    // that presentation before its separate readback connection requests pixels.
    const PRESENT_FRAMES: usize = 2;
    const FRAME_TIMEOUT: Duration = Duration::from_secs(30);
    for _ in 0..PRESENT_FRAMES {
        let receive = cx.update(|cx| {
            handle
                .update(cx, |_, window, _| {
                    let (send, receive) = tokio::sync::oneshot::channel();
                    window.on_next_frame(move |window, _| {
                        window.refresh();
                        let _ = send.send(());
                    });
                    receive
                })
                .map_err(|error| error.to_string())
        })?;
        tokio::select! {
            result = receive => result.map_err(|_| "capture window closed before presenting")?,
            _ = cx.background_executor().timer(FRAME_TIMEOUT) => {
                return Err("Linux capture timed out waiting for a presented frame".into());
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::Reader;
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{ConnectionExt, CreateWindowAux, WindowClass};

    #[test]
    #[ignore = "requires a private 24-bit Xvfb display"]
    fn each_overlapping_window_reads_its_own_repainted_pixels() {
        let reader = Reader::new().unwrap();
        let screen = &reader.connection.setup().roots[0];
        let mut windows = Vec::new();
        for colour in [0xff0000, 0x0000ff] {
            let id = reader.connection.generate_id().unwrap();
            reader
                .connection
                .create_window(
                    screen.root_depth,
                    id,
                    screen.root,
                    10,
                    10,
                    100,
                    100,
                    0,
                    WindowClass::INPUT_OUTPUT,
                    screen.root_visual,
                    &CreateWindowAux::new().background_pixel(colour),
                )
                .unwrap()
                .check()
                .unwrap();
            reader.connection.map_window(id).unwrap().check().unwrap();
            windows.push(id);
        }
        for (id, colour) in windows.iter().zip([[255, 0, 0, 255], [0, 0, 255, 255]]) {
            reader.raise_window(*id).unwrap();
            // The app repaints after raising; reproduce that ordering here.
            reader
                .connection
                .clear_area(false, *id, 0, 0, 0, 0)
                .unwrap()
                .check()
                .unwrap();
            let image = reader.read_window(*id).unwrap();
            assert_eq!(image.get_pixel(50, 50).0, colour);
        }
    }
}
