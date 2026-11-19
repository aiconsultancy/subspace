//! Box drawing, U+2500 to U+257F.
//!
//! ```text
//! ─━│┃┄┅┆┇┈┉┊┋┌┍┎┏ ┐┑┒┓└┕┖┗┘┙┚┛├┝┞┟
//! ┠┡┢┣┤┥┦┧┨┩┪┫┬┭┮┯ ┰┱┲┳┴┵┶┷┸┹┺┻┼┽┾┿
//! ╀╁╂╃╄╅╆╇╈╉╊╋╌╍╎╏ ═║╒╓╔╕╖╗╘╙╚╛╜╝╞╟
//! ╠╡╢╣╤╥╦╧╨╩╪╫╬╭╮╯ ╰╱╲╳╴╵╶╷╸╹╺╻╼╽╾╿
//! ```
//!
//! Ported from Ghostty's `src/font/sprite/draw/box.zig` (MIT, credited in
//! `licenses/terminal`). The intersection glyphs are
//! [`crate::sprite::box_lines`]; the dashed, rounded and diagonal ones are
//! here.

use crate::sprite::box_lines::line_shapes;
use crate::sprite::box_table::lines_of;
use crate::sprite::shape::{
    Axis, HALF, Shape, Slant, SpriteCell, Weight, centred, curve, diagonal, pt, stroke,
};

/// The first code point of the block.
const FIRST: u32 = 0x2500;
/// The last code point of the block.
const LAST: u32 = 0x257f;
/// The gap a widely dashed glyph aims for, in points, before the cell is
/// too narrow to hold it.
const WIDE_GAP: f32 = 4.0;
/// A gap never takes more than this share of the cell, or the dashes grow
/// too short to read as a line.
const MAX_GAP_SHARE: f32 = 0.5;
/// How far from the centre a rounded corner's control points sit, as a
/// share of the radius. Ghostty's `s`.
const ARC_CONTROL: f32 = 0.25;
/// An arm that leaves the middle toward the right or the bottom edge.
const TOWARD_END: f32 = 1.0;
/// An arm that leaves it toward the left or the top edge.
const TOWARD_START: f32 = -1.0;

/// How wide a dashed glyph wants its gaps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Gap {
    /// Four points, or less in a narrow cell.
    Wide,
    /// One light stroke.
    Light,
    /// One heavy stroke.
    Heavy,
}

/// One dashed glyph.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Dash {
    /// Which way the line runs.
    axis: Axis,
    /// How many dashes it holds.
    count: u8,
    /// How thick each dash is.
    weight: Weight,
    /// How wide the gaps between them are.
    gap: Gap,
}

/// Every dashed glyph of the block: which way it runs, how many dashes it
/// holds, how thick they are and how wide the gaps between them are.
#[rustfmt::skip]
const DASHES: &[(u32, Axis, u8, Weight, Gap)] = &[
    (0x2504, Axis::Horizontal, 3, Weight::Light, Gap::Wide),  // '┄'
    (0x2505, Axis::Horizontal, 3, Weight::Heavy, Gap::Wide),  // '┅'
    (0x2506, Axis::Vertical,   3, Weight::Light, Gap::Wide),  // '┆'
    (0x2507, Axis::Vertical,   3, Weight::Heavy, Gap::Wide),  // '┇'
    (0x2508, Axis::Horizontal, 4, Weight::Light, Gap::Wide),  // '┈'
    (0x2509, Axis::Horizontal, 4, Weight::Heavy, Gap::Wide),  // '┉'
    (0x250a, Axis::Vertical,   4, Weight::Light, Gap::Wide),  // '┊'
    (0x250b, Axis::Vertical,   4, Weight::Heavy, Gap::Wide),  // '┋'
    (0x254c, Axis::Horizontal, 2, Weight::Light, Gap::Light), // '╌'
    (0x254d, Axis::Horizontal, 2, Weight::Heavy, Gap::Heavy), // '╍'
    (0x254e, Axis::Vertical,   2, Weight::Light, Gap::Heavy), // '╎'
    (0x254f, Axis::Vertical,   2, Weight::Heavy, Gap::Heavy), // '╏'
];

/// Which two edges a rounded corner joins.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Corner {
    /// The top and left edges.
    TopLeft,
    /// The top and right edges.
    TopRight,
    /// The bottom and left edges.
    BottomLeft,
    /// The bottom and right edges.
    BottomRight,
}

/// Every rounded corner of the block.
const CORNERS: &[(u32, Corner)] = &[
    (0x256d, Corner::BottomRight), // '╭'
    (0x256e, Corner::BottomLeft),  // '╮'
    (0x256f, Corner::TopLeft),     // '╯'
    (0x2570, Corner::TopRight),    // '╰'
];

/// Every diagonal of the block, and the slants it is drawn from.
const DIAGONALS: &[(u32, &[Slant])] = &[
    (0x2571, &[Slant::Up]),              // '╱'
    (0x2572, &[Slant::Down]),            // '╲'
    (0x2573, &[Slant::Up, Slant::Down]), // '╳'
];

/// The shapes a box-drawing code point draws, and `None` for a code point
/// outside the block.
pub(crate) fn shapes(code_point: u32, cell: SpriteCell) -> Option<Vec<Shape>> {
    if !(FIRST..=LAST).contains(&code_point) {
        return None;
    }
    Some(figure(code_point, cell))
}

/// The shapes one code point of the block draws: a dashed line, a rounded
/// corner, a diagonal, or an intersection of arms. Every code point of the
/// block is one of the four, which a test checks.
fn figure(code_point: u32, cell: SpriteCell) -> Vec<Shape> {
    dashed(code_point, cell)
        .or_else(|| rounded(code_point, cell))
        .or_else(|| crossing(code_point, cell))
        .or_else(|| intersection(code_point, cell))
        .unwrap_or_default()
}

/// The dashes of a dashed glyph.
fn dashed(code_point: u32, cell: SpriteCell) -> Option<Vec<Shape>> {
    let found = DASHES
        .iter()
        .find(|(candidate, ..)| *candidate == code_point)?;
    let spec = Dash {
        axis: found.1,
        count: found.2,
        weight: found.3,
        gap: found.4,
    };
    Some(dash_shapes(spec, cell))
}

/// The stroke of a rounded corner.
fn rounded(code_point: u32, cell: SpriteCell) -> Option<Vec<Shape>> {
    let found = CORNERS
        .iter()
        .find(|(candidate, _)| *candidate == code_point)?;
    Some(vec![arc_shape(found.1, cell)])
}

/// The strokes of a diagonal glyph.
fn crossing(code_point: u32, cell: SpriteCell) -> Option<Vec<Shape>> {
    let found = DIAGONALS
        .iter()
        .find(|(candidate, _)| *candidate == code_point)?;
    Some(found.1.iter().map(|slant| diagonal(*slant, cell)).collect())
}

/// The strokes of an intersection glyph.
fn intersection(code_point: u32, cell: SpriteCell) -> Option<Vec<Shape>> {
    lines_of(code_point).map(|lines| line_shapes(lines, cell))
}

/// The dashes of one dashed glyph, laid out so the same glyph in the next
/// cell continues the line evenly.
///
/// A horizontal line keeps half a gap at each end, which centres its dashes.
/// A vertical one keeps a whole gap at the bottom, which joins better to a
/// solid glyph below it.
fn dash_shapes(spec: Dash, cell: SpriteCell) -> Vec<Shape> {
    let along = dash_length(spec.axis, cell);
    let across = centred(dash_width(spec.axis, cell), spec.weight.points(cell));
    let count = f32::from(spec.count);
    let gap = gap_points(spec.gap, cell).min(along * MAX_GAP_SHARE / count);
    let length = (along - gap * count) / count;
    if length <= 0.0 {
        return vec![stroke(spec.axis, (0.0, along), across)];
    }
    let start = dash_start(spec.axis, gap);
    (0..spec.count)
        .map(|index| {
            let at = start + f32::from(index) * (length + gap);
            stroke(spec.axis, (at, at + length), across)
        })
        .collect()
}

/// How long the cell is along a dashed line.
fn dash_length(axis: Axis, cell: SpriteCell) -> f32 {
    match axis {
        Axis::Vertical => cell.height,
        Axis::Horizontal => cell.width,
    }
}

/// How wide the cell is across a dashed line.
fn dash_width(axis: Axis, cell: SpriteCell) -> f32 {
    match axis {
        Axis::Vertical => cell.width,
        Axis::Horizontal => cell.height,
    }
}

/// Where the first dash of a line starts.
fn dash_start(axis: Axis, gap: f32) -> f32 {
    match axis {
        Axis::Vertical => 0.0,
        Axis::Horizontal => gap * HALF,
    }
}

/// How wide the gaps of a dashed glyph are, before the cell narrows them.
fn gap_points(gap: Gap, cell: SpriteCell) -> f32 {
    match gap {
        Gap::Wide => WIDE_GAP.max(cell.thickness),
        Gap::Light => Weight::Light.points(cell),
        Gap::Heavy => Weight::Heavy.points(cell),
    }
}

/// The stroke of one rounded corner: from one cell edge to the middle,
/// round the corner, and out to the other edge.
fn arc_shape(corner: Corner, cell: SpriteCell) -> Shape {
    let (toward_x, toward_y) = corner_direction(corner);
    let middle = pt(cell.middle_x(), cell.middle_y());
    let radius = cell.width.min(cell.height) * HALF;
    let turn = pt(middle.x, middle.y + toward_y * radius);
    let reach = radius * ARC_CONTROL;
    let mut points = vec![pt(middle.x, middle.y + toward_y * cell.middle_y()), turn];
    let end = pt(middle.x + toward_x * radius, middle.y);
    points.extend(curve(
        turn,
        pt(middle.x, middle.y + toward_y * reach),
        pt(middle.x + toward_x * reach, middle.y),
        end,
    ));
    points.push(pt(middle.x + toward_x * cell.middle_x(), middle.y));
    Shape::Stroke {
        points,
        width: Weight::Light.points(cell),
    }
}

/// Which way the two arms of a rounded corner leave the middle.
fn corner_direction(corner: Corner) -> (f32, f32) {
    match corner {
        Corner::TopLeft => (TOWARD_START, TOWARD_START),
        Corner::TopRight => (TOWARD_END, TOWARD_START),
        Corner::BottomLeft => (TOWARD_START, TOWARD_END),
        Corner::BottomRight => (TOWARD_END, TOWARD_END),
    }
}

#[cfg(test)]
mod tests {
    use crate::sprite::box_drawing::shapes;
    use crate::sprite::shape::{CellRect, Pt, Shape, SpriteCell, pt};

    /// The far end of a stroked glyph's points.
    fn last(points: &[Pt]) -> Pt {
        points.last().copied().unwrap_or(pt(0.0, 0.0))
    }

    /// A cell of plausible size for a 13 point font with a fifth more height.
    const CELL: SpriteCell = SpriteCell {
        width: 8.0,
        height: 19.0,
        thickness: 1.0,
    };

    /// The rectangles one glyph draws.
    fn rects(code_point: u32) -> Vec<CellRect> {
        shapes(code_point, CELL)
            .expect("a box-drawing glyph")
            .into_iter()
            .filter_map(|shape| match shape {
                Shape::Rect(rect) => Some(rect),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_code_point_outside_the_block_draws_nothing() {
        assert!(shapes(0x24ff, CELL).is_none());
        assert!(shapes(0x2580, CELL).is_none());
    }

    #[test]
    fn the_light_horizontal_crosses_the_whole_cell() {
        let drawn = rects(0x2500);

        let left = drawn.iter().map(|rect| rect.x0).fold(f32::MAX, f32::min);
        let right = drawn.iter().map(|rect| rect.x1).fold(0.0_f32, f32::max);
        assert_eq!((left, right), (0.0, CELL.width));
    }

    #[test]
    fn a_dashed_line_draws_one_stroke_for_each_dash() {
        let three = rects(0x2504);
        let four = rects(0x2508);

        assert_eq!(three.len(), 3);
        assert_eq!(four.len(), 4);
        assert!(
            three[0].x0 > 0.0,
            "a horizontal line keeps half a gap at each end"
        );
        assert!(three[2].x1 < CELL.width);
    }

    #[test]
    fn a_dashed_vertical_starts_at_the_top_edge() {
        let drawn = rects(0x2506);

        assert_eq!(drawn.len(), 3);
        assert_eq!(drawn[0].y0, 0.0);
        assert!(drawn[2].y1 < CELL.height, "the gap sits at the bottom");
    }

    #[test]
    fn a_rounded_corner_leaves_the_middle_for_two_edges() {
        let drawn = shapes(0x256d, CELL).expect("'╭'");

        let Shape::Stroke { points, width } = &drawn[0] else {
            panic!("a rounded corner is stroked, not {drawn:?}");
        };
        assert_eq!(*width, 1.0);
        assert_eq!(points[0], pt(4.0, 19.0), "it starts at the bottom");
        assert_eq!(last(points).x, CELL.width, "and ends at the right edge");
        assert!(
            (last(points).y - 9.5).abs() < 0.001,
            "the arm leaves at the middle of the cell"
        );
    }

    #[test]
    fn a_rounded_corner_curves_rather_than_turning_square() {
        let drawn = shapes(0x256d, CELL).expect("'╭'");

        let Shape::Stroke { points, .. } = &drawn[0] else {
            panic!("a rounded corner is stroked");
        };
        let turning = points
            .iter()
            .filter(|point| point.x > 4.0 && point.y > 9.5 && point.y < 13.5)
            .count();
        assert!(
            turning >= 2,
            "the corner is rounded, not mitred: {points:?}"
        );
    }

    #[test]
    fn the_other_corners_leave_the_middle_the_other_ways() {
        let up_right = shapes(0x2570, CELL).expect("'╰'");

        let Shape::Stroke { points, .. } = &up_right[0] else {
            panic!("a rounded corner is stroked");
        };
        assert_eq!(points[0].y, 0.0, "'╰' starts at the top");
        assert_eq!(last(points).x, CELL.width, "and ends at the right");
    }

    #[test]
    fn a_cross_of_diagonals_draws_both_slants() {
        let one = shapes(0x2571, CELL).expect("'╱'");
        let both = shapes(0x2573, CELL).expect("'╳'");

        assert_eq!(one.len(), 1);
        assert_eq!(both.len(), 2);
    }

    #[test]
    fn every_code_point_of_the_block_draws_something() {
        let empty: Vec<u32> = (0x2500..=0x257f)
            .filter(|code| shapes(*code, CELL).is_none_or(|drawn| drawn.is_empty()))
            .collect();

        assert_eq!(empty, Vec::<u32>::new(), "no glyph of the block is missing");
    }
}
