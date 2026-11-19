//! Drawing a sprite glyph's shapes into a GPUI window.
//!
//! The only part of the sprite module that touches GPUI: a rectangle is a
//! quad, a filled polygon and a stroked polyline are tessellated paths. The
//! geometry itself is pure and tested without a window.

use gpui_kit::{Bounds, Hsla, PathBuilder, Pixels, Point, Window, fill, point, px};

use crate::sprite::shape::{CellRect, Pt, Shape};

/// Where one sprite glyph is painted and in what colour.
pub(crate) struct SpritePaint {
    /// The cell the glyph fills, in window coordinates.
    pub(crate) bounds: Bounds<Pixels>,
    /// The glyph's colour.
    pub(crate) color: Hsla,
}

/// Draw every shape of one glyph.
// qual:allow(test_quality, untested) reason: "Runs only while GPUI paints TerminalElement (Element::paint), a dispatch the call graph cannot follow; view_tests draws the element with render_frame."
pub(crate) fn paint_shapes(shapes: &[Shape], at: &SpritePaint, window: &mut Window) {
    for shape in shapes {
        paint_shape(shape, at, window);
    }
}

/// Draw one shape.
// qual:allow(test_quality, untested) reason: "Runs only while GPUI paints TerminalElement (Element::paint), a dispatch the call graph cannot follow; view_tests draws the element with render_frame."
fn paint_shape(shape: &Shape, at: &SpritePaint, window: &mut Window) {
    match shape {
        Shape::Rect(rect) => window.paint_quad(fill(
            rect_bounds(*rect, at.bounds),
            shaded(at.color, rect.alpha),
        )),
        Shape::Fill(points) => paint_path(PathBuilder::fill(), points, true, at, window),
        Shape::Stroke { points, width } => {
            paint_path(PathBuilder::stroke(px(*width)), points, false, at, window);
        }
    }
}

/// Tessellate one polygon or polyline and draw it. A path that cannot be
/// tessellated is dropped rather than drawn wrong, which keeps the rest of
/// the frame intact.
fn paint_path(
    mut builder: PathBuilder,
    points: &[Pt],
    closed: bool,
    at: &SpritePaint,
    window: &mut Window,
) {
    let mut placed: Vec<Point<Pixels>> = points
        .iter()
        .map(|cell_point| window_point(*cell_point, at.bounds))
        .collect();
    placed.dedup();
    if placed.len() < 2 {
        return;
    }
    builder.add_polygon(&placed, closed);
    if let Ok(path) = builder.build() {
        window.paint_path(path, at.color);
    }
}

/// Where one cell-local point lands in the window.
fn window_point(cell_point: Pt, bounds: Bounds<Pixels>) -> Point<Pixels> {
    point(
        bounds.origin.x + px(cell_point.x),
        bounds.origin.y + px(cell_point.y),
    )
}

/// Where one cell-local rectangle lands in the window.
fn rect_bounds(rect: CellRect, bounds: Bounds<Pixels>) -> Bounds<Pixels> {
    Bounds::from_corners(
        window_point(
            Pt {
                x: rect.x0,
                y: rect.y0,
            },
            bounds,
        ),
        window_point(
            Pt {
                x: rect.x1,
                y: rect.y1,
            },
            bounds,
        ),
    )
}

/// The glyph's colour, held back to the share a shaded block keeps.
fn shaded(color: Hsla, alpha: f32) -> Hsla {
    Hsla {
        a: color.a * alpha,
        ..color
    }
}

#[cfg(test)]
mod tests {
    use gpui_kit::{Bounds, Hsla, point, px, rgb, size};

    use crate::sprite::paint::{rect_bounds, shaded, window_point};
    use crate::sprite::shape::{CellRect, OPAQUE, Pt};

    /// One cell at an offset, so a test can see the translation.
    fn cell() -> Bounds<gpui_kit::Pixels> {
        Bounds::new(point(px(10.), px(20.)), size(px(8.), px(19.)))
    }

    #[test]
    fn a_cell_local_point_lands_at_the_cell_it_belongs_to() {
        let placed = window_point(Pt { x: 4.0, y: 9.5 }, cell());

        assert_eq!(placed, point(px(14.), px(29.5)));
    }

    #[test]
    fn a_cell_local_rectangle_covers_the_same_part_of_the_cell() {
        let rect = CellRect {
            x0: 0.0,
            y0: 9.5,
            x1: 8.0,
            y1: 19.0,
            alpha: OPAQUE,
        };

        let bounds = rect_bounds(rect, cell());

        assert_eq!(bounds.origin, point(px(10.), px(29.5)));
        assert_eq!(bounds.size, size(px(8.), px(9.5)));
    }

    #[test]
    fn a_shade_keeps_its_share_of_the_colour() {
        let color = Hsla::from(rgb(0xabb2bf));

        let light = shaded(color, 0.25);

        assert_eq!(light.a, color.a * 0.25);
        assert_eq!(light.h, color.h);
        assert_eq!(shaded(color, OPAQUE), color);
    }
}
