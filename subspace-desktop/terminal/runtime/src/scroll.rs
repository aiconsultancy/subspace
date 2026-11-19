//! Moving through scrollback: the wheel, the page keys and the scrollbar
//! (REQ-16).
//!
//! libghostty keeps the viewport itself: it follows the bottom while the
//! viewport is active and holds its row once the user scrolls back, so this
//! module only turns gestures into [`libghostty_vt::ScrollViewport`] requests
//! and reports where the scrollbar thumb belongs.
//!
//! Shift+Page reaches this through
//! [`crate::TerminalView::scroll_pages`] and the scrollbar drag through
//! [`crate::TerminalView::scroll_to_fraction`], which `crate::pointer`
//! calls while the thumb is held. The wheel is wired in the element,
//! because REQ-16 asks for the `mouse-scroll-multiplier`.

/// The smallest page a grid can have: one row of content plus the row of
/// context a page keeps.
const MIN_PAGE_ROWS: f32 = 2.0;

/// How wide the scrollbar is, in points: what the element draws and what a
/// drag has to land on (REQ-16).
pub(crate) const SCROLLBAR_WIDTH: f32 = 6.0;

/// Whether a position is over the scrollbar, which is only on screen while
/// there is scrollback to show (REQ-16).
///
/// `right` is the right edge of the session's area and `thumb` is what
/// [`thumb`] reported for the frame being shown.
pub(crate) fn on_scrollbar(x: f32, right: f32, thumb: Option<(f32, f32)>) -> bool {
    thumb.is_some() && x >= right - SCROLLBAR_WIDTH && x <= right
}

/// Where a position down the scrollbar track sits, as a fraction from its
/// top (REQ-16).
pub(crate) fn track_fraction(y: f32, top: f32, height: f32) -> f32 {
    if !height.is_finite() || height <= 0.0 {
        return 0.0;
    }
    ((y - top) / height).clamp(0.0, 1.0)
}

/// How many rows a wheel movement scrolls, with the Ghostty
/// `mouse-scroll-multiplier` applied.
///
/// GPUI reports the wheel in lines, positive upwards; libghostty counts rows
/// positive downwards, so the sign flips. A movement that rounds to no row
/// still scrolls one row in its direction, so a fine trackpad never stalls.
pub(crate) fn wheel_rows(lines: f32, multiplier: f32) -> isize {
    let rows = -lines * multiplier.max(0.0);
    if rows == 0.0 || !rows.is_finite() {
        return 0;
    }
    let whole = rows.trunc() as isize;
    if whole != 0 {
        return whole;
    }
    if rows.is_sign_negative() { -1 } else { 1 }
}

/// How many rows a page movement scrolls on a grid of `rows` rows.
///
/// A page keeps one row of context, as `less` and `vim` do, and always moves
/// at least one row so a one-row grid still scrolls.
pub(crate) fn page_rows(pages: f32, rows: u16) -> isize {
    let page = f32::from(rows.max(1)).max(MIN_PAGE_ROWS) - 1.0;
    let moved = (pages * page).round();
    if moved == 0.0 || !moved.is_finite() {
        return 0;
    }
    moved as isize
}

/// Where the scrollbar thumb sits, as fractions of the track from its top,
/// and `None` when everything fits and no scrollbar belongs on screen.
///
/// `total` is the scrollable rows, `offset` the viewport's first row and
/// `visible` the viewport's rows, exactly as `Terminal::scrollbar` reports
/// them.
pub(crate) fn thumb(total: u64, offset: u64, visible: u64) -> Option<(f32, f32)> {
    if visible == 0 || total <= visible {
        return None;
    }
    let total = total as f32;
    let length = (visible as f32 / total).clamp(MIN_THUMB, 1.0);
    let start = (offset as f32 / total).clamp(0.0, 1.0 - length);
    Some((start, length))
}

/// The shortest thumb, as a fraction of the track, so a long scrollback still
/// leaves something to grab.
const MIN_THUMB: f32 = 0.04;

/// The row a position on the scrollbar track asks for.
///
/// `fraction` is the position from the top of the track, and the result is the
/// absolute row `libghostty_vt::ScrollViewport::Row` takes.
pub(crate) fn row_at(fraction: f32, total: u64, visible: u64) -> usize {
    let last = total.saturating_sub(visible) as f32;
    let row = (fraction.clamp(0.0, 1.0) * last).round();
    if !row.is_finite() || row < 0.0 {
        return 0;
    }
    row as usize
}

#[cfg(test)]
mod tests {
    use crate::scroll::{on_scrollbar, page_rows, row_at, thumb, track_fraction, wheel_rows};

    #[test]
    fn only_the_right_edge_of_a_scrollable_session_is_the_scrollbar() {
        let thumb = Some((0.0, 0.5));

        assert!(on_scrollbar(636.0, 640.0, thumb));
        assert!(on_scrollbar(640.0, 640.0, thumb));
        assert!(!on_scrollbar(600.0, 640.0, thumb));
        assert!(
            !on_scrollbar(636.0, 640.0, None),
            "a session with no scrollback shows no scrollbar"
        );
    }

    #[test]
    fn a_position_down_the_track_is_its_fraction_from_the_top() {
        assert_eq!(track_fraction(20.0, 20.0, 400.0), 0.0);
        assert_eq!(track_fraction(220.0, 20.0, 400.0), 0.5);
        assert_eq!(track_fraction(900.0, 20.0, 400.0), 1.0);
        assert_eq!(track_fraction(0.0, 20.0, 400.0), 0.0);
        assert_eq!(track_fraction(100.0, 20.0, 0.0), 0.0);
    }

    #[test]
    fn the_wheel_scrolls_up_as_negative_rows_and_applies_the_multiplier() {
        assert_eq!(wheel_rows(3.0, 1.0), -3);
        assert_eq!(wheel_rows(-3.0, 1.0), 3);
        assert_eq!(wheel_rows(2.0, 3.0), -6);
        assert_eq!(wheel_rows(4.0, 0.5), -2);
    }

    #[test]
    fn a_fine_wheel_movement_still_scrolls_one_row() {
        assert_eq!(wheel_rows(0.2, 1.0), -1);
        assert_eq!(wheel_rows(-0.2, 1.0), 1);
        assert_eq!(wheel_rows(0.0, 1.0), 0);
    }

    #[test]
    fn a_negative_multiplier_does_not_reverse_the_wheel() {
        assert_eq!(wheel_rows(3.0, -2.0), 0);
    }

    #[test]
    fn a_page_keeps_one_row_of_context_and_follows_its_sign() {
        assert_eq!(page_rows(-1.0, 24), -23);
        assert_eq!(page_rows(1.0, 24), 23);
        assert_eq!(page_rows(-0.5, 25), -12);
    }

    #[test]
    fn a_page_on_the_smallest_grid_still_moves_a_row() {
        assert_eq!(page_rows(-1.0, 1), -1);
        assert_eq!(page_rows(1.0, 0), 1);
    }

    #[test]
    fn a_viewport_that_shows_everything_has_no_thumb() {
        assert_eq!(thumb(24, 0, 24), None);
        assert_eq!(thumb(10, 0, 24), None);
        assert_eq!(thumb(100, 0, 0), None);
    }

    #[test]
    fn the_thumb_follows_the_viewport_and_keeps_a_minimum_length() {
        let (top, length) = thumb(100, 0, 25).expect("a scrollbar");
        assert_eq!(top, 0.0);
        assert_eq!(length, 0.25);

        let (middle, _) = thumb(100, 50, 25).expect("a scrollbar");
        assert_eq!(middle, 0.5);

        let (_, short) = thumb(100_000, 0, 25).expect("a scrollbar");
        assert!(short >= 0.04, "{short}");
    }

    #[test]
    fn the_thumb_never_runs_past_the_end_of_the_track() {
        let (top, length) = thumb(100, 95, 25).expect("a scrollbar");

        assert!(top + length <= 1.0, "{top} + {length}");
    }

    #[test]
    fn a_track_position_maps_onto_the_scrollable_rows() {
        assert_eq!(row_at(0.0, 100, 25), 0);
        assert_eq!(row_at(1.0, 100, 25), 75);
        assert_eq!(row_at(0.5, 100, 25), 38);
        assert_eq!(row_at(2.0, 100, 25), 75);
        assert_eq!(row_at(0.5, 10, 25), 0);
    }
}
