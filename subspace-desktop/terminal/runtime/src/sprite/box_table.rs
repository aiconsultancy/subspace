//! Which arms every box-drawing intersection glyph has, U+2500 to U+257F.
//!
//! The table is ported from the `linesChar` arms of Ghostty's
//! `src/font/sprite/draw/box.zig` (MIT, credited in `licenses/terminal`).
//! The dashed, rounded and diagonal glyphs of the block are not
//! intersections and are drawn by [`crate::sprite::box_drawing`] instead.

/// One arm's line style.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ArmStyle {
    /// The arm is absent.
    None,
    /// One stroke of the cell's own thickness.
    Light,
    /// One stroke of twice that.
    Heavy,
    /// Two light strokes with a light gap between them.
    Double,
}

use ArmStyle::{Double as DOUBLE, Heavy as HEAVY, Light as LIGHT, None as NONE};

/// The four arms of one intersection glyph.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Lines {
    /// The arm toward the top edge.
    pub(crate) up: ArmStyle,
    /// The arm toward the right edge.
    pub(crate) right: ArmStyle,
    /// The arm toward the bottom edge.
    pub(crate) down: ArmStyle,
    /// The arm toward the left edge.
    pub(crate) left: ArmStyle,
}

/// The arms of one intersection glyph, and `None` for a code point that is
/// not one.
pub(crate) fn lines_of(code_point: u32) -> Option<Lines> {
    let found = LINES
        .binary_search_by_key(&code_point, |(candidate, _)| *candidate)
        .ok()?;
    let [up, right, down, left] = LINES.get(found)?.1;
    Some(Lines {
        up,
        right,
        down,
        left,
    })
}

/// Every intersection glyph of the block, with its arms clockwise from the
/// top, in code-point order so a lookup can bisect it.
#[rustfmt::skip]
const LINES: &[(u32, [ArmStyle; 4])] = &[
    (0x2500, [NONE, LIGHT, NONE, LIGHT]), // '─'
    (0x2501, [NONE, HEAVY, NONE, HEAVY]), // '━'
    (0x2502, [LIGHT, NONE, LIGHT, NONE]), // '│'
    (0x2503, [HEAVY, NONE, HEAVY, NONE]), // '┃'
    (0x250c, [NONE, LIGHT, LIGHT, NONE]), // '┌'
    (0x250d, [NONE, HEAVY, LIGHT, NONE]), // '┍'
    (0x250e, [NONE, LIGHT, HEAVY, NONE]), // '┎'
    (0x250f, [NONE, HEAVY, HEAVY, NONE]), // '┏'
    (0x2510, [NONE, NONE, LIGHT, LIGHT]), // '┐'
    (0x2511, [NONE, NONE, LIGHT, HEAVY]), // '┑'
    (0x2512, [NONE, NONE, HEAVY, LIGHT]), // '┒'
    (0x2513, [NONE, NONE, HEAVY, HEAVY]), // '┓'
    (0x2514, [LIGHT, LIGHT, NONE, NONE]), // '└'
    (0x2515, [LIGHT, HEAVY, NONE, NONE]), // '┕'
    (0x2516, [HEAVY, LIGHT, NONE, NONE]), // '┖'
    (0x2517, [HEAVY, HEAVY, NONE, NONE]), // '┗'
    (0x2518, [LIGHT, NONE, NONE, LIGHT]), // '┘'
    (0x2519, [LIGHT, NONE, NONE, HEAVY]), // '┙'
    (0x251a, [HEAVY, NONE, NONE, LIGHT]), // '┚'
    (0x251b, [HEAVY, NONE, NONE, HEAVY]), // '┛'
    (0x251c, [LIGHT, LIGHT, LIGHT, NONE]), // '├'
    (0x251d, [LIGHT, HEAVY, LIGHT, NONE]), // '┝'
    (0x251e, [HEAVY, LIGHT, LIGHT, NONE]), // '┞'
    (0x251f, [LIGHT, LIGHT, HEAVY, NONE]), // '┟'
    (0x2520, [HEAVY, LIGHT, HEAVY, NONE]), // '┠'
    (0x2521, [HEAVY, HEAVY, LIGHT, NONE]), // '┡'
    (0x2522, [LIGHT, HEAVY, HEAVY, NONE]), // '┢'
    (0x2523, [HEAVY, HEAVY, HEAVY, NONE]), // '┣'
    (0x2524, [LIGHT, NONE, LIGHT, LIGHT]), // '┤'
    (0x2525, [LIGHT, NONE, LIGHT, HEAVY]), // '┥'
    (0x2526, [HEAVY, NONE, LIGHT, LIGHT]), // '┦'
    (0x2527, [LIGHT, NONE, HEAVY, LIGHT]), // '┧'
    (0x2528, [HEAVY, NONE, HEAVY, LIGHT]), // '┨'
    (0x2529, [HEAVY, NONE, LIGHT, HEAVY]), // '┩'
    (0x252a, [LIGHT, NONE, HEAVY, HEAVY]), // '┪'
    (0x252b, [HEAVY, NONE, HEAVY, HEAVY]), // '┫'
    (0x252c, [NONE, LIGHT, LIGHT, LIGHT]), // '┬'
    (0x252d, [NONE, LIGHT, LIGHT, HEAVY]), // '┭'
    (0x252e, [NONE, HEAVY, LIGHT, LIGHT]), // '┮'
    (0x252f, [NONE, HEAVY, LIGHT, HEAVY]), // '┯'
    (0x2530, [NONE, LIGHT, HEAVY, LIGHT]), // '┰'
    (0x2531, [NONE, LIGHT, HEAVY, HEAVY]), // '┱'
    (0x2532, [NONE, HEAVY, HEAVY, LIGHT]), // '┲'
    (0x2533, [NONE, HEAVY, HEAVY, HEAVY]), // '┳'
    (0x2534, [LIGHT, LIGHT, NONE, LIGHT]), // '┴'
    (0x2535, [LIGHT, LIGHT, NONE, HEAVY]), // '┵'
    (0x2536, [LIGHT, HEAVY, NONE, LIGHT]), // '┶'
    (0x2537, [LIGHT, HEAVY, NONE, HEAVY]), // '┷'
    (0x2538, [HEAVY, LIGHT, NONE, LIGHT]), // '┸'
    (0x2539, [HEAVY, LIGHT, NONE, HEAVY]), // '┹'
    (0x253a, [HEAVY, HEAVY, NONE, LIGHT]), // '┺'
    (0x253b, [HEAVY, HEAVY, NONE, HEAVY]), // '┻'
    (0x253c, [LIGHT, LIGHT, LIGHT, LIGHT]), // '┼'
    (0x253d, [LIGHT, LIGHT, LIGHT, HEAVY]), // '┽'
    (0x253e, [LIGHT, HEAVY, LIGHT, LIGHT]), // '┾'
    (0x253f, [LIGHT, HEAVY, LIGHT, HEAVY]), // '┿'
    (0x2540, [HEAVY, LIGHT, LIGHT, LIGHT]), // '╀'
    (0x2541, [LIGHT, LIGHT, HEAVY, LIGHT]), // '╁'
    (0x2542, [HEAVY, LIGHT, HEAVY, LIGHT]), // '╂'
    (0x2543, [HEAVY, LIGHT, LIGHT, HEAVY]), // '╃'
    (0x2544, [HEAVY, HEAVY, LIGHT, LIGHT]), // '╄'
    (0x2545, [LIGHT, LIGHT, HEAVY, HEAVY]), // '╅'
    (0x2546, [LIGHT, HEAVY, HEAVY, LIGHT]), // '╆'
    (0x2547, [HEAVY, HEAVY, LIGHT, HEAVY]), // '╇'
    (0x2548, [LIGHT, HEAVY, HEAVY, HEAVY]), // '╈'
    (0x2549, [HEAVY, LIGHT, HEAVY, HEAVY]), // '╉'
    (0x254a, [HEAVY, HEAVY, HEAVY, LIGHT]), // '╊'
    (0x254b, [HEAVY, HEAVY, HEAVY, HEAVY]), // '╋'
    (0x2550, [NONE, DOUBLE, NONE, DOUBLE]), // '═'
    (0x2551, [DOUBLE, NONE, DOUBLE, NONE]), // '║'
    (0x2552, [NONE, DOUBLE, LIGHT, NONE]), // '╒'
    (0x2553, [NONE, LIGHT, DOUBLE, NONE]), // '╓'
    (0x2554, [NONE, DOUBLE, DOUBLE, NONE]), // '╔'
    (0x2555, [NONE, NONE, LIGHT, DOUBLE]), // '╕'
    (0x2556, [NONE, NONE, DOUBLE, LIGHT]), // '╖'
    (0x2557, [NONE, NONE, DOUBLE, DOUBLE]), // '╗'
    (0x2558, [LIGHT, DOUBLE, NONE, NONE]), // '╘'
    (0x2559, [DOUBLE, LIGHT, NONE, NONE]), // '╙'
    (0x255a, [DOUBLE, DOUBLE, NONE, NONE]), // '╚'
    (0x255b, [LIGHT, NONE, NONE, DOUBLE]), // '╛'
    (0x255c, [DOUBLE, NONE, NONE, LIGHT]), // '╜'
    (0x255d, [DOUBLE, NONE, NONE, DOUBLE]), // '╝'
    (0x255e, [LIGHT, DOUBLE, LIGHT, NONE]), // '╞'
    (0x255f, [DOUBLE, LIGHT, DOUBLE, NONE]), // '╟'
    (0x2560, [DOUBLE, DOUBLE, DOUBLE, NONE]), // '╠'
    (0x2561, [LIGHT, NONE, LIGHT, DOUBLE]), // '╡'
    (0x2562, [DOUBLE, NONE, DOUBLE, LIGHT]), // '╢'
    (0x2563, [DOUBLE, NONE, DOUBLE, DOUBLE]), // '╣'
    (0x2564, [NONE, DOUBLE, LIGHT, DOUBLE]), // '╤'
    (0x2565, [NONE, LIGHT, DOUBLE, LIGHT]), // '╥'
    (0x2566, [NONE, DOUBLE, DOUBLE, DOUBLE]), // '╦'
    (0x2567, [LIGHT, DOUBLE, NONE, DOUBLE]), // '╧'
    (0x2568, [DOUBLE, LIGHT, NONE, LIGHT]), // '╨'
    (0x2569, [DOUBLE, DOUBLE, NONE, DOUBLE]), // '╩'
    (0x256a, [LIGHT, DOUBLE, LIGHT, DOUBLE]), // '╪'
    (0x256b, [DOUBLE, LIGHT, DOUBLE, LIGHT]), // '╫'
    (0x256c, [DOUBLE, DOUBLE, DOUBLE, DOUBLE]), // '╬'
    (0x2574, [NONE, NONE, NONE, LIGHT]), // '╴'
    (0x2575, [LIGHT, NONE, NONE, NONE]), // '╵'
    (0x2576, [NONE, LIGHT, NONE, NONE]), // '╶'
    (0x2577, [NONE, NONE, LIGHT, NONE]), // '╷'
    (0x2578, [NONE, NONE, NONE, HEAVY]), // '╸'
    (0x2579, [HEAVY, NONE, NONE, NONE]), // '╹'
    (0x257a, [NONE, HEAVY, NONE, NONE]), // '╺'
    (0x257b, [NONE, NONE, HEAVY, NONE]), // '╻'
    (0x257c, [NONE, HEAVY, NONE, LIGHT]), // '╼'
    (0x257d, [LIGHT, NONE, HEAVY, NONE]), // '╽'
    (0x257e, [NONE, LIGHT, NONE, HEAVY]), // '╾'
    (0x257f, [HEAVY, NONE, LIGHT, NONE]), // '╿'
];

#[cfg(test)]
mod tests {
    use crate::sprite::box_table::{ArmStyle, LINES, Lines, lines_of};

    #[test]
    fn the_table_is_in_code_point_order_so_a_lookup_can_bisect_it() {
        let ordered = LINES.windows(2).all(|pair| pair[0].0 < pair[1].0);
        let missed = LINES
            .iter()
            .filter(|(code_point, _)| lines_of(*code_point).is_none())
            .count();

        assert!(ordered, "the table has to stay sorted");
        assert_eq!(missed, 0, "a bisecting lookup finds every entry");
        assert_eq!(LINES.len(), 109);
    }

    #[test]
    fn the_plain_cross_and_the_plain_lines_carry_the_arms_they_show() {
        assert_eq!(
            lines_of(0x2500),
            Some(Lines {
                up: ArmStyle::None,
                right: ArmStyle::Light,
                down: ArmStyle::None,
                left: ArmStyle::Light,
            })
        );
        assert_eq!(
            lines_of(0x2502),
            Some(Lines {
                up: ArmStyle::Light,
                right: ArmStyle::None,
                down: ArmStyle::Light,
                left: ArmStyle::None,
            })
        );
        assert_eq!(
            lines_of(0x253c),
            Some(Lines {
                up: ArmStyle::Light,
                right: ArmStyle::Light,
                down: ArmStyle::Light,
                left: ArmStyle::Light,
            })
        );
    }

    #[test]
    fn a_doubled_glyph_carries_doubled_arms() {
        let double_cross = lines_of(0x256c).expect("'╬' is an intersection");

        assert_eq!(double_cross.up, ArmStyle::Double);
        assert_eq!(double_cross.left, ArmStyle::Double);
    }

    #[test]
    fn a_half_line_carries_one_arm_only() {
        let left_stub = lines_of(0x2574).expect("'╴' is an intersection");

        assert_eq!(left_stub.left, ArmStyle::Light);
        assert_eq!(left_stub.right, ArmStyle::None);
        assert_eq!(left_stub.up, ArmStyle::None);
        assert_eq!(left_stub.down, ArmStyle::None);
    }

    #[test]
    fn the_dashed_rounded_and_diagonal_glyphs_are_not_in_the_table() {
        assert_eq!(lines_of(0x2504), None, "'┄' is dashed");
        assert_eq!(lines_of(0x254c), None, "'╌' is dashed");
        assert_eq!(lines_of(0x256d), None, "'╭' is rounded");
        assert_eq!(lines_of(0x2571), None, "'╱' is a diagonal");
        assert_eq!(lines_of(0x2580), None, "'▀' is a block");
    }
}
