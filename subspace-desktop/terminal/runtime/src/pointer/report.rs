//! Reporting the pointer to a program that asked for it (REQ-23).
//!
//! While the program has mouse reporting on, a press, a release and a move
//! are encoded for the mode it selected and written to it, and the session
//! starts no selection of its own. Shift overrides that, which
//! [`reports`] answers for the listeners.

use gpui_kit::Modifiers;

use crate::mouse::{self, PointerEvent};
use crate::pointer::Pointer;
use crate::state::TerminalState;

/// Whether the program is taking the pointer: it asked for the mouse and
/// the user is not holding shift to select over it (REQ-23).
pub(super) fn reports(state: &TerminalState, modifiers: Modifiers) -> bool {
    !modifiers.shift && mouse::reporting(&state.session.terminal)
}

/// Send one pointer event to the program, and report whether it received
/// anything.
pub(super) fn report(
    state: &mut TerminalState,
    pointer: &mut Pointer,
    event: PointerEvent,
) -> bool {
    let viewport = state.viewport;
    let pressed = pointer.pressed;
    let TerminalState { session, .. } = state;
    let encoded = pointer
        .mouse
        .encode(&session.terminal, viewport, &event, pressed);
    match encoded {
        Ok(bytes) => !bytes.is_empty() && session.write(bytes).is_ok(),
        Err(error) => {
            state.failure = Some(error);
            false
        }
    }
}
