//! The libghostty key encoder one session keeps (REQ-18).
//!
//! The encoder and the event object are created once and reused, as
//! libghostty intends. [`KeyWriter::encode`] reads the program's current
//! modes with `set_options_from_terminal` before every encode, so
//! application cursor keys, the keypad modes and the Kitty keyboard protocol
//! stay in step with the program without the view tracking them.

use libghostty_vt::Terminal;
use libghostty_vt::key::{Action, Encoder, Event};

use crate::error::{SessionError, emulator_error};
use crate::input::keys::KeyPress;

/// How much room one encoded key usually needs, so a session's first
/// keystrokes do not grow the buffer.
const INITIAL_BYTES: usize = 32;

/// One session's key encoder, its event object and the bytes it produced
/// last.
pub(crate) struct KeyWriter {
    encoder: Encoder<'static>,
    event: Event<'static>,
    bytes: Vec<u8>,
}

impl KeyWriter {
    /// The encoder for one session.
    pub(crate) fn new() -> Result<Self, SessionError> {
        Ok(Self {
            encoder: Encoder::new().map_err(emulator_error)?,
            event: Event::new().map_err(emulator_error)?,
            bytes: Vec::with_capacity(INITIAL_BYTES),
        })
    }

    /// The bytes one key press sends the program in its current modes, which
    /// is empty for a press the program does not receive, such as a bare
    /// modifier.
    pub(crate) fn encode(
        &mut self,
        terminal: &Terminal<'static, 'static>,
        press: &KeyPress,
    ) -> Result<&[u8], SessionError> {
        self.encoder.set_options_from_terminal(terminal);
        describe(&mut self.event, press);
        self.bytes.clear();
        self.encoder
            .encode_to_vec(&self.event, &mut self.bytes)
            .map_err(emulator_error)?;
        Ok(&self.bytes)
    }

    /// The bytes committed text sends the program: the text itself, encoded
    /// for the program's modes through an unidentified key, which is what an
    /// input-method commit is (REQ-18).
    pub(crate) fn encode_text(
        &mut self,
        terminal: &Terminal<'static, 'static>,
        text: &str,
    ) -> Result<&[u8], SessionError> {
        self.encode(terminal, &KeyPress::from_text(text))
    }
}

/// Describe one key press in the reused event object.
fn describe(event: &mut Event<'static>, press: &KeyPress) {
    event.set_action(Action::Press);
    event.set_key(press.key);
    event.set_mods(press.mods);
    event.set_consumed_mods(press.consumed);
    event.set_composing(false);
    event.set_utf8(press.text.clone());
    if let Some(codepoint) = press.unshifted {
        event.set_unshifted_codepoint(codepoint);
    }
}

#[cfg(test)]
mod tests {
    use libghostty_vt::Terminal;
    use libghostty_vt::key::{Key, Mods};
    use libghostty_vt::terminal::Mode;

    use crate::input::keys::KeyPress;
    use crate::input::writer::KeyWriter;

    /// A press of one key with the given modifiers and no typed text.
    fn press(key: Key, mods: Mods) -> KeyPress {
        KeyPress {
            key,
            mods,
            consumed: Mods::empty(),
            text: None,
            unshifted: None,
        }
    }

    /// An emulator a test encodes against.
    fn terminal() -> Terminal<'static, 'static> {
        Terminal::new(20, 3).expect("a terminal")
    }

    #[test]
    fn a_control_combination_encodes_its_control_byte() {
        let mut writer = KeyWriter::new().expect("an encoder");
        let terminal = terminal();

        let bytes = writer
            .encode(&terminal, &press(Key::C, Mods::CTRL))
            .expect("the key should encode");

        assert_eq!(bytes, b"\x03");
    }

    #[test]
    fn the_arrow_keys_follow_the_programs_cursor_key_mode() {
        let mut writer = KeyWriter::new().expect("an encoder");
        let mut terminal = terminal();

        let normal = writer
            .encode(&terminal, &press(Key::ArrowUp, Mods::empty()))
            .expect("the key should encode")
            .to_vec();
        terminal
            .set_mode(Mode::DECCKM, true)
            .expect("the mode should set");
        let application = writer
            .encode(&terminal, &press(Key::ArrowUp, Mods::empty()))
            .expect("the key should encode");

        assert_eq!(normal, b"\x1b[A");
        assert_eq!(
            application, b"\x1bOA",
            "set_options_from_terminal runs before every encode"
        );
    }

    #[test]
    fn committed_text_reaches_the_program_as_itself() {
        let mut writer = KeyWriter::new().expect("an encoder");
        let terminal = terminal();

        let bytes = writer
            .encode_text(&terminal, "é")
            .expect("the text should encode");

        assert_eq!(bytes, "é".as_bytes());
    }

    #[test]
    fn a_bare_modifier_sends_the_program_nothing() {
        let mut writer = KeyWriter::new().expect("an encoder");
        let terminal = terminal();

        let bytes = writer
            .encode(&terminal, &press(Key::ShiftLeft, Mods::SHIFT))
            .expect("the key should encode");

        assert!(bytes.is_empty(), "{bytes:?}");
    }
}
