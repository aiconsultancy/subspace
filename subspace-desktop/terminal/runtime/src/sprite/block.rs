//! Block elements, U+2580 to U+259F.
//!
//! ```text
//! ▀▁▂▃▄▅▆▇█▉▊▋▌▍▎▏ ▐░▒▓▔▕▖▗▘▙▚▛▜▝▞▟
//! ```
//!
//! Ported from Ghostty's `src/font/sprite/draw/block.zig` (MIT, credited in
//! `licenses/terminal`). A block fills a share of its cell exactly, so a
//! progress bar or a sparkline drawn from them has no seams.

use crate::sprite::shape::{CellRect, HALF, OPAQUE, Shape, SpriteCell, shaded_cell};

/// An eighth of the cell.
const ONE_EIGHTH: f32 = 0.125;
/// A quarter of the cell.
const ONE_QUARTER: f32 = 0.25;
/// Three eighths of the cell.
const THREE_EIGHTHS: f32 = 0.375;
/// Five eighths of the cell.
const FIVE_EIGHTHS: f32 = 0.625;
/// Three quarters of the cell.
const THREE_QUARTERS: f32 = 0.75;
/// Seven eighths of the cell.
const SEVEN_EIGHTHS: f32 = 0.875;
/// The whole cell.
const FULL: f32 = 1.0;

/// How much of the foreground a light shade keeps: Ghostty's `0x40` of
/// `0xff`.
const LIGHT_SHADE: f32 = 64.0 / 255.0;
/// The same for a medium shade, Ghostty's `0x80`.
const MEDIUM_SHADE: f32 = 128.0 / 255.0;
/// The same for a dark shade, Ghostty's `0xc0`.
const DARK_SHADE: f32 = 192.0 / 255.0;

/// The cell's top left quarter.
const TL: u8 = 0b0001;
/// The cell's top right quarter.
const TR: u8 = 0b0010;
/// The cell's bottom left quarter.
const BL: u8 = 0b0100;
/// The cell's bottom right quarter.
const BR: u8 = 0b1000;

/// Which edge of the cell a partial block sits against.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Align {
    /// The top edge.
    Top,
    /// The bottom edge.
    Bottom,
    /// The left edge.
    Left,
    /// The right edge.
    Right,
}

/// One partial block: the edge it sits against and how much of each axis it
/// covers.
#[derive(Clone, Copy, Debug, PartialEq)]
struct BlockPart {
    /// The edge it is drawn from.
    align: Align,
    /// How much of the cell's width it covers.
    width: f32,
    /// How much of the cell's height it covers.
    height: f32,
}

/// One partial block, named for the table below.
const fn part(align: Align, width: f32, height: f32) -> BlockPart {
    BlockPart {
        align,
        width,
        height,
    }
}

/// Every partial block of the range.
#[rustfmt::skip]
const PARTS: &[(u32, BlockPart)] = &[
    (0x2580, part(Align::Top,    FULL, HALF)),           // '▀'
    (0x2581, part(Align::Bottom, FULL, ONE_EIGHTH)),     // '▁'
    (0x2582, part(Align::Bottom, FULL, ONE_QUARTER)),    // '▂'
    (0x2583, part(Align::Bottom, FULL, THREE_EIGHTHS)),  // '▃'
    (0x2584, part(Align::Bottom, FULL, HALF)),           // '▄'
    (0x2585, part(Align::Bottom, FULL, FIVE_EIGHTHS)),   // '▅'
    (0x2586, part(Align::Bottom, FULL, THREE_QUARTERS)), // '▆'
    (0x2587, part(Align::Bottom, FULL, SEVEN_EIGHTHS)),  // '▇'
    (0x2589, part(Align::Left,   SEVEN_EIGHTHS,  FULL)), // '▉'
    (0x258a, part(Align::Left,   THREE_QUARTERS, FULL)), // '▊'
    (0x258b, part(Align::Left,   FIVE_EIGHTHS,   FULL)), // '▋'
    (0x258c, part(Align::Left,   HALF,           FULL)), // '▌'
    (0x258d, part(Align::Left,   THREE_EIGHTHS,  FULL)), // '▍'
    (0x258e, part(Align::Left,   ONE_QUARTER,    FULL)), // '▎'
    (0x258f, part(Align::Left,   ONE_EIGHTH,     FULL)), // '▏'
    (0x2590, part(Align::Right,  HALF,           FULL)), // '▐'
    (0x2594, part(Align::Top,    FULL, ONE_EIGHTH)),     // '▔'
    (0x2595, part(Align::Right,  ONE_EIGHTH,     FULL)), // '▕'
];

/// Every block that fills the whole cell with part of the foreground.
const SHADES: &[(u32, f32)] = &[
    (0x2588, OPAQUE),       // '█'
    (0x2591, LIGHT_SHADE),  // '░'
    (0x2592, MEDIUM_SHADE), // '▒'
    (0x2593, DARK_SHADE),   // '▓'
];

/// Every block made of whole quarters of the cell.
#[rustfmt::skip]
const QUADRANTS: &[(u32, u8)] = &[
    (0x2596, BL),               // '▖'
    (0x2597, BR),               // '▗'
    (0x2598, TL),               // '▘'
    (0x2599, TL | BL | BR),     // '▙'
    (0x259a, TL | BR),          // '▚'
    (0x259b, TL | TR | BL),     // '▛'
    (0x259c, TL | TR | BR),     // '▜'
    (0x259d, TR),               // '▝'
    (0x259e, TR | BL),          // '▞'
    (0x259f, TR | BL | BR),     // '▟'
];

/// The shapes a block code point draws, and `None` for a code point this
/// module does not cover.
pub(crate) fn shapes(code_point: u32, cell: SpriteCell) -> Option<Vec<Shape>> {
    shaded(code_point, cell)
        .or_else(|| quartered(code_point, cell))
        .or_else(|| partial(code_point, cell))
}

/// The whole cell, in part of the foreground colour.
fn shaded(code_point: u32, cell: SpriteCell) -> Option<Vec<Shape>> {
    let found = SHADES
        .iter()
        .find(|(candidate, _)| *candidate == code_point)?;
    Some(vec![shaded_cell(cell, found.1)])
}

/// The quarters of the cell one quadrant block fills.
fn quartered(code_point: u32, cell: SpriteCell) -> Option<Vec<Shape>> {
    let found = QUADRANTS
        .iter()
        .find(|(candidate, _)| *candidate == code_point)?;
    Some(quadrant_shapes(found.1, cell))
}

/// The part of the cell one partial block fills.
fn partial(code_point: u32, cell: SpriteCell) -> Option<Vec<Shape>> {
    let found = PARTS
        .iter()
        .find(|(candidate, _)| *candidate == code_point)?;
    Some(vec![part_shape(found.1, cell)])
}

/// The rectangle one partial block covers.
fn part_shape(part: BlockPart, cell: SpriteCell) -> Shape {
    let width = cell.width * part.width;
    let height = cell.height * part.height;
    let x = match part.align {
        Align::Right => cell.width - width,
        _ => 0.0,
    };
    let y = match part.align {
        Align::Bottom => cell.height - height,
        _ => 0.0,
    };
    Shape::Rect(CellRect {
        x0: x,
        y0: y,
        x1: x + width,
        y1: y + height,
        alpha: OPAQUE,
    })
}

/// One rectangle for each quarter a quadrant block fills.
fn quadrant_shapes(quarters: u8, cell: SpriteCell) -> Vec<Shape> {
    let middle = (cell.middle_x(), cell.middle_y());
    let corners = [
        (TL, 0.0, 0.0),
        (TR, middle.0, 0.0),
        (BL, 0.0, middle.1),
        (BR, middle.0, middle.1),
    ];
    corners
        .iter()
        .filter(|(quarter, _, _)| quarters & quarter != 0)
        .map(|(_, x, y)| quarter_shape(*x, *y, middle))
        .collect()
}

/// The rectangle one quarter of the cell covers.
fn quarter_shape(x: f32, y: f32, size: (f32, f32)) -> Shape {
    Shape::Rect(CellRect {
        x0: x,
        y0: y,
        x1: x + size.0,
        y1: y + size.1,
        alpha: OPAQUE,
    })
}

#[cfg(test)]
mod tests {
    use crate::sprite::block::shapes;
    use crate::sprite::shape::{CellRect, Shape, SpriteCell};

    /// A cell of plausible size for a 13 point font with a fifth more height.
    const CELL: SpriteCell = SpriteCell {
        width: 8.0,
        height: 19.0,
        thickness: 1.0,
    };

    /// The rectangles one block glyph draws.
    fn rects(code_point: u32) -> Vec<CellRect> {
        shapes(code_point, CELL)
            .expect("a block glyph")
            .into_iter()
            .map(|shape| match shape {
                Shape::Rect(rect) => rect,
                other => panic!("{code_point:#x} draws rectangles, not {other:?}"),
            })
            .collect()
    }

    #[test]
    fn a_code_point_outside_the_range_draws_nothing() {
        assert!(shapes(0x257f, CELL).is_none());
        assert!(shapes(0x25a0, CELL).is_none());
    }

    #[test]
    fn the_full_block_covers_the_whole_cell() {
        let drawn = rects(0x2588);

        assert_eq!(drawn.len(), 1);
        assert_eq!((drawn[0].x0, drawn[0].y0), (0.0, 0.0));
        assert_eq!((drawn[0].x1, drawn[0].y1), (CELL.width, CELL.height));
        assert_eq!(drawn[0].alpha, 1.0);
    }

    #[test]
    fn the_lower_half_block_covers_the_bottom_half_of_the_cell() {
        let drawn = rects(0x2584);

        assert_eq!((drawn[0].x0, drawn[0].x1), (0.0, CELL.width));
        assert_eq!((drawn[0].y0, drawn[0].y1), (9.5, 19.0));
    }

    #[test]
    fn the_upper_half_block_covers_the_top_half() {
        let drawn = rects(0x2580);

        assert_eq!((drawn[0].y0, drawn[0].y1), (0.0, 9.5));
    }

    #[test]
    fn the_eighths_divide_the_cell_evenly_from_their_own_edge() {
        let lower_eighth = rects(0x2581);
        let left_eighth = rects(0x258f);
        let right_eighth = rects(0x2595);

        assert_eq!(lower_eighth[0].y0, 19.0 - 19.0 / 8.0);
        assert_eq!((left_eighth[0].x0, left_eighth[0].x1), (0.0, 1.0));
        assert_eq!((right_eighth[0].x0, right_eighth[0].x1), (7.0, 8.0));
    }

    #[test]
    fn a_shade_covers_the_whole_cell_with_part_of_the_colour() {
        let light = rects(0x2591);
        let medium = rects(0x2592);
        let dark = rects(0x2593);

        assert_eq!((light[0].x1, light[0].y1), (CELL.width, CELL.height));
        assert!(light[0].alpha > 0.0 && light[0].alpha < medium[0].alpha);
        assert!(medium[0].alpha < dark[0].alpha);
        assert!(dark[0].alpha < 1.0, "a dark shade is not the full block");
    }

    #[test]
    fn a_quadrant_block_fills_the_quarters_it_names() {
        let bottom_left = rects(0x2596);
        let three = rects(0x2599);

        assert_eq!(bottom_left.len(), 1);
        assert_eq!(
            (
                bottom_left[0].x0,
                bottom_left[0].y0,
                bottom_left[0].x1,
                bottom_left[0].y1
            ),
            (0.0, 9.5, 4.0, 19.0)
        );
        assert_eq!(three.len(), 3, "'▙' fills three quarters");
    }

    #[test]
    fn every_code_point_of_the_range_draws_something() {
        let empty: Vec<u32> = (0x2580..=0x259f)
            .filter(|code| shapes(*code, CELL).is_none_or(|drawn| drawn.is_empty()))
            .collect();

        assert_eq!(empty, Vec::<u32>::new(), "no block glyph is missing");
    }
}
