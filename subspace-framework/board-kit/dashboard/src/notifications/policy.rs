//! Channel policy and persisted values, independent of a GUI or native adapters.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Off,
    InApp,
    System,
    Both,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Style {
    Compact,
    #[default]
    Standard,
    Persistent,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Sound {
    #[default]
    Silent,
    Glass,
    Ping,
    Pop,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Preferences {
    pub mode: Mode,
    pub style: Style,
    pub sound: Sound,
}

impl Preferences {
    pub fn decode(mode: Option<&str>, style: Option<&str>, sound: Option<&str>) -> Self {
        Self {
            mode: match mode {
                Some("in-app") => Mode::InApp,
                Some("system") => Mode::System,
                Some("both") => Mode::Both,
                _ => Mode::Off,
            },
            style: match style {
                Some("compact") => Style::Compact,
                Some("persistent") => Style::Persistent,
                _ => Style::Standard,
            },
            sound: match sound {
                Some("Glass") => Sound::Glass,
                Some("Ping") => Sound::Ping,
                Some("Pop") => Sound::Pop,
                _ => Sound::Silent,
            },
        }
    }

    /// These choices, with a sound this platform cannot play read as Silent.
    pub fn playable(mut self) -> Self {
        if !self.sound.available() {
            self.sound = Sound::Silent;
        }
        self
    }

    /// Returns (in-app, system, one sound). Missing windows drop the toast;
    /// sound is independent of visual delivery. Off suppresses every channel.
    pub fn channels(self, window_open: bool) -> (bool, bool, bool) {
        let in_app = window_open && matches!(self.mode, Mode::InApp | Mode::Both);
        let system = matches!(self.mode, Mode::System | Mode::Both);
        (
            in_app,
            system,
            self.mode != Mode::Off && self.sound != Sound::Silent,
        )
    }
}

impl Mode {
    pub fn value(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::InApp => "in-app",
            Self::System => "system",
            Self::Both => "both",
        }
    }
}
impl Style {
    pub fn value(self) -> &'static str {
        match self {
            Self::Compact => "compact",
            Self::Standard => "standard",
            Self::Persistent => "persistent",
        }
    }
}
impl Sound {
    pub fn value(self) -> &'static str {
        match self {
            Self::Silent => "Silent",
            Self::Glass => "Glass",
            Self::Ping => "Ping",
            Self::Pop => "Pop",
        }
    }
    pub fn available(self) -> bool {
        if self == Self::Silent {
            return true;
        }
        cfg!(target_os = "macos")
            && std::path::Path::new(&format!("/System/Library/Sounds/{}.aiff", self.value()))
                .is_file()
    }
}

#[cfg(test)]
mod tests {
    use super::{Mode, Preferences, Sound, Style};
    #[test]
    fn channels_do_not_replay_windowless_toasts_and_off_silences_everything() {
        let mut prefs = Preferences {
            mode: Mode::Off,
            style: Style::Persistent,
            sound: Sound::Ping,
        };
        assert_eq!(prefs.channels(true), (false, false, false));
        prefs.mode = Mode::Both;
        assert_eq!(prefs.channels(true), (true, true, true));
        assert_eq!(prefs.channels(false), (false, true, true));
        prefs.mode = Mode::InApp;
        assert_eq!(prefs.channels(false), (false, false, true));
        prefs.sound = Sound::Silent;
        assert_eq!(prefs.channels(true), (true, false, false));
    }
    #[test]
    fn invalid_saved_values_are_safe_defaults() {
        assert_eq!(
            Preferences::decode(Some("oops"), Some("oops"), Some("../Glass")),
            Preferences::default()
        );
        assert_eq!(
            Preferences::decode(Some("system"), Some("compact"), Some("Pop")),
            Preferences {
                mode: Mode::System,
                style: Style::Compact,
                sound: Sound::Pop
            }
        );
    }
}
