//! Selecting text with the pointer, and reading the selection back
//! (REQ-22, REQ-23).
//!
//! libghostty's selection gestures own the state machine: a press starts a
//! click sequence (a cell, then a word on the second click and a line on the
//! third), a drag extends it and a release ends it. The gesture and its
//! event objects are created once and reused, as the library intends, and
//! every selection it produces is installed as the terminal's own, which is
//! what the renderer draws from.
//!
//! The copied text follows Ghostty's own `selectionString`: plain output
//! with soft-wrapped lines unwrapped, so the padding of a wrapped line is
//! dropped, and trailing whitespace trimmed (REQ-22).

use std::time::Duration;

use libghostty_vt::Terminal;
use libghostty_vt::fmt::Format;
use libghostty_vt::mouse::Position;
use libghostty_vt::selection::FormatOptions;
use libghostty_vt::selection::gesture::{DragEvent, Geometry, Gesture, PressEvent, ReleaseEvent};
use libghostty_vt::terminal::{Point, PointCoordinate};

use crate::error::{SessionError, emulator_error};
use crate::metrics::Viewport;

/// How much text one copy usually holds, so a selection of a few lines
/// needs no second allocation.
const INITIAL_BYTES: usize = 1024;

/// Padding applies to both sides of an axis.
const BOTH_SIDES: f32 = 2.0;

/// How many clicks select a word, which is the first count that selects
/// without a drag (REQ-23).
const WORD_CLICKS: u8 = 2;

/// How long after a click the next one counts as a double click, which is
/// the interval both macOS and the GNOME desktops default to.
const REPEAT_INTERVAL: Duration = Duration::from_millis(500);

/// How far a second click may land from the first and still count, in
/// pixels: about one cell, so a steady hand on a small font still repeats.
const REPEAT_DISTANCE: f64 = 10.0;

/// One session's gesture state machine and its reusable events.
pub(crate) struct Gestures {
    gesture: Gesture<'static>,
    press: PressEvent<'static>,
    drag: DragEvent<'static>,
    release: ReleaseEvent<'static>,
}

impl Gestures {
    /// The gestures for one session.
    pub(crate) fn new() -> Result<Self, SessionError> {
        Ok(Self {
            gesture: Gesture::new().map_err(emulator_error)?,
            press: PressEvent::new().map_err(emulator_error)?,
            drag: DragEvent::new().map_err(emulator_error)?,
            release: ReleaseEvent::new().map_err(emulator_error)?,
        })
    }

    /// Start a click sequence at a position: the first click selects the
    /// cell, the second the word and the third the line (REQ-23).
    ///
    /// `since` is the monotonic time of the press. libghostty counts the
    /// clicks of a sequence itself, and without a time it only ever sees
    /// single clicks.
    pub(crate) fn press(
        &mut self,
        terminal: &Terminal<'static, 'static>,
        viewport: Viewport,
        at: Position,
        since: Duration,
    ) -> Result<(), SessionError> {
        let grid_ref = grid_ref(terminal, viewport, at)?;
        self.press.set_time(since).map_err(emulator_error)?;
        self.press
            .set_repeat_interval(REPEAT_INTERVAL)
            .map_err(emulator_error)?;
        self.press
            .set_repeat_distance(REPEAT_DISTANCE)
            .map_err(emulator_error)?;
        self.press
            .set_position(f64::from(at.x), f64::from(at.y))
            .map_err(emulator_error)?;
        let selection = self
            .press
            .apply(&mut self.gesture, terminal, grid_ref)
            .map_err(emulator_error)?;
        terminal
            .set_selection(selection.as_ref())
            .map_err(emulator_error)?;
        Ok(())
    }

    /// Extend the sequence to a position, at the granularity the press
    /// chose (REQ-23).
    pub(crate) fn drag(
        &mut self,
        terminal: &Terminal<'static, 'static>,
        viewport: Viewport,
        at: Position,
    ) -> Result<(), SessionError> {
        let grid_ref = grid_ref(terminal, viewport, at)?;
        self.drag
            .set_position(f64::from(at.x), f64::from(at.y))
            .map_err(emulator_error)?;
        let selection = self
            .drag
            .apply(&mut self.gesture, terminal, grid_ref, geometry(viewport))
            .map_err(emulator_error)?;
        if selection.is_some() {
            terminal
                .set_selection(selection.as_ref())
                .map_err(emulator_error)?;
        }
        Ok(())
    }

    /// End the sequence, which leaves the selection the drag produced.
    pub(crate) fn release(
        &mut self,
        terminal: &Terminal<'static, 'static>,
    ) -> Result<(), SessionError> {
        self.release
            .apply(&mut self.gesture, terminal, None)
            .map_err(emulator_error)
    }

    /// Whether the last click sequence produced a selection worth copying:
    /// it dragged, or it was a double or triple click, which select a word
    /// and a line on the press itself (REQ-23).
    pub(crate) fn selected(&self, terminal: &Terminal<'static, 'static>) -> bool {
        let dragged = self.gesture.dragged(terminal).unwrap_or(false);
        let clicks = self.gesture.click_count(terminal).unwrap_or(0);
        dragged || clicks >= WORD_CLICKS
    }

    /// Cancel the sequence and release the references it tracks, which a
    /// session does when its program takes the pointer over.
    pub(crate) fn reset(&mut self, terminal: &Terminal<'static, 'static>) {
        self.gesture.reset(terminal);
    }
}

/// The cell a position in the session falls on, as the grid reference the
/// gestures take.
fn grid_ref<'t>(
    terminal: &'t Terminal<'static, 'static>,
    viewport: Viewport,
    at: Position,
) -> Result<libghostty_vt::screen::GridRef<'t>, SessionError> {
    let (column, row) = cell_at(viewport, at);
    terminal
        .grid_ref(Point::Viewport(PointCoordinate { x: column, y: row }))
        .map_err(emulator_error)
}

/// The column and row a position in the session falls on, clamped to the
/// grid so a drag past an edge selects to it.
fn cell_at(viewport: Viewport, at: Position) -> (u16, u32) {
    let column = cell_index(at.x - viewport.padding.x, viewport.cell.width);
    let row = cell_index(at.y - viewport.padding.y, viewport.cell.height);
    (column.min(u32::from(u16::MAX)) as u16, row)
}

/// Which cell along one axis a distance falls in.
fn cell_index(distance: f32, size: f32) -> u32 {
    if !distance.is_finite() || distance < 0.0 || !size.is_finite() || size <= 0.0 {
        return 0;
    }
    (distance / size).floor() as u32
}

/// The rendered geometry a drag is interpreted against.
fn geometry(viewport: Viewport) -> Geometry {
    Geometry {
        columns: columns(viewport).max(1),
        cell_width: cell_pixels(viewport.cell.width),
        padding_left: cell_pixels(viewport.padding.x),
        screen_height: cell_pixels(f32::from(viewport.bounds.size.height)).max(1),
    }
}

/// How many columns the session's area holds, with the padding kept inside
/// it (REQ-17).
fn columns(viewport: Viewport) -> u32 {
    let inside = f32::from(viewport.bounds.size.width) - viewport.padding.x * BOTH_SIDES;
    cell_index(inside, viewport.cell.width)
}

/// A point measurement as the whole pixels libghostty's geometry carries,
/// never zero where libghostty requires a size.
fn cell_pixels(points: f32) -> u32 {
    if !points.is_finite() || points < 0.0 {
        return 0;
    }
    points.round() as u32
}

/// The session's selection as text, and nothing when there is none
/// (REQ-22).
pub(crate) fn text(terminal: &Terminal<'static, 'static>) -> Option<String> {
    let mut buffer = vec![0u8; INITIAL_BYTES];
    let written = format(terminal, &mut buffer)?;
    buffer.truncate(written);
    String::from_utf8(buffer)
        .ok()
        .filter(|text| !text.is_empty())
}

/// Format the selection into the buffer, growing it once when libghostty
/// asks for more room.
fn format(terminal: &Terminal<'static, 'static>, buffer: &mut Vec<u8>) -> Option<usize> {
    match terminal.format_selection_buf(copy_options(), buffer) {
        Ok(written) => written,
        Err(libghostty_vt::Error::OutOfSpace { required }) => {
            buffer.resize(required, 0);
            terminal.format_selection_buf(copy_options(), buffer).ok()?
        }
        Err(_) => None,
    }
}

/// How a copy is formatted: plain text with soft-wrapped lines unwrapped
/// and trailing whitespace trimmed, which is Ghostty's own copy (REQ-22).
fn copy_options() -> FormatOptions<'static, 'static> {
    FormatOptions::new()
        .with_emit_format(Format::Plain)
        .with_unwrap(true)
        .with_trim(true)
}

/// Drop the session's selection, which a new click and a program taking the
/// pointer over both do.
pub(crate) fn clear(terminal: &Terminal<'static, 'static>) {
    let _ = terminal.set_selection(None);
}

#[cfg(test)]
mod tests {
    use gpui_kit::{Bounds, point, px, size};

    use libghostty_vt::Terminal;
    use libghostty_vt::mouse::Position;

    use crate::metrics::{CellMetrics, Padding, Viewport};
    use crate::selection::{cell_at, columns, geometry, text};

    /// A session of 8 by 16 point cells at an offset inside the window.
    fn viewport() -> Viewport {
        Viewport {
            bounds: Bounds::new(point(px(10.), px(20.)), size(px(644.), px(400.))),
            cell: CellMetrics {
                width: 8.0,
                height: 16.0,
            },
            padding: Padding { x: 2.0, y: 2.0 },
            scale_factor: 1.0,
        }
    }

    /// A position inside the session.
    fn at(x: f32, y: f32) -> Position {
        Position { x, y }
    }

    #[test]
    fn a_position_falls_on_the_cell_under_it_with_the_padding_removed() {
        assert_eq!(cell_at(viewport(), at(2.0, 2.0)), (0, 0));
        assert_eq!(cell_at(viewport(), at(18.0, 34.0)), (2, 2));
    }

    #[test]
    fn a_position_above_or_left_of_the_grid_falls_on_its_first_cell() {
        assert_eq!(cell_at(viewport(), at(0.0, 0.0)), (0, 0));
        assert_eq!(cell_at(viewport(), at(-40.0, -40.0)), (0, 0));
    }

    #[test]
    fn the_geometry_reports_the_columns_the_area_holds() {
        let viewport = viewport();

        assert_eq!(columns(viewport), 80);
        let geometry = geometry(viewport);
        assert_eq!(geometry.columns, 80);
        assert_eq!(geometry.cell_width, 8);
        assert_eq!(geometry.padding_left, 2);
        assert_eq!(geometry.screen_height, 400);
    }

    #[test]
    fn a_session_with_no_selection_has_no_text() {
        let terminal = Terminal::new(20, 3).expect("a terminal");

        assert_eq!(text(&terminal), None);
    }

    #[test]
    fn the_whole_screen_reads_back_as_its_text_without_the_wrap_padding() {
        let mut terminal = Terminal::new(6, 3).expect("a terminal");
        terminal.vt_write(b"abcdefgh\r\nij");
        let all = terminal
            .select_all()
            .expect("a selection")
            .expect("the screen holds text");
        terminal.set_selection(Some(&all)).expect("set");

        let copied = text(&terminal).expect("the selection should read back");

        assert!(
            copied.starts_with("abcdefgh"),
            "a wrapped line is unwrapped: {copied:?}"
        );
        assert!(copied.contains("ij"), "{copied:?}");
    }
}
