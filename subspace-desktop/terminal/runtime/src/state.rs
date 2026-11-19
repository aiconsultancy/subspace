//! The state one session draws from: the session itself, the render state
//! that reads it, the style it draws with and the last frame built.
//!
//! [`TerminalView`](crate::TerminalView) and its element share this through an
//! `Rc<RefCell<_>>` rather than reaching it through the entity, because GPUI
//! renders a view inside that entity's own update: an element that updated or
//! read the entity it was rendered from would panic. The element therefore
//! touches only this state, and the entity keeps the focus handle, the public
//! API and the two foreground tasks. The fields are crate-visible because
//! both of them read the same facts.

// qual:allow(coupling, sdp) reason: "The state is the crate's one orchestrator: it drives the session, the reader and the style, and only the view and the element depend on it. The session it depends on is a leaf that nothing in this crate depends upon in turn."

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gpui_kit::{Bounds, Pixels, SharedString, Window};
use libghostty_vt::terminal::{ScrollViewport, Scrollbar};

use crate::command::SessionOptions;
use crate::effects::{self, ClipboardEffects};
use crate::error::SessionError;
use crate::font::measure_cell;
use crate::grid::SessionSize;
use crate::input::Typing;
use crate::metrics::{CellMetrics, Viewport, fit_grid, session_size};
use crate::scroll::{page_rows, row_at, thumb, wheel_rows};
use crate::session::Session;
use crate::snapshot::{FrameReader, TerminalFrame, blinks, read_frame, selection_colors};
use crate::style::{self, TerminalStyle};

/// How long each half of the blink cycle lasts (REQ-13, REQ-15).
pub(crate) const BLINK_INTERVAL: Duration = Duration::from_millis(530);

/// How often the view applies what the session's reader thread has sent. One
/// drain per interval batches a noisy program into one repaint.
pub(crate) const DRAIN_INTERVAL: Duration = Duration::from_millis(8);

/// The cell a session holds before its first layout measures one, which
/// keeps the grid arithmetic sound until then.
const UNMEASURED_CELL: CellMetrics = CellMetrics {
    width: 1.0,
    height: 1.0,
};

/// The scale a session holds before its first layout reads the window's,
/// which leaves the grid arithmetic in whole points until then.
const UNMEASURED_SCALE: f32 = 1.0;

/// What the painter draws from: the last frame built, where the scrollbar
/// thumb belongs and which half of the blink cycle shows (REQ-13 to
/// REQ-16).
#[derive(Default)]
pub(crate) struct Painted {
    /// The last frame built, repainted while the emulator stays clean.
    pub(crate) frame: TerminalFrame,
    /// Where the scrollbar thumb belongs, when there is scrollback.
    pub(crate) thumb: Option<(f32, f32)>,
    /// Which half of the blink cycle is showing.
    pub(crate) blink_shows: bool,
    /// Whether the last frame used a blink at all, so an idle session asks
    /// for no repaint on the cycle.
    pub(crate) blink_in_use: bool,
}

/// One session and everything drawing it needs.
pub(crate) struct TerminalState {
    /// The session, its pseudo-terminal and its emulator.
    pub(crate) session: Session,
    /// The style it draws with (REQ-12, REQ-14 to REQ-17).
    pub(crate) style: TerminalStyle,
    /// What the painter draws from (REQ-13 to REQ-16).
    pub(crate) painted: Painted,
    /// Whether the session draws as focused: its view has the focus and its
    /// window is active. A filled cursor needs both, and an inactive window
    /// shows the hollow one (REQ-15).
    pub(crate) focused: bool,
    /// The title the program set, while it has set one (REQ-07).
    pub(crate) title: Option<SharedString>,
    /// The last failure, which the board can show beside its own warnings.
    pub(crate) failure: Option<SessionError>,
    /// The encoder and the composition one session types through (REQ-18).
    pub(crate) typing: Typing,
    /// The clipboard sequences the program sends and the replies it reads,
    /// which the view carries between the callbacks and GPUI's clipboard
    /// (REQ-24).
    pub(crate) clipboard: Rc<RefCell<ClipboardEffects>>,
    /// Where the last layout put the session and how big one cell is, which
    /// places the cursor and turns a window position into a cell (REQ-18,
    /// REQ-23).
    pub(crate) viewport: Viewport,
    /// The render state and the iterators, reused for every frame.
    reader: FrameReader,
    /// Build the next frame even if the emulator reports no change, which a
    /// new selection and a new style both need (REQ-12, REQ-23).
    pub(crate) rebuild: bool,
}

impl TerminalState {
    /// Start a session and the state that draws it, which is everything
    /// behind one tab (REQ-08, REQ-12).
    pub(crate) fn start(
        options: &SessionOptions,
        style: TerminalStyle,
    ) -> Result<Self, SessionError> {
        let session = Session::start(options, SessionSize::default())?;
        Self::new(session, FrameReader::new()?, style)
    }

    /// The state for a session that has already started.
    pub(crate) fn new(
        mut session: Session,
        reader: FrameReader,
        style: TerminalStyle,
    ) -> Result<Self, SessionError> {
        let clipboard = ClipboardEffects::new(style.clipboard);
        effects::install(&mut session.terminal, &clipboard)?;
        Ok(Self {
            clipboard,
            viewport: Viewport {
                bounds: Bounds::default(),
                cell: UNMEASURED_CELL,
                padding: style.padding,
                scale_factor: UNMEASURED_SCALE,
            },
            session,
            style,
            painted: Painted {
                blink_shows: true,
                ..Painted::default()
            },
            focused: false,
            title: None,
            failure: None,
            typing: Typing::new()?,
            reader,
            rebuild: true,
        })
    }

    /// Give the emulator the style the state holds: the palette, the default
    /// colours and the cursor the program resets to (REQ-12, REQ-15).
    pub(crate) fn apply_style(&mut self) {
        let applied = style::apply_to_terminal(&self.style, &mut self.session.terminal);
        self.failure = applied.err();
    }

    /// Draw with a new style and rebuild the next frame from it: the colours
    /// are resolved into a frame's runs, so a recoloured session has to build
    /// them again (REQ-12, REQ-14, REQ-33).
    /// Measure one cell, fit the grid to the area and keep the frame current.
    ///
    /// The element calls this from `prepaint`, so a session learns its size
    /// before anything is drawn on it (REQ-17).
    pub(crate) fn lay_out(&mut self, bounds: Bounds<Pixels>, window: &Window) -> CellMetrics {
        let cell = self.measure(window);
        self.viewport = Viewport {
            bounds,
            cell,
            padding: self.style.padding,
            scale_factor: window.scale_factor(),
        };
        self.fit(bounds, cell);
        self.refresh();
        cell
    }

    /// One cell, measured in the session's own font (REQ-14).
    pub(crate) fn measure(&self, window: &Window) -> CellMetrics {
        measure_cell(&self.style.fonts, window)
    }

    /// Apply everything the reader thread has sent, and report whether a
    /// repaint is worth asking for (REQ-09).
    pub(crate) fn drain(&mut self) -> bool {
        if !self.session.drain_output().changed() {
            return false;
        }
        self.read_title();
        true
    }

    /// Show the other half of the blink cycle, and report whether the last
    /// frame used a blink at all. The painter reads the phase itself, so a
    /// blink costs no frame rebuild.
    pub(crate) fn flip_blink(&mut self) -> bool {
        self.painted.blink_shows = !self.painted.blink_shows;
        self.painted.blink_in_use
    }

    /// Scroll by whole rows, up for a negative count (REQ-16).
    pub(crate) fn scroll_rows(&mut self, rows: isize) -> bool {
        if rows == 0 {
            return false;
        }
        self.session
            .terminal
            .scroll_viewport(ScrollViewport::Delta(rows));
        self.rebuild = true;
        true
    }

    /// Scroll with the wheel, applying `mouse-scroll-multiplier` (REQ-16).
    pub(crate) fn scroll_wheel(&mut self, lines: f32) -> bool {
        self.scroll_rows(wheel_rows(lines, self.style.mouse_scroll_multiplier))
    }

    /// Scroll by pages, up for a negative count, for Shift+Page (REQ-16).
    pub(crate) fn scroll_pages(&mut self, pages: f32) {
        self.scroll_rows(page_rows(pages, self.session.size().rows));
    }

    /// Scroll to a position on the scrollbar track, as a fraction from its
    /// top, which is what a scrollbar drag produces (REQ-16).
    pub(crate) fn scroll_to_fraction(&mut self, fraction: f32) {
        let Some(scrollbar) = self.scrollbar() else {
            return;
        };
        self.scroll_to(ScrollViewport::Row(row_at(
            fraction,
            scrollbar.total,
            scrollbar.len,
        )));
    }

    /// Follow new output again (REQ-16).
    pub(crate) fn scroll_to_bottom(&mut self) {
        self.scroll_to(ScrollViewport::Bottom);
    }

    /// Move the viewport and rebuild the next frame from its new rows.
    fn scroll_to(&mut self, scroll: ScrollViewport) {
        self.session.terminal.scroll_viewport(scroll);
        self.rebuild = true;
    }

    /// Fit the grid to the area and tell the session, unless nothing changed
    /// or the area holds less than one cell (REQ-17).
    fn fit(&mut self, bounds: Bounds<Pixels>, cell: CellMetrics) {
        let Some((columns, rows)) = fit_grid(
            f32::from(bounds.size.width),
            f32::from(bounds.size.height),
            self.style.padding,
            cell,
        ) else {
            return;
        };
        let size = session_size(columns, rows, cell);
        if size == self.session.size() {
            return;
        }
        self.failure = self.session.resize(size).err();
        self.rebuild = true;
    }

    /// Rebuild the frame when the emulator changed, and keep the last one
    /// otherwise: the dirty tracking decides whether a frame is rebuilt.
    fn refresh(&mut self) {
        let selection = selection_colors(&self.style.palette);
        let rebuild = std::mem::take(&mut self.rebuild);
        match read_frame(&mut self.reader, &self.session.terminal, selection, rebuild) {
            Ok(Some(frame)) => {
                self.painted.blink_in_use = blinks(&frame);
                self.painted.frame = frame;
            }
            Ok(None) => (),
            Err(error) => self.failure = Some(error),
        }
        self.painted.thumb = self
            .scrollbar()
            .and_then(|scrollbar| thumb(scrollbar.total, scrollbar.offset, scrollbar.len));
    }

    /// The scrollbar the emulator reports, when it can report one (REQ-16).
    fn scrollbar(&self) -> Option<Scrollbar> {
        self.session.terminal.scrollbar().ok()
    }

    /// The title the program set, if it set one (REQ-07).
    pub(crate) fn read_title(&mut self) {
        self.title = self
            .session
            .terminal
            .title()
            .ok()
            .filter(|title| !title.is_empty())
            .map(|title| SharedString::from(title.to_owned()));
    }
}
