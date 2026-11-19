//! Generic process notifications. Service targets remain private to each board.

mod policy;
use std::collections::HashMap;

use gpui_kit::{App, Global, SharedString};
pub use policy::{Mode, Preferences, Sound, Style};

/// Master sound preference; individual sound choices remain stored separately.
pub const SOUNDS_KEY: &str = "notifications.sounds";

/// Notification sounds are enabled until the person saves a muted choice.
pub fn sounds_enabled(cx: &App) -> bool {
    crate::prefs::get(cx, SOUNDS_KEY).as_deref() != Some("0")
}

/// Installed once by the GUI shell. No sink exists in headless tools.
pub struct Sink(pub fn(&str, u64, Notice, &mut App));
impl Global for Sink {}

/// What a board asks to show. An in-app toast shows `text`. An OS
/// notification shows the product name and generic text, unless the board
/// built the notice with [`Notice::described`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notice {
    text: SharedString,
    /// The OS notification's body, under `text` as its title. Only for text
    /// that names who and where, never message content: OS notifications
    /// show on locked screens and stay in the notification history.
    system_body: Option<SharedString>,
}

impl Notice {
    /// A notice whose OS notification keeps the generic text, for text a
    /// board does not control, such as a web page's.
    pub fn new(text: impl Into<SharedString>) -> Self {
        Self {
            text: text.into(),
            system_body: None,
        }
    }

    /// A notice whose OS notification may show `text` above `body`.
    // qual:api
    pub fn described(text: impl Into<SharedString>, body: impl Into<SharedString>) -> Self {
        Self {
            text: text.into(),
            system_body: Some(body.into()),
        }
    }

    /// What happened, in one line: the toast, and the OS title when
    /// described.
    pub fn text(&self) -> &SharedString {
        &self.text
    }

    /// The OS body of a described notice.
    pub fn system_body(&self) -> Option<&SharedString> {
        self.system_body.as_ref()
    }
}

/// The notification choices each board declares
/// (`BoardSettings::notification_defaults`), installed by the GUI shell.
/// A board that declares none starts Off.
#[derive(Default)]
pub struct Defaults(HashMap<String, Preferences>);
impl Global for Defaults {}

/// Installs the boards' declared defaults, replacing earlier ones.
pub fn install_defaults<'a>(
    boards: impl IntoIterator<Item = (&'a str, Preferences)>,
    cx: &mut App,
) {
    let declared = boards
        .into_iter()
        .map(|(source, preferences)| (source.to_owned(), preferences))
        .collect();
    cx.set_global(Defaults(declared));
}

// qual:api - board-to-shell adapter, no window or service work.
pub fn request(source: &str, target: u64, notice: Notice, cx: &mut App) {
    if preferences(source, cx).mode == Mode::Off {
        return;
    }
    if let Some(sink) = cx.try_global::<Sink>().map(|sink| sink.0) {
        sink(source, target, notice, cx);
    }
}

/// The person's choice, or the board's declared defaults until they make one.
pub fn preferences(source: &str, cx: &App) -> Preferences {
    let stored = |name| crate::prefs::get(cx, &key(source, name));
    let saved = stored("mode").map(|mode| {
        Preferences::decode(
            Some(&mode),
            stored("style").as_deref(),
            stored("sound").as_deref(),
        )
    });
    let marked = stored(CHOSEN).is_some();
    chosen(saved, marked)
        .unwrap_or_else(|| declared(source, cx))
        .playable()
}

/// The marker [`Preferences::save`] writes beside a choice.
const CHOSEN: &str = "chosen";

/// The saved choice that holds. Before the marker, Settings stored every
/// board's Off default on any Save, so an unmarked save of exactly that
/// default is no choice; anything else unmarked was the person's own.
fn chosen(saved: Option<Preferences>, marked: bool) -> Option<Preferences> {
    saved.filter(|saved| marked || *saved != Preferences::default())
}

fn declared(source: &str, cx: &App) -> Preferences {
    cx.try_global::<Defaults>()
        .and_then(|defaults| defaults.0.get(source).copied())
        .unwrap_or_default()
}

fn key(source: &str, name: &str) -> String {
    format!("notifications.{source}.{name}")
}

impl Preferences {
    /// Stores these as the person's choice for `source`, which then holds
    /// over the board's declared defaults.
    pub fn save(self, source: &str, cx: &mut App) {
        for (name, value) in [
            ("mode", self.mode.value()),
            ("style", self.style.value()),
            ("sound", self.sound.value()),
            (CHOSEN, "1"),
        ] {
            crate::prefs::set(cx, &key(source, name), value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Mode, Notice, Preferences, Sound, Style};

    #[gpui_kit::test]
    fn board_defaults_apply_until_a_choice_is_saved(cx: &mut gpui_kit::TestAppContext) {
        let declared = Preferences {
            mode: Mode::System,
            style: Style::Compact,
            sound: Sound::Silent,
        };
        cx.update(|cx| {
            crate::prefs::init_file(None, cx);
            super::install_defaults([("fixture", declared)], cx);
            assert_eq!(super::preferences("fixture", cx), declared);
            assert_eq!(super::preferences("other", cx), Preferences::default());
            Preferences::default().save("fixture", cx);
            assert_eq!(
                super::preferences("fixture", cx),
                Preferences::default(),
                "a saved Off wins over the board's default"
            );
        });
    }

    #[gpui_kit::test]
    fn an_unmarked_old_default_reads_as_no_choice(cx: &mut gpui_kit::TestAppContext) {
        let declared = Preferences {
            mode: Mode::System,
            ..Preferences::default()
        };
        cx.update(|cx| {
            crate::prefs::init_file(None, cx);
            super::install_defaults([("fixture", declared)], cx);
            // Settings up to 0.2.1 stored every board's Off default on any Save.
            for (name, value) in [("mode", "off"), ("style", "standard"), ("sound", "Silent")] {
                crate::prefs::set(cx, &super::key("fixture", name), value);
            }
            assert_eq!(super::preferences("fixture", cx), declared);
            // An earlier choice other than that default was the person's own.
            crate::prefs::set(cx, &super::key("fixture", "mode"), "in-app");
            assert_eq!(super::preferences("fixture", cx).mode, Mode::InApp);
        });
    }

    #[test]
    fn only_described_notices_carry_text_to_the_operating_system() {
        let plain = Notice::new(String::from("Notes: three new notes"));
        assert_eq!(plain.text().as_ref(), "Notes: three new notes");
        assert_eq!(plain.system_body(), None);
        let described = Notice::described("alice mentioned you in #subspace", "Local Ergo");
        assert_eq!(
            described.text().as_ref(),
            "alice mentioned you in #subspace"
        );
        assert_eq!(
            described.system_body().map(|body| body.as_ref()),
            Some("Local Ergo")
        );
    }

    #[gpui_kit::test]
    fn off_requests_never_reach_the_process_sink(cx: &mut gpui_kit::TestAppContext) {
        struct Delivered(Vec<u64>);
        impl gpui_kit::Global for Delivered {}
        cx.update(|cx| {
            crate::prefs::init_file(None, cx);
            cx.set_global(Delivered(Vec::new()));
            cx.set_global(super::Sink(|_, target, _, cx| {
                cx.global_mut::<Delivered>().0.push(target)
            }));
            super::request("fixture", 1, Notice::new("Generic activity"), cx);
            assert!(cx.global::<Delivered>().0.is_empty());
            Preferences {
                mode: Mode::InApp,
                ..Preferences::default()
            }
            .save("fixture", cx);
            super::request("fixture", 2, Notice::new("Generic activity"), cx);
            assert_eq!(cx.global::<Delivered>().0, [2]);
            Preferences::default().save("fixture", cx);
            super::request("fixture", 3, Notice::new("Generic activity"), cx);
            assert_eq!(cx.global::<Delivered>().0, [2]);
        });
    }
}
