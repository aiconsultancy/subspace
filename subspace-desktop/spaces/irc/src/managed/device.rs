//! This install's device ID, which the SASL username carries as
//! `account@device` so the IRC server replays to each device the messages
//! it missed. It is random, made once and kept in the state file with the
//! app's other choices; nothing about the computer goes into it.

use gpui_kit::App;
use subspace_framework::dashboard::prefs;

/// The state-file key.
pub(crate) const KEY: &str = "irc.device";
const LENGTH: usize = 16;

/// This install's device ID: the stored one, else a new one, stored.
pub(crate) fn load(cx: &mut App) -> String {
    if let Some(device) = parse(prefs::get(cx, KEY).as_deref()) {
        return device;
    }
    let device = generate();
    prefs::set(cx, KEY, device.clone());
    device
}

/// A stored device ID, when it is one this app made: 16 lower-case
/// hexadecimal digits.
pub(crate) fn parse(stored: Option<&str>) -> Option<String> {
    stored
        .filter(|device| {
            device.len() == LENGTH
                && device
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        })
        .map(str::to_owned)
}

/// A new random device ID.
pub(crate) fn generate() -> String {
    let mut device = uuid::Uuid::new_v4().simple().to_string();
    device.truncate(LENGTH);
    device
}

/// The SASL username for `account` on this device. The nick stays the
/// plain account name.
pub(crate) fn sasl_username(account: &str, device: &str) -> String {
    format!("{account}@{device}")
}

#[cfg(test)]
mod tests {
    use super::{generate, parse, sasl_username};

    #[test]
    fn a_device_id_is_random_and_kept_once_valid() {
        let first = generate();
        assert_eq!(parse(Some(&first)), Some(first.clone()));
        assert_ne!(first, generate());
        for stored in [
            None,
            Some(""),
            Some("0123456789abcdeF"),
            Some("0123456789abcde"),
            Some("0123456789abcdef0"),
            Some("laptop@home00000"),
        ] {
            assert_eq!(parse(stored), None, "{stored:?}");
        }
    }

    #[test]
    fn the_sasl_username_carries_the_device_after_the_account() {
        assert_eq!(
            sasl_username("ada", "0123456789abcdef"),
            "ada@0123456789abcdef"
        );
    }
}
