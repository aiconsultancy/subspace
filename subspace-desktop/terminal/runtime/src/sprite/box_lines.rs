//! The strokes one box-drawing intersection glyph is drawn from.
//!
//! Ported from `linesChar` in Ghostty's `src/font/sprite/draw/box.zig` (MIT,
//! credited in `licenses/terminal`). Every arm runs from its cell edge to
//! the middle, so the same glyph in the cell above or beside it joins with
//! no gap however tall `adjust-cell-height` makes the cell.
//!
//! The two axes are drawn by the same code: on each one, the arm from the
//! low edge (the top or the left) and the arm from the high edge cross the
//! pair of arms on the other axis.

use crate::sprite::box_table::{ArmStyle, Lines};
use crate::sprite::shape::{Axis, Shape, SpriteCell, Weight, centred, stroke};

/// The edges one weight of stroke sits between, on one axis.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Pairs {
    /// The edges of a light stroke.
    light: (f32, f32),
    /// The edges of a heavy stroke.
    heavy: (f32, f32),
    /// The outer edges of a doubled stroke.
    double: (f32, f32),
}

/// Which edge of a crossing stroke an arm stops at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum End {
    /// Its top or left edge, which the arm from the high edge stops at.
    Low,
    /// Its bottom or right edge, which the arm from the low edge stops at.
    High,
}

/// What decides where an arm stops: the styles of the two arms it crosses,
/// the styles of the pair it belongs to, and where those crossing strokes
/// sit in the cell.
struct Crossing<'a> {
    /// The styles of the two arms it crosses.
    cross: (ArmStyle, ArmStyle),
    /// The styles of the two arms on its own axis.
    along: (ArmStyle, ArmStyle),
    /// Where the strokes it crosses sit.
    pairs: &'a Pairs,
}

/// One arm, ready to draw.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Arm {
    /// How the arm is styled.
    style: ArmStyle,
    /// The cell edge it starts at.
    outer: f32,
    /// Where it stops in the middle of the cell.
    inner: f32,
    /// Where its two doubled strokes stop, which a doubled crossing arm
    /// cuts short so the four strokes meet in a square.
    cuts: (f32, f32),
}

/// The strokes of one intersection glyph.
pub(crate) fn line_shapes(lines: Lines, cell: SpriteCell) -> Vec<Shape> {
    let across = pairs(cell.height, cell);
    let down = pairs(cell.width, cell);
    let vertical = (lines.up, lines.down);
    let horizontal = (lines.left, lines.right);
    let mut shapes = axis_shapes(
        vertical,
        horizontal,
        (&across, &down),
        (Axis::Vertical, cell.height),
    );
    shapes.extend(axis_shapes(
        horizontal,
        vertical,
        (&down, &across),
        (Axis::Horizontal, cell.width),
    ));
    shapes
}

/// The edges of a light, a heavy and a doubled stroke across `size`. A
/// doubled stroke is a light stroke either side of a light gap, so its
/// outer edges sit one thickness beyond the light ones.
fn pairs(size: f32, cell: SpriteCell) -> Pairs {
    let light = centred(size, Weight::Light.points(cell));
    Pairs {
        light,
        heavy: centred(size, Weight::Heavy.points(cell)),
        double: (light.0 - cell.thickness, light.1 + cell.thickness),
    }
}

/// The strokes of the two arms on one axis.
///
/// `pair` is the arm from the low edge of the cell and the arm from the
/// high one, `cross` the pair they cross, `stops` where the crossed strokes
/// sit and where their own sit, and `run` the axis they lie on with the
/// size of the cell along it.
fn axis_shapes(
    pair: (ArmStyle, ArmStyle),
    cross: (ArmStyle, ArmStyle),
    stops: (&Pairs, &Pairs),
    run: (Axis, f32),
) -> Vec<Shape> {
    let crossing = Crossing {
        cross,
        along: pair,
        pairs: stops.0,
    };
    let mut shapes = arm_shapes(&arm(pair.0, 0.0, End::High, &crossing), stops.1, run.0);
    shapes.extend(arm_shapes(
        &arm(pair.1, run.1, End::Low, &crossing),
        stops.1,
        run.0,
    ));
    shapes
}

/// One arm, from its cell edge to where the strokes it crosses let it stop.
fn arm(style: ArmStyle, outer: f32, end: End, crossing: &Crossing<'_>) -> Arm {
    let inner = edge(arm_stop(crossing), end);
    let near = edge(flipped(crossing.pairs.light), end);
    Arm {
        style,
        outer,
        inner,
        cuts: (
            cut(crossing.cross.0, near, inner),
            cut(crossing.cross.1, near, inner),
        ),
    }
}

/// The pair of edges an arm stops between, from Ghostty's `linesChar`: the
/// heavy or doubled stroke it crosses, the light one it crosses, or that
/// light one flipped, which stops the arm short of it.
fn arm_stop(crossing: &Crossing<'_>) -> (f32, f32) {
    let (first, second) = crossing.cross;
    let stops = crossing.pairs;
    if first == ArmStyle::Heavy || second == ArmStyle::Heavy {
        return stops.heavy;
    }
    if first != second || crossing.along.0 == crossing.along.1 {
        if first == ArmStyle::Double || second == ArmStyle::Double {
            return stops.double;
        }
        return stops.light;
    }
    if first == ArmStyle::None && second == ArmStyle::None {
        return stops.light;
    }
    flipped(stops.light)
}

/// A pair of edges the other way round, so an arm stops at the near one.
fn flipped(pair: (f32, f32)) -> (f32, f32) {
    (pair.1, pair.0)
}

/// One edge of a pair of stops.
fn edge(pair: (f32, f32), end: End) -> f32 {
    match end {
        End::Low => pair.0,
        End::High => pair.1,
    }
}

/// Where one of a doubled arm's two strokes stops: short of a doubled
/// crossing arm, so the four strokes meet in a square, and at the arm's own
/// stop otherwise.
fn cut(crossing: ArmStyle, near: f32, inner: f32) -> f32 {
    match crossing {
        ArmStyle::Double => near,
        _ => inner,
    }
}

/// The strokes of one arm, from the cell edge to the middle.
fn arm_shapes(arm: &Arm, own: &Pairs, axis: Axis) -> Vec<Shape> {
    match arm.style {
        ArmStyle::None => Vec::new(),
        ArmStyle::Light => vec![stroke(axis, (arm.outer, arm.inner), own.light)],
        ArmStyle::Heavy => vec![stroke(axis, (arm.outer, arm.inner), own.heavy)],
        ArmStyle::Double => vec![
            stroke(axis, (arm.outer, arm.cuts.0), (own.double.0, own.light.0)),
            stroke(axis, (arm.outer, arm.cuts.1), (own.light.1, own.double.1)),
        ],
    }
}

#[cfg(test)]
mod tests {
    use crate::sprite::box_lines::line_shapes;
    use crate::sprite::box_table::lines_of;
    use crate::sprite::shape::{CellRect, Shape, SpriteCell};

    /// A cell of plausible size for a 13 point font with a fifth more height.
    const CELL: SpriteCell = SpriteCell {
        width: 8.0,
        height: 19.0,
        thickness: 1.0,
    };

    /// The rectangles one intersection glyph draws.
    fn rects(code_point: u32) -> Vec<CellRect> {
        let lines = lines_of(code_point).expect("an intersection glyph");
        line_shapes(lines, CELL)
            .into_iter()
            .map(|shape| match shape {
                Shape::Rect(rect) => rect,
                other => panic!("{code_point:#x} draws rectangles, not {other:?}"),
            })
            .collect()
    }

    #[test]
    fn the_light_horizontal_runs_edge_to_edge_through_the_middle() {
        let drawn = rects(0x2500);

        assert_eq!(drawn.len(), 2, "one arm each way");
        let left = drawn
            .iter()
            .find(|rect| rect.x0 == 0.0)
            .expect("a left arm");
        let right = drawn
            .iter()
            .find(|rect| rect.x1 == CELL.width)
            .expect("a right arm");
        assert_eq!((left.y0, left.y1), (9.0, 10.0), "centred on the cell");
        assert_eq!((right.y0, right.y1), (9.0, 10.0));
        assert!(
            left.x1 >= right.x0,
            "the two arms cross the middle without a gap"
        );
    }

    #[test]
    fn the_light_vertical_runs_top_to_bottom_through_the_middle() {
        let drawn = rects(0x2502);

        let up = drawn.iter().find(|rect| rect.y0 == 0.0).expect("an up arm");
        let down = drawn
            .iter()
            .find(|rect| rect.y1 == CELL.height)
            .expect("a down arm");
        assert_eq!((up.x0, up.x1), (3.5, 4.5), "centred on the cell");
        assert!(
            up.y1 >= down.y0,
            "the two arms cross the middle without a gap"
        );
        assert_eq!(down.y1, CELL.height, "the line reaches the row below");
    }

    #[test]
    fn a_heavy_line_is_twice_as_thick_as_a_light_one() {
        let light = rects(0x2500);
        let heavy = rects(0x2501);

        assert_eq!(light[0].y1 - light[0].y0, 1.0);
        assert_eq!(heavy[0].y1 - heavy[0].y0, 2.0);
    }

    #[test]
    fn a_corner_draws_one_arm_each_way_and_nothing_else() {
        let drawn = rects(0x250c);

        assert_eq!(drawn.len(), 2, "'┌' has a down arm and a right arm");
        assert!(drawn.iter().any(|rect| rect.y1 == CELL.height));
        assert!(drawn.iter().any(|rect| rect.x1 == CELL.width));
        assert!(
            !drawn.iter().any(|rect| rect.y0 == 0.0),
            "nothing reaches the top edge"
        );
    }

    #[test]
    fn a_doubled_line_draws_two_strokes_either_side_of_a_gap() {
        let drawn = rects(0x2550);

        assert_eq!(drawn.len(), 4, "two strokes for each arm");
        let top = drawn
            .iter()
            .find(|rect| rect.y0 < 9.0)
            .expect("a top stroke");
        let bottom = drawn
            .iter()
            .find(|rect| rect.y0 > 9.5)
            .expect("a bottom stroke");
        assert_eq!((top.y0, top.y1), (8.0, 9.0));
        assert_eq!((bottom.y0, bottom.y1), (10.0, 11.0));
    }

    #[test]
    fn a_half_line_stops_in_the_middle_of_the_cell() {
        let drawn = rects(0x2574);

        assert_eq!(drawn.len(), 1, "'╴' has a left arm only");
        assert_eq!(drawn[0].x0, 0.0);
        assert_eq!(drawn[0].x1, 4.5, "it crosses the middle of the cell");
    }

    #[test]
    fn an_arm_crossing_a_heavy_stroke_reaches_its_far_edge() {
        let drawn = rects(0x2540);

        let left = drawn
            .iter()
            .find(|rect| rect.x0 == 0.0)
            .expect("'╀' has a left arm");
        let down = drawn
            .iter()
            .find(|rect| rect.y1 == CELL.height)
            .expect("'╀' has a light down arm");
        assert_eq!(left.x1, 5.0, "the light arm crosses the whole heavy stroke");
        assert_eq!(
            down.y0, 10.0,
            "the light arm below stops short of the crossing stroke"
        );
    }

    #[test]
    fn every_glyph_of_the_table_stays_inside_its_cell() {
        let outside = (0x2500..=0x257f)
            .filter_map(lines_of)
            .flat_map(|lines| line_shapes(lines, CELL))
            .filter(|shape| !inside(shape))
            .count();

        assert_eq!(outside, 0, "no stroke may leave the cell");
    }

    /// Whether one shape sits inside the test cell.
    fn inside(shape: &Shape) -> bool {
        let Shape::Rect(rect) = shape else {
            return false;
        };
        rect.x0 >= 0.0 && rect.y0 >= 0.0 && rect.x1 <= CELL.width && rect.y1 <= CELL.height
    }
}
