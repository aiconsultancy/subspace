//! Sending one session what the user typed (REQ-18, REQ-31a).
//!
//! The view's key listener and its platform input handler call these; the
//! session's state only holds the encoder and the composition
//! (`crate::input::Typing`), so this module is where the two meet. A
//! `keybind` line that sends bytes is checked before the encoder, and
//! committed text goes through the encoder as well, so the program's modes
//! apply to it.

use gpui_kit::Keystroke;

use crate::error::SessionError;
use crate::input::ime;
use crate::input::keys::{self, KeyPress};
use crate::input::writer::KeyWriter;
use crate::metrics::{self, Caret};
use crate::session::Session;
use crate::state::TerminalState;
use crate::style::TerminalStyle;

/// Send one key press to the program, encoded for its current modes, and
/// report whether the program received anything, which is what decides
/// whether the key stops there (REQ-18, REQ-31a).
pub(crate) fn type_keystroke(state: &mut TerminalState, keystroke: &Keystroke) -> bool {
    let press = keys::key_press(keystroke);
    let TerminalState {
        session,
        style,
        typing,
        ..
    } = state;
    match write_press(session, style, &mut typing.keys, &press) {
        Ok(sent) => sent,
        Err(error) => {
            state.failure = Some(error);
            true
        }
    }
}

/// Send text an input method committed, which the composition no longer
/// holds (REQ-18).
pub(crate) fn commit_text(state: &mut TerminalState, text: &str) {
    state.typing.pre_edit.clear();
    if text.is_empty() {
        return;
    }
    let TerminalState {
        session, typing, ..
    } = state;
    state.failure = write_text(session, &mut typing.keys, text).err();
}

/// Where the cursor sits on screen, which places an input method's
/// candidate window and the pre-edit text drawn in place (REQ-18).
pub(crate) fn caret(state: &TerminalState) -> Caret {
    let cursor = state
        .painted
        .frame
        .cursor
        .as_ref()
        .map_or((0, 0), |cursor| (cursor.column, cursor.row));
    metrics::caret(state.viewport, cursor, state.session.size().columns)
}

/// The composition in progress and where to draw it: the marked text,
/// clipped to the columns left on the cursor's row (REQ-18).
pub(crate) fn composition(state: &TerminalState) -> Option<(String, Caret)> {
    if !state.typing.pre_edit.is_active() {
        return None;
    }
    let at = caret(state);
    let text = ime::clipped(state.typing.pre_edit.text(), at.columns_left);
    Some((text.to_owned(), at))
}

/// Send one key press: the bytes a `keybind` line names, else the bytes the
/// encoder produces for the program's current modes (REQ-18, REQ-31a).
///
/// Reports whether the program received anything, so a press the program
/// ignores, such as a bare modifier, leaves the key for the shell.
fn write_press(
    session: &mut Session,
    style: &TerminalStyle,
    keys: &mut KeyWriter,
    press: &KeyPress,
) -> Result<bool, SessionError> {
    if let Some(bytes) = style.keybinds.bytes_for(press.key, press.mods) {
        session.write(bytes)?;
        return Ok(true);
    }
    let bytes = keys.encode(&session.terminal, press)?;
    if bytes.is_empty() {
        return Ok(false);
    }
    session.write(bytes).map(|()| true)
}

/// Send committed text through the encoder, so the program's modes still
/// apply to it (REQ-18).
fn write_text(session: &mut Session, keys: &mut KeyWriter, text: &str) -> Result<(), SessionError> {
    let bytes = keys.encode_text(&session.terminal, text)?;
    session.write(bytes)
}
