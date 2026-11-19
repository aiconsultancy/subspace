//! The box drawn over the inspected element, and the shield that keeps the
//! picker on the application while the pointer crosses the panel. Both paint
//! directly: a `div` would register its own picking hitbox and pick itself.

use gpui_kit::{
    App, BorderStyle, Bounds, Edges, Element, ElementId, GlobalElementId, HitboxBehavior, Hsla,
    InspectorElementId, IntoElement, LayoutId, Pixels, Point, Position, SharedString, Style,
    TextAlign, TextRun, Window, deferred, fill, outline, point, px, quad, rems, size,
};
use subspace_dashboard::Tokens;

use super::model::BoxModel;

/// Above every popup, so the box stays visible over menus and dialogs.
const OVERLAY_PRIORITY: usize = usize::MAX;
const TAG_FONT: Pixels = px(11.);
const TAG_LINE: Pixels = px(16.);
const TAG_PAD_X: Pixels = px(6.);
const TAG_GAP: Pixels = px(4.);
const TAG_RADIUS: Pixels = px(4.);
const TAG_BORDER: Pixels = px(1.);
const INSPECTOR_WIDTH_REM: f32 = 30.;
const PADDING_ALPHA: f32 = 0.28;
const MARGIN_ALPHA: f32 = 0.22;

/// Colours from the active theme: the outline and tag use the accent and
/// raised surface, padding and margin the green and orange tones.
#[derive(Clone, Copy)]
struct Palette {
    outline: Hsla,
    padding: Hsla,
    margin: Hsla,
    tag: Hsla,
    tag_line: Hsla,
    text: Hsla,
}

impl Palette {
    fn new(tokens: &Tokens) -> Self {
        Self {
            outline: tokens.accent,
            padding: tokens.green.opacity(PADDING_ALPHA),
            margin: tokens.orange.opacity(MARGIN_ALPHA),
            tag: tokens.surface,
            tag_line: tokens.line,
            text: tokens.fg_strong,
        }
    }
}

/// The highlight for `layout`, labelled with `tag`, drawn above everything.
pub(super) fn highlight(layout: BoxModel, tag: String, tokens: &Tokens) -> impl IntoElement {
    deferred(Highlight {
        layout,
        tag: tag.into(),
        palette: Palette::new(tokens),
    })
    .with_priority(OVERLAY_PRIORITY)
}

struct Highlight {
    layout: BoxModel,
    tag: SharedString,
    palette: Palette,
}

impl IntoElement for Highlight {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

fn absolute(window: &mut Window, cx: &mut App) -> LayoutId {
    let style = Style {
        position: Position::Absolute,
        ..Style::default()
    };
    window.request_layout(style, None, cx)
}

impl Element for Highlight {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
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
        (absolute(window, cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut Window,
        _: &mut App,
    ) {
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
        let bounds = self.layout.bounds;
        let palette = self.palette;
        for band in bands(outset(bounds, &self.layout.margin), bounds) {
            window.paint_quad(fill(band, palette.margin));
        }
        let inside_border = inset(bounds, &self.layout.border);
        let content = inset(inside_border, &self.layout.padding);
        for band in bands(inside_border, content) {
            window.paint_quad(fill(band, palette.padding));
        }
        window.paint_quad(outline(bounds, palette.outline, BorderStyle::Solid));
        paint_tag(&self.tag, bounds, palette, window, cx);
    }
}

fn inset(bounds: Bounds<Pixels>, edges: &Edges<Pixels>) -> Bounds<Pixels> {
    let width = (bounds.size.width - edges.left - edges.right).max(Pixels::ZERO);
    let height = (bounds.size.height - edges.top - edges.bottom).max(Pixels::ZERO);
    Bounds {
        origin: point(bounds.origin.x + edges.left, bounds.origin.y + edges.top),
        size: size(width, height),
    }
}

fn outset(bounds: Bounds<Pixels>, edges: &Edges<Pixels>) -> Bounds<Pixels> {
    Bounds {
        origin: point(bounds.origin.x - edges.left, bounds.origin.y - edges.top),
        size: size(
            bounds.size.width + edges.left + edges.right,
            bounds.size.height + edges.top + edges.bottom,
        ),
    }
}

/// The four rectangles between `outer` and `inner`, which lies inside it.
fn bands(outer: Bounds<Pixels>, inner: Bounds<Pixels>) -> [Bounds<Pixels>; 4] {
    let top = inner.origin.y - outer.origin.y;
    let bottom = outer.bottom() - inner.bottom();
    let left = inner.origin.x - outer.origin.x;
    let right = outer.right() - inner.right();
    [
        Bounds::new(outer.origin, size(outer.size.width, top)),
        Bounds::new(
            point(outer.origin.x, inner.bottom()),
            size(outer.size.width, bottom),
        ),
        Bounds::new(
            point(outer.origin.x, inner.origin.y),
            size(left, inner.size.height),
        ),
        Bounds::new(
            point(inner.right(), inner.origin.y),
            size(right, inner.size.height),
        ),
    ]
}

/// Where the tag goes: above the element when it fits, otherwise below it,
/// kept inside the window horizontally.
pub(super) fn tag_origin(
    element: Bounds<Pixels>,
    tag: gpui_kit::Size<Pixels>,
    viewport: gpui_kit::Size<Pixels>,
) -> Point<Pixels> {
    let above = element.origin.y - TAG_GAP - tag.height;
    let y = if above >= Pixels::ZERO {
        above
    } else {
        element.bottom() + TAG_GAP
    };
    let x = element
        .origin
        .x
        .min(viewport.width - tag.width)
        .max(Pixels::ZERO);
    point(x, y)
}

fn paint_tag(
    tag: &SharedString,
    element: Bounds<Pixels>,
    palette: Palette,
    window: &mut Window,
    cx: &mut App,
) {
    let run = TextRun {
        len: tag.len(),
        font: window.text_style().font(),
        color: palette.text,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let line = window
        .text_system()
        .shape_line(tag.clone(), TAG_FONT, &[run], None);
    let tag_size = size(line.width() + TAG_PAD_X + TAG_PAD_X, TAG_LINE);
    // GPUI gives its inspector the window's last 30rem; keep the tag left of it.
    let mut area = window.viewport_size();
    area.width -= rems(INSPECTOR_WIDTH_REM).to_pixels(window.rem_size());
    let origin = tag_origin(element, tag_size, area);
    let frame = Bounds::new(origin, tag_size);
    window.paint_quad(quad(
        frame,
        TAG_RADIUS,
        palette.tag,
        TAG_BORDER,
        palette.tag_line,
        BorderStyle::Solid,
    ));
    let text_origin = point(origin.x + TAG_PAD_X, origin.y);
    _ = line.paint(text_origin, TAG_LINE, TextAlign::Left, None, window, cx);
}

/// Covers its parent and, while picking, swallows the pointer there, so
/// moving across the panel keeps the element picked in the application.
pub(super) struct PickShield {
    pub(super) picking: bool,
}

impl IntoElement for PickShield {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for PickShield {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
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
        let mut style = Style {
            position: Position::Absolute,
            ..Style::default()
        };
        style.inset.top = Pixels::ZERO.into();
        style.inset.left = Pixels::ZERO.into();
        style.size.width = gpui_kit::relative(1.).into();
        style.size.height = gpui_kit::relative(1.).into();
        (window.request_layout(style, None, cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        _: &mut App,
    ) {
        if self.picking {
            window.insert_hitbox(bounds, HitboxBehavior::BlockMouse);
        }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        _: &mut Window,
        _: &mut App,
    ) {
    }
}

#[cfg(test)]
mod tests {
    use super::{bands, tag_origin};
    use gpui_kit::{Bounds, point, px, size};

    #[test]
    fn the_tag_sits_above_the_element_unless_it_would_leave_the_window() {
        let tag = size(px(80.), px(16.));
        let viewport = size(px(400.), px(300.));
        let low = Bounds::new(point(px(20.), px(100.)), size(px(28.), px(28.)));
        assert_eq!(tag_origin(low, tag, viewport), point(px(20.), px(80.)));
        let top = Bounds::new(point(px(20.), px(4.)), size(px(28.), px(28.)));
        assert_eq!(tag_origin(top, tag, viewport), point(px(20.), px(36.)));
        let right = Bounds::new(point(px(390.), px(100.)), size(px(10.), px(10.)));
        assert_eq!(tag_origin(right, tag, viewport).x, px(320.));
    }

    #[test]
    fn bands_cover_the_ring_between_two_boxes() {
        let outer = Bounds::new(point(px(0.), px(0.)), size(px(10.), px(10.)));
        let inner = Bounds::new(point(px(2.), px(1.)), size(px(5.), px(6.)));
        let [top, bottom, left, right] = bands(outer, inner);
        assert_eq!(top.size, size(px(10.), px(1.)));
        assert_eq!(bottom.size, size(px(10.), px(3.)));
        assert_eq!(left.size, size(px(2.), px(6.)));
        assert_eq!(right.size, size(px(3.), px(6.)));
    }
}
