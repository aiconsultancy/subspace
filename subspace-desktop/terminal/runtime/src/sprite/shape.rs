//! The shapes a sprite glyph is drawn from, measured in points inside one
//! cell, and the helpers every glyph family builds them with.
//!
//! Nothing here touches GPUI: a geometry function takes a code point and a
//! [`SpriteCell`] and returns shapes, so every glyph is tested without a
//! window. [`crate::sprite::paint`] is the only part that draws.

/// A light stroke as a share of the cell width. Ghostty takes
/// `Metrics.box_thickness` from the font's underline thickness, which comes
/// out at about an eighth of a monospace cell.
const LIGHT_SHARE: f32 = 0.125;
/// The thinnest stroke that still covers a whole point.
const MIN_THICKNESS: f32 = 1.0;
/// A heavy stroke against a light one.
const HEAVY_FACTOR: f32 = 2.0;
/// How many straight segments one curve is flattened into. A cell is a few
/// points across, so eight segments stay well inside half a point.
const CURVE_STEPS: usize = 8;
/// How much each of a cubic curve's two control points weighs against its
/// ends, from the Bernstein form of the curve.
const CONTROL_WEIGHT: f32 = 3.0;
/// The whole of the foreground colour.
pub(crate) const OPAQUE: f32 = 1.0;
/// Half of a measurement, which halves and middles are taken with.
pub(crate) const HALF: f32 = 0.5;
/// How close to a cell edge a point counts as touching it, in points. Every
/// edge point of a glyph is produced from the cell size itself, so this only
/// has to absorb the rounding of a flattened curve.
const EDGE_TOLERANCE: f32 = 0.01;

/// The cell a sprite glyph fills exactly: its size in points and the width a
/// light stroke in it is drawn with.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SpriteCell {
    /// The cell's width in points.
    pub(crate) width: f32,
    /// The cell's height in points.
    pub(crate) height: f32,
    /// The width of a light stroke, in points.
    pub(crate) thickness: f32,
}

impl SpriteCell {
    /// The cell of a measured grid cell, with the stroke width its lines
    /// join at.
    pub(crate) fn new(width: f32, height: f32) -> Self {
        Self {
            width,
            height,
            thickness: (width * LIGHT_SHARE).round().max(MIN_THICKNESS),
        }
    }

    /// The middle of the cell, horizontally.
    pub(crate) fn middle_x(self) -> f32 {
        self.width * HALF
    }

    /// The middle of the cell, vertically.
    pub(crate) fn middle_y(self) -> f32 {
        self.height * HALF
    }
}

/// How thick one line is drawn, after Ghostty's `Thickness`. The glyphs of
/// the three ranges use only these two weights; Ghostty's `super_light` is
/// for the legacy-computing blocks, which stay with the font.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Weight {
    /// The cell's own stroke width.
    Light,
    /// Twice the cell's stroke width.
    Heavy,
}

impl Weight {
    /// The width of a line of this weight, in points.
    pub(crate) fn points(self, cell: SpriteCell) -> f32 {
        match self {
            Self::Light => cell.thickness,
            Self::Heavy => cell.thickness * HEAVY_FACTOR,
        }
    }
}

/// Which way a stroke runs across the cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Axis {
    /// Along the cell's height.
    Vertical,
    /// Along the cell's width.
    Horizontal,
}

/// Which way a diagonal leans.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Slant {
    /// From the upper left corner to the lower right one.
    Down,
    /// From the upper right corner to the lower left one.
    Up,
}

/// A point inside a cell, from its top left corner, in points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Pt {
    /// Points from the cell's left edge.
    pub(crate) x: f32,
    /// Points from the cell's top edge.
    pub(crate) y: f32,
}

/// A point inside a cell.
pub(crate) const fn pt(x: f32, y: f32) -> Pt {
    Pt { x, y }
}

/// An axis-aligned rectangle inside a cell, with the share of the foreground
/// colour it is filled with.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CellRect {
    /// The left edge.
    pub(crate) x0: f32,
    /// The top edge.
    pub(crate) y0: f32,
    /// The right edge.
    pub(crate) x1: f32,
    /// The bottom edge.
    pub(crate) y1: f32,
    /// How much of the foreground colour the fill keeps, for the shades.
    pub(crate) alpha: f32,
}

/// One shape of a sprite glyph, in cell-local points.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Shape {
    /// A rectangle the painter fills with a quad.
    Rect(CellRect),
    /// A closed polygon the painter fills as a path.
    Fill(Vec<Pt>),
    /// An open polyline the painter strokes as a path.
    Stroke {
        /// The line the stroke follows.
        points: Vec<Pt>,
        /// How wide the stroke is, in points.
        width: f32,
    },
}

/// An opaque rectangle, with its corners put in order so a caller may give
/// an arm's outer edge first.
pub(crate) fn rect(x0: f32, y0: f32, x1: f32, y1: f32) -> Shape {
    Shape::Rect(CellRect {
        x0: x0.min(x1),
        y0: y0.min(y1),
        x1: x0.max(x1),
        y1: y0.max(y1),
        alpha: OPAQUE,
    })
}

/// One stroke of an arm: `along` is its span on its own axis and `across`
/// the pair of edges it sits between.
pub(crate) fn stroke(axis: Axis, along: (f32, f32), across: (f32, f32)) -> Shape {
    match axis {
        Axis::Vertical => rect(across.0, along.0, across.1, along.1),
        Axis::Horizontal => rect(along.0, across.0, along.1, across.1),
    }
}

/// The whole cell, filled with part of the foreground colour: the shades
/// U+2591 to U+2593 and the full block U+2588.
pub(crate) fn shaded_cell(cell: SpriteCell, alpha: f32) -> Shape {
    Shape::Rect(CellRect {
        x0: 0.0,
        y0: 0.0,
        x1: cell.width,
        y1: cell.height,
        alpha,
    })
}

/// The two edges of a stroke of `thickness` centred across `size`.
pub(crate) fn centred(size: f32, thickness: f32) -> (f32, f32) {
    let start = ((size - thickness) * HALF).max(0.0);
    (start, start + thickness)
}

/// A cubic Bézier curve as a polyline, without its first point, so a caller
/// extends a path with it.
pub(crate) fn curve(from: Pt, first: Pt, second: Pt, to: Pt) -> Vec<Pt> {
    let mut points = Vec::with_capacity(CURVE_STEPS);
    for step in 1..=CURVE_STEPS {
        let at = step as f32 / CURVE_STEPS as f32;
        points.push(pt(
            cubic(from.x, first.x, second.x, to.x, at),
            cubic(from.y, first.y, second.y, to.y, at),
        ));
    }
    points
}

/// One axis of a cubic Bézier curve at `at` along it.
fn cubic(from: f32, first: f32, second: f32, to: f32, at: f32) -> f32 {
    let rest = 1.0 - at;
    let weights = (
        rest * rest * rest,
        CONTROL_WEIGHT * rest * rest * at,
        CONTROL_WEIGHT * rest * at * at,
        at * at * at,
    );
    from * weights.0 + first * weights.1 + second * weights.2 + to * weights.3
}

/// The same points, mirrored across the cell's vertical middle, which is how
/// Ghostty draws the right-facing half of a mirrored pair.
pub(crate) fn mirrored(points: &[Pt], cell: SpriteCell) -> Vec<Pt> {
    points
        .iter()
        .map(|point| pt(cell.width - point.x, point.y))
        .collect()
}

/// The same shapes, with every path point that sits on a cell edge pushed
/// `bleed` points past it.
///
/// A rectangle is drawn as a quad, which covers its pixels exactly, but a
/// polygon and a polyline are tessellated and anti-aliased: a path edge that
/// stops exactly on the cell boundary leaves that pixel column part-covered,
/// and the neighbouring cell's background quad does not fill the rest of it,
/// which reads as a thin seam. Half a device pixel of bleed covers the
/// boundary pixel without reaching the cell beyond it. Ghostty overdraws its
/// diagonals the same way.
pub(crate) fn bled(shapes: Vec<Shape>, cell: SpriteCell, bleed: f32) -> Vec<Shape> {
    shapes
        .into_iter()
        .map(|shape| bleed_shape(shape, cell, bleed))
        .collect()
}

/// One shape, with its path points pushed past the edges they touch.
fn bleed_shape(shape: Shape, cell: SpriteCell, bleed: f32) -> Shape {
    match shape {
        Shape::Rect(rect) => Shape::Rect(rect),
        Shape::Fill(points) => Shape::Fill(bleed_points(&points, cell, bleed)),
        Shape::Stroke { points, width } => Shape::Stroke {
            points: bleed_points(&points, cell, bleed),
            width,
        },
    }
}

/// The same points, each pushed past any cell edge it sits on.
fn bleed_points(points: &[Pt], cell: SpriteCell, bleed: f32) -> Vec<Pt> {
    points
        .iter()
        .map(|point| {
            pt(
                bleed_axis(point.x, cell.width, bleed),
                bleed_axis(point.y, cell.height, bleed),
            )
        })
        .collect()
}

/// One coordinate, pushed outward when it sits on either end of its axis.
fn bleed_axis(at: f32, size: f32, bleed: f32) -> f32 {
    if at.abs() <= EDGE_TOLERANCE {
        return at - bleed;
    }
    if (at - size).abs() <= EDGE_TOLERANCE {
        return at + bleed;
    }
    at
}

/// A diagonal from corner to corner, overshooting the corners a little so
/// the line keeps its slope where it leaves the cell, as Ghostty does.
pub(crate) fn diagonal(slant: Slant, cell: SpriteCell) -> Shape {
    let slope = overshoot(cell);
    let (top, bottom) = (-slope.1, cell.height + slope.1);
    let (left, right) = (-slope.0, cell.width + slope.0);
    let points = match slant {
        Slant::Down => vec![pt(left, top), pt(right, bottom)],
        Slant::Up => vec![pt(right, top), pt(left, bottom)],
    };
    Shape::Stroke {
        points,
        width: Weight::Light.points(cell),
    }
}

/// How far a diagonal runs past each corner, keeping its slope.
fn overshoot(cell: SpriteCell) -> (f32, f32) {
    let width = cell.width.max(MIN_THICKNESS);
    let height = cell.height.max(MIN_THICKNESS);
    (
        (width / height).min(1.0) * HALF,
        (height / width).min(1.0) * HALF,
    )
}

#[cfg(test)]
mod tests {
    use crate::sprite::shape::{
        Axis, Shape, Slant, SpriteCell, Weight, centred, curve, diagonal, pt,
    };

    /// A cell of plausible size for a 13 point font with a fifth more height.
    const CELL: SpriteCell = SpriteCell {
        width: 8.0,
        height: 19.0,
        thickness: 1.0,
    };

    #[test]
    fn a_cell_takes_its_stroke_width_from_its_width() {
        let small = SpriteCell::new(8.0, 19.0);
        let large = SpriteCell::new(24.0, 48.0);

        assert_eq!(small.thickness, 1.0);
        assert_eq!(large.thickness, 3.0);
        assert_eq!(small.middle_x(), 4.0);
        assert_eq!(small.middle_y(), 9.5);
    }

    #[test]
    fn a_heavy_line_is_twice_a_light_one_and_never_thinner_than_a_point() {
        assert_eq!(Weight::Light.points(CELL), 1.0);
        assert_eq!(Weight::Heavy.points(CELL), 2.0);
        assert_eq!(Weight::Heavy.points(SpriteCell::new(24.0, 48.0)), 6.0);
    }

    #[test]
    fn a_stroke_is_centred_across_the_size_it_crosses() {
        assert_eq!(centred(19.0, 1.0), (9.0, 10.0));
        assert_eq!(centred(8.0, 2.0), (3.0, 5.0));
    }

    #[test]
    fn a_stroke_lays_its_span_along_the_axis_it_runs_on() {
        let vertical = super::stroke(Axis::Vertical, (0.0, 10.0), (3.0, 4.0));
        let horizontal = super::stroke(Axis::Horizontal, (0.0, 8.0), (9.0, 10.0));

        assert_eq!(vertical, super::rect(3.0, 0.0, 4.0, 10.0));
        assert_eq!(horizontal, super::rect(0.0, 9.0, 8.0, 10.0));
    }

    #[test]
    fn a_curve_ends_on_its_last_point() {
        let points = curve(pt(0.0, 0.0), pt(4.0, 0.0), pt(8.0, 4.0), pt(8.0, 8.0));

        let last = points.last().copied().expect("a flattened curve");
        assert_eq!(points.len(), 8);
        assert!(
            (last.x - 8.0).abs() < 0.001,
            "{last:?} should end at the corner"
        );
        assert!(
            (last.y - 8.0).abs() < 0.001,
            "{last:?} should end at the corner"
        );
    }

    #[test]
    fn a_curve_bulges_away_from_the_straight_line_between_its_ends() {
        let points = curve(pt(0.0, 0.0), pt(8.0, 0.0), pt(8.0, 0.0), pt(8.0, 8.0));

        let middle = points[3];
        assert!(
            middle.x > middle.y,
            "{middle:?} should bow toward the control points"
        );
    }

    #[test]
    fn mirroring_reflects_every_point_across_the_cell() {
        let mirrored = super::mirrored(&[pt(0.0, 1.0), pt(3.0, 2.0)], CELL);

        assert_eq!(mirrored, vec![pt(8.0, 1.0), pt(5.0, 2.0)]);
    }

    #[test]
    fn a_filled_edge_is_pushed_past_the_cell_edge_it_sits_on() {
        let separator = Shape::Fill(vec![
            pt(0.0, 0.0),
            pt(CELL.width, CELL.height * 0.5),
            pt(0.0, CELL.height),
        ]);

        let drawn = super::bled(vec![separator], CELL, 0.25);

        let Shape::Fill(points) = &drawn[0] else {
            panic!("a fill stays a fill");
        };
        assert_eq!(points[0], pt(-0.25, -0.25), "the top left corner");
        assert_eq!(points[1], pt(8.25, 9.5), "the tip at the right edge");
        assert_eq!(points[2], pt(-0.25, 19.25), "the bottom left corner");
    }

    #[test]
    fn a_point_inside_the_cell_is_left_where_it_is() {
        let inside = Shape::Fill(vec![pt(4.0, 9.5), pt(0.0, 9.5)]);

        let drawn = super::bled(vec![inside], CELL, 0.25);

        let Shape::Fill(points) = &drawn[0] else {
            panic!("a fill stays a fill");
        };
        assert_eq!(points[0], pt(4.0, 9.5), "the middle of the cell");
        assert_eq!(points[1], pt(-0.25, 9.5), "only the edge point moves");
    }

    #[test]
    fn a_stroke_bleeds_but_a_rectangle_does_not() {
        let stroke = Shape::Stroke {
            points: vec![pt(4.0, CELL.height), pt(4.0, 9.5), pt(CELL.width, 9.5)],
            width: 1.0,
        };
        let rectangle = super::rect(0.0, 0.0, CELL.width, CELL.height);

        let drawn = super::bled(vec![stroke, rectangle.clone()], CELL, 0.25);

        let Shape::Stroke { points, width } = &drawn[0] else {
            panic!("a stroke stays a stroke");
        };
        assert_eq!(*width, 1.0);
        assert_eq!(points[0], pt(4.0, 19.25), "the arm reaching the row below");
        assert_eq!(points[2], pt(8.25, 9.5), "the arm reaching the next column");
        assert_eq!(drawn[1], rectangle, "a quad covers its pixels already");
    }

    #[test]
    fn a_diagonal_runs_corner_to_corner_past_both_ends() {
        let Shape::Stroke { points, width } = diagonal(Slant::Down, CELL) else {
            panic!("a diagonal is stroked");
        };

        assert_eq!(width, 1.0);
        assert!(points[0].x < 0.0 && points[0].y < 0.0);
        assert!(points[1].x > CELL.width && points[1].y > CELL.height);
    }

    #[test]
    fn a_diagonal_the_other_way_starts_at_the_upper_right() {
        let Shape::Stroke { points, .. } = diagonal(Slant::Up, CELL) else {
            panic!("a diagonal is stroked");
        };

        assert!(
            points[0].x > CELL.width,
            "{points:?} should start past the right edge"
        );
        assert!(
            points[1].x < 0.0,
            "{points:?} should end past the left edge"
        );
    }
}
