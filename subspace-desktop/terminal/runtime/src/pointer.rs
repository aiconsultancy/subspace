//! The pointer, against the element's own hitbox (REQ-16, REQ-23).
//!
//! A session's pointer has two meanings, and the program chooses which:
//! while it asks for the mouse, every press, release and move is reported to
//! it in the mode it selected; otherwise the same events drive libghostty's
//! selection gestures. Shift overrides the program, so text stays selectable
//! in a full-screen program (REQ-23).
//!
//! The listeners are registered per painted frame, as GPUI expects, and they
//! only touch the session's shared state: the element may not reach the view
//! it was rendered from. What a finished selection is copied to is the
//! view's own work, which it reads back through [`Pointer::copied`].

// qual:allow(coupling, sdp) reason: "The pointer is the crate's one pointer driver: it drives the mouse encoder, the selection gestures, the clipboard destinations and the scroll arithmetic, and only the element and the view depend on it. Every dependency points at a leaf module that nothing in this crate depends upon in turn."

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

use gpui_kit::{
    DispatchPhase, Hitbox, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels,
    Point, Window,
};
use libghostty_vt::mouse::Position;

use crate::error::SessionError;
use crate::mouse::{MouseWriter, PointerEvent, Reported};
use crate::pointer::report::{report, reports};
use crate::scroll;
use crate::selection::{self, Gestures};
use crate::state::TerminalState;

mod report;

/// A click count beyond which libghostty's gestures repeat, so the second
/// and third clicks of a sequence are the ones that change the granularity.
const DRAG_BUTTON: MouseButton = MouseButton::Left;

/// One session's pointer: the encoder a reporting program reads, the
/// gestures a selection uses, and what the view still has to act on.
pub(crate) struct Pointer {
    /// The mouse encoder (REQ-23).
    mouse: MouseWriter,
    /// The selection gestures (REQ-23).
    gestures: Gestures,
    /// Whether a button is down, which the encoder reports with a motion.
    pressed: bool,
    /// Whether a selection gesture is in progress, so a drag keeps
    /// extending it even if the user lets shift go (REQ-23).
    selecting: bool,
    /// Whether the scrollbar thumb is being dragged (REQ-16).
    dragging_scrollbar: bool,
    /// The text a finished selection asks the view to copy (REQ-23).
    copied: Option<String>,
    /// When the session's pointer started, so a press carries a monotonic
    /// time and libghostty can count double and triple clicks (REQ-23).
    clock: Instant,
}

impl Pointer {
    /// The pointer state for one session.
    pub(crate) fn new() -> Result<Rc<RefCell<Self>>, SessionError> {
        Ok(Rc::new(RefCell::new(Self {
            mouse: MouseWriter::new()?,
            gestures: Gestures::new()?,
            pressed: false,
            selecting: false,
            dragging_scrollbar: false,
            copied: None,
            clock: Instant::now(),
        })))
    }

    /// Take the text a finished selection asks the view to copy, which the
    /// view puts on the clipboards `copy-on-select` names (REQ-23).
    pub(crate) fn take_copied(&mut self) -> Option<String> {
        self.copied.take()
    }
}

/// The session's selection as text, and nothing when there is none
/// (REQ-22).
pub(crate) fn selected_text(state: &TerminalState) -> Option<String> {
    selection::text(&state.session.terminal)
}

/// Whether the program is taking the pointer, so a right click belongs to
/// it rather than to the session's own menu (REQ-23).
pub(crate) fn offers_menu(state: &TerminalState) -> bool {
    !crate::mouse::reporting(&state.session.terminal)
}

/// What one pointer listener works on.
pub(crate) struct PointerTarget {
    /// The session's shared state.
    pub(crate) state: Rc<RefCell<TerminalState>>,
    /// The session's pointer.
    pub(crate) pointer: Rc<RefCell<Pointer>>,
}

impl PointerTarget {
    /// Another handle on the same session.
    fn clone(&self) -> Self {
        Self {
            state: Rc::clone(&self.state),
            pointer: Rc::clone(&self.pointer),
        }
    }
}

/// Register the press, release and move listeners against the element's
/// hitbox (REQ-16, REQ-23).
pub(crate) fn listen(session: &PointerTarget, hitbox: &Hitbox, window: &mut Window) {
    on_down(session.clone(), hitbox.clone(), window);
    on_up(session.clone(), hitbox.clone(), window);
    on_move(session.clone(), hitbox.clone(), window);
}

/// A press starts a selection, reports to the program, or takes the
/// scrollbar thumb.
fn on_down(session: PointerTarget, hitbox: Hitbox, window: &mut Window) {
    window.on_mouse_event(move |event: &MouseDownEvent, phase, window, _cx| {
        if phase != DispatchPhase::Bubble || !hitbox.is_hovered(window) {
            return;
        }
        if press(&session, event) {
            window.refresh();
        }
    });
}

/// A release ends the gesture and leaves the selection it produced.
fn on_up(session: PointerTarget, hitbox: Hitbox, window: &mut Window) {
    window.on_mouse_event(move |event: &MouseUpEvent, phase, window, _cx| {
        if phase != DispatchPhase::Bubble {
            return;
        }
        let _ = &hitbox;
        if release(&session, event) {
            window.refresh();
        }
    });
}

/// A move extends the gesture, drags the scrollbar or reports to the
/// program.
fn on_move(session: PointerTarget, hitbox: Hitbox, window: &mut Window) {
    window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, _cx| {
        if phase != DispatchPhase::Bubble || !hitbox.is_hovered(window) {
            return;
        }
        if moved(&session, event) {
            window.refresh();
        }
    });
}

/// What a press does, and whether the session has to be repainted.
pub(crate) fn press(session: &PointerTarget, event: &MouseDownEvent) -> bool {
    let mut state = session.state.borrow_mut();
    let mut pointer = session.pointer.borrow_mut();
    pointer.pressed = true;
    if on_scrollbar(&state, event.position) && event.button == DRAG_BUTTON {
        pointer.dragging_scrollbar = true;
        return scroll_to(&mut state, event.position);
    }
    if reports(&state, event.modifiers) {
        let reported = pointer_event(&state, event);
        // The program owns this click, so no click sequence survives it.
        pointer.selecting = false;
        pointer.gestures.reset(&state.session.terminal);
        pointer.mouse.reset();
        selection::clear(&state.session.terminal);
        state.rebuild = true;
        return report(&mut state, &mut pointer, reported);
    }
    if event.button != DRAG_BUTTON {
        return false;
    }
    pointer.selecting = true;
    let at = position(&state, event.position);
    select(&mut state, &mut pointer, at)
}

/// What a release does, and whether the session has to be repainted.
pub(crate) fn release(session: &PointerTarget, event: &MouseUpEvent) -> bool {
    let mut state = session.state.borrow_mut();
    let mut pointer = session.pointer.borrow_mut();
    pointer.pressed = false;
    if std::mem::take(&mut pointer.dragging_scrollbar) {
        return false;
    }
    if !pointer.selecting && reports(&state, event.modifiers) {
        let reported = pointer_event(&state, event);
        return report(&mut state, &mut pointer, reported);
    }
    if event.button != DRAG_BUTTON {
        return false;
    }
    pointer.selecting = false;
    finish(&mut state, &mut pointer)
}

/// What a move does, and whether the session has to be repainted.
pub(crate) fn moved(session: &PointerTarget, event: &MouseMoveEvent) -> bool {
    let mut state = session.state.borrow_mut();
    let mut pointer = session.pointer.borrow_mut();
    if pointer.dragging_scrollbar {
        return scroll_to(&mut state, event.position);
    }
    if !pointer.selecting && reports(&state, event.modifiers) {
        let reported = pointer_event(&state, event);
        return report(&mut state, &mut pointer, reported);
    }
    if !pointer.selecting || event.pressed_button != Some(DRAG_BUTTON) {
        return false;
    }
    let at = position(&state, event.position);
    extend(&mut state, &mut pointer, at)
}

/// Start a click sequence at a position.
fn select(state: &mut TerminalState, pointer: &mut Pointer, at: Position) -> bool {
    let viewport = state.viewport;
    let since = pointer.clock.elapsed();
    if let Err(error) = pointer
        .gestures
        .press(&state.session.terminal, viewport, at, since)
    {
        state.failure = Some(error);
    }
    state.rebuild = true;
    true
}

/// Extend the click sequence to a position.
fn extend(state: &mut TerminalState, pointer: &mut Pointer, at: Position) -> bool {
    let viewport = state.viewport;
    if let Err(error) = pointer.gestures.drag(&state.session.terminal, viewport, at) {
        state.failure = Some(error);
    }
    state.rebuild = true;
    true
}

/// End the click sequence, keeping the text a finished selection asks the
/// view to copy (REQ-23).
fn finish(state: &mut TerminalState, pointer: &mut Pointer) -> bool {
    let selected = pointer.gestures.selected(&state.session.terminal);
    if let Err(error) = pointer.gestures.release(&state.session.terminal) {
        state.failure = Some(error);
    }
    if selected {
        pointer.copied = selection::text(&state.session.terminal);
    }
    state.rebuild = true;
    true
}

/// Move the viewport to the place on the scrollbar track the pointer is at
/// (REQ-16).
fn scroll_to(state: &mut TerminalState, position: Point<Pixels>) -> bool {
    let bounds = state.viewport.bounds;
    let fraction = scroll::track_fraction(
        f32::from(position.y),
        f32::from(bounds.origin.y),
        f32::from(bounds.size.height),
    );
    state.scroll_to_fraction(fraction);
    true
}

/// Whether a position is over the scrollbar, which is only on screen while
/// there is scrollback to show (REQ-16).
fn on_scrollbar(state: &TerminalState, position: Point<Pixels>) -> bool {
    let bounds = state.viewport.bounds;
    scroll::on_scrollbar(
        f32::from(position.x),
        f32::from(bounds.origin.x + bounds.size.width),
        state.painted.thumb,
    )
}

/// Where a window position sits inside the session.
fn position(state: &TerminalState, at: Point<Pixels>) -> Position {
    crate::mouse::position_in(state.viewport, at)
}

/// One pointer event as the encoder takes it.
fn pointer_event(state: &TerminalState, event: &impl Reported) -> PointerEvent {
    crate::mouse::pointer_event(event, position(state, event.at()))
}
