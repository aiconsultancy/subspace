//! Defer layout until trigger prepaint has captured this frame's bounds.

use super::{Anchor, Dismissal};
use gpui_kit::base::{Align, POPUP_PRIORITY, Placement, Positioner};
use gpui_kit::{
    AnyElement, App, Bounds, Context, Element, GlobalElementId, InspectorElementId,
    InteractiveElement as _, IntoElement, LayoutId, ParentElement as _, Pixels, Point, Position,
    Style, Window, deferred, div, px,
};
use std::{cell::Cell, rc::Rc};

// Physical viewport clearance, matching the pinned native popup.
const WINDOW_MARGIN: Pixels = px(8.);

pub(super) fn render(anchor: &Anchor, cx: &mut Context<Anchor>) -> AnyElement {
    let Some(content) = &anchor.content else {
        return div().into_any_element();
    };
    let close = cx.weak_entity();
    let outside = cx.weak_entity();
    let native_menu = matches!(content.dismissal, Dismissal::NativeMenu { .. });
    let body = div()
        .track_focus(&anchor.surface_focus)
        .key_context(crate::MODAL_CONTEXT)
        .on_key_down(move |event, window, cx| {
            if !native_menu && event.keystroke.key == "escape" {
                _ = close.update(cx, |anchor, cx| anchor.dismiss(window, cx));
                cx.stop_propagation();
            }
        })
        .on_mouse_down_out(move |event, window, cx| {
            if !native_menu {
                _ = outside.update(cx, |anchor, cx| {
                    if !anchor.bounds.contains(&event.position) {
                        anchor.dismiss(window, cx);
                    }
                });
            }
        })
        .child(content.view.clone());
    deferred(Surface {
        anchor: cx.entity(),
        body: Some(body.into_any_element()),
        drawn: None,
    })
    .with_priority(POPUP_PRIORITY)
    .into_any_element()
}

struct Surface {
    anchor: gpui_kit::Entity<Anchor>,
    body: Option<AnyElement>,
    drawn: Option<AnyElement>,
}

impl IntoElement for Surface {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for Surface {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<gpui_kit::ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let style = Style {
            position: Position::Absolute,
            ..Style::default()
        };
        (window.request_layout(style, None, cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        let Some(body) = self.body.take() else {
            return;
        };
        let origin = Rc::new(Cell::new(Point::default()));
        let resolved = origin.clone();
        let mut drawn = Positioner::side(self.anchor.read(cx).bounds)
            .placement(Placement::Bottom)
            .align(Align::Start)
            .margin(WINDOW_MARGIN)
            .occlude()
            .on_position(move |position| resolved.set(position.bounds.origin))
            .child(ExactBody { body, origin })
            .into_any_element();
        // Trigger bounds are window-relative. Starting at a fractional parent
        // origin makes Positioner round an offset, then add that origin again,
        // shifting the popup away from the trigger by a physical pixel.
        drawn.prepaint_as_root(
            Default::default(),
            window.viewport_size().into(),
            window,
            cx,
        );
        self.drawn = Some(drawn);
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Some(drawn) = &mut self.drawn {
            drawn.paint(window, cx);
        }
    }
}

/// Kit 0.9.1 rounds Positioner's child offset to logical pixels, even on a
/// high-DPI window. Preserve its resolved placement and hitbox, but prepaint
/// the body at that exact origin so half-point trigger edges remain aligned.
struct ExactBody {
    body: AnyElement,
    origin: Rc<Cell<Point<Pixels>>>,
}

impl IntoElement for ExactBody {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl Element for ExactBody {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<gpui_kit::ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        (self.body.request_layout(window, cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        window.with_element_offset(self.origin.get() - bounds.origin, |window| {
            self.body.prepaint(window, cx);
        });
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.body.paint(window, cx);
    }
}
