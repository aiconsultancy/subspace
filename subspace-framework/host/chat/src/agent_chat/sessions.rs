//! The Sessions tab: this account's saved chats, grouped by when they last
//! changed. The rules here are pure; the rendering lives in `sessions/view`.

use chrono::{DateTime, Local, Utc};
use gpui_kit::component::input::InputState;
use gpui_kit::{Entity, SharedString};
use subspace_agent::SessionSummary;

mod actions;
mod menu;
mod view;

/// How much of a session id a row shows.
const ID_PREFIX: usize = 8;

/// When a saved chat last changed, as the list groups it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Group {
    Today,
    Yesterday,
    Older,
}

impl Group {
    pub(super) const ALL: [Self; 3] = [Self::Today, Self::Yesterday, Self::Older];

    pub(super) fn heading(self) -> &'static str {
        match self {
            Self::Today => "Today",
            Self::Yesterday => "Yesterday",
            Self::Older => "Older",
        }
    }
}

/// Everything the Sessions tab keeps: its search text, the highlighted row,
/// an in-place rename, and a delete waiting for confirmation.
pub(in crate::agent_chat) struct SessionsView {
    pub(in crate::agent_chat) search: Entity<InputState>,
    pub(in crate::agent_chat) query: String,
    /// The highlighted row, as an index into the listed sessions.
    pub(in crate::agent_chat) selected: usize,
    /// How wide the list was last laid out, so the metadata can give way to
    /// the name in a narrow panel.
    pub(in crate::agent_chat) width: Entity<f32>,
    /// The session being renamed in place.
    pub(in crate::agent_chat) renaming: Option<String>,
    pub(in crate::agent_chat) rename: Entity<InputState>,
    /// The session a confirmation dialog is asking about.
    pub(in crate::agent_chat) confirm_delete: Option<SessionSummary>,
    /// One line of feedback, such as a finished export.
    pub(in crate::agent_chat) notice: Option<String>,
}

impl SessionsView {
    pub(in crate::agent_chat) fn new(
        search: Entity<InputState>,
        rename: Entity<InputState>,
        width: Entity<f32>,
    ) -> Self {
        Self {
            search,
            rename,
            width,
            query: String::new(),
            selected: 0,
            renaming: None,
            confirm_delete: None,
            notice: None,
        }
    }
}

/// Whether a saved chat matches what was typed in the search field. The
/// title is the session name, or the cleaned first message when it has none,
/// so one case-insensitive match covers both.
pub(super) fn matches(session: &SessionSummary, query: &str) -> bool {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return true;
    }
    session.title.to_lowercase().contains(&query)
}

/// The sessions the list shows, in the runtime's order, newest first.
pub(super) fn listed<'a>(sessions: &'a [SessionSummary], query: &str) -> Vec<&'a SessionSummary> {
    sessions
        .iter()
        .filter(|session| matches(session, query))
        .collect()
}

/// Which group a time belongs to, by local calendar date.
pub(super) fn group_of(modified: &str, now: DateTime<Utc>) -> Group {
    let Ok(then) = DateTime::parse_from_rfc3339(modified) else {
        return Group::Older;
    };
    let today = now.with_timezone(&Local).date_naive();
    let date = then.with_timezone(&Local).date_naive();
    match (today - date).num_days() {
        0 => Group::Today,
        1 => Group::Yesterday,
        _ => Group::Older,
    }
}

/// The listed sessions in their groups, in heading order, with the empty
/// groups left out.
pub(super) fn grouped<'a>(
    sessions: &[&'a SessionSummary],
    now: DateTime<Utc>,
) -> Vec<(Group, Vec<&'a SessionSummary>)> {
    Group::ALL
        .into_iter()
        .filter_map(|group| {
            let rows: Vec<&SessionSummary> = sessions
                .iter()
                .copied()
                .filter(|session| group_of(&session.modified_at, now) == group)
                .collect();
            (!rows.is_empty()).then_some((group, rows))
        })
        .collect()
}

/// The letter on a row's badge: the first letter of the name, upper case.
pub(super) fn badge_letter(title: &str) -> SharedString {
    title
        .chars()
        .find(|character| character.is_alphanumeric())
        .map(|character| character.to_uppercase().to_string())
        .unwrap_or_else(|| "·".to_owned())
        .into()
}

/// How much of the session id a row shows, so two chats with one name can
/// still be told apart.
pub(super) fn id_prefix(id: &str) -> &str {
    let end = id
        .char_indices()
        .nth(ID_PREFIX)
        .map_or(id.len(), |(index, _)| index);
    &id[..end]
}

/// The created time a hovered row shows, in the local time zone. An
/// unparsable time is left out rather than guessed at.
pub(super) fn created_label(created: &str) -> Option<SharedString> {
    let then = DateTime::parse_from_rfc3339(created).ok()?;
    Some(
        then.with_timezone(&Local)
            .format("%-d %b %Y %H:%M")
            .to_string()
            .into(),
    )
}

/// Whether a row shows its extra metadata without being hovered: the
/// keyboard highlight is on it. The pointer's own rows show it through the
/// group's hover style, which costs no render and so cannot disturb a click.
pub(in crate::agent_chat) fn shows_metadata(index: usize, selected: usize) -> bool {
    index == selected
}

/// Roughly what the created time takes, with its icon and the gap after it.
const CREATED_WIDTH: f32 = 128.;
/// Roughly what the session id prefix takes, with its icon.
const ID_WIDTH: f32 = 84.;
/// The badge, the relative time and the gaps, which every row shows.
const FIXED_WIDTH: f32 = 116.;
/// The share of what is left over that the metadata may claim; the name
/// keeps the rest, which is never less than half.
const METADATA_SHARE: f32 = 0.5;

/// Which parts of the metadata a row of this width shows. The name has
/// priority: the id prefix gives way first, then the created time, so a
/// narrow panel shows the name rather than a few letters of it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::agent_chat) struct MetadataFit {
    pub(in crate::agent_chat) created: bool,
    pub(in crate::agent_chat) id: bool,
}

pub(in crate::agent_chat) fn metadata_fit(row_width: f32) -> MetadataFit {
    if !row_width.is_finite() || row_width <= FIXED_WIDTH {
        return MetadataFit::default();
    }
    let budget = (row_width - FIXED_WIDTH) * METADATA_SHARE;
    MetadataFit {
        created: budget >= CREATED_WIDTH,
        id: budget >= CREATED_WIDTH + ID_WIDTH,
    }
}

/// The highlighted row after a key press, clamped to the listed rows.
pub(super) fn moved_selection(count: usize, selected: usize, step: isize) -> usize {
    if count == 0 {
        return 0;
    }
    let last = count - 1;
    let moved = selected as isize + step;
    moved.clamp(0, last as isize) as usize
}

/// The row the highlight addresses, if the list still has one.
pub(super) fn selected_session<'a>(
    sessions: &[&'a SessionSummary],
    selected: usize,
) -> Option<&'a SessionSummary> {
    sessions.get(selected).copied()
}

#[cfg(test)]
mod tests {
    use super::{
        Group, badge_letter, created_label, group_of, grouped, id_prefix, listed, matches,
        metadata_fit, moved_selection, selected_session, shows_metadata,
    };
    use chrono::{DateTime, Local, TimeZone as _, Utc};
    use subspace_agent::SessionSummary;

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-10-05T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    /// A local time on `days` ago, so the grouping is tested in the zone the
    /// list actually renders in.
    fn local_days_ago(days: i64) -> String {
        let local = now().with_timezone(&Local) - chrono::Duration::days(days);
        local.to_rfc3339()
    }

    fn session(id: &str, title: &str, modified_at: &str) -> SessionSummary {
        SessionSummary {
            id: id.into(),
            title: title.into(),
            modified: "just now".into(),
            modified_at: modified_at.into(),
            created_at: "2026-10-05T09:00:00Z".into(),
        }
    }

    #[test]
    fn search_matches_the_name_case_insensitively() {
        let release = session("1", "Release notes", "");
        assert!(matches(&release, ""));
        assert!(matches(&release, "  "));
        assert!(matches(&release, "release"));
        assert!(matches(&release, "NOTES"));
        assert!(!matches(&release, "risks"));
    }

    #[test]
    fn the_list_keeps_the_runtimes_order() {
        let sessions = vec![
            session("1", "Release notes", ""),
            session("2", "Risk review", ""),
            session("3", "Release plan", ""),
        ];
        let shown = listed(&sessions, "release");
        let ids: Vec<&str> = shown.iter().map(|row| row.id.as_str()).collect();
        assert_eq!(ids, ["1", "3"]);
        assert_eq!(listed(&sessions, "").len(), 3);
    }

    #[test]
    fn sessions_group_by_their_local_date() {
        assert_eq!(group_of(&local_days_ago(0), now()), Group::Today);
        assert_eq!(group_of(&local_days_ago(1), now()), Group::Yesterday);
        assert_eq!(group_of(&local_days_ago(4), now()), Group::Older);
        assert_eq!(
            group_of("whenever", now()),
            Group::Older,
            "an unreadable time is not guessed into today"
        );
    }

    #[test]
    fn empty_groups_are_left_out_and_headings_keep_their_order() {
        let sessions = vec![
            session("today", "Today chat", &local_days_ago(0)),
            session("old", "Old chat", &local_days_ago(9)),
            session("today-2", "Another today", &local_days_ago(0)),
        ];
        let listed = listed(&sessions, "");
        let groups = grouped(&listed, now());
        assert_eq!(groups.len(), 2, "yesterday is empty and hidden");
        assert_eq!(groups[0].0, Group::Today);
        assert_eq!(groups[0].1.len(), 2);
        assert_eq!(groups[1].0, Group::Older);
        assert_eq!(groups[1].1[0].id, "old");
        assert!(grouped(&[], now()).is_empty());
    }

    #[test]
    fn group_headings_read_as_dates() {
        assert_eq!(Group::Today.heading(), "Today");
        assert_eq!(Group::Yesterday.heading(), "Yesterday");
        assert_eq!(Group::Older.heading(), "Older");
    }

    #[test]
    fn a_badge_shows_the_first_letter_of_the_name() {
        assert_eq!(badge_letter("Release notes"), "R");
        assert_eq!(badge_letter("  risk review"), "R");
        assert_eq!(badge_letter("42 items"), "4");
        assert_eq!(badge_letter("   "), "·");
        assert_eq!(badge_letter(""), "·");
    }

    #[test]
    fn a_row_shows_only_the_head_of_the_session_id() {
        assert_eq!(id_prefix("0123456789abcdef"), "01234567");
        assert_eq!(id_prefix("short"), "short");
        assert_eq!(id_prefix(""), "");
    }

    #[test]
    fn a_created_time_is_shown_only_when_the_sdk_gave_one() {
        let label = created_label("2026-10-05T09:30:00Z").unwrap();
        assert!(label.contains("2026"), "{label}");
        assert!(created_label("").is_none());
        assert!(created_label("whenever").is_none());
    }

    #[test]
    fn the_highlighted_row_shows_its_metadata_without_being_hovered() {
        assert!(shows_metadata(0, 0));
        assert!(!shows_metadata(1, 0));
    }

    #[test]
    fn a_narrow_row_keeps_the_name_and_drops_the_metadata_in_order() {
        let wide = metadata_fit(800.);
        assert!(wide.created && wide.id, "both fit beside the name");
        let middling = metadata_fit(420.);
        assert!(
            middling.created && !middling.id,
            "the id prefix gives way first: {middling:?}"
        );
        let narrow = metadata_fit(260.);
        assert_eq!(
            narrow,
            super::MetadataFit::default(),
            "the narrow panel that squeezed the name to a few letters"
        );
    }

    #[test]
    fn an_unmeasured_list_shows_no_metadata() {
        assert_eq!(metadata_fit(0.), super::MetadataFit::default());
        assert_eq!(metadata_fit(f32::NAN), super::MetadataFit::default());
        assert_eq!(
            metadata_fit(super::FIXED_WIDTH),
            super::MetadataFit::default()
        );
    }

    /// The name keeps half of what the badge and the time leave, so the
    /// parts shown never claim more than that.
    #[test]
    fn the_metadata_never_claims_the_names_share() {
        for width in [200., 300., 420., 500., 800., 1200.] {
            let fit = metadata_fit(width);
            let claimed = f32::from(u8::from(fit.created)) * super::CREATED_WIDTH
                + f32::from(u8::from(fit.id)) * super::ID_WIDTH;
            let name = (width - super::FIXED_WIDTH).max(0.) * (1. - super::METADATA_SHARE);
            assert!(
                claimed <= (width - super::FIXED_WIDTH - name).max(0.),
                "{width} left the name {name} and gave away {claimed}"
            );
        }
    }

    #[test]
    fn the_highlight_stops_at_both_ends() {
        assert_eq!(moved_selection(3, 0, 1), 1);
        assert_eq!(moved_selection(3, 2, 1), 2);
        assert_eq!(moved_selection(3, 0, -1), 0);
        assert_eq!(moved_selection(0, 0, 1), 0);
    }

    #[test]
    fn the_highlight_addresses_a_listed_session() {
        let sessions = vec![session("1", "One", ""), session("2", "Two", "")];
        let listed = listed(&sessions, "");
        assert_eq!(
            selected_session(&listed, 1).map(|row| row.id.as_str()),
            Some("2")
        );
        assert!(selected_session(&listed, 9).is_none());
        assert!(selected_session(&[], 0).is_none());
    }

    #[test]
    fn a_utc_timestamp_lands_in_the_local_day_it_belongs_to() {
        let midday = Local.with_ymd_and_hms(2026, 10, 5, 12, 0, 0).unwrap();
        assert_eq!(
            group_of(&midday.to_rfc3339(), midday.with_timezone(&Utc)),
            Group::Today
        );
    }
}
