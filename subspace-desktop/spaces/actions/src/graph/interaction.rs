use super::{
    Drag, FlowGraph,
    layout::{self, Point},
};
use gpui_kit::{Context, KeyDownEvent, MouseMoveEvent, ScrollEvent, Window, px};

const WHEEL_SENSITIVITY: f32 = 0.002;
const SCROLL_LINE_HEIGHT: f32 = 1.;

impl FlowGraph {
    pub(super) fn begin_drag(
        &mut self,
        node: Option<u64>,
        position: gpui_kit::Point<gpui_kit::Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus.focus(window, cx);
        self.drag = Some(Drag {
            node,
            position: Point {
                x: position.x.into(),
                y: position.y.into(),
            },
        });
        cx.stop_propagation();
    }

    pub(super) fn move_pointer(
        &mut self,
        event: &MouseMoveEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(drag) = &mut self.drag else {
            return;
        };
        if event.pressed_button != Some(gpui_kit::MouseButton::Left) {
            self.drag = None;
            return;
        }
        let position = Point {
            x: event.position.x.into(),
            y: event.position.y.into(),
        };
        let delta = Point {
            x: position.x - drag.position.x,
            y: position.y - drag.position.y,
        };
        drag.position = position;
        self.viewport.drag(drag.node, delta);
        cx.notify();
    }

    pub(super) fn scroll(&mut self, event: &ScrollEvent, _: &mut Window, cx: &mut Context<Self>) {
        let delta = f32::from(event.delta.pixel_delta(px(SCROLL_LINE_HEIGHT)).y);
        let anchor = Point {
            x: f32::from(event.position.x) - self.origin.x,
            y: f32::from(event.position.y) - self.origin.y,
        };
        self.viewport
            .zoom((delta * WHEEL_SENSITIVITY).exp(), anchor);
        cx.stop_propagation();
        cx.notify();
    }

    pub(super) fn pinch(
        &mut self,
        event: &gpui_kit::PinchEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let anchor = Point {
            x: f32::from(event.position.x) - self.origin.x,
            y: f32::from(event.position.y) - self.origin.y,
        };
        self.viewport
            .zoom(super::viewport::IDENTITY_SCALE + event.delta, anchor);
        cx.stop_propagation();
        cx.notify();
    }

    pub(super) fn key(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        match event.keystroke.key.as_str() {
            "0" => self.fit(cx),
            "enter" => {
                if let Some(id) = self.selected {
                    self.select(id, cx);
                }
            }
            key @ ("left" | "right" | "up" | "down") => {
                if let Some(id) = layout::neighbor(&self.layout.nodes, self.selected, key) {
                    self.select(id, cx);
                }
            }
            _ => return,
        }
        cx.stop_propagation();
    }
}
