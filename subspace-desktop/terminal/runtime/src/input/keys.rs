//! One table from a GPUI keystroke to a libghostty key event (REQ-18).
//!
//! GPUI reports the key a keystroke came from as the character printed on it
//! (`Keystroke::key`, always the unshifted form) and the text it would type as
//! `Keystroke::key_char`. libghostty wants the physical key, the modifier
//! bitmask, the modifiers the layout already consumed, the unshifted
//! codepoint, and the typed text separately, so this module splits one
//! keystroke into those five facts and nothing else.

use gpui_kit::{Keystroke, Modifiers};
use libghostty_vt::key::{Key, Mods};

/// The first codepoint of the private-use block macOS reports function keys
/// in. libghostty refuses those as text, so they never reach it.
const PRIVATE_USE_FIRST: char = '\u{f700}';
/// The last codepoint of that block.
const PRIVATE_USE_LAST: char = '\u{f8ff}';

/// The radix a digit key's name is read in.
const DECIMAL: u32 = 10;

/// What one key press tells the encoder (REQ-18).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct KeyPress {
    /// The physical key libghostty encodes from.
    pub(crate) key: Key,
    /// The modifiers held down.
    pub(crate) mods: Mods,
    /// The modifiers the keyboard layout already used to produce the text.
    pub(crate) consumed: Mods,
    /// The text the key types, where it types any.
    pub(crate) text: Option<String>,
    /// The codepoint printed on the key, where the key prints one.
    pub(crate) unshifted: Option<char>,
}

impl KeyPress {
    /// A press that carries nothing but text, which is what an input-method
    /// commit is: no physical key produced it (REQ-18).
    pub(crate) fn from_text(text: &str) -> Self {
        Self {
            key: Key::Unidentified,
            mods: Mods::empty(),
            consumed: Mods::empty(),
            text: Some(text.to_owned()),
            unshifted: None,
        }
    }
}

/// The keys whose GPUI name is a word rather than a character.
const NAMED_KEYS: &[(&str, Key)] = &[
    ("space", Key::Space),
    ("tab", Key::Tab),
    ("enter", Key::Enter),
    ("backspace", Key::Backspace),
    ("escape", Key::Escape),
    ("delete", Key::Delete),
    ("insert", Key::Insert),
    ("home", Key::Home),
    ("end", Key::End),
    ("pageup", Key::PageUp),
    ("pagedown", Key::PageDown),
    ("up", Key::ArrowUp),
    ("down", Key::ArrowDown),
    ("left", Key::ArrowLeft),
    ("right", Key::ArrowRight),
    ("menu", Key::ContextMenu),
];

/// The keys GPUI names with the punctuation mark printed on them.
const PUNCTUATION_KEYS: &[(char, Key)] = &[
    ('`', Key::Backquote),
    ('\\', Key::Backslash),
    ('[', Key::BracketLeft),
    (']', Key::BracketRight),
    (',', Key::Comma),
    ('=', Key::Equal),
    ('-', Key::Minus),
    ('.', Key::Period),
    ('\'', Key::Quote),
    (';', Key::Semicolon),
    ('/', Key::Slash),
];

/// `a` to `z`, in order.
const LETTER_KEYS: [Key; 26] = [
    Key::A,
    Key::B,
    Key::C,
    Key::D,
    Key::E,
    Key::F,
    Key::G,
    Key::H,
    Key::I,
    Key::J,
    Key::K,
    Key::L,
    Key::M,
    Key::N,
    Key::O,
    Key::P,
    Key::Q,
    Key::R,
    Key::S,
    Key::T,
    Key::U,
    Key::V,
    Key::W,
    Key::X,
    Key::Y,
    Key::Z,
];

/// `0` to `9`, in order.
const DIGIT_KEYS: [Key; 10] = [
    Key::Digit0,
    Key::Digit1,
    Key::Digit2,
    Key::Digit3,
    Key::Digit4,
    Key::Digit5,
    Key::Digit6,
    Key::Digit7,
    Key::Digit8,
    Key::Digit9,
];

/// `f1` to `f25`, in order: the function keys both GPUI and libghostty name.
const FUNCTION_KEYS: [Key; 25] = [
    Key::F1,
    Key::F2,
    Key::F3,
    Key::F4,
    Key::F5,
    Key::F6,
    Key::F7,
    Key::F8,
    Key::F9,
    Key::F10,
    Key::F11,
    Key::F12,
    Key::F13,
    Key::F14,
    Key::F15,
    Key::F16,
    Key::F17,
    Key::F18,
    Key::F19,
    Key::F20,
    Key::F21,
    Key::F22,
    Key::F23,
    Key::F24,
    Key::F25,
];

/// The five facts one keystroke gives the encoder (REQ-18).
pub(crate) fn key_press(keystroke: &Keystroke) -> KeyPress {
    let text = typed_text(keystroke);
    KeyPress {
        key: named_key(&keystroke.key),
        mods: mods_of(keystroke.modifiers),
        consumed: consumed_mods(keystroke.modifiers, text.is_some()),
        unshifted: unshifted_codepoint(&keystroke.key),
        text,
    }
}

/// The physical key a GPUI key name stands for, and
/// [`Key::Unidentified`] for a name no table holds: the encoder then works
/// from the typed text alone.
pub(crate) fn named_key(name: &str) -> Key {
    if let Some(key) = word_key(name) {
        return key;
    }
    if let Some(key) = function_key(name) {
        return key;
    }
    match one_char(name) {
        Some(character) => char_key(character),
        None => Key::Unidentified,
    }
}

/// The key a one-word name holds.
fn word_key(name: &str) -> Option<Key> {
    NAMED_KEYS
        .iter()
        .find(|(named, _)| *named == name)
        .map(|(_, key)| *key)
}

/// The key an `fN` name holds, for the `N` libghostty knows.
fn function_key(name: &str) -> Option<Key> {
    let number: usize = name.strip_prefix('f')?.parse().ok()?;
    FUNCTION_KEYS.get(number.checked_sub(1)?).copied()
}

/// The key one printed character stands for.
fn char_key(character: char) -> Key {
    if let Some(offset) = letter_offset(character) {
        return LETTER_KEYS[offset];
    }
    if let Some(digit) = character.to_digit(DECIMAL) {
        return DIGIT_KEYS[digit as usize];
    }
    PUNCTUATION_KEYS
        .iter()
        .find(|(printed, _)| *printed == character)
        .map_or(Key::Unidentified, |(_, key)| *key)
}

/// A letter's place in the alphabet, ignoring case.
fn letter_offset(character: char) -> Option<usize> {
    let lower = character.to_ascii_lowercase();
    if !lower.is_ascii_lowercase() {
        return None;
    }
    Some(usize::from(lower as u8 - b'a'))
}

/// The one character of a single-character name.
fn one_char(name: &str) -> Option<char> {
    let mut characters = name.chars();
    let character = characters.next()?;
    characters.next().is_none().then_some(character)
}

/// The modifier bitmask libghostty encodes from.
pub(crate) fn mods_of(modifiers: Modifiers) -> Mods {
    let mut mods = Mods::empty();
    mods.set(Mods::SHIFT, modifiers.shift);
    mods.set(Mods::CTRL, modifiers.control);
    mods.set(Mods::ALT, modifiers.alt);
    mods.set(Mods::SUPER, modifiers.platform);
    mods
}

/// The modifiers the keyboard layout already used.
///
/// A printable key reports the shifted character in its text, so shift has
/// been consumed and libghostty must not encode it again. Every other
/// modifier reaches the program (REQ-18).
fn consumed_mods(modifiers: Modifiers, prints: bool) -> Mods {
    let mut consumed = Mods::empty();
    consumed.set(Mods::SHIFT, modifiers.shift && prints);
    consumed
}

/// The text a keystroke types, and nothing where it types none.
///
/// libghostty derives control and meta sequences from the key and the
/// modifiers, so a control character or a macOS function-key codepoint is
/// refused here and the encoder works from the key instead.
fn typed_text(keystroke: &Keystroke) -> Option<String> {
    let text = keystroke.key_char.as_deref()?;
    (!text.is_empty() && text.chars().all(is_typeable)).then(|| text.to_owned())
}

/// Whether one character may be handed to libghostty as typed text.
fn is_typeable(character: char) -> bool {
    !character.is_control() && !(PRIVATE_USE_FIRST..=PRIVATE_USE_LAST).contains(&character)
}

/// The codepoint printed on the key, which the Kitty keyboard protocol
/// reports as the unshifted key.
fn unshifted_codepoint(name: &str) -> Option<char> {
    one_char(name).filter(|character| !character.is_control())
}

#[cfg(test)]
mod tests {
    use gpui_kit::{Keystroke, Modifiers};
    use libghostty_vt::key::{Key, Mods};

    use crate::input::keys::{key_press, named_key};

    /// A keystroke with the given key, text and modifiers.
    fn keystroke(key: &str, text: Option<&str>, modifiers: Modifiers) -> Keystroke {
        Keystroke {
            modifiers,
            key: key.to_owned(),
            key_char: text.map(str::to_owned),
        }
    }

    #[test]
    fn letters_digits_and_punctuation_map_to_their_physical_keys() {
        assert_eq!(named_key("a"), Key::A);
        assert_eq!(named_key("z"), Key::Z);
        assert_eq!(named_key("0"), Key::Digit0);
        assert_eq!(named_key("9"), Key::Digit9);
        assert_eq!(named_key("/"), Key::Slash);
        assert_eq!(named_key("`"), Key::Backquote);
    }

    #[test]
    fn the_named_editing_and_function_keys_are_all_mapped() {
        assert_eq!(named_key("enter"), Key::Enter);
        assert_eq!(named_key("escape"), Key::Escape);
        assert_eq!(named_key("backspace"), Key::Backspace);
        assert_eq!(named_key("pageup"), Key::PageUp);
        assert_eq!(named_key("left"), Key::ArrowLeft);
        assert_eq!(named_key("f1"), Key::F1);
        assert_eq!(named_key("f12"), Key::F12);
        assert_eq!(named_key("f25"), Key::F25);
    }

    #[test]
    fn a_key_outside_the_tables_is_left_for_the_text_to_carry() {
        assert_eq!(named_key("f0"), Key::Unidentified);
        assert_eq!(named_key("f26"), Key::Unidentified);
        assert_eq!(named_key("fn"), Key::Unidentified);
        assert_eq!(named_key("§"), Key::Unidentified);
    }

    #[test]
    fn every_modifier_held_reaches_the_encoder() {
        let modifiers = Modifiers {
            control: true,
            alt: true,
            shift: true,
            platform: true,
            function: false,
        };

        let press = key_press(&keystroke("c", None, modifiers));

        assert_eq!(
            press.mods,
            Mods::CTRL | Mods::ALT | Mods::SHIFT | Mods::SUPER
        );
        assert_eq!(press.key, Key::C);
    }

    #[test]
    fn a_printable_key_consumes_shift_and_a_named_one_does_not() {
        let shifted = Modifiers {
            shift: true,
            ..Modifiers::default()
        };

        let typed = key_press(&keystroke("2", Some("@"), shifted));
        let named = key_press(&keystroke("enter", None, shifted));

        assert_eq!(typed.consumed, Mods::SHIFT);
        assert_eq!(typed.text.as_deref(), Some("@"));
        assert_eq!(typed.unshifted, Some('2'));
        assert_eq!(named.consumed, Mods::empty());
        assert_eq!(named.mods, Mods::SHIFT);
    }

    #[test]
    fn a_control_character_or_a_function_codepoint_is_not_sent_as_text() {
        let newline = key_press(&keystroke("enter", Some("\n"), Modifiers::default()));
        let tab = key_press(&keystroke("tab", Some("\t"), Modifiers::default()));
        let function = key_press(&keystroke("f9", Some("\u{f70c}"), Modifiers::default()));
        let empty = key_press(&keystroke("a", Some(""), Modifiers::default()));

        assert_eq!(newline.text, None);
        assert_eq!(newline.key, Key::Enter);
        assert_eq!(tab.text, None);
        assert_eq!(function.text, None);
        assert_eq!(empty.text, None);
    }

    #[test]
    fn a_named_key_reports_no_unshifted_codepoint() {
        let press = key_press(&keystroke("pagedown", None, Modifiers::default()));

        assert_eq!(press.unshifted, None);
        assert_eq!(press.key, Key::PageDown);
    }
}
