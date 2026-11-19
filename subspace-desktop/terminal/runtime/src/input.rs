//! Keys, input methods and `keybind` lines (REQ-18, REQ-19, REQ-31a).
//!
//! The view's focusable `div` owns the key listener and the
//! `EntityInputHandler` (`crate::view::keys`); this module holds everything
//! those two need and nothing that touches GPUI's event plumbing:
//!
//! - [`keys`] turns one `Keystroke` into the five facts libghostty encodes
//!   from.
//! - [`writer::KeyWriter`] keeps the libghostty encoder and reads the
//!   program's modes before every encode.
//! - [`keybind::TerminalKeybinds`] holds the `keybind` lines that send bytes,
//!   which are checked before the encoder.
//! - [`ime`] holds the pre-edit text of a composition in progress.

pub(crate) mod ime;
mod keybind;
pub(crate) mod keys;
pub(crate) mod writer;

pub use keybind::TerminalKeybinds;

use ime::PreEdit;
use writer::KeyWriter;

/// What one session types through: the key encoder, which reads the
/// program's modes before every encode, and the marked text of a
/// composition, which the program never sees until the input method commits
/// it (REQ-18).
///
/// `crate::typing` drives both; the session's state only holds them.
pub(crate) struct Typing {
    /// The key encoder.
    pub(crate) keys: KeyWriter,
    /// The marked text of a composition in progress.
    pub(crate) pre_edit: PreEdit,
}

impl Typing {
    /// The encoder and the empty composition one session starts with.
    pub(crate) fn new() -> Result<Self, crate::error::SessionError> {
        Ok(Self {
            keys: KeyWriter::new()?,
            pre_edit: PreEdit::default(),
        })
    }
}
