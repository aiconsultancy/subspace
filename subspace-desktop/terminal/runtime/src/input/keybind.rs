//! The Ghostty `keybind` lines that send bytes (REQ-31a).
//!
//! A line reads `trigger=action`. Only the three actions that send bytes
//! apply: `text:` sends the text itself, `esc:` prefixes it with `ESC` and
//! `csi:` with `ESC [`. Everything else is ignored without a warning,
//! including a `global:`, `all:` or `unconsumed:` prefix and a key sequence
//! (`>`), because those belong to Ghostty's own windows.
//!
//! A Subspace app or board key wins over a line for the same key: GPUI
//! dispatches action bindings before the element key listener that reaches
//! this table, so the shell's bindings never arrive here.

use libghostty_vt::key::{Key, Mods};

use crate::input::keys::named_key;

/// The escape byte `esc:` and `csi:` prefix their text with.
const ESCAPE: u8 = 0x1b;

/// How many hex digits a `\xNN` escape carries.
const HEX_DIGITS: usize = 2;

/// Hexadecimal.
const HEX_RADIX: u32 = 16;

/// The action prefixes that put a line out of scope (REQ-31a).
const IGNORED_PREFIXES: &[&str] = &["global:", "all:", "unconsumed:"];

/// The Ghostty key names that differ from the GPUI name for the same key.
const TRIGGER_ALIASES: &[(&str, &str)] = &[
    ("return", "enter"),
    ("esc", "escape"),
    ("page_up", "pageup"),
    ("page_down", "pagedown"),
    ("arrow_up", "up"),
    ("arrow_down", "down"),
    ("arrow_left", "left"),
    ("arrow_right", "right"),
    ("grave_accent", "`"),
    ("minus", "-"),
    ("equal", "="),
    ("comma", ","),
    ("period", "."),
    ("slash", "/"),
    ("semicolon", ";"),
    ("apostrophe", "'"),
    ("bracket_left", "["),
    ("bracket_right", "]"),
    ("backslash", "\\"),
];

/// The modifier names a trigger may carry.
const TRIGGER_MODIFIERS: &[(&str, Mods)] = &[
    ("shift", Mods::SHIFT),
    ("ctrl", Mods::CTRL),
    ("control", Mods::CTRL),
    ("alt", Mods::ALT),
    ("opt", Mods::ALT),
    ("option", Mods::ALT),
    ("cmd", Mods::SUPER),
    ("command", Mods::SUPER),
    ("super", Mods::SUPER),
];

/// One trigger and the bytes it sends.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Keybind {
    /// The physical key the trigger names.
    key: Key,
    /// The modifiers the trigger names.
    mods: Mods,
    /// What the key sends instead of its own encoding.
    bytes: Vec<u8>,
}

/// The `keybind` lines that send bytes, in the order the configuration gave
/// them: a later line for the same trigger replaces an earlier one, as
/// Ghostty does (REQ-31a).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TerminalKeybinds {
    bindings: Vec<Keybind>,
}

impl TerminalKeybinds {
    /// The applicable lines of a configuration's raw `keybind` values.
    pub(crate) fn parse(lines: &[String]) -> Self {
        let mut bindings = Vec::new();
        for line in lines {
            if let Some(keybind) = parse_line(line) {
                bindings
                    .retain(|held: &Keybind| held.key != keybind.key || held.mods != keybind.mods);
                bindings.push(keybind);
            }
        }
        Self { bindings }
    }

    /// What a key press sends instead of its own encoding, where a line
    /// names it.
    pub(crate) fn bytes_for(&self, key: Key, mods: Mods) -> Option<&[u8]> {
        self.bindings
            .iter()
            .find(|keybind| keybind.key == key && keybind.mods == mods)
            .map(|keybind| keybind.bytes.as_slice())
    }
}

/// One `keybind` value as a trigger and its bytes, and nothing for a line
/// this delivery does not apply.
fn parse_line(line: &str) -> Option<Keybind> {
    let (trigger, action) = line.split_once('=')?;
    let trigger = trigger.trim();
    if trigger.contains('>') || has_ignored_prefix(trigger) {
        return None;
    }
    let bytes = action_bytes(action.trim())?;
    let (key, mods) = parse_trigger(trigger)?;
    Some(Keybind { key, mods, bytes })
}

/// Whether a trigger carries a prefix that puts the line out of scope.
fn has_ignored_prefix(trigger: &str) -> bool {
    IGNORED_PREFIXES
        .iter()
        .any(|prefix| trigger.starts_with(prefix))
}

/// The bytes an action sends, and nothing for an action that sends none.
fn action_bytes(action: &str) -> Option<Vec<u8>> {
    let (name, value) = action.split_once(':')?;
    let text = unescape(value);
    match name {
        "text" => Some(text),
        "esc" => Some(prefixed(&[ESCAPE], text)),
        "csi" => Some(prefixed(&[ESCAPE, b'['], text)),
        _ => None,
    }
}

/// A sequence's bytes behind their introducer.
fn prefixed(introducer: &[u8], text: Vec<u8>) -> Vec<u8> {
    let mut bytes = introducer.to_vec();
    bytes.extend(text);
    bytes
}

/// The key and the modifiers a trigger names, and nothing for a trigger
/// naming no key this crate knows.
fn parse_trigger(trigger: &str) -> Option<(Key, Mods)> {
    let mut mods = Mods::empty();
    let mut key = None;
    for part in trigger.split('+') {
        match trigger_modifier(part) {
            Some(modifier) => mods |= modifier,
            None => key = Some(trigger_key(part)?),
        }
    }
    Some((key?, mods))
}

/// The modifier one part of a trigger names.
fn trigger_modifier(part: &str) -> Option<Mods> {
    let name = part.to_lowercase();
    TRIGGER_MODIFIERS
        .iter()
        .find(|(named, _)| *named == name)
        .map(|(_, modifier)| *modifier)
}

/// The physical key a Ghostty key name stands for.
fn trigger_key(part: &str) -> Option<Key> {
    let lower = part.to_lowercase();
    let name = trigger_alias(&lower).unwrap_or(lower.as_str());
    match named_key(name) {
        Key::Unidentified => None,
        key => Some(key),
    }
}

/// The GPUI name for a Ghostty key name that differs from it.
fn trigger_alias(name: &str) -> Option<&'static str> {
    TRIGGER_ALIASES
        .iter()
        .find(|(ghostty, _)| *ghostty == name)
        .map(|(_, gpui)| *gpui)
}

/// An action's text with its escapes resolved, as Zig string escapes.
fn unescape(value: &str) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut characters = value.chars();
    while let Some(character) = characters.next() {
        if character != '\\' {
            push_char(&mut bytes, character);
            continue;
        }
        match characters.next() {
            Some(escaped) => push_escape(&mut bytes, escaped, &mut characters),
            None => bytes.push(b'\\'),
        }
    }
    bytes
}

/// One escaped character's bytes, reading a `\xNN` escape's digits from the
/// rest of the value.
fn push_escape(bytes: &mut Vec<u8>, escaped: char, rest: &mut std::str::Chars<'_>) {
    match escaped {
        'n' => bytes.push(b'\n'),
        'r' => bytes.push(b'\r'),
        't' => bytes.push(b'\t'),
        'e' => bytes.push(ESCAPE),
        '0' => bytes.push(0),
        'x' => push_hex(bytes, rest),
        other => push_char(bytes, other),
    }
}

/// The byte a `\xNN` escape names, and the digits themselves where they are
/// not a byte.
fn push_hex(bytes: &mut Vec<u8>, rest: &mut std::str::Chars<'_>) {
    let digits: String = rest.clone().take(HEX_DIGITS).collect();
    match u8::from_str_radix(&digits, HEX_RADIX) {
        Ok(byte) => {
            bytes.push(byte);
            rest.nth(HEX_DIGITS - 1);
        }
        Err(_) => bytes.push(b'x'),
    }
}

/// One character as its UTF-8 bytes.
fn push_char(bytes: &mut Vec<u8>, character: char) {
    let mut buffer = [0u8; 4];
    bytes.extend_from_slice(character.encode_utf8(&mut buffer).as_bytes());
}

#[cfg(test)]
mod tests {
    use libghostty_vt::key::{Key, Mods};

    use crate::input::keybind::TerminalKeybinds;

    /// The keybinds of the given raw lines.
    fn keybinds(lines: &[&str]) -> TerminalKeybinds {
        let lines: Vec<String> = lines.iter().map(|line| (*line).to_owned()).collect();
        TerminalKeybinds::parse(&lines)
    }

    #[test]
    fn a_text_action_sends_its_escaped_bytes() {
        let parsed = keybinds(&["shift+enter=text:\\x0a"]);

        assert_eq!(
            parsed.bytes_for(Key::Enter, Mods::SHIFT),
            Some(b"\n".as_slice())
        );
        assert_eq!(parsed.bytes_for(Key::Enter, Mods::empty()), None);
    }

    #[test]
    fn esc_and_csi_actions_carry_their_introducer() {
        let parsed = keybinds(&["ctrl+a=esc:b", "alt+z=csi:5~"]);

        assert_eq!(
            parsed.bytes_for(Key::A, Mods::CTRL),
            Some(b"\x1bb".as_slice())
        );
        assert_eq!(
            parsed.bytes_for(Key::Z, Mods::ALT),
            Some(b"\x1b[5~".as_slice())
        );
    }

    #[test]
    fn every_action_that_sends_no_bytes_is_ignored() {
        let parsed = keybinds(&[
            "ctrl+a=new_tab",
            "ctrl+b=ignore",
            "ctrl+c=",
            "ctrl+d",
            "global:ctrl+e=text:x",
            "all:ctrl+f=text:x",
            "unconsumed:ctrl+g=text:x",
            "ctrl+h>ctrl+i=text:x",
        ]);

        for key in [
            Key::A,
            Key::B,
            Key::C,
            Key::D,
            Key::E,
            Key::F,
            Key::G,
            Key::H,
        ] {
            assert_eq!(parsed.bytes_for(key, Mods::CTRL), None, "{key:?}");
        }
    }

    #[test]
    fn ghostty_key_and_modifier_names_resolve_to_the_same_keys() {
        let parsed = keybinds(&[
            "command+page_up=text:a",
            "option+grave_accent=text:b",
            "control+shift+return=text:c",
            "super+minus=text:d",
        ]);

        assert_eq!(
            parsed.bytes_for(Key::PageUp, Mods::SUPER),
            Some(b"a".as_slice())
        );
        assert_eq!(
            parsed.bytes_for(Key::Backquote, Mods::ALT),
            Some(b"b".as_slice())
        );
        assert_eq!(
            parsed.bytes_for(Key::Enter, Mods::CTRL | Mods::SHIFT),
            Some(b"c".as_slice())
        );
        assert_eq!(
            parsed.bytes_for(Key::Minus, Mods::SUPER),
            Some(b"d".as_slice())
        );
    }

    #[test]
    fn a_trigger_naming_no_known_key_is_ignored() {
        let parsed = keybinds(&["ctrl+physical:unknown=text:x", "ctrl=text:y"]);

        assert_eq!(parsed.bytes_for(Key::Unidentified, Mods::CTRL), None);
        assert_eq!(parsed, keybinds(&[]));
    }

    #[test]
    fn the_last_line_for_a_trigger_wins() {
        let parsed = keybinds(&["ctrl+a=text:first", "ctrl+a=text:second"]);

        assert_eq!(
            parsed.bytes_for(Key::A, Mods::CTRL),
            Some(b"second".as_slice())
        );
    }

    #[test]
    fn the_escapes_a_text_action_may_carry_all_resolve() {
        let parsed = keybinds(&["ctrl+a=text:\\e[A\\t\\r\\n\\\\\\q\\x1b\\xzz"]);

        assert_eq!(
            parsed.bytes_for(Key::A, Mods::CTRL),
            Some(b"\x1b[A\t\r\n\\q\x1bxzz".as_slice())
        );
    }
}
