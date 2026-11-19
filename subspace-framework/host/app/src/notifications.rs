//! Saved chat alert choices and best-effort native delivery adapters.

mod runtime;
mod system;
pub(crate) use runtime::preview_sound;
pub(crate) use runtime::{Route, Runtime, init, preview};

#[cfg(all(not(test), any(target_os = "windows", target_os = "linux")))]
use std::process::{Command, Stdio};

#[cfg(all(not(test), not(target_os = "macos")))]
use gpui_kit::AppContext as _;
use gpui_kit::{App, SystemNotification};
use subspace_chat::ChatAttentionKind;
use subspace_dashboard::prefs;

pub(crate) const COMPLETION_KEY: &str = "chat.alerts.completion";
pub(crate) const WAITING_KEY: &str = "chat.alerts.waiting";
pub(crate) const OS_KEY: &str = "chat.alerts.os";
pub(crate) const SOUND_KEY: &str = "chat.alerts.sound";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct NotificationPreferences {
    pub(crate) sounds: bool,
    pub(crate) completion: bool,
    pub(crate) waiting: bool,
    pub(crate) os: bool,
    pub(crate) sound: bool,
}

#[derive(Clone, Copy)]
pub(crate) enum NotificationChoice {
    Sounds,
    Completion,
    Waiting,
    Os,
    Sound,
}

impl NotificationPreferences {
    pub(crate) fn load(cx: &App) -> Self {
        Self {
            sounds: subspace_dashboard::notifications::sounds_enabled(cx),
            completion: stored_flag(cx, COMPLETION_KEY, true),
            waiting: stored_flag(cx, WAITING_KEY, true),
            os: stored_flag(cx, OS_KEY, false),
            sound: stored_flag(cx, SOUND_KEY, false),
        }
    }

    pub(crate) fn save(self, cx: &mut App) {
        for (key, enabled) in [
            (subspace_dashboard::notifications::SOUNDS_KEY, self.sounds),
            (COMPLETION_KEY, self.completion),
            (WAITING_KEY, self.waiting),
            (OS_KEY, self.os),
            (SOUND_KEY, self.sound),
        ] {
            prefs::set(cx, key, flag(enabled));
        }
    }

    pub(crate) fn allows(self, kind: ChatAttentionKind) -> bool {
        match kind {
            ChatAttentionKind::TurnCompleted => self.completion,
            ChatAttentionKind::WaitingForInput => self.waiting,
        }
    }

    pub(crate) fn set(&mut self, choice: NotificationChoice, enabled: bool) {
        match choice {
            NotificationChoice::Sounds => self.sounds = enabled,
            NotificationChoice::Completion => self.completion = enabled,
            NotificationChoice::Waiting => self.waiting = enabled,
            NotificationChoice::Os => self.os = enabled,
            NotificationChoice::Sound => self.sound = enabled,
        }
    }
}

/// The declared notification defaults of every board that offers
/// notification settings, so they apply before the person saves a choice.
pub(crate) fn install_defaults(boards: &[crate::registry::BoardRegistration], cx: &mut App) {
    let declared = boards
        .iter()
        .filter(|board| board.settings.notifications)
        .map(|board| (board.id, board.settings.notification_defaults));
    subspace_dashboard::notifications::install_defaults(declared, cx);
}

pub(crate) fn alert_text(kind: ChatAttentionKind) -> &'static str {
    match kind {
        ChatAttentionKind::TurnCompleted => "An AI turn completed.",
        ChatAttentionKind::WaitingForInput => "AI needs your input.",
    }
}

pub(crate) fn show_system_alert(kind: ChatAttentionKind, _chat: u64, cx: &mut App) {
    let tag = runtime::register(Route::Unavailable, cx);
    system::post(
        SystemNotification {
            tag: tag.into(),
            title: product_name(cx).into(),
            body: alert_text(kind).into(),
            actions: Vec::new(),
        },
        cx,
    );
}

fn product_name(cx: &App) -> &str {
    cx.try_global::<subspace_dashboard::CurrentProduct>()
        .map(|context| context.0.product.display_name.as_str())
        .unwrap_or("Subspace")
}

#[cfg(target_os = "macos")]
pub(crate) fn play_sound(cx: &App) {
    subspace_dashboard::alerts::play_sound(subspace_dashboard::alerts::DEFAULT_SOUND, cx);
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn play_sound(cx: &App) {
    if !subspace_dashboard::notifications::sounds_enabled(cx) {
        return;
    }
    #[cfg(test)]
    {
        TEST_SOUND_PLAYS.with(|plays| plays.set(plays.get() + 1));
    }
    #[cfg(not(test))]
    cx.background_spawn(async move {
        run_sound();
    })
    .detach();
}

#[cfg(all(test, not(target_os = "macos")))]
thread_local! {
    static TEST_SOUND_PLAYS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

fn stored_flag(cx: &App, key: &str, default: bool) -> bool {
    prefs::get(cx, key)
        .map(|value| value != "0")
        .unwrap_or(default)
}

fn flag(enabled: bool) -> &'static str {
    if enabled { "1" } else { "0" }
}

#[cfg(all(target_os = "windows", not(test)))]
fn run_sound() {
    spawn_sound(
        "powershell.exe",
        &[
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "[System.Media.SystemSounds]::Asterisk.Play()",
        ],
    );
}

#[cfg(all(target_os = "linux", not(test)))]
fn run_sound() {
    spawn_sound("canberra-gtk-play", &["--id", "message"]);
}

#[cfg(all(
    not(test),
    not(any(target_os = "macos", target_os = "windows", target_os = "linux"))
))]
fn run_sound() {}

#[cfg(all(not(test), any(target_os = "windows", target_os = "linux")))]
fn spawn_sound(program: &str, arguments: &[&str]) {
    let result = Command::new(program)
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    let _ = sound_failure(result).map(|message| eprintln!("subspace: {message}"));
}

#[cfg(any(target_os = "windows", target_os = "linux"))]
fn sound_failure(result: std::io::Result<std::process::ExitStatus>) -> Option<String> {
    match result {
        Ok(status) if status.success() => None,
        Ok(_) => Some("notification sound command failed".into()),
        Err(error) => Some(format!("could not play notification sound: {error}")),
    }
}

#[cfg(test)]
mod tests {
    use super::{NotificationPreferences, alert_text};
    use subspace_chat::ChatAttentionKind;

    #[cfg(not(target_os = "macos"))]
    #[gpui_kit::test]
    fn master_mutes_shell_playback(cx: &mut gpui_kit::TestAppContext) {
        cx.update(|cx| {
            subspace_dashboard::prefs::init_file(None, cx);
            super::TEST_SOUND_PLAYS.with(|plays| plays.set(0));
            super::play_sound(cx);
            subspace_dashboard::prefs::set(cx, subspace_dashboard::notifications::SOUNDS_KEY, "0");
            super::play_sound(cx);
            super::TEST_SOUND_PLAYS.with(|plays| assert_eq!(plays.get(), 1));
        });
    }

    #[gpui_kit::test]
    fn copilot_and_board_os_posts_follow_master_including_windowless_and_test(
        cx: &mut gpui_kit::TestAppContext,
    ) {
        use subspace_dashboard::notifications::{Mode, Notice, Preferences, SOUNDS_KEY, Sound};
        cx.update(|cx| {
            subspace_dashboard::prefs::init_file(None, cx);
            super::init(cx);
            cx.set_global(super::system::TestPosts::default());
            let preferences = Preferences {
                mode: Mode::System,
                sound: Sound::Glass,
                ..Preferences::default()
            };
            for muted in [false, true] {
                subspace_dashboard::prefs::set(cx, SOUNDS_KEY, if muted { "0" } else { "1" });
                super::show_system_alert(ChatAttentionKind::TurnCompleted, 1, cx);
                for route in [super::Route::Board("fixture", 42), super::Route::Test] {
                    super::runtime::deliver(route, Notice::new("Fixture"), preferences, cx);
                }
            }
            let posts = &cx.global::<super::system::TestPosts>().0;
            assert_eq!(posts.len(), 6);
            assert_eq!(
                posts.iter().map(|(_, silent)| *silent).collect::<Vec<_>>(),
                [false, false, false, true, true, true]
            );
            assert!(posts.iter().all(|(post, _)| !post.tag.is_empty()));
        });
    }

    #[test]
    fn channels_are_independent_and_text_is_generic() {
        let preferences = NotificationPreferences {
            sounds: true,
            completion: true,
            waiting: false,
            os: false,
            sound: true,
        };
        assert!(preferences.allows(ChatAttentionKind::TurnCompleted));
        assert!(!preferences.allows(ChatAttentionKind::WaitingForInput));
        assert_eq!(
            alert_text(ChatAttentionKind::TurnCompleted),
            "An AI turn completed."
        );
        assert_eq!(
            alert_text(ChatAttentionKind::WaitingForInput),
            "AI needs your input."
        );
    }

    #[test]
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    fn unavailable_sound_playback_has_a_diagnostic() {
        let missing = std::io::Error::new(std::io::ErrorKind::NotFound, "sound player unavailable");
        assert_eq!(
            super::sound_failure(Err(missing)).as_deref(),
            Some("could not play notification sound: sound player unavailable")
        );
    }
}
