//! The shell's stored choices and the small facts derived from them.

use gpui_kit::{Context, SharedString};
use subspace_core::palette::push_recent;
use subspace_dashboard::{DashboardHandle, prefs};

use super::{MAX_RECENT, SubspaceApp};

/// The dashboard to show at startup: the stored one, else the first.
pub(super) fn active_index(dashboards: &[DashboardHandle], saved: Option<&str>) -> usize {
    dashboards
        .iter()
        .position(|d| Some(d.id) == saved)
        .unwrap_or(0)
}

/// The dashboard `step` places away, wrapping at both ends.
pub(super) fn step_index(active: usize, step: isize, count: usize) -> Option<usize> {
    if count == 0 {
        return None;
    }
    Some((active as isize + step).rem_euclid(count as isize) as usize)
}

pub(super) fn recent_key(dashboard: &str) -> String {
    format!("{dashboard}.recent")
}

impl SubspaceApp {
    /// Record `item` as the newest recently opened item of `dashboard`.
    pub(super) fn push_recent(
        &mut self,
        dashboard: &'static str,
        item: String,
        cx: &mut Context<Self>,
    ) {
        let list = self.recent.entry(dashboard).or_default();
        *list = push_recent(list, item, MAX_RECENT);
        let list = list.clone();
        prefs::set_list(cx, &recent_key(dashboard), &list);
    }
}

/// A stored flag is written as "1" or "0"; anything else reads as off.
pub(super) fn stored_flag(value: Option<&str>) -> bool {
    value == Some("1")
}

pub(super) fn wrap_preference(wrap: bool) -> &'static str {
    if wrap { "1" } else { "0" }
}

/// Compose the app identity, registered board title, and board context.
pub(super) fn window_title(
    product: &str,
    dashboard: Option<&str>,
    own: Option<SharedString>,
) -> String {
    match (dashboard, own) {
        (Some(name), Some(context)) => format!("{product} - {name} · {context}"),
        (Some(name), None) => format!("{product} - {name}"),
        (None, None) => product.to_owned(),
        (None, Some(context)) => format!("{product} - {context}"),
    }
}

pub(super) fn config_notice(notice: &str) -> String {
    format!("Config: {notice}")
}

#[cfg(test)]
mod tests {
    use super::{active_index, recent_key, step_index, stored_flag, window_title, wrap_preference};

    #[test]
    fn stored_choices_are_written_as_one_or_zero() {
        assert!(stored_flag(Some("1")));
        assert!(!stored_flag(Some("0")));
        assert!(!stored_flag(None));
        assert_eq!(wrap_preference(true), "1");
        assert_eq!(wrap_preference(false), "0");
        assert_eq!(recent_key("board"), "board.recent");
    }

    #[test]
    fn the_startup_dashboard_falls_back_to_the_first() {
        assert_eq!(active_index(&[], Some("board")), 0);
        assert_eq!(active_index(&[], None), 0);
    }

    #[test]
    fn stepping_wraps_at_both_ends_and_stops_with_no_dashboards() {
        assert_eq!(step_index(0, 1, 4), Some(1));
        assert_eq!(step_index(3, 1, 4), Some(0));
        assert_eq!(step_index(0, -1, 4), Some(3));
        assert_eq!(step_index(0, 1, 0), None);
    }

    #[test]
    fn the_window_title_is_board_first_and_adds_context() {
        assert_eq!(
            window_title("Subspace", Some("Board"), Some("Board — 12".into())),
            "Subspace - Board · Board — 12"
        );
        assert_eq!(
            window_title("Subspace", Some("Board"), None),
            "Subspace - Board"
        );
        assert_eq!(window_title("GitHub", None, None), "GitHub");
    }
}
