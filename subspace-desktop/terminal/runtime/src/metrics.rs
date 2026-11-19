//! Cell metrics and the grid an area produces (REQ-14, REQ-17).
//!
//! The arithmetic here is pure: the element measures one cell with the window
//! text system, then this module turns an area in points into columns and
//! rows. Everything is in points, which is what GPUI's `Pixels` carries.
//!
//! # Whole device pixels
//!
//! A cell and the top left of the grid are snapped to whole device pixels,
//! which is what Ghostty's `font.Metrics.calc` does when it rounds
//! `cell_width` and `cell_height` to whole pixels. Shaping `M` gives a
//! fractional advance, so without snapping a column edge falls inside a
//! pixel: the quad behind one cell and the sprite drawn in the cell beside
//! it then anti-alias against each other and leave a visible seam between
//! them. Snapped, every cell edge, background quad and sprite edge lands on
//! the same pixel boundary.

use gpui_kit::{Bounds, Pixels, Point, point, px, size};
use subspace_ghostty_config::AdjustCellHeight;

use crate::grid::SessionSize;

/// Padding applies to both sides of an axis.
const BOTH_SIDES: f32 = 2.0;

/// `adjust-cell-height` percentages are out of a hundred.
const PERCENT: f32 = 100.0;

/// The smallest a cell may be, in device pixels, which keeps the grid
/// arithmetic sound however far `adjust-cell-height` reduces a row.
const MIN_CELL_PIXELS: f32 = 1.0;

/// The scale a window with no usable scale factor is measured at, which
/// leaves a measurement in whole points.
const UNSCALED: f32 = 1.0;

/// The size of one cell, in points.
///
/// A shaped line is centred inside the line height it is painted with, so the
/// cell height is all the element needs to place a row: no baseline of its
/// own.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CellMetrics {
    /// The advance of one column.
    pub width: f32,
    /// The height of one row, after `adjust-cell-height`.
    pub height: f32,
}

/// The padding Ghostty keeps inside the bounds, in points (REQ-17).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Padding {
    /// `window-padding-x`, on the left and the right.
    pub x: f32,
    /// `window-padding-y`, at the top and the bottom.
    pub y: f32,
}

impl Padding {
    /// The points the padding takes out of the width.
    fn horizontal(self) -> f32 {
        self.x * BOTH_SIDES
    }

    /// The points the padding takes out of the height.
    fn vertical(self) -> f32 {
        self.y * BOTH_SIDES
    }
}

/// The cell a shaped line produces: whole device pixels, with
/// `adjust-cell-height` applied the way Ghostty applies it, to the already
/// rounded height (REQ-14).
pub(crate) fn snapped_cell(
    measured: CellMetrics,
    adjust: Option<AdjustCellHeight>,
    scale_factor: f32,
) -> CellMetrics {
    let rounded = snap_size(measured.height, scale_factor);
    CellMetrics {
        width: snap_size(measured.width, scale_factor),
        height: snap_size(adjusted_height(rounded, adjust), scale_factor),
    }
}

/// A measurement in points at the nearest whole device pixel, which is
/// where a quad edge draws without an anti-aliased seam.
pub(crate) fn snap_to_pixel(points: f32, scale_factor: f32) -> f32 {
    let scale = usable_scale(scale_factor);
    if !points.is_finite() {
        return points;
    }
    (points * scale).round() / scale
}

/// The same, for a size, which never rounds away to nothing.
fn snap_size(points: f32, scale_factor: f32) -> f32 {
    let scale = usable_scale(scale_factor);
    if !points.is_finite() {
        return points;
    }
    (points * scale).round().max(MIN_CELL_PIXELS) / scale
}

/// The device pixels one point covers, and one for a window that reports no
/// usable scale factor.
fn usable_scale(scale_factor: f32) -> f32 {
    if !scale_factor.is_finite() || scale_factor <= 0.0 {
        return UNSCALED;
    }
    scale_factor
}

/// The row height a measured line height and `adjust-cell-height` produce
/// (REQ-14). A percentage adjusts the measured height; a point value adds to
/// it. [`snap_size`] keeps the result at a device pixel or more.
fn adjusted_height(line_height: f32, adjust: Option<AdjustCellHeight>) -> f32 {
    match adjust {
        Some(AdjustCellHeight::Percent(percent)) => line_height * (1.0 + percent / PERCENT),
        Some(AdjustCellHeight::Points(points)) => line_height + points,
        None => line_height,
    }
}

/// Where one session's grid sits on screen: the area the element laid it out
/// in, the cell it measured and the padding it keeps inside that area.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Viewport {
    /// The area the element laid the session out in, in window coordinates.
    pub(crate) bounds: Bounds<Pixels>,
    /// The cell the element measured.
    pub(crate) cell: CellMetrics,
    /// The Ghostty padding, kept inside the bounds (REQ-17).
    pub(crate) padding: Padding,
    /// The device pixels one point covers, which the grid is snapped to.
    pub(crate) scale_factor: f32,
}

/// Where the cursor is on screen, which an input method needs in two frames
/// of reference (REQ-18).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Caret {
    /// The cell the cursor covers, in window coordinates.
    pub(crate) rect: Bounds<Pixels>,
    /// The same cell, measured from the session's own top left.
    pub(crate) offset: Point<Pixels>,
    /// How many columns are left on the cursor's row, including its own.
    pub(crate) columns_left: usize,
}

/// The cursor's place in a viewport, for a grid of `columns` columns.
pub(crate) fn caret(viewport: Viewport, cursor: (u16, u16), columns: u16) -> Caret {
    let origin = cell_origin(
        viewport.bounds.origin,
        viewport.padding,
        viewport.scale_factor,
    );
    let rect = cell_bounds(origin, viewport.cell, cursor.0, cursor.1);
    Caret {
        offset: point(
            rect.origin.x - viewport.bounds.origin.x,
            rect.origin.y - viewport.bounds.origin.y,
        ),
        columns_left: usize::from(columns.saturating_sub(cursor.0)),
        rect,
    }
}

/// The top left of the first cell: an area's origin, with the Ghostty
/// padding kept inside it (REQ-17), snapped to a device pixel so every cell
/// edge lands on one.
pub(crate) fn cell_origin(
    origin: Point<Pixels>,
    padding: Padding,
    scale_factor: f32,
) -> Point<Pixels> {
    point(
        px(snap_to_pixel(f32::from(origin.x) + padding.x, scale_factor)),
        px(snap_to_pixel(f32::from(origin.y) + padding.y, scale_factor)),
    )
}

/// The top left of one cell, measured from the first cell's own top left.
pub(crate) fn cell_point(
    origin: Point<Pixels>,
    cell: CellMetrics,
    column: u16,
    row: u16,
) -> Point<Pixels> {
    point(
        origin.x + px(f32::from(column) * cell.width),
        origin.y + px(f32::from(row) * cell.height),
    )
}

/// The rectangle one cell covers, which an input method's candidate window
/// is placed against (REQ-18).
pub(crate) fn cell_bounds(
    origin: Point<Pixels>,
    cell: CellMetrics,
    column: u16,
    row: u16,
) -> Bounds<Pixels> {
    Bounds::new(
        cell_point(origin, cell, column, row),
        size(px(cell.width), px(cell.height)),
    )
}

/// The columns and rows that fit an area, with the padding kept inside it.
///
/// `None` when fewer than one column or one row fits, which REQ-17 answers by
/// leaving the session's size alone.
pub(crate) fn fit_grid(
    width: f32,
    height: f32,
    padding: Padding,
    cell: CellMetrics,
) -> Option<(u16, u16)> {
    let columns = fit_count(width - padding.horizontal(), cell.width)?;
    let rows = fit_count(height - padding.vertical(), cell.height)?;
    Some((columns, rows))
}

/// How many cells of `size` fit in `available`, and `None` below one.
fn fit_count(available: f32, size: f32) -> Option<u16> {
    if !size.is_finite() || size <= 0.0 || !available.is_finite() {
        return None;
    }
    let count = (available / size).floor();
    if count < 1.0 {
        return None;
    }
    Some(count.min(f32::from(u16::MAX)) as u16)
}

/// The session size a fitted grid reports to the program. The cell fields are
/// whole points, so a fractional cell size rounds to at least one point.
pub(crate) fn session_size(columns: u16, rows: u16, cell: CellMetrics) -> SessionSize {
    SessionSize {
        columns,
        rows,
        cell_width: whole_points(cell.width),
        cell_height: whole_points(cell.height),
    }
}

/// A point measurement as the whole points the kernel's window size carries.
fn whole_points(points: f32) -> u16 {
    let rounded = points.round();
    if !rounded.is_finite() || rounded < 1.0 {
        return 1;
    }
    rounded.min(f32::from(u16::MAX)) as u16
}

#[cfg(test)]
mod tests {
    use gpui_kit::{Bounds, point, px, size};
    use subspace_ghostty_config::AdjustCellHeight;

    use crate::metrics::{
        CellMetrics, Padding, Viewport, caret, cell_bounds, cell_origin, fit_grid, session_size,
        snap_to_pixel, snapped_cell,
    };

    /// A window with one device pixel per point.
    const SCALE_1X: f32 = 1.0;

    /// A window with two, which is what a Retina display reports.
    const SCALE_2X: f32 = 2.0;

    /// A cell of plausible size for a 13 point monospace font.
    const CELL: CellMetrics = CellMetrics {
        width: 8.0,
        height: 16.0,
    };

    /// Ghostty's default padding.
    const PADDING: Padding = Padding { x: 2.0, y: 2.0 };

    #[test]
    fn a_cell_covers_its_column_and_row_inside_the_padding() {
        let origin = cell_origin(point(px(10.), px(20.)), PADDING, SCALE_1X);

        let bounds = cell_bounds(origin, CELL, 3, 2);

        assert_eq!(origin, point(px(12.), px(22.)));
        assert_eq!(bounds.origin, point(px(36.), px(54.)));
        assert_eq!(bounds.size, size(px(8.), px(16.)));
    }

    #[test]
    fn the_cursor_reports_its_cell_in_the_window_and_in_the_session() {
        let viewport = Viewport {
            bounds: Bounds::new(point(px(10.), px(20.)), size(px(640.), px(400.))),
            cell: CELL,
            padding: PADDING,
            scale_factor: SCALE_1X,
        };

        let placed = caret(viewport, (3, 2), 80);

        assert_eq!(placed.rect.origin, point(px(36.), px(54.)));
        assert_eq!(placed.offset, point(px(26.), px(34.)));
        assert_eq!(placed.rect.size, size(px(8.), px(16.)));
        assert_eq!(placed.columns_left, 77);
    }

    #[test]
    fn a_cursor_past_the_last_column_leaves_no_room() {
        let viewport = Viewport {
            bounds: Bounds::new(point(px(0.), px(0.)), size(px(640.), px(400.))),
            cell: CELL,
            padding: PADDING,
            scale_factor: SCALE_1X,
        };

        assert_eq!(caret(viewport, (80, 0), 80).columns_left, 0);
        assert_eq!(caret(viewport, (0, 0), 80).columns_left, 80);
    }

    #[test]
    fn the_grid_keeps_the_padding_inside_the_bounds() {
        let padded = fit_grid(640.0, 384.0, PADDING, CELL);
        let unpadded = fit_grid(640.0, 384.0, Padding { x: 0.0, y: 0.0 }, CELL);

        assert_eq!(padded, Some((79, 23)));
        assert_eq!(unpadded, Some((80, 24)));
    }

    #[test]
    fn a_partial_cell_does_not_count_as_a_column_or_a_row() {
        let grid = fit_grid(611.0, 299.0, Padding { x: 0.0, y: 0.0 }, CELL);

        assert_eq!(grid, Some((76, 18)));
    }

    #[test]
    fn an_area_below_one_row_or_column_produces_no_grid() {
        assert_eq!(fit_grid(4.0, 388.0, PADDING, CELL), None);
        assert_eq!(fit_grid(644.0, 4.0, PADDING, CELL), None);
        assert_eq!(fit_grid(0.0, 0.0, PADDING, CELL), None);
    }

    #[test]
    fn a_cell_with_no_size_produces_no_grid() {
        let broken = CellMetrics { width: 0.0, ..CELL };

        assert_eq!(fit_grid(644.0, 388.0, PADDING, broken), None);
    }

    /// The cell a shaped line of the given size produces.
    fn measured(width: f32, height: f32) -> CellMetrics {
        CellMetrics { width, height }
    }

    #[test]
    fn adjust_cell_height_scales_a_percentage_and_adds_points() {
        let cell = measured(8.0, 16.0);

        assert_eq!(snapped_cell(cell, None, SCALE_1X).height, 16.0);
        assert_eq!(
            snapped_cell(cell, Some(AdjustCellHeight::Percent(25.0)), SCALE_1X).height,
            20.0
        );
        assert_eq!(
            snapped_cell(cell, Some(AdjustCellHeight::Percent(-50.0)), SCALE_1X).height,
            8.0
        );
        assert_eq!(
            snapped_cell(cell, Some(AdjustCellHeight::Points(3.0)), SCALE_1X).height,
            19.0
        );
    }

    #[test]
    fn an_adjustment_that_would_remove_the_row_keeps_one_pixel() {
        let cell = measured(8.0, 16.0);

        assert_eq!(
            snapped_cell(cell, Some(AdjustCellHeight::Percent(-200.0)), SCALE_1X).height,
            1.0
        );
        assert_eq!(
            snapped_cell(cell, Some(AdjustCellHeight::Percent(-200.0)), SCALE_2X).height,
            0.5,
            "half a point is one device pixel at 2x"
        );
    }

    #[test]
    fn a_shaped_cell_lands_on_whole_device_pixels() {
        let shaped = measured(7.83, 16.2);

        let retina = snapped_cell(shaped, None, SCALE_2X);
        let plain = snapped_cell(shaped, None, SCALE_1X);

        assert_eq!(retina.width, 8.0, "15.66 pixels rounds to 16");
        assert_eq!(retina.height, 16.0, "32.4 pixels rounds to 32");
        assert_eq!(plain.width, 8.0);
        assert_eq!(plain.height, 16.0);
    }

    #[test]
    fn a_snapped_cell_covers_a_whole_number_of_device_pixels() {
        let shaped = measured(7.4, 18.3);

        let cell = snapped_cell(shaped, None, SCALE_2X);

        assert_eq!(cell.width * SCALE_2X, 15.0, "7.4pt is 14.8 pixels");
        assert_eq!(cell.height * SCALE_2X, 37.0, "18.3pt is 36.6 pixels");
    }

    #[test]
    fn adjust_cell_height_applies_to_the_rounded_height_and_is_rounded_again() {
        let shaped = measured(7.83, 16.2);

        let taller = snapped_cell(shaped, Some(AdjustCellHeight::Percent(20.0)), SCALE_2X);

        assert_eq!(taller.height, 19.0, "32 pixels plus a fifth is 38.4, so 38");
        assert_eq!(taller.height * SCALE_2X, 38.0);
    }

    #[test]
    fn a_window_with_no_usable_scale_keeps_whole_points() {
        let shaped = measured(7.83, 16.2);

        assert_eq!(snapped_cell(shaped, None, 0.0).width, 8.0);
        assert_eq!(snapped_cell(shaped, None, f32::NAN).height, 16.0);
    }

    #[test]
    fn the_grid_origin_lands_on_a_device_pixel() {
        let origin = cell_origin(point(px(10.3), px(20.4)), PADDING, SCALE_2X);

        assert_eq!(origin, point(px(12.5), px(22.5)));
        assert_eq!(f32::from(origin.x) * SCALE_2X, 25.0);
        assert_eq!(snap_to_pixel(0.3, SCALE_2X), 0.5);
        assert_eq!(snap_to_pixel(-0.3, SCALE_2X), -0.5);
    }

    #[test]
    fn the_session_size_reports_whole_points_per_cell() {
        let size = session_size(
            80,
            24,
            CellMetrics {
                width: 7.4,
                height: 16.6,
            },
        );

        assert_eq!(size.columns, 80);
        assert_eq!(size.rows, 24);
        assert_eq!(size.cell_width, 7);
        assert_eq!(size.cell_height, 17);
    }
}
