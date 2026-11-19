//! The cursor the program asked for, drawn over the cell it sits on
//! (REQ-15).
//!
//! It is filled while the view has focus and hollow while it does not, and
//! a filled block repaints the glyph it covers in the cursor's text colour.
//! A bar and an underline are one physical pixel thick, as Ghostty's
//! `Metrics.cursor_thickness` of one means at the font's own scale; GPUI
//! measures in points, so the window's scale factor turns it back into a
//! pixel.

use gpui_kit::{
    App, BorderStyle, Bounds, Hsla, Pixels, SharedString, Window, fill, outline, point, px, size,
};
use libghostty_vt::render::CursorVisualStyle;

use super::{Grid, TextPlace, cell_position, cell_rect, paint_text};
use crate::frame::CellLook;
use crate::palette::paint_color;
use crate::snapshot::FrameCursor;
use crate::sprite;
use crate::state::TerminalState;
use crate::style::TerminalFonts;

/// One physical pixel, in points at a scale factor of one.
const ONE_PIXEL: f32 = 1.0;

/// What the glyph a filled block cursor covers is drawn with.
struct CursorGlyph<'a> {
    /// The cursor's text colour.
    color: Hsla,
    /// Where the cells land.
    grid: &'a Grid,
    /// The families and the size to shape with.
    fonts: &'a TerminalFonts,
}

/// Draw the cursor the program asked for.
// qual:allow(test_quality, untested) reason: "Runs only while GPUI paints TerminalElement (Element::paint), a dispatch the call graph cannot follow; view_tests draws the element with render_frame."
pub(super) fn paint_cursor(state: &TerminalState, grid: &Grid, window: &mut Window, cx: &mut App) {
    let Some(cursor) = state.painted.frame.cursor.as_ref() else {
        return;
    };
    if cursor.blinking && !state.painted.blink_shows {
        return;
    }
    let color = paint_color(state.style.palette.cursor);
    let bounds = cursor_bounds(
        cursor,
        state.focused,
        grid,
        cursor_thickness(window.scale_factor()),
    );
    if hollow(cursor, state.focused) {
        window.paint_quad(outline(bounds, color, BorderStyle::Solid));
        return;
    }
    window.paint_quad(fill(bounds, color));
    if cursor.text.is_empty() || cursor.shape != CursorVisualStyle::Block {
        return;
    }
    paint_covered_glyph(
        cursor,
        &CursorGlyph {
            color: paint_color(state.style.palette.cursor_text),
            grid,
            fonts: &state.style.fonts,
        },
        window,
        cx,
    );
}

/// Repaint the glyph a filled block cursor covers in the cursor's text
/// colour: as a sprite when this crate draws the glyph itself, and as
/// shaped text otherwise.
// qual:allow(test_quality, untested) reason: "Runs only while GPUI paints TerminalElement (Element::paint), a dispatch the call graph cannot follow; view_tests draws the element with render_frame."
fn paint_covered_glyph(
    cursor: &FrameCursor,
    glyph: &CursorGlyph<'_>,
    window: &mut Window,
    cx: &mut App,
) {
    let bounds = cell_rect(cursor.column, cursor.row, glyph.grid);
    if let Some(drawn) = sprite::sprite_of(&cursor.text) {
        sprite::paint(drawn, bounds, glyph.color, window);
        return;
    }
    paint_text(
        SharedString::from(cursor.text.clone()),
        CellLook::plain(glyph.color),
        &TextPlace {
            origin: bounds.origin,
            line_height: glyph.grid.cell.height,
            fonts: glyph.fonts,
        },
        window,
        cx,
    );
}

/// How wide a bar or an underline cursor is, in points: one physical pixel.
/// A window that reports no usable scale falls back to one point, which is
/// one pixel at 1x.
fn cursor_thickness(scale_factor: f32) -> f32 {
    if !scale_factor.is_finite() || scale_factor <= 0.0 {
        return ONE_PIXEL;
    }
    ONE_PIXEL / scale_factor
}

/// Whether the cursor is drawn as an outline: an unfocused session, or the
/// hollow block the program asked for.
fn hollow(cursor: &FrameCursor, focused: bool) -> bool {
    !focused || cursor.shape == CursorVisualStyle::BlockHollow
}

/// The rectangle the cursor covers, by shape.
fn cursor_bounds(
    cursor: &FrameCursor,
    focused: bool,
    grid: &Grid,
    thickness: f32,
) -> Bounds<Pixels> {
    let origin = cell_position(cursor.column, cursor.row, grid);
    let cell = size(px(grid.cell.width), px(grid.cell.height));
    if !focused {
        return Bounds::new(origin, cell);
    }
    match cursor.shape {
        CursorVisualStyle::Bar => Bounds::new(origin, size(px(thickness), cell.height)),
        CursorVisualStyle::Underline => Bounds::new(
            point(origin.x, origin.y + cell.height - px(thickness)),
            size(cell.width, px(thickness)),
        ),
        _ => Bounds::new(origin, cell),
    }
}

#[cfg(test)]
mod tests {
    use gpui_kit::{point, px, size};
    use libghostty_vt::render::CursorVisualStyle;

    use crate::element::Grid;
    use crate::element::cursor::{cursor_bounds, cursor_thickness, hollow};
    use crate::metrics::CellMetrics;
    use crate::snapshot::FrameCursor;

    /// A grid of 8 by 16 point cells, with Ghostty's padding applied.
    fn grid() -> Grid {
        Grid {
            origin: point(px(2.), px(2.)),
            cell: CellMetrics {
                width: 8.0,
                height: 16.0,
            },
        }
    }

    /// A cursor of the given shape at column 1, row 0.
    fn cursor(shape: CursorVisualStyle) -> FrameCursor {
        FrameCursor {
            column: 1,
            row: 0,
            shape,
            blinking: false,
            text: "x".to_owned(),
        }
    }

    #[test]
    fn an_unfocused_session_draws_a_hollow_cursor_over_the_whole_cell() {
        let block = cursor(CursorVisualStyle::Block);

        assert!(hollow(&block, false));
        assert!(!hollow(&block, true));
        assert!(hollow(&cursor(CursorVisualStyle::BlockHollow), true));
        assert_eq!(
            cursor_bounds(&cursor(CursorVisualStyle::Bar), false, &grid(), 1.0).size,
            size(px(8.), px(16.)),
            "an unfocused bar still outlines its cell"
        );
    }

    #[test]
    fn each_cursor_shape_covers_its_own_part_of_the_cell() {
        let bar = cursor_bounds(&cursor(CursorVisualStyle::Bar), true, &grid(), 1.0);
        let underline = cursor_bounds(&cursor(CursorVisualStyle::Underline), true, &grid(), 1.0);
        let block = cursor_bounds(&cursor(CursorVisualStyle::Block), true, &grid(), 1.0);

        assert_eq!(bar.size, size(px(1.), px(16.)));
        assert_eq!(bar.origin, point(px(10.), px(2.)));
        assert_eq!(underline.size, size(px(8.), px(1.)));
        assert_eq!(underline.origin, point(px(10.), px(17.)));
        assert_eq!(block.size, size(px(8.), px(16.)));
    }

    #[test]
    fn a_bar_cursor_is_one_physical_pixel_at_every_scale() {
        assert_eq!(cursor_thickness(1.0), 1.0, "one point at 1x");
        assert_eq!(cursor_thickness(2.0), 0.5, "half a point at 2x");
        assert_eq!(cursor_thickness(3.0), 1.0 / 3.0);
        assert_eq!(cursor_thickness(0.0), 1.0, "an unusable scale falls back");
        assert_eq!(cursor_thickness(f32::NAN), 1.0);
    }

    #[test]
    fn a_thinner_cursor_narrows_the_bar_and_lowers_the_underline() {
        let bar = cursor_bounds(&cursor(CursorVisualStyle::Bar), true, &grid(), 0.5);
        let underline = cursor_bounds(&cursor(CursorVisualStyle::Underline), true, &grid(), 0.5);

        assert_eq!(bar.size, size(px(0.5), px(16.)));
        assert_eq!(underline.size, size(px(8.), px(0.5)));
        assert_eq!(underline.origin, point(px(10.), px(17.5)));
    }
}
