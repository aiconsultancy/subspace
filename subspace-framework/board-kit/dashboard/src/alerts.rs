//! Alerts for monitored agent tasks: a system sound and a macOS
//! notification. Both start a process on a thread of their own, so a caller
//! on the UI thread never waits for one. On other platforms they do nothing.
//!
//! Settings live beside the other stored choices (see [`crate::prefs`]):
//! `alerts.sound`, `alerts.sound-name` and `alerts.notify`.

use gpui_kit::App;

use crate::prefs;

pub const SOUND_KEY: &str = "alerts.sound";
pub const SOUND_NAME_KEY: &str = "alerts.sound-name";
pub const NOTIFY_KEY: &str = "alerts.notify";
/// The sound a new install plays; a macOS system sound.
pub const DEFAULT_SOUND: &str = "Glass";
/// The longest sound name accepted, so the path stays a system sound.
const MAX_SOUND_NAME: usize = 64;

/// What the user chose for task alerts. Both alerts default to on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlertSettings {
    pub sound: bool,
    pub sound_name: String,
    pub notify: bool,
}

impl Default for AlertSettings {
    fn default() -> Self {
        Self {
            sound: true,
            sound_name: DEFAULT_SOUND.to_owned(),
            notify: true,
        }
    }
}

pub fn settings(cx: &App) -> AlertSettings {
    AlertSettings {
        sound: prefs::get(cx, SOUND_KEY).as_deref() != Some("0"),
        sound_name: sound_name(prefs::get(cx, SOUND_NAME_KEY)),
        notify: prefs::get(cx, NOTIFY_KEY).as_deref() != Some("0"),
    }
}

/// A stored name that is not a plain system sound reads as the default.
fn sound_name(stored: Option<String>) -> String {
    stored
        .filter(|name| is_sound_name(name))
        .unwrap_or_else(|| DEFAULT_SOUND.to_owned())
}

// qual:api
pub fn set_sound(cx: &mut App, on: bool) {
    prefs::set(cx, SOUND_KEY, flag(on));
}

/// Stores `name` when it is a plain sound name; anything else is ignored.
// qual:api
pub fn set_sound_name(cx: &mut App, name: &str) {
    if is_sound_name(name) {
        prefs::set(cx, SOUND_NAME_KEY, name);
    }
}

// qual:api
pub fn set_notify(cx: &mut App, on: bool) {
    prefs::set(cx, NOTIFY_KEY, flag(on));
}

fn flag(on: bool) -> &'static str {
    if on { "1" } else { "0" }
}

/// A sound name names one file in the system sound directory: letters,
/// digits, spaces, underscores and hyphens only, so no path can escape it.
pub fn is_sound_name(name: &str) -> bool {
    (1..=MAX_SOUND_NAME).contains(&name.len())
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b" _-".contains(&byte))
}

/// An AppleScript string literal body: the characters that would end the
/// literal or break the line are escaped.
pub fn script_literal(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            other => escaped.push(other),
        }
    }
    escaped
}

/// The one-line AppleScript that shows `title` and `body`.
pub fn notification_script(title: &str, body: &str) -> String {
    format!(
        "display notification \"{}\" with title \"{}\"",
        script_literal(body),
        script_literal(title)
    )
}

/// Plays the system sound `name` once. An unknown name plays nothing.
pub fn play_sound(name: &str, cx: &App) {
    if crate::notifications::sounds_enabled(cx) {
        preview_sound(name);
    }
}

/// Explicit previews bypass the master notification sound preference.
pub fn preview_sound(name: &str) {
    if is_sound_name(name) {
        #[cfg(test)]
        SOUND_PLAYS.with(|plays| plays.borrow_mut().push(name.to_owned()));
        run(
            "/usr/bin/afplay",
            vec![format!("/System/Library/Sounds/{name}.aiff")],
        );
    }
}

#[cfg(test)]
thread_local! {
    static SOUND_PLAYS: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Shows a macOS notification.
pub fn notify(title: &str, body: &str) {
    run(
        "/usr/bin/osascript",
        vec!["-e".to_owned(), notification_script(title, body)],
    );
}

/// Starts the process on a thread of its own and reaps it there, so the
/// caller returns at once and no child is left behind.
#[cfg(all(not(test), target_os = "macos"))]
fn run(program: &'static str, arguments: Vec<String>) {
    std::thread::spawn(move || {
        let _ = std::process::Command::new(program).args(arguments).status();
    });
}

/// Alerts are macOS system facilities; elsewhere there is nothing to run.
#[cfg(any(test, not(target_os = "macos")))]
fn run(_program: &'static str, _arguments: Vec<String>) {}

#[cfg(test)]
mod tests {
    use super::{
        AlertSettings, DEFAULT_SOUND, is_sound_name, notification_script, script_literal,
        sound_name,
    };

    #[gpui_kit::test]
    fn master_mutes_shared_playback_but_keeps_sound_choices_and_explicit_previews(
        cx: &mut gpui_kit::TestAppContext,
    ) {
        cx.update(|cx| {
            crate::prefs::init_file(None, cx);
            super::SOUND_PLAYS.with(|plays| plays.borrow_mut().clear());
            assert!(crate::notifications::sounds_enabled(cx));
            super::set_sound_name(cx, "Ping");
            super::play_sound("Glass", cx);
            crate::prefs::set(cx, crate::notifications::SOUNDS_KEY, "0");
            super::play_sound("Glass", cx);
            super::play_sound("Ping", cx);
            super::preview_sound("Pop");
            assert_eq!(super::settings(cx).sound_name, "Ping");
            crate::prefs::set(cx, crate::notifications::SOUNDS_KEY, "1");
            super::play_sound("Ping", cx);
            super::SOUND_PLAYS.with(|plays| assert_eq!(*plays.borrow(), ["Glass", "Pop", "Ping"]));
        });
    }

    #[test]
    fn both_alerts_default_to_on_with_the_default_sound() {
        let settings = AlertSettings::default();
        assert!(settings.sound);
        assert!(settings.notify);
        assert_eq!(settings.sound_name, DEFAULT_SOUND);
        assert_eq!(sound_name(None), DEFAULT_SOUND);
        assert_eq!(sound_name(Some("Submarine".into())), "Submarine");
        assert_eq!(sound_name(Some("../../etc/passwd".into())), DEFAULT_SOUND);
    }

    #[test]
    fn sound_names_cannot_leave_the_system_sound_directory() {
        assert!(is_sound_name("Glass"));
        assert!(is_sound_name("Sosumi 2"));
        assert!(!is_sound_name(""));
        assert!(!is_sound_name("../Glass"));
        assert!(!is_sound_name("Glass.aiff"));
        assert!(!is_sound_name("Glass;rm -rf /"));
        assert!(!is_sound_name(&"a".repeat(65)));
    }

    #[test]
    fn quotes_backslashes_and_line_breaks_cannot_escape_the_script_literal() {
        assert_eq!(script_literal(r#"say "hi""#), r#"say \"hi\""#);
        assert_eq!(script_literal(r"back\slash"), r"back\\slash");
        assert_eq!(script_literal("two\nlines\there"), "two\\nlines\\there");
        assert_eq!(script_literal("plain text"), "plain text");
        assert_eq!(
            notification_script("Copilot Agents", r#"Task "a\b" completed"#),
            r#"display notification "Task \"a\\b\" completed" with title "Copilot Agents""#
        );
    }
}
