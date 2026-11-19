//! Copy, paste and the clipboard policy of the Ghostty configuration
//! (REQ-22, REQ-23, REQ-24).
//!
//! Everything here is a decision, not an effect: which clipboards a finished
//! selection is copied to, whether a paste has to be confirmed first, and
//! whether a program's own clipboard sequence is allowed. `crate::pointer`
//! and the view carry the decisions out through GPUI's clipboard, and
//! `crate::effects` answers the program's OSC 52 sequences with them.

use gpui_kit::{App, KeyBinding, actions};
use libghostty_vt::{Error, paste};
use subspace_ghostty_config::{ClipboardAccess, CopyOnSelect, GhosttyConfig};

use crate::error::{SessionError, emulator_error};

/// How much room the bracketed-paste markers need around a paste.
const PASTE_MARKERS: usize = 16;

actions!(
    subspace_terminal,
    [
        /// Put the session's selection on the system clipboard (REQ-22).
        CopySelection,
        /// Send the system clipboard to the program (REQ-22).
        PasteClipboard
    ]
);

/// Where a copy goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CopyTarget {
    /// The system clipboard, which every platform has.
    Clipboard,
    /// The X11 and Wayland primary selection, which only Linux has.
    Primary,
}

/// The clipboard values of the Ghostty configuration a session applies
/// (REQ-22 to REQ-24).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerminalClipboard {
    /// `clipboard-read`: whether a program may read the clipboard.
    pub read: ClipboardAccess,
    /// `clipboard-write`: whether a program may set it.
    pub write: ClipboardAccess,
    /// `clipboard-paste-protection`: whether a paste that could run a
    /// command is confirmed first.
    pub paste_protection: bool,
    /// `clipboard-paste-bracketed-safe`: whether bracketed paste counts as
    /// safe on its own.
    pub paste_bracketed_safe: bool,
    /// `copy-on-select`: which clipboards a finished selection goes to.
    pub copy_on_select: CopyOnSelect,
}

impl TerminalClipboard {
    /// The configuration's clipboard values.
    pub(crate) fn from_config(config: &GhosttyConfig) -> Self {
        Self {
            read: config.clipboard_read,
            write: config.clipboard_write,
            paste_protection: config.clipboard_paste_protection,
            paste_bracketed_safe: config.clipboard_paste_bracketed_safe,
            copy_on_select: config.copy_on_select,
        }
    }

    /// Whether a program's clipboard write is honoured. `ask` behaves as
    /// `deny`, because this delivery has no prompt (REQ-24).
    pub(crate) fn allows_program_write(self) -> bool {
        self.write == ClipboardAccess::Allow
    }

    /// Whether a program's clipboard read is answered. Only an explicit
    /// `allow` does; everything else is refused with no reply and no
    /// content (REQ-24).
    pub(crate) fn allows_program_read(self) -> bool {
        self.read == ClipboardAccess::Allow
    }

    /// Whether a paste has to be confirmed before it is sent (REQ-22).
    ///
    /// Only text that could run a command needs confirming: a line break
    /// while bracketed paste is off. With `clipboard-paste-bracketed-safe`
    /// a bracketed paste is safe whatever it holds, because the program
    /// receives it as one block.
    pub(crate) fn confirms_paste(self, text: &str, bracketed: bool) -> bool {
        if !self.paste_protection {
            return false;
        }
        if bracketed {
            return !self.paste_bracketed_safe;
        }
        text.contains('\n') || text.contains('\r')
    }

    /// Which clipboards a finished selection is copied to (REQ-23).
    pub(crate) fn copy_on_select(self) -> &'static [CopyTarget] {
        destinations(self.copy_on_select)
    }
}

impl Default for TerminalClipboard {
    fn default() -> Self {
        Self::from_config(&GhosttyConfig::default())
    }
}

/// The `copy-on-select` destinations, by platform: macOS and Windows have no
/// primary selection, so `primary` copies nothing there and `both` copies to
/// the system clipboard alone (REQ-23).
#[cfg(any(target_os = "macos", windows))]
fn destinations(value: CopyOnSelect) -> &'static [CopyTarget] {
    match value {
        CopyOnSelect::None | CopyOnSelect::Primary => &[],
        CopyOnSelect::Clipboard | CopyOnSelect::Both => &[CopyTarget::Clipboard],
    }
}

/// The `copy-on-select` destinations, by platform (REQ-23).
#[cfg(not(any(target_os = "macos", windows)))]
fn destinations(value: CopyOnSelect) -> &'static [CopyTarget] {
    match value {
        CopyOnSelect::None => &[],
        CopyOnSelect::Primary => &[CopyTarget::Primary],
        CopyOnSelect::Clipboard => &[CopyTarget::Clipboard],
        CopyOnSelect::Both => &[CopyTarget::Primary, CopyTarget::Clipboard],
    }
}

/// The bytes a paste sends the program: the text with unsafe control bytes
/// removed, wrapped in the bracketed-paste markers when the program enabled
/// that mode (REQ-22).
pub(crate) fn encode_paste(text: &str, bracketed: bool) -> Result<Vec<u8>, SessionError> {
    let mut data = text.as_bytes().to_vec();
    let mut encoded = vec![0u8; data.len() + PASTE_MARKERS];
    match paste::encode(&mut data, bracketed, &mut encoded) {
        Ok(written) => Ok(truncated(encoded, written)),
        Err(Error::OutOfSpace { required }) => {
            let mut data = text.as_bytes().to_vec();
            encoded.resize(required, 0);
            let written =
                paste::encode(&mut data, bracketed, &mut encoded).map_err(emulator_error)?;
            Ok(truncated(encoded, written))
        }
        Err(error) => Err(emulator_error(error)),
    }
}

/// The encoded bytes, with the room the encoder did not need dropped.
fn truncated(mut encoded: Vec<u8>, written: usize) -> Vec<u8> {
    encoded.truncate(written);
    encoded
}

/// Bind copy and paste in the session's key context: Cmd on macOS,
/// Ctrl+Shift elsewhere, which is what the shell leaves free (REQ-19,
/// REQ-20, REQ-22).
pub(crate) fn bind_keys(context: &str, cx: &mut App) {
    let context = Some(context);
    #[cfg(target_os = "macos")]
    cx.bind_keys([
        KeyBinding::new("cmd-c", CopySelection, context),
        KeyBinding::new("cmd-v", PasteClipboard, context),
    ]);
    #[cfg(not(target_os = "macos"))]
    cx.bind_keys([
        KeyBinding::new("ctrl-shift-c", CopySelection, context),
        KeyBinding::new("ctrl-shift-v", PasteClipboard, context),
    ]);
}

#[cfg(test)]
mod tests {
    use subspace_ghostty_config::{ClipboardAccess, CopyOnSelect, GhosttyConfig};

    use crate::clipboard::{CopyTarget, TerminalClipboard};

    /// The clipboard policy of a configuration with the given values.
    fn policy(read: ClipboardAccess, write: ClipboardAccess) -> TerminalClipboard {
        TerminalClipboard::from_config(&GhosttyConfig {
            clipboard_read: read,
            clipboard_write: write,
            ..GhosttyConfig::default()
        })
    }

    /// The policy for one `copy-on-select` value.
    fn on_select(value: CopyOnSelect) -> TerminalClipboard {
        TerminalClipboard {
            copy_on_select: value,
            ..TerminalClipboard::default()
        }
    }

    #[test]
    fn with_no_configuration_a_program_may_write_the_clipboard_but_not_read_it() {
        let default = TerminalClipboard::default();

        assert!(default.allows_program_write(), "clipboard-write is allow");
        assert!(!default.allows_program_read(), "clipboard-read is ask");
    }

    #[test]
    fn ask_behaves_as_deny_for_both_directions() {
        let asked = policy(ClipboardAccess::Ask, ClipboardAccess::Ask);
        let denied = policy(ClipboardAccess::Deny, ClipboardAccess::Deny);
        let allowed = policy(ClipboardAccess::Allow, ClipboardAccess::Allow);

        assert!(!asked.allows_program_write());
        assert!(!asked.allows_program_read());
        assert!(!denied.allows_program_write());
        assert!(!denied.allows_program_read());
        assert!(allowed.allows_program_write());
        assert!(allowed.allows_program_read());
    }

    #[test]
    fn a_paste_with_a_line_break_is_confirmed_only_while_it_could_run() {
        let default = TerminalClipboard::default();

        assert!(
            default.confirms_paste("one\ntwo", false),
            "unbracketed, so the program would run it"
        );
        assert!(!default.confirms_paste("one\ntwo", true), "bracketed-safe");
        assert!(!default.confirms_paste("one two", false), "no line break");
        assert!(
            default.confirms_paste("one\rtwo", false),
            "a carriage return runs a command too"
        );
    }

    #[test]
    fn the_two_protection_values_switch_the_decision_off() {
        let unprotected = TerminalClipboard {
            paste_protection: false,
            ..TerminalClipboard::default()
        };
        let unsafe_bracketed = TerminalClipboard {
            paste_bracketed_safe: false,
            ..TerminalClipboard::default()
        };

        assert!(!unprotected.confirms_paste("one\ntwo", false));
        assert!(unsafe_bracketed.confirms_paste("one\ntwo", true));
        assert!(unsafe_bracketed.confirms_paste("one two", true));
    }

    #[test]
    fn copy_on_select_follows_the_value_and_the_platform() {
        let no_primary = cfg!(any(target_os = "macos", windows));

        assert!(on_select(CopyOnSelect::None).copy_on_select().is_empty());
        assert_eq!(
            on_select(CopyOnSelect::Clipboard).copy_on_select(),
            [CopyTarget::Clipboard]
        );
        assert_eq!(
            on_select(CopyOnSelect::Primary).copy_on_select().is_empty(),
            no_primary,
            "macOS and Windows have no primary selection"
        );
        assert_eq!(
            on_select(CopyOnSelect::Both).copy_on_select().len(),
            if no_primary { 1 } else { 2 }
        );
    }

    #[test]
    fn the_default_copy_on_select_follows_the_platform() {
        let destinations = TerminalClipboard::default().copy_on_select();

        if cfg!(any(target_os = "macos", windows)) {
            assert!(destinations.is_empty(), "the default is none");
        } else {
            assert_eq!(destinations, [CopyTarget::Primary]);
        }
    }
}
