//! Pending board notification choices. Only Settings Save writes preferences.

use super::{
    Settings,
    form::{Choice, choice},
};
use gpui_kit::{App, Context, Window};
use subspace_dashboard::notifications::{self, Mode, Preferences, Sound, Style};

pub(super) struct Source {
    pub id: &'static str,
    pub title: gpui_kit::SharedString,
    pub mode: Choice,
    pub style: Choice,
    pub sound: Choice,
    pub unavailable_sound: bool,
    /// What the choices showed when loaded or last saved. Save stores a
    /// source only when it differs, so an unrelated Save leaves the board on
    /// its declared defaults.
    pub loaded: Preferences,
}

pub(super) fn load(
    boards: &[crate::registry::BoardRegistration],
    window: &mut Window,
    cx: &mut Context<Settings>,
) -> Vec<Source> {
    boards
        .iter()
        .filter(|board| board.settings.notifications)
        .map(|board| {
            let saved = notifications::preferences(board.id, cx);
            let raw_sound =
                subspace_dashboard::prefs::get(cx, &format!("notifications.{}.sound", board.id));
            let unavailable_sound = raw_sound
                .as_deref()
                .is_some_and(|value| value != "Silent" && saved.sound == Sound::Silent);
            Source {
                id: board.id,
                title: board.title.to_string().into(),
                mode: choice(
                    strings(&["Off", "In-app", "OS", "Both"]),
                    Some(mode_label(saved.mode)),
                    false,
                    window,
                    cx,
                ),
                style: choice(
                    strings(&["Compact", "Standard", "Persistent"]),
                    Some(style_label(saved.style)),
                    false,
                    window,
                    cx,
                ),
                sound: choice(
                    [Sound::Silent, Sound::Glass, Sound::Ping, Sound::Pop]
                        .into_iter()
                        .filter(|sound| sound.available())
                        .map(|sound| sound.value().to_owned())
                        .collect(),
                    Some(saved.sound.value()),
                    false,
                    window,
                    cx,
                ),
                unavailable_sound,
                loaded: saved,
            }
        })
        .collect()
}

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}
fn mode_label(mode: Mode) -> &'static str {
    match mode {
        Mode::Off => "Off",
        Mode::InApp => "In-app",
        Mode::System => "OS",
        Mode::Both => "Both",
    }
}
fn style_label(style: Style) -> &'static str {
    match style {
        Style::Compact => "Compact",
        Style::Standard => "Standard",
        Style::Persistent => "Persistent",
    }
}

impl Source {
    pub(super) fn preferences(&self, cx: &App) -> Preferences {
        let mode = self.mode.read(cx).selected_value().map(String::as_str);
        let style = self.style.read(cx).selected_value().map(String::as_str);
        Preferences::decode(
            Some(match mode {
                Some("In-app") => "in-app",
                Some("OS") => "system",
                Some("Both") => "both",
                _ => "off",
            }),
            Some(match style {
                Some("Compact") => "compact",
                Some("Persistent") => "persistent",
                _ => "standard",
            }),
            self.sound.read(cx).selected_value().map(String::as_str),
        )
    }
}

impl Settings {
    pub(super) fn preview_notification(
        &mut self,
        index: usize,
        sound_only: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let selected = self.sources[index].preferences(cx);
        self.notice = None;
        self.error = None;
        if sound_only {
            match crate::notifications::preview_sound(selected.sound) {
                Ok(()) => {
                    self.notice = Some(
                        if selected.sound == Sound::Silent {
                            "Silent selected; no sound played."
                        } else {
                            "Sound preview requested."
                        }
                        .into(),
                    )
                }
                Err(error) => self.error = Some(error.into()),
            }
        } else if selected.mode == Mode::Off {
            self.notice = Some("Notifications are Off. No test was posted.".into());
        } else {
            crate::notifications::preview(selected, window, cx);
            self.notice = Some(
                "Test requested. OS delivery depends on system permission and availability.".into(),
            );
        }
        cx.notify();
    }
}
