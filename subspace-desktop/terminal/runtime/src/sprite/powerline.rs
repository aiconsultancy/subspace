//! Powerline separators and caps, U+E0B0 to U+E0BF plus U+E0D2 and U+E0D4:
//! the solid and hollow triangles, the solid and hollow half circles, the
//! four corner wedges with their hypotenuses, and the two flames. These are
//! private-use code points, so the table below names each one rather than
//! showing it.
//!
//! The geometry is ported from Ghostty's `src/font/sprite/draw/powerline.zig`
//! (MIT, credited in `licenses/terminal`). A separator has to fill its cell
//! exactly, because the colour either side of it is a cell background: a
//! glyph drawn at the font's own size leaves the two segments unjoined.

use crate::sprite::shape::{
    HALF, Pt, Shape, Slant, SpriteCell, Weight, curve, diagonal, mirrored, pt,
};

/// How far from a quarter circle's corner its Bézier control points sit, as
/// a share of the radius: `(sqrt(2) - 1) * 4 / 3`, the usual approximation.
const ARC_CONTROL: f32 = 0.552_284_8;

/// One of the points a Powerline wedge runs through, as a share of the
/// cell's width and of its height.
type Anchor = (f32, f32);

/// The cell's top left corner.
const TOP_LEFT: Anchor = (0.0, 0.0);
/// The cell's top right corner.
const TOP_RIGHT: Anchor = (1.0, 0.0);
/// The cell's bottom left corner.
const BOTTOM_LEFT: Anchor = (0.0, 1.0);
/// The cell's bottom right corner.
const BOTTOM_RIGHT: Anchor = (1.0, 1.0);
/// The middle of the cell's left edge.
const LEFT_MIDDLE: Anchor = (0.0, HALF);
/// The middle of the cell's right edge.
const RIGHT_MIDDLE: Anchor = (1.0, HALF);

/// Which way round a half circle or a notch faces.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Side {
    /// Filling the cell's left half and bulging to the right.
    Left,
    /// The mirror of it.
    Right,
}

/// What one Powerline code point draws.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Figure {
    /// A filled triangle through three points of the cell.
    Wedge(Anchor, Anchor, Anchor),
    /// The same three points, stroked as an open line.
    Outline(Anchor, Anchor, Anchor),
    /// A filled half circle.
    Circle(Side),
    /// The outline of the same half circle, kept inside the cell.
    CircleOutline(Side),
    /// A corner-to-corner diagonal, as the box-drawing glyphs draw it.
    Diagonal(Slant),
    /// Two wedges meeting at the middle with one stroke between them.
    Notch(Side),
}

/// Every Powerline code point this module draws.
#[rustfmt::skip]
const FIGURES: &[(u32, Figure)] = &[
    (0xe0b0, Figure::Wedge(TOP_LEFT, RIGHT_MIDDLE, BOTTOM_LEFT)),    // solid separator
    (0xe0b1, Figure::Outline(TOP_LEFT, RIGHT_MIDDLE, BOTTOM_LEFT)),  // the same as a line
    (0xe0b2, Figure::Wedge(TOP_RIGHT, LEFT_MIDDLE, BOTTOM_RIGHT)),   // the other way round
    (0xe0b3, Figure::Outline(TOP_RIGHT, LEFT_MIDDLE, BOTTOM_RIGHT)), // the same as a line
    (0xe0b4, Figure::Circle(Side::Left)),                            // solid cap
    (0xe0b5, Figure::CircleOutline(Side::Left)),                     // the same as a line
    (0xe0b6, Figure::Circle(Side::Right)),                           // the other way round
    (0xe0b7, Figure::CircleOutline(Side::Right)),                    // the same as a line
    (0xe0b8, Figure::Wedge(TOP_LEFT, BOTTOM_RIGHT, BOTTOM_LEFT)),    // lower left triangle
    (0xe0b9, Figure::Diagonal(Slant::Down)),                         // its hypotenuse
    (0xe0ba, Figure::Wedge(TOP_RIGHT, BOTTOM_RIGHT, BOTTOM_LEFT)),   // lower right triangle
    (0xe0bb, Figure::Diagonal(Slant::Up)),                           // its hypotenuse
    (0xe0bc, Figure::Wedge(TOP_LEFT, TOP_RIGHT, BOTTOM_LEFT)),       // upper left triangle
    (0xe0bd, Figure::Diagonal(Slant::Up)),                           // its hypotenuse
    (0xe0be, Figure::Wedge(TOP_LEFT, TOP_RIGHT, BOTTOM_RIGHT)),      // upper right triangle
    (0xe0bf, Figure::Diagonal(Slant::Down)),                         // its hypotenuse
    (0xe0d2, Figure::Notch(Side::Left)),                             // right-pointing flame
    (0xe0d4, Figure::Notch(Side::Right)),                            // the other way round
];

/// The shapes a Powerline code point draws, and `None` for a code point
/// this module does not cover.
pub(crate) fn shapes(code_point: u32, cell: SpriteCell) -> Option<Vec<Shape>> {
    let figure = figure_of(code_point)?;
    Some(draw(figure, cell))
}

/// The figure one code point draws.
fn figure_of(code_point: u32) -> Option<Figure> {
    FIGURES
        .iter()
        .find(|(candidate, _)| *candidate == code_point)
        .map(|(_, figure)| *figure)
}

/// The shapes one figure draws in a cell.
fn draw(figure: Figure, cell: SpriteCell) -> Vec<Shape> {
    match figure {
        Figure::Wedge(first, second, third) => vec![Shape::Fill(wedge(first, second, third, cell))],
        Figure::Outline(first, second, third) => vec![Shape::Stroke {
            points: wedge(first, second, third, cell),
            width: Weight::Light.points(cell),
        }],
        Figure::Circle(side) => vec![Shape::Fill(half_circle(side, cell, 0.0))],
        Figure::CircleOutline(side) => vec![Shape::Stroke {
            points: half_circle(side, cell, cell.thickness * HALF),
            width: cell.thickness,
        }],
        Figure::Diagonal(slant) => vec![diagonal(slant, cell)],
        Figure::Notch(side) => notch(side, cell),
    }
}

/// The three points of a wedge.
fn wedge(first: Anchor, second: Anchor, third: Anchor, cell: SpriteCell) -> Vec<Pt> {
    vec![
        anchor(first, cell),
        anchor(second, cell),
        anchor(third, cell),
    ]
}

/// Where one anchor of the cell sits.
fn anchor(anchor: Anchor, cell: SpriteCell) -> Pt {
    pt(cell.width * anchor.0, cell.height * anchor.1)
}

/// The outline of a half circle that fills the cell, from the top corner on
/// `side`, out to the middle of the cell and back to the bottom corner.
///
/// `inset` pulls the line that far inside the cell, so a stroke of twice it
/// stays inside the cell the way Ghostty's inner stroke does.
fn half_circle(side: Side, cell: SpriteCell, inset: f32) -> Vec<Pt> {
    let facing_left = arc_points(cell, inset);
    match side {
        Side::Left => facing_left,
        Side::Right => mirrored(&facing_left, cell),
    }
}

/// The left-facing half circle: a quarter circle down from the top left, a
/// straight middle, then a quarter circle back to the bottom left.
fn arc_points(cell: SpriteCell, inset: f32) -> Vec<Pt> {
    let radius = (cell.width.min(cell.middle_y()) - inset).max(0.0);
    let (top, bottom) = (inset, cell.height - inset);
    let reach = radius * ARC_CONTROL;
    let start = pt(0.0, top);
    let mut points = vec![start];
    points.extend(curve(
        start,
        pt(reach, top),
        pt(radius, top + radius - reach),
        pt(radius, top + radius),
    ));
    let turn = pt(radius, bottom - radius);
    points.push(turn);
    points.extend(curve(
        turn,
        pt(radius, bottom - radius + reach),
        pt(reach, bottom),
        pt(0.0, bottom),
    ));
    points
}

/// The two wedges of a Powerline flame. They meet at the middle of the cell
/// with one stroke of space between them.
fn notch(side: Side, cell: SpriteCell) -> Vec<Shape> {
    let gap = cell.thickness * HALF;
    let middle = cell.middle_y();
    let wedges = [
        vec![
            pt(0.0, 0.0),
            pt(cell.width, 0.0),
            pt(cell.middle_x(), middle - gap),
            pt(0.0, middle - gap),
        ],
        vec![
            pt(0.0, cell.height),
            pt(cell.width, cell.height),
            pt(cell.middle_x(), middle + gap),
            pt(0.0, middle + gap),
        ],
    ];
    wedges
        .iter()
        .map(|wedge| facing(wedge, side, cell))
        .collect()
}

/// One polygon, mirrored when it faces the other way.
fn facing(points: &[Pt], side: Side, cell: SpriteCell) -> Shape {
    match side {
        Side::Left => Shape::Fill(points.to_vec()),
        Side::Right => Shape::Fill(mirrored(points, cell)),
    }
}

#[cfg(test)]
mod tests {
    use crate::sprite::powerline::shapes;
    use crate::sprite::shape::{Pt, Shape, SpriteCell};

    /// A cell of plausible size for a 13 point font with a fifth more height.
    const CELL: SpriteCell = SpriteCell {
        width: 8.0,
        height: 19.0,
        thickness: 1.0,
    };

    /// The one filled polygon of a glyph.
    fn polygon(code_point: u32) -> Vec<Pt> {
        let drawn = shapes(code_point, CELL).expect("a Powerline glyph");
        match drawn.into_iter().next().expect("one shape") {
            Shape::Fill(points) => points,
            other => panic!("{code_point:#x} should be filled, not {other:?}"),
        }
    }

    #[test]
    fn a_code_point_outside_the_table_draws_nothing() {
        assert!(shapes(0xe0c0, CELL).is_none());
        assert!(shapes(u32::from('a'), CELL).is_none());
    }

    #[test]
    fn the_solid_separator_fills_from_the_left_edge_to_the_middle_of_the_right() {
        let points = polygon(0xe0b0);

        assert_eq!(points.len(), 3);
        assert_eq!(points[0], Pt { x: 0.0, y: 0.0 });
        assert_eq!(points[1], Pt { x: 8.0, y: 9.5 });
        assert_eq!(points[2], Pt { x: 0.0, y: 19.0 });
    }

    #[test]
    fn the_separator_the_other_way_points_at_the_left_edge() {
        let points = polygon(0xe0b2);

        assert_eq!(points[0], Pt { x: 8.0, y: 0.0 });
        assert_eq!(points[1], Pt { x: 0.0, y: 9.5 });
        assert_eq!(points[2], Pt { x: 8.0, y: 19.0 });
    }

    #[test]
    fn the_hollow_separator_strokes_the_same_three_points() {
        let drawn = shapes(0xe0b1, CELL).expect("a Powerline glyph");

        let Shape::Stroke { points, width } = &drawn[0] else {
            panic!("a hollow separator is stroked, not {drawn:?}");
        };
        assert_eq!(points.len(), 3);
        assert_eq!(*width, 1.0);
        assert_eq!(points[1], Pt { x: 8.0, y: 9.5 });
    }

    #[test]
    fn the_solid_cap_is_a_half_circle_from_corner_to_corner() {
        let points = polygon(0xe0b4);

        let first = points[0];
        let last = points[points.len() - 1];
        let widest = points.iter().fold(0.0_f32, |far, point| far.max(point.x));
        let middle = points
            .iter()
            .filter(|point| (point.x - widest).abs() < 0.001)
            .count();
        assert_eq!(first, Pt { x: 0.0, y: 0.0 }, "it starts at the top left");
        assert_eq!(last, Pt { x: 0.0, y: 19.0 }, "and ends at the bottom left");
        assert!(
            (widest - 8.0).abs() < 0.001,
            "the bulge reaches the right edge, not {widest}"
        );
        assert!(middle >= 2, "the straight middle holds the widest point");
    }

    #[test]
    fn the_cap_bulges_a_quarter_of_the_way_out_a_quarter_of_the_way_down() {
        let points = polygon(0xe0b4);

        let quarter = points
            .iter()
            .find(|point| point.y > 4.0)
            .copied()
            .expect("a point a quarter down");
        assert!(
            quarter.x > 4.0,
            "{quarter:?} should already be past the middle of the cell"
        );
    }

    #[test]
    fn the_mirrored_cap_bulges_the_other_way() {
        let points = polygon(0xe0b6);

        assert_eq!(points[0], Pt { x: 8.0, y: 0.0 });
        let narrowest = points.iter().fold(8.0_f32, |near, point| near.min(point.x));
        assert!(narrowest < 0.001, "the bulge reaches the left edge");
    }

    #[test]
    fn the_hollow_cap_keeps_its_stroke_inside_the_cell() {
        let drawn = shapes(0xe0b5, CELL).expect("a Powerline glyph");

        let Shape::Stroke { points, width } = &drawn[0] else {
            panic!("a hollow cap is stroked, not {drawn:?}");
        };
        assert_eq!(*width, 1.0);
        assert!(
            points[0].y >= width * 0.5,
            "the top end sits inside the cell"
        );
        let widest = points.iter().fold(0.0_f32, |far, point| far.max(point.x));
        assert!(widest <= CELL.width, "the bulge stays inside the cell");
    }

    #[test]
    fn the_flame_draws_two_wedges_that_meet_at_the_middle() {
        let drawn = shapes(0xe0d2, CELL).expect("a Powerline glyph");

        assert_eq!(drawn.len(), 2);
        let Shape::Fill(top) = &drawn[0] else {
            panic!("a flame is filled, not {drawn:?}");
        };
        let Shape::Fill(bottom) = &drawn[1] else {
            panic!("a flame is filled, not {drawn:?}");
        };
        assert!(top[2].y < 9.5, "the top wedge stops above the middle");
        assert!(bottom[2].y > 9.5, "the bottom wedge starts below it");
        assert_eq!(top[2].x, 4.0, "they meet at the middle of the cell");
    }

    #[test]
    fn the_mirrored_flame_points_the_other_way() {
        let drawn = shapes(0xe0d4, CELL).expect("a Powerline glyph");

        let Shape::Fill(top) = &drawn[0] else {
            panic!("a flame is filled, not {drawn:?}");
        };
        assert_eq!(top[0], Pt { x: 8.0, y: 0.0 });
        assert_eq!(top[1], Pt { x: 0.0, y: 0.0 });
    }

    #[test]
    fn a_corner_wedge_covers_three_corners_of_the_cell() {
        let points = polygon(0xe0b8);

        assert_eq!(points[0], Pt { x: 0.0, y: 0.0 });
        assert_eq!(points[1], Pt { x: 8.0, y: 19.0 });
        assert_eq!(points[2], Pt { x: 0.0, y: 19.0 });
    }

    #[test]
    fn a_hypotenuse_glyph_strokes_one_diagonal() {
        let drawn = shapes(0xe0b9, CELL).expect("a Powerline glyph");

        let Shape::Stroke { points, .. } = &drawn[0] else {
            panic!("a hypotenuse is stroked, not {drawn:?}");
        };
        assert_eq!(points.len(), 2);
        assert!(points[0].x < points[1].x && points[0].y < points[1].y);
    }
}
