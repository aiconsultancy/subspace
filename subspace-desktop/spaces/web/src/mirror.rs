//! Debug capture builds only. The capture harness renders GPUI's own scene,
//! which does not contain a native web view, and its clicks reach GPUI, not
//! the page. While `SUBSPACE_CAPTURE_DIR` is set, a web board therefore
//! keeps a snapshot of its page drawn over the view (where it is hidden
//! behind the native view on screen) and forwards GPUI clicks and pointer
//! moves on it to the page as DOM events. Release builds never do either.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui_kit::{
    AnyElement, App, AppContext as _, Bounds, Context, InteractiveElement as _, IntoElement,
    MouseButton, MouseDownEvent, MouseMoveEvent, ParentElement as _, Pixels, Point, RenderImage,
    Styled as _, canvas, div, img,
};

use crate::WebBoard;

const CAPTURE_ENV: &str = "SUBSPACE_CAPTURE_DIR";
/// How often the snapshot follows the page.
const REFRESH: Duration = Duration::from_millis(500);

#[derive(Default)]
pub(crate) struct Mirror {
    on: bool,
    image: Option<Arc<RenderImage>>,
    bounds: Rc<Cell<Bounds<Pixels>>>,
    requested: Option<Instant>,
    /// A snapshot is being decoded off the UI thread.
    decoding: bool,
}

impl Mirror {
    pub(crate) fn new() -> Self {
        Self {
            on: cfg!(all(target_os = "macos", debug_assertions))
                && std::env::var_os(CAPTURE_ENV).is_some(),
            ..Self::default()
        }
    }
}

/// Follows the shown page with a new snapshot, decoded off the UI thread;
/// called from the board's poll.
pub(crate) fn refresh(board: &mut WebBoard, cx: &mut Context<WebBoard>) {
    if board.active
        && !board.covered
        && let Some(page) = &board.page
    {
        board.mirror.snapshot(page, |board| &mut board.mirror, cx);
    }
}

impl Mirror {
    pub(crate) fn snapshot<T: 'static>(
        &mut self,
        page: &crate::page::Page,
        mirror: fn(&mut T) -> &mut Self,
        cx: &mut Context<T>,
    ) {
        let now = Instant::now();
        let due = self
            .requested
            .is_none_or(|last| now.duration_since(last) >= REFRESH);
        if !self.on || !due || self.decoding {
            return;
        }
        self.requested = Some(now);
        let Some(png) = snapshot(page, cx) else {
            return;
        };
        self.decoding = true;
        let decoded = cx.background_spawn(async move {
            subspace_framework::dashboard::images::decode(&png, "image/png").ok()
        });
        cx.spawn(async move |this, cx| {
            let image = decoded.await;
            _ = this.update(cx, |board, cx| {
                let mirror = mirror(board);
                if mirror.requested == Some(now) {
                    mirror.decoding = false;
                    if image.is_some() {
                        mirror.image = image;
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }

    #[cfg(all(feature = "agentsview", any(target_os = "macos", windows)))]
    pub(crate) fn image_over(&self, page: AnyElement) -> AnyElement {
        let Some(image) = &self.image else {
            return page;
        };
        div()
            .relative()
            .size_full()
            .child(page)
            .child(img(image.clone()).absolute().top_0().left_0().size_full())
            .into_any_element()
    }
}

#[cfg(all(target_os = "macos", debug_assertions))]
fn snapshot(page: &crate::page::Page, cx: &App) -> Option<Vec<u8>> {
    page.snapshot(cx)
}

#[cfg(not(all(target_os = "macos", debug_assertions)))]
fn snapshot(_: &crate::page::Page, _: &App) -> Option<Vec<u8>> {
    None
}

/// The page's view, with the mirror over it while capturing.
pub(crate) fn view(board: &WebBoard, page: AnyElement, cx: &mut Context<WebBoard>) -> AnyElement {
    if !board.mirror.on {
        return page;
    }
    let bounds = board.mirror.bounds.clone();
    div()
        .relative()
        .size_full()
        .child(page)
        .children(
            board
                .mirror
                .image
                .clone()
                .map(|image| img(image).absolute().top_0().left_0().size_full()),
        )
        // On top of the native view's own hitbox, which takes GPUI's pointer.
        .child(
            div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .child(
                    canvas(move |area, _, _| bounds.set(area), |_, _, _, _| {})
                        .absolute()
                        .size_full(),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|board, event: &MouseDownEvent, _, cx| {
                        forward(board, event.position, CLICK, cx)
                    }),
                )
                .on_mouse_move(cx.listener(|board, event: &MouseMoveEvent, _, cx| {
                    forward(board, event.position, MOVE, cx)
                })),
        )
        .into_any_element()
}

const CLICK: &[&str] = &["pointerdown", "mousedown", "pointerup", "mouseup", "click"];
const MOVE: &[&str] = &["pointermove", "mousemove"];

fn forward(board: &WebBoard, position: Point<Pixels>, events: &[&str], cx: &Context<WebBoard>) {
    let origin = board.mirror.bounds.get().origin;
    let local = position - origin;
    if let Some(page) = &board.page {
        page.push(&script(f32::from(local.x), f32::from(local.y), events), cx);
    }
}

/// Dispatches `events` at a point of the page, as the pointer would.
fn script(x: f32, y: f32, events: &[&str]) -> String {
    let events = serde_json::json!(events);
    format!(
        "(()=>{{const x={x},y={y};const target=document.elementFromPoint(x,y)||document.body;\
         for(const type of {events}){{const Kind=type.startsWith('pointer')?PointerEvent:MouseEvent;\
         target.dispatchEvent(new Kind(type,{{bubbles:true,cancelable:true,composed:true,clientX:x,clientY:y,\
         pointerType:'mouse',isPrimary:true,button:0,buttons:type.endsWith('down')?1:0,view:window}}));}}}})();"
    )
}

#[cfg(test)]
mod tests {
    use super::{CLICK, script};

    #[test]
    fn a_forwarded_click_dispatches_each_event_at_the_point() {
        let script = script(12.5, 40.0, CLICK);
        assert!(script.contains("const x=12.5,y=40"));
        assert!(
            script.contains("[\"pointerdown\",\"mousedown\",\"pointerup\",\"mouseup\",\"click\"]")
        );
        assert!(script.contains("elementFromPoint(x,y)"));
    }
}
