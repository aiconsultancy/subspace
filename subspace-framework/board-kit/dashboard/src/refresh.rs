//! Shared preferences and timing for automatic board reads, not manual refresh.

use std::time::Duration;

use gpui_kit::App;

pub const MIN_INTERVAL_SECS: u64 = 30;
pub const MAX_INTERVAL_SECS: u64 = 86_400;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Preferences {
    pub enabled: bool,
    /// None preserves each existing refresh path's default interval.
    pub interval_seconds: Option<u64>,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            enabled: true,
            interval_seconds: None,
        }
    }
}

impl Preferences {
    pub fn decode(enabled: Option<&str>, interval: Option<&str>) -> Self {
        Self {
            enabled: enabled != Some("0"),
            interval_seconds: interval
                .and_then(|value| value.parse().ok())
                .filter(|value| (MIN_INTERVAL_SECS..=MAX_INTERVAL_SECS).contains(value)),
        }
    }

    pub fn save(self, board: &str, cx: &mut App) {
        crate::prefs::set(
            cx,
            &format!("{board}.background-updates"),
            if self.enabled { "1" } else { "0" },
        );
        let key = format!("{board}.refresh-interval-seconds");
        match self
            .interval_seconds
            .filter(|value| (MIN_INTERVAL_SECS..=MAX_INTERVAL_SECS).contains(value))
        {
            Some(seconds) => crate::prefs::set(cx, &key, seconds.to_string()),
            None => crate::prefs::remove(cx, &key),
        }
    }
}

pub fn preferences(board: &str, cx: &App) -> Preferences {
    Preferences::decode(
        crate::prefs::get(cx, &format!("{board}.background-updates")).as_deref(),
        crate::prefs::get(cx, &format!("{board}.refresh-interval-seconds")).as_deref(),
    )
}

pub fn enabled(board: &str, cx: &App) -> bool {
    crate::services(cx).is_none_or(|services| services.config.auto_refresh)
        && preferences(board, cx).enabled
}

pub fn interval(board: &str, default: Duration, cx: &App) -> Duration {
    preferences(board, cx)
        .interval_seconds
        .map_or(default, Duration::from_secs)
}

pub struct Schedule {
    start_after: Duration,
    last_refresh: Option<Duration>,
}

impl Schedule {
    pub fn new(start_after: Duration) -> Self {
        Self {
            start_after,
            last_refresh: None,
        }
    }

    pub fn ready(&mut self, now: Duration, enabled: bool, interval: Duration) -> bool {
        let deadline = self
            .last_refresh
            .map_or(self.start_after, |last| last.saturating_add(interval));
        if !enabled || now < deadline {
            return false;
        }
        self.last_refresh = Some(now);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::{Preferences, Schedule};
    use std::time::Duration;

    #[test]
    fn disabled_updates_do_not_advance_the_deadline_and_reenable_without_restart() {
        let mut schedule = Schedule::new(Duration::from_secs(2));
        let interval = Duration::from_secs(60);
        assert!(!schedule.ready(Duration::from_secs(1), true, interval));
        assert!(!schedule.ready(Duration::from_secs(70), false, interval));
        assert!(schedule.ready(Duration::from_secs(70), true, interval));
        assert!(!schedule.ready(Duration::from_secs(71), true, interval));
        assert!(schedule.ready(Duration::from_secs(130), true, interval));
    }

    #[test]
    fn interval_changes_use_the_previous_refresh_time_instead_of_a_stale_sleep() {
        let mut schedule = Schedule::new(Duration::ZERO);
        assert!(schedule.ready(Duration::ZERO, true, Duration::from_secs(300)));
        assert!(!schedule.ready(Duration::from_secs(60), true, Duration::from_secs(300)));
        assert!(schedule.ready(Duration::from_secs(60), true, Duration::from_secs(30)));
        assert!(!schedule.ready(Duration::from_secs(90), true, Duration::from_secs(600)));
        assert!(schedule.ready(Duration::from_secs(660), true, Duration::from_secs(600)));
    }

    #[test]
    fn saved_preferences_are_validated_without_changing_existing_defaults() {
        assert_eq!(Preferences::decode(None, None), Preferences::default());
        assert_eq!(
            Preferences::decode(Some("0"), Some("600")),
            Preferences {
                enabled: false,
                interval_seconds: Some(600)
            }
        );
        for value in ["0", "29", "86401", "invalid", "-30"] {
            assert_eq!(
                Preferences::decode(None, Some(value)).interval_seconds,
                None
            );
        }
        assert_eq!(
            Preferences::decode(Some("1"), Some("30")).interval_seconds,
            Some(30)
        );
    }

    #[gpui_kit::test]
    fn saving_preferences_keeps_other_boards_and_respects_the_global_switch(
        cx: &mut gpui_kit::TestAppContext,
    ) {
        cx.update(|cx| {
            crate::prefs::init_file(None, cx);
            Preferences {
                enabled: false,
                interval_seconds: Some(600),
            }
            .save("github", cx);
            assert!(!super::enabled("github", cx));
            assert!(super::enabled("actions", cx));
            assert_eq!(
                super::interval("github", Duration::from_secs(60), cx),
                Duration::from_secs(600)
            );
            assert_eq!(
                super::interval("actions", Duration::from_secs(30), cx),
                Duration::from_secs(30)
            );
            let config = subspace_core::config::Config {
                auto_refresh: false,
                ..Default::default()
            };
            cx.set_global(crate::Services {
                credentials: std::sync::Arc::new(
                    subspace_core::credentials::Credentials::from_system(&config),
                ),
                config: std::sync::Arc::new(config),
                tools: Default::default(),
                search: None,
                config_file: Default::default(),
            });
            assert!(!super::enabled("actions", cx));
            assert!(super::preferences("actions", cx).enabled);
        });
    }
}
