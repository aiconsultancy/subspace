//! Reading libghostty's render state into a paintable frame.
//!
//! [`FrameReader`] owns the three libghostty handles a view reuses for every
//! frame: the render state and the row and cell iterators are created once, as
//! the library intends. [`FrameReader::read`] returns `None` while the render
//! state is clean, so a frame that changed nothing is painted from the view's
//! cached frame instead of being rebuilt (the dirty tracking REQ-12 to REQ-15
//! rely on for their cost).
//!
//! `graphemes_buf` is never called here: `clippy.toml` bans it and
//! `graphemes_len` plus `graphemes_utf8` carry the capacity (libghostty-rs
//! issue 70).

use gpui_kit::Hsla;
use libghostty_vt::Terminal;
use libghostty_vt::render::{
    CellIteration, CellIterator, CursorVisualStyle, Dirty, RenderState, RowIteration, RowIterator,
    RowSelection, Snapshot,
};

use crate::error::{SessionError, emulator_error};
use crate::frame::{CellFacts, FrameDefaults, FrameRow, RowCell, group_row, look_from};
use crate::palette::{TerminalPalette, paint_color, to_hsla};

/// Where the cursor is and what it looks like (REQ-15).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FrameCursor {
    /// The cursor's column in the viewport.
    pub(crate) column: u16,
    /// The cursor's row in the viewport.
    pub(crate) row: u16,
    /// The shape the program asked for.
    pub(crate) shape: CursorVisualStyle,
    /// Whether the program asked the cursor to blink.
    pub(crate) blinking: bool,
    /// The grapheme cluster a filled cursor covers, repainted on top of it.
    pub(crate) text: String,
}

/// Everything one frame paints.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct TerminalFrame {
    /// The rows of the viewport, top to bottom.
    pub(crate) rows: Vec<FrameRow>,
    /// The cursor, when the program shows one inside the viewport.
    pub(crate) cursor: Option<FrameCursor>,
}

/// Whether a frame uses a blink at all, so an idle session asks for no
/// repaint on the blink cycle (REQ-13, REQ-15).
pub(crate) fn blinks(frame: &TerminalFrame) -> bool {
    let cursor = frame.cursor.as_ref().is_some_and(|cursor| cursor.blinking);
    cursor || frame.rows.iter().any(row_blinks)
}

/// Whether one row holds a blinking run or a blinking sprite cell.
fn row_blinks(row: &FrameRow) -> bool {
    row.runs.iter().any(|run| run.look.blink) || row.sprites.iter().any(|sprite| sprite.look.blink)
}

/// The selection colours a frame resolves selected cells to (REQ-12).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SelectionColors {
    /// The background of a selected cell.
    pub(crate) background: Hsla,
    /// The foreground of a selected cell.
    pub(crate) foreground: Hsla,
}

/// The selection pair of a session's palette, as a frame paints it (REQ-12).
pub(crate) fn selection_colors(palette: &TerminalPalette) -> SelectionColors {
    SelectionColors {
        background: paint_color(palette.selection_background),
        foreground: paint_color(palette.selection_foreground),
    }
}

/// The libghostty handles one view reuses for every frame.
///
/// libghostty asks for the render state and the two iterators to be created
/// once and reused, so all three live here. They have to be borrowed apart
/// from one another to satisfy the borrow checker, which is why
/// [`read_frame`] is a function over them rather than a method.
pub(crate) struct FrameReader {
    state: RenderState<'static>,
    rows: RowIterator<'static>,
    cells: CellIterator<'static>,
}

/// The rows a read produced, with the glyph the cursor sits on.
struct ReadRows {
    rows: Vec<FrameRow>,
    cursor_text: String,
}

impl FrameReader {
    /// Create the render state and the iterators, once per session.
    pub(crate) fn new() -> Result<Self, SessionError> {
        Ok(Self {
            state: RenderState::new().map_err(emulator_error)?,
            rows: RowIterator::new().map_err(emulator_error)?,
            cells: CellIterator::new().map_err(emulator_error)?,
        })
    }
}

/// The frame the emulator shows now, and `None` while nothing has changed
/// since the last read.
///
/// `force` builds a frame from a clean render state as well, which a new
/// style needs: the colours of a frame are resolved into its runs, so a
/// recoloured session has to build them again (REQ-12, REQ-33).
pub(crate) fn read_frame(
    reader: &mut FrameReader,
    terminal: &Terminal<'static, 'static>,
    selection: SelectionColors,
    force: bool,
) -> Result<Option<TerminalFrame>, SessionError> {
    let FrameReader { state, rows, cells } = reader;
    let snapshot = state.update(terminal).map_err(emulator_error)?;
    if !force && snapshot.dirty().map_err(emulator_error)? == Dirty::Clean {
        return Ok(None);
    }
    let frame = build_frame(&snapshot, rows, cells, selection)?;
    snapshot.clean().map_err(emulator_error)?;
    Ok(Some(frame))
}

/// Build one frame from a snapshot of the render state.
fn build_frame(
    snapshot: &Snapshot<'static, '_>,
    rows: &mut RowIterator<'static>,
    cells: &mut CellIterator<'static>,
    selection: SelectionColors,
) -> Result<TerminalFrame, SessionError> {
    let defaults = frame_defaults(snapshot, selection)?;
    let cursor = cursor_position(snapshot)?;
    let read = read_rows(rows, cells, snapshot, &defaults, cursor)?;
    Ok(TerminalFrame {
        rows: read.rows,
        cursor: frame_cursor(snapshot, cursor, read.cursor_text)?,
    })
}

/// The colours the cells of a frame resolve against. The default foreground
/// and background are the emulator's own, which the style already set from
/// the palette, so an OSC 10 or 11 sequence changes them as well (REQ-12).
fn frame_defaults(
    snapshot: &Snapshot<'static, '_>,
    selection: SelectionColors,
) -> Result<FrameDefaults, SessionError> {
    let colors = snapshot.colors().map_err(emulator_error)?;
    Ok(FrameDefaults {
        foreground: to_hsla(colors.foreground),
        background: to_hsla(colors.background),
        selection_background: selection.background,
        selection_foreground: selection.foreground,
    })
}

/// Where the cursor is, when the program shows one inside the viewport.
fn cursor_position(snapshot: &Snapshot<'static, '_>) -> Result<Option<(u16, u16)>, SessionError> {
    let cursor = snapshot.cursor().map_err(emulator_error)?;
    Ok(cursor
        .viewport
        .filter(|_| cursor.visible)
        .map(|viewport| (viewport.x, viewport.y)))
}

/// The cursor a frame paints, with the glyph it covers.
fn frame_cursor(
    snapshot: &Snapshot<'static, '_>,
    position: Option<(u16, u16)>,
    text: String,
) -> Result<Option<FrameCursor>, SessionError> {
    let Some((column, row)) = position else {
        return Ok(None);
    };
    let cursor = snapshot.cursor().map_err(emulator_error)?;
    Ok(Some(FrameCursor {
        column,
        row,
        shape: cursor.visual_style,
        blinking: cursor.blinking,
        text,
    }))
}

/// Read every row of the viewport into grouped spans.
fn read_rows(
    rows: &mut RowIterator<'static>,
    cells: &mut CellIterator<'static>,
    snapshot: &Snapshot<'static, '_>,
    defaults: &FrameDefaults,
    cursor: Option<(u16, u16)>,
) -> Result<ReadRows, SessionError> {
    let mut iteration = rows.update(snapshot).map_err(emulator_error)?;
    let mut read = ReadRows {
        rows: Vec::new(),
        cursor_text: String::new(),
    };
    while let Some(row) = iteration.next() {
        let Some(index) = viewport_row(row)? else {
            continue;
        };
        let row_cells = read_cells(cells, row, defaults)?;
        take_cursor_glyph(&mut read.cursor_text, &row_cells, cursor, index);
        read.rows.push(group_row(index, &row_cells));
    }
    Ok(read)
}

/// A row's place in the viewport, and `None` for a row outside it. No
/// overscan is requested, so every captured row is a viewport row.
fn viewport_row(row: &RowIteration<'static, '_>) -> Result<Option<u16>, SessionError> {
    let y = row.viewport_y().map_err(emulator_error)?;
    Ok(u16::try_from(y).ok())
}

/// Keep the glyph the cursor sits on, so a filled cursor can repaint it in
/// the cursor's text colour (REQ-15).
fn take_cursor_glyph(text: &mut String, cells: &[RowCell], cursor: Option<(u16, u16)>, row: u16) {
    let Some((column, cursor_row)) = cursor else {
        return;
    };
    if cursor_row != row {
        return;
    }
    if let Some(cell) = cells.get(usize::from(column)) {
        text.clone_from(&cell.text);
    }
}

/// Read one row's cells, resolving each cell's colours and attributes.
fn read_cells(
    cells: &mut CellIterator<'static>,
    row: &RowIteration<'static, '_>,
    defaults: &FrameDefaults,
) -> Result<Vec<RowCell>, SessionError> {
    let selection = row.selection().map_err(emulator_error)?;
    let mut iteration = cells.update(row).map_err(emulator_error)?;
    let mut read = Vec::new();
    let mut text = String::new();
    while let Some(cell) = iteration.next() {
        let column = read.len();
        read.push(read_cell(
            cell,
            defaults,
            selected(selection, column),
            &mut text,
        )?);
    }
    Ok(read)
}

/// Whether a column is inside the row's selected range, which libghostty
/// reports with both ends included.
fn selected(selection: Option<RowSelection>, column: usize) -> bool {
    selection.is_some_and(|range| {
        usize::from(range.start_x) <= column && column <= usize::from(range.end_x)
    })
}

/// One cell: its grapheme cluster and the look its colours and style produce.
fn read_cell(
    cell: &CellIteration<'static, '_>,
    defaults: &FrameDefaults,
    selected: bool,
    text: &mut String,
) -> Result<RowCell, SessionError> {
    let facts = cell_facts(cell)?;
    read_text(cell, text)?;
    Ok(RowCell {
        text: text.clone(),
        look: look_from(&facts, defaults, selected),
    })
}

/// What the emulator reports for one cell.
fn cell_facts(cell: &CellIteration<'static, '_>) -> Result<CellFacts, SessionError> {
    Ok(CellFacts {
        foreground: cell.fg_color().map_err(emulator_error)?.map(to_hsla),
        background: cell.bg_color().map_err(emulator_error)?.map(to_hsla),
        style: cell_style(cell)?,
    })
}

/// A cell's style, and the default style for a cell with no styling of its
/// own, which saves the style lookup on most cells.
fn cell_style(
    cell: &CellIteration<'static, '_>,
) -> Result<libghostty_vt::style::Style, SessionError> {
    if !cell.has_styling().map_err(emulator_error)? {
        return Ok(libghostty_vt::style::Style::default());
    }
    cell.style().map_err(emulator_error)
}

/// A cell's whole grapheme cluster, and nothing for a cell with no text: a
/// blank, or the spacer that follows a double-width character (REQ-13).
fn read_text(cell: &CellIteration<'static, '_>, text: &mut String) -> Result<(), SessionError> {
    if cell.graphemes_len().map_err(emulator_error)? == 0 {
        text.clear();
        return Ok(());
    }
    cell.graphemes_utf8(text).map_err(emulator_error)
}

#[cfg(test)]
mod tests {
    use gpui_kit::{Hsla, rgb};
    use libghostty_vt::Terminal;
    use libghostty_vt::render::RowSelection;

    use crate::snapshot::{FrameReader, SelectionColors, read_frame, selected};

    /// The selection colours a test reads with.
    const SELECTION: SelectionColors = SelectionColors {
        background: Hsla {
            h: 0.0,
            s: 0.0,
            l: 0.2,
            a: 1.0,
        },
        foreground: Hsla {
            h: 0.0,
            s: 0.0,
            l: 0.9,
            a: 1.0,
        },
    };

    /// A terminal with text and attributes written into it.
    fn terminal(bytes: &[u8]) -> Terminal<'static, 'static> {
        let mut terminal = Terminal::new(20, 3).expect("a terminal");
        terminal.vt_write(bytes);
        terminal
    }

    #[test]
    fn a_read_reports_the_rows_and_then_nothing_until_a_change() {
        let mut terminal = terminal(b"hello");
        let mut reader = FrameReader::new().expect("a reader");

        let first = read_frame(&mut reader, &terminal, SELECTION, false)
            .expect("a frame")
            .expect("the first frame is dirty");
        let second = read_frame(&mut reader, &terminal, SELECTION, false).expect("a frame");
        terminal.vt_write(b" again");
        let third = read_frame(&mut reader, &terminal, SELECTION, false).expect("a frame");

        assert_eq!(first.rows.len(), 3);
        assert_eq!(first.rows[0].runs[0].text, "hello");
        assert_eq!(second, None, "a clean render state repaints the last frame");
        assert!(third.is_some(), "new output makes the render state dirty");
    }

    #[test]
    fn a_direct_colour_arrives_exactly_as_requested() {
        let terminal = terminal(b"\x1b[38;2;20;40;80mx");
        let mut reader = FrameReader::new().expect("a reader");

        let frame = read_frame(&mut reader, &terminal, SELECTION, false)
            .expect("a frame")
            .expect("a dirty frame");

        let foreground = frame.rows[0].runs[0].look.foreground;
        assert_eq!(foreground, Hsla::from(rgb(0x142850)));
    }

    #[test]
    fn a_wide_character_reports_a_spacer_cell_without_text() {
        let terminal = terminal("中x".as_bytes());
        let mut reader = FrameReader::new().expect("a reader");

        let frame = read_frame(&mut reader, &terminal, SELECTION, false)
            .expect("a frame")
            .expect("a dirty frame");

        let runs = &frame.rows[0].runs;
        assert_eq!(runs[0].text, "中");
        assert_eq!(runs[1].column, 2, "the spacer keeps the column grid");
        assert_eq!(runs[1].text, "x");
    }

    #[test]
    fn the_cursor_reports_its_position_and_the_glyph_it_covers() {
        let terminal = terminal(b"ab\x1b[1;1H");
        let mut reader = FrameReader::new().expect("a reader");

        let frame = read_frame(&mut reader, &terminal, SELECTION, false)
            .expect("a frame")
            .expect("a dirty frame");

        let cursor = frame.cursor.expect("a visible cursor");
        assert_eq!((cursor.column, cursor.row), (0, 0));
        assert_eq!(cursor.text, "a");
    }

    #[test]
    fn a_hidden_cursor_is_not_painted() {
        let terminal = terminal(b"ab\x1b[?25l");
        let mut reader = FrameReader::new().expect("a reader");

        let frame = read_frame(&mut reader, &terminal, SELECTION, false)
            .expect("a frame")
            .expect("a dirty frame");

        assert_eq!(frame.cursor, None);
    }

    #[test]
    fn a_row_selection_covers_its_whole_range_including_both_ends() {
        let range = RowSelection {
            size: std::mem::size_of::<RowSelection>(),
            start_x: 2,
            end_x: 4,
        };

        assert!(!selected(None, 3));
        assert!(!selected(Some(range), 1));
        assert!(selected(Some(range), 2));
        assert!(selected(Some(range), 4));
        assert!(!selected(Some(range), 5));
    }
}
