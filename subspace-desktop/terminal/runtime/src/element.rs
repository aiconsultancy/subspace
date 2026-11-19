//! The custom GPUI element that draws one session's viewport, and the input
//! wiring only `paint` may do (REQ-12 to REQ-18, REQ-23).
//!
//! `request_layout` claims the available area, `prepaint` measures one cell,
//! fits the grid and reports the new size to the session, and `paint` draws:
//! row backgrounds, then the runs with their underline and strike, then the
//! cursor, then the scrollbar. Selected cells carry the selection pair in
//! their own look, so the selection needs no pass of its own. It draws from
//! the shared [`TerminalState`] and never touches the view's entity, because
//! GPUI renders a view inside that entity's own update.
//!
//! `paint` also registers, once per frame, the wheel (REQ-16 applies the
//! `mouse-scroll-multiplier`), the platform input handler for the focused
//! session (REQ-18) and the pointer listeners (REQ-23).
//!
//! A cell whose whole cluster is one Powerline, box-drawing or block code
//! point is drawn by [`crate::sprite`] instead of being shaped, which
//! [`crate::frame`] has already separated from the runs.

// qual:allow(coupling, max_instability=0.91) reason: "The element is the crate's one painter and the one place GPUI allows input wiring: it draws the frame, the fonts, the sprites, the palette, the metrics and the scrollbar of a session and registers its pointer and input-method listeners, and only the view and its own cursor module depend on it. Every dependency points at a leaf module, which is the direction the stable-dependencies principle asks for."

mod cursor;

use std::cell::RefCell;
use std::rc::Rc;

use gpui_kit::{
    App, Bounds, DispatchPhase, Element, ElementId, ElementInputHandler, Entity,
    EntityInputHandler, FocusHandle, GlobalElementId, Hitbox, HitboxBehavior, Hsla,
    InspectorElementId, IntoElement, LayoutId, Pixels, Point, ScrollEvent, SharedString, Style,
    TextAlign, Window, fill, point, px, relative, size,
};
use subspace_ghostty_config::Rgb;

use crate::element::cursor::paint_cursor;
use crate::font::text_run;
use crate::frame::{BackgroundSpan, CellLook, SpriteSpan, TextRunSpan};
use crate::metrics::{self, CellMetrics};
use crate::palette::paint_color;
use crate::pointer::{self, Pointer};
use crate::scroll::SCROLLBAR_WIDTH;
use crate::sprite;
use crate::state::TerminalState;
use crate::style::TerminalFonts;

/// The whole of the area offered, as a fraction of it.
const WHOLE: f32 = 1.0;
/// How much of the foreground colour the scrollbar thumb keeps.
const SCROLLBAR_ALPHA: f32 = 0.35;

/// Draws one session's viewport.
///
/// The view it draws for is a type parameter, not the crate's own
/// [`TerminalView`](crate::TerminalView): the element is a leaf that the view
/// depends on, and naming the view here would make the two depend on each
/// other. All the element needs of it is that the platform can route
/// input-method events to it.
pub(crate) struct TerminalElement<V: EntityInputHandler> {
    state: Rc<RefCell<TerminalState>>,
    /// The session's pointer, which the press, release and move listeners
    /// drive (REQ-23).
    pointer: Rc<RefCell<Pointer>>,
    /// The view the element draws for, which the platform input handler
    /// delivers an input method's text to (REQ-18).
    view: Entity<V>,
    /// The view's focus, which decides whether the window takes platform
    /// input-method events for this session (REQ-18).
    focus: FocusHandle,
}

/// What `prepaint` leaves for `paint`.
pub(crate) struct Prepainted {
    /// The measured cell, which also decided the session's grid.
    cell: CellMetrics,
    /// The element's own hitbox, for the wheel and for task 09.
    hitbox: Hitbox,
}

/// Where a frame's cells land inside the element's bounds.
struct Grid {
    /// The top left of the first cell, with the padding already inside.
    origin: Point<Pixels>,
    /// The measured cell.
    cell: CellMetrics,
}

/// What the runs of a frame need to draw themselves.
struct RunPaint<'a> {
    /// Where the cells land.
    grid: &'a Grid,
    /// The families and the size to shape with.
    fonts: &'a TerminalFonts,
    /// Whether the blink phase currently shows blinking text (REQ-13).
    blink_shows: bool,
}

impl<V: EntityInputHandler> TerminalElement<V> {
    /// The element for the state a view draws from.
    pub(crate) fn new(
        state: Rc<RefCell<TerminalState>>,
        pointer: Rc<RefCell<Pointer>>,
        view: Entity<V>,
        focus: FocusHandle,
    ) -> Self {
        Self {
            state,
            pointer,
            view,
            focus,
        }
    }
}

impl<V: EntityInputHandler> IntoElement for TerminalElement<V> {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl<V: EntityInputHandler> Element for TerminalElement<V> {
    type RequestLayoutState = ();
    type PrepaintState = Prepainted;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        // The element claims the whole area the board gives it.
        let style = Style {
            size: size(relative(WHOLE).into(), relative(WHOLE).into()),
            ..Style::default()
        };
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut (),
        window: &mut Window,
        _cx: &mut App,
    ) -> Prepainted {
        let cell = self.state.borrow_mut().lay_out(bounds, window);
        Prepainted {
            cell,
            hitbox: window.insert_hitbox(bounds, HitboxBehavior::Normal),
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut (),
        prepaint: &mut Prepainted,
        window: &mut Window,
        cx: &mut App,
    ) {
        let state = self.state.borrow();
        let grid = Grid {
            origin: metrics::cell_origin(bounds.origin, state.style.padding, window.scale_factor()),
            cell: prepaint.cell,
        };
        paint_view(&state, &grid, bounds, window, cx);
        drop(state);
        window.handle_input(
            &self.focus,
            ElementInputHandler::new(bounds, self.view.clone()),
            cx,
        );
        on_wheel(&self.state, &prepaint.hitbox, grid.cell.height, window);
        pointer::listen(
            &pointer::PointerTarget {
                state: Rc::clone(&self.state),
                pointer: Rc::clone(&self.pointer),
            },
            &prepaint.hitbox,
            window,
        );
    }
}

/// Draw one session: its background, the row backgrounds, the runs with
/// their underline and strike, then the cursor and the scrollbar.
// qual:allow(test_quality, untested) reason: "Runs only while GPUI paints TerminalElement (Element::paint), a dispatch the call graph cannot follow; view_tests draws the element with render_frame."
fn paint_view(
    state: &TerminalState,
    grid: &Grid,
    bounds: Bounds<Pixels>,
    window: &mut Window,
    cx: &mut App,
) {
    window.paint_quad(fill(bounds, paint_color(state.style.palette.background)));
    for row in &state.painted.frame.rows {
        for span in &row.backgrounds {
            window.paint_quad(fill(span_bounds(span, row.row, grid), span.color));
        }
    }
    let runs = RunPaint {
        grid,
        fonts: &state.style.fonts,
        blink_shows: state.painted.blink_shows,
    };
    for row in &state.painted.frame.rows {
        for run in &row.runs {
            paint_run(run, row.row, &runs, window, cx);
        }
        for span in &row.sprites {
            paint_sprite(span, row.row, &runs, window);
        }
    }
    paint_cursor(state, grid, window, cx);
    paint_scrollbar(state, bounds, window);
}

/// The rectangle a background span covers.
fn span_bounds(span: &BackgroundSpan, row: u16, grid: &Grid) -> Bounds<Pixels> {
    Bounds::new(
        cell_position(span.column, row, grid),
        size(
            px(f32::from(span.columns) * grid.cell.width),
            px(grid.cell.height),
        ),
    )
}

/// The top left of one cell.
fn cell_position(column: u16, row: u16, grid: &Grid) -> Point<Pixels> {
    metrics::cell_point(grid.origin, grid.cell, column, row)
}

/// The rectangle one cell covers.
fn cell_rect(column: u16, row: u16, grid: &Grid) -> Bounds<Pixels> {
    metrics::cell_bounds(grid.origin, grid.cell, column, row)
}

/// Draw one cell this crate draws itself, filling the cell exactly
/// (REQ-13). Its look already carries the inverse and selection colours, so
/// the sprite needs no rule of its own.
// qual:allow(test_quality, untested) reason: "Runs only while GPUI paints TerminalElement (Element::paint), a dispatch the call graph cannot follow; view_tests draws the element with render_frame."
fn paint_sprite(span: &SpriteSpan, row: u16, paint: &RunPaint<'_>, window: &mut Window) {
    if span.look.blink && !paint.blink_shows {
        return;
    }
    sprite::paint(
        span.glyph,
        cell_rect(span.column, row, paint.grid),
        span.look.foreground,
        window,
    );
}

/// Shape and draw one run, with its underline and strike (REQ-13).
// qual:allow(test_quality, untested) reason: "Runs only while GPUI paints TerminalElement (Element::paint), a dispatch the call graph cannot follow; view_tests draws the element with render_frame."
fn paint_run(run: &TextRunSpan, row: u16, paint: &RunPaint<'_>, window: &mut Window, cx: &mut App) {
    if run.look.blink && !paint.blink_shows {
        return;
    }
    paint_text(
        SharedString::from(run.text.clone()),
        run.look,
        &TextPlace {
            origin: cell_position(run.column, row, paint.grid),
            line_height: paint.grid.cell.height,
            fonts: paint.fonts,
        },
        window,
        cx,
    );
}

/// Shape and draw one piece of text at a cell position.
// qual:allow(test_quality, untested) reason: "Runs only while GPUI paints TerminalElement (Element::paint), a dispatch the call graph cannot follow; view_tests draws the element with render_frame."
fn paint_text(
    text: SharedString,
    look: CellLook,
    at: &TextPlace<'_>,
    window: &mut Window,
    cx: &mut App,
) {
    let shaped = window.text_system().shape_line(
        text.clone(),
        at.fonts.size,
        &[text_run(text.len(), look, at.fonts)],
        None,
    );
    let _ = shaped.paint(
        at.origin,
        px(at.line_height),
        TextAlign::Left,
        None,
        window,
        cx,
    );
}

/// Where and with what one piece of text is drawn.
struct TextPlace<'a> {
    /// The top left of the first cell the text covers.
    origin: Point<Pixels>,
    /// The height of the row it sits in.
    line_height: f32,
    /// The families and the size to shape with.
    fonts: &'a TerminalFonts,
}

/// Draw the scrollback position on the right edge, when there is scrollback
/// to show (REQ-16).
// qual:allow(test_quality, untested) reason: "Runs only while GPUI paints TerminalElement (Element::paint), a dispatch the call graph cannot follow; view_tests draws the element with render_frame."
fn paint_scrollbar(state: &TerminalState, bounds: Bounds<Pixels>, window: &mut Window) {
    let Some((top, length)) = state.painted.thumb else {
        return;
    };
    let height = bounds.size.height;
    let thumb = Bounds::new(
        point(
            bounds.origin.x + bounds.size.width - px(SCROLLBAR_WIDTH),
            bounds.origin.y + height * top,
        ),
        size(px(SCROLLBAR_WIDTH), height * length),
    );
    window.paint_quad(fill(thumb, thumb_color(state.style.palette.foreground)));
}

/// The thumb's colour: the session's foreground, held back so it reads as a
/// control rather than as text.
fn thumb_color(foreground: Rgb) -> Hsla {
    Hsla {
        a: SCROLLBAR_ALPHA,
        ..paint_color(foreground)
    }
}

/// Scroll the session with the wheel, applying the Ghostty
/// `mouse-scroll-multiplier` (REQ-16).
// qual:allow(test_quality, untested) reason: "Runs only while GPUI paints TerminalElement (Element::paint), a dispatch the call graph cannot follow; view_tests draws the element with render_frame."
fn on_wheel(
    state: &Rc<RefCell<TerminalState>>,
    hitbox: &Hitbox,
    line_height: f32,
    window: &mut Window,
) {
    let state = Rc::clone(state);
    let hitbox = hitbox.clone();
    window.on_mouse_event(move |event: &ScrollEvent, phase, window, _cx| {
        if phase != DispatchPhase::Bubble || !hitbox.should_handle_scroll(window) {
            return;
        }
        let lines = event.delta.pixel_delta(px(line_height)).y / px(line_height);
        if state.borrow_mut().scroll_wheel(lines) {
            window.refresh();
        }
    });
}

#[cfg(test)]
mod tests {
    use gpui_kit::{Hsla, point, px, rgb, size};

    use crate::element::{Grid, cell_position, cell_rect, span_bounds, thumb_color};
    use crate::frame::BackgroundSpan;
    use crate::metrics::CellMetrics;

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

    #[test]
    fn a_cell_sits_at_its_column_and_row() {
        let position = cell_position(3, 2, &grid());

        assert_eq!(position, point(px(26.), px(34.)));
    }

    #[test]
    fn a_background_span_covers_every_column_it_holds() {
        let span = BackgroundSpan {
            column: 1,
            columns: 3,
            color: Hsla::from(rgb(0xe06c75)),
        };

        let bounds = span_bounds(&span, 0, &grid());

        assert_eq!(bounds.size, size(px(24.), px(16.)));
        assert_eq!(bounds.origin.x, px(10.));
    }

    #[test]
    fn a_sprite_cell_covers_the_whole_cell_it_sits_in() {
        let bounds = cell_rect(3, 2, &grid());

        assert_eq!(bounds.origin, point(px(26.), px(34.)));
        assert_eq!(bounds.size, size(px(8.), px(16.)));
    }

    #[test]
    fn the_scrollbar_thumb_holds_the_foreground_back() {
        let foreground = subspace_ghostty_config::Rgb {
            r: 0xab,
            g: 0xb2,
            b: 0xbf,
        };

        let color = thumb_color(foreground);

        assert!(color.a < 1.0);
        assert_eq!(color.h, Hsla::from(rgb(0xabb2bf)).h);
    }
}
