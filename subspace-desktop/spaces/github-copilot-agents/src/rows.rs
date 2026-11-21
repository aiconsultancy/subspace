//! The merged rows as the shared list draws them: one group per state
//! group, with the rows already sorted.

use chrono::{DateTime, Utc};
use subspace_framework::client_github::model::PrState;
use subspace_framework::dashboard::{Glyph, Group, Row, Tone};

use crate::dates;
use crate::model::{AgentRow, StateGroup, UNKNOWN, state_label};

/// The colour a state reads in: what waits for an answer stands out, what
/// failed is red, what finished is green, what runs is blue.
pub fn state_tone(state: &str) -> Tone {
    match state {
        "waiting_for_user" => Tone::Yellow,
        "failed" | "timed_out" => Tone::Red,
        "cancelled" | "idle" => Tone::Muted,
        "completed" => Tone::Green,
        "queued" | "in_progress" | crate::model::LOCAL_BUSY => Tone::Blue,
        UNKNOWN => Tone::Muted,
        _ => Tone::Fg,
    }
}

/// Every pull request state, with the colour and the word it reads as.
const PULL_STATES: [(PrState, Tone, &str); 4] = [
    (PrState::Merged, Tone::Magenta, "merged"),
    (PrState::Closed, Tone::Red, "closed"),
    (PrState::Draft, Tone::Muted, "draft"),
    (PrState::Open, Tone::Green, "open"),
];

/// The colour a pull request's state reads in.
pub fn pull_tone(state: PrState) -> Tone {
    PULL_STATES
        .iter()
        .find(|(known, _, _)| *known == state)
        .map_or(Tone::Fg, |(_, tone, _)| *tone)
}

/// The word a pull request's state reads as.
pub fn pull_label(state: PrState) -> &'static str {
    PULL_STATES
        .iter()
        .find(|(known, _, _)| *known == state)
        .map_or("unknown", |(_, _, label)| *label)
}

/// The colour a state group's header reads in.
fn group_tone(group: StateGroup) -> Tone {
    match group {
        StateGroup::NeedsInput => Tone::Yellow,
        StateGroup::Active => Tone::Blue,
        StateGroup::Finished => Tone::Muted,
    }
}

/// The marks after a row's title: its state, its pull request, whether it is
/// monitored, and how long ago it changed.
fn trailing(row: &AgentRow, now: DateTime<Utc>) -> Vec<Glyph> {
    let mut marks = vec![Glyph::new(state_label(&row.state), state_tone(&row.state))];
    if let Some(pull) = &row.pull_request {
        marks.push(Glyph::new(
            format!("#{} {}", pull.number, pull_label(pull.state)),
            pull_tone(pull.state),
        ));
    }
    if row.monitor {
        marks.push(Glyph::new("\u{25c9} watched", Tone::Accent));
    }
    let age = dates::age(&row.updated_at, now);
    if !age.is_empty() {
        marks.push(Glyph::new(age, Tone::Muted));
    }
    marks
}

/// One list row. It has no short id: the list draws the repository, or
/// that a row is a local chat, in a lane after the title (`lanes.rs`), so
/// the titles line up.
pub fn list_row(row: &AgentRow, now: DateTime<Utc>) -> Row {
    Row {
        key: row.key.clone().into(),
        icon: Glyph::new(row.kind.mark(), state_tone(&row.state)),
        id: None,
        title: row.title.clone().into(),
        trailing: trailing(row, now),
        done: row.group() == StateGroup::Finished,
        indent: 0,
    }
}

/// The groups the list shows: one per state group, in their order, with the
/// empty ones left out.
pub fn groups(rows: &[AgentRow], now: DateTime<Utc>) -> Vec<Group> {
    StateGroup::ALL
        .into_iter()
        .filter_map(|group| {
            let rows: Vec<Row> = rows
                .iter()
                .filter(|row| row.group() == group)
                .map(|row| list_row(row, now))
                .collect();
            (!rows.is_empty()).then(|| Group {
                key: format!("group:{}", group.id()).into(),
                icon: Glyph::new("\u{25c6}", group_tone(group)),
                name: group.label().into(),
                count: Some(rows.len().to_string().into()),
                rows,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use chrono::{DateTime, Utc};
    use subspace_framework::client_github::copilot_agents::PullRequestArtifact;
    use subspace_framework::client_github::model::PrState;
    use subspace_framework::dashboard::Tone;

    use super::{groups, pull_label, pull_tone, state_tone};
    use crate::model::{AgentRow, Kind, StateGroup};

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-10-07T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    fn row(key: &str, state: &str) -> AgentRow {
        AgentRow {
            key: format!("cloud:{key}"),
            kind: Kind::Cloud,
            id: key.into(),
            title: format!("Fix {key}"),
            repo: "Owner/one".into(),
            state: state.into(),
            created_at: "2026-10-01T09:00:00Z".into(),
            updated_at: "2026-10-07T09:00:00Z".into(),
            monitor: false,
            subject: None,
            html_url: String::new(),
            pull_request: None,
        }
    }

    #[test]
    fn empty_state_groups_are_left_out_and_the_rest_keep_their_order() {
        let built = groups(
            &[row("a", "completed"), row("b", "waiting_for_user")],
            now(),
        );
        assert_eq!(built.len(), 2);
        assert_eq!(built[0].name, StateGroup::NeedsInput.label());
        assert_eq!(built[0].key, "group:needs-input");
        assert_eq!(built[1].name, StateGroup::Finished.label());
        assert_eq!(built[0].count.as_deref(), Some("1"));
        assert!(groups(&[], now()).is_empty());
    }

    #[test]
    fn a_row_shows_its_state_and_age_and_leaves_its_repository_to_a_lane() {
        let built = groups(&[row("a", "in_progress")], now());
        let listed = &built[0].rows[0];
        assert_eq!(listed.key, "cloud:a");
        assert!(listed.id.is_none(), "no id column before the title");
        assert_eq!(listed.title, "Fix a");
        assert!(!listed.done);
        let marks: Vec<&str> = listed.trailing.iter().map(|m| m.text.as_ref()).collect();
        assert_eq!(marks, ["in progress", "3h"]);
    }

    #[test]
    fn a_local_row_is_finished_work_once_it_is_closed() {
        let mut local = row("s", "open");
        local.kind = Kind::Local;
        local.repo = String::new();
        local.key = "local:s".into();
        let built = groups(&[local], now());
        assert!(built[0].rows[0].id.is_none());
        assert!(built[0].rows[0].done, "a closed chat is finished work");
    }

    #[test]
    fn a_watched_row_with_a_pull_request_shows_both() {
        let mut watched = row("a", "waiting_for_user");
        watched.monitor = true;
        watched.pull_request = Some(PullRequestArtifact {
            number: 7,
            url: "https://github.com/Owner/one/pull/7".into(),
            state: PrState::Draft,
        });
        let built = groups(&[watched], now());
        let marks: Vec<&str> = built[0].rows[0]
            .trailing
            .iter()
            .map(|m| m.text.as_ref())
            .collect();
        assert_eq!(
            marks,
            ["waiting for user", "#7 draft", "\u{25c9} watched", "3h"]
        );
    }

    #[test]
    fn states_and_pull_requests_read_in_their_own_colours() {
        assert_eq!(state_tone("waiting_for_user"), Tone::Yellow);
        assert_eq!(state_tone("failed"), Tone::Red);
        assert_eq!(state_tone("completed"), Tone::Green);
        assert_eq!(state_tone("in_progress"), Tone::Blue);
        assert_eq!(state_tone("unknown"), Tone::Muted);
        assert_eq!(state_tone("new_state"), Tone::Fg);
        assert_eq!(pull_tone(PrState::Merged), Tone::Magenta);
        assert_eq!(pull_label(PrState::Open), "open");
        assert_eq!(pull_label(PrState::Merged), "merged");
    }
}
