//! An issue's tile under the message that keeps it: the state dot and key,
//! the title, then one line of what the tracker says (Ready for agents,
//! Tracker, the state, who it is with and when it is due) and, for a
//! tracker with sub-items, its progress. Only what the summary carries is
//! shown, and a missing fact leaves no gap.

use chrono::{DateTime, NaiveDate, Utc};
use gpui_kit::component::h_flex;
use gpui_kit::{
    AnyElement, App, Div, IntoElement as _, ParentElement as _, SharedString, Styled as _, div,
};
use subspace_framework::contracts::issues::IssueSummary;
use subspace_framework::dashboard::avatar::{self, AgentIcons, AgentRank};
use subspace_framework::dashboard::{Tokens, Tone};

use super::{activity, facts};
use crate::tiles::{self, Lead, Slot};

/// What separates the facts on the second line.
const SEPARATOR: &str = " · ";

/// One fact on an issue tile's second line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::issues) enum Part {
    /// The issue is marked Ready for agents.
    Ready,
    Text(String),
    /// Who it is assigned to, with an agent's rank when it has one the
    /// agent icon can show.
    Assignee {
        name: String,
        rank: Option<AgentRank>,
    },
}

/// What an issue tile's second line says, in order: Ready for agents,
/// Tracker, Archived, the state, the assignee (or else the owner), and the
/// due date relative to `today`.
pub(in crate::issues) fn parts(summary: &IssueSummary, today: NaiveDate) -> Vec<Part> {
    let mut parts = Vec::new();
    if summary.ready_for_agents() == Some(true) {
        parts.push(Part::Ready);
    }
    if summary.tracker {
        parts.push(Part::Text("Tracker".into()));
    }
    if summary.archived {
        parts.push(Part::Text("Archived".into()));
    }
    parts.push(Part::Text(summary.state.name.clone()));
    parts.extend(person(summary));
    parts.extend(
        summary
            .due_date()
            .and_then(|due| activity::due(due, today))
            .map(|due| Part::Text(due.replacen("Due", "due", 1))),
    );
    parts
}

/// The assignee, or the owner when nobody is assigned.
fn person(summary: &IssueSummary) -> Option<Part> {
    match &summary.assignee {
        Some(name) => Some(Part::Assignee {
            name: name.clone(),
            rank: rank(summary),
        }),
        None => summary.owner().map(|owner| Part::Text(owner.name.clone())),
    }
}

/// An agent assignee's rank as the agent icon draws it: captains and the
/// rear admiral. Crew and people have none.
fn rank(summary: &IssueSummary) -> Option<AgentRank> {
    if !facts::is_agent(summary) {
        return None;
    }
    match summary.assignee_rank() {
        Some("captain") => Some(AgentRank::Captain),
        Some("rearAdmiral" | "rear_admiral" | "rear-admiral") => Some(AgentRank::RearAdmiral),
        _ => None,
    }
}

/// `done of total` and its spoken form, for a tracker with sub-items.
pub(in crate::issues) fn progress(summary: &IssueSummary) -> Option<(u32, u32, String)> {
    summary
        .progress()
        .filter(|progress| progress.total > 0)
        .map(|progress| {
            let words = format!("{} of {}", progress.done, progress.total);
            (progress.done, progress.total, words)
        })
}

/// The tile's content, read at `now` with the app's Icons setting.
pub(in crate::issues) fn tile(
    summary: &IssueSummary,
    (now, icons): (DateTime<Utc>, AgentIcons),
    tokens: &Tokens,
    cx: &App,
) -> Div {
    let parts = parts(summary, now.date_naive());
    let slot = Slot {
        lead: Lead {
            mark: crate::text::look::issue_mark(&summary.state, tokens),
            key: summary.identifier.clone(),
        },
        title: summary.title.clone().into(),
        meta: meta(&summary.identifier, &parts, icons, tokens),
        trailing: meter(summary, tokens),
    };
    tiles::tile(slot, tokens, cx)
}

fn meter(summary: &IssueSummary, tokens: &Tokens) -> Option<AnyElement> {
    progress(summary).map(|(done, total, words)| {
        let spoken = format!("{words} done");
        let id = SharedString::from(format!("issue-tile-progress:{}", summary.identifier));
        tiles::meter(id, (done, total), (words, spoken), Tone::Green, tokens)
    })
}

/// The second line: each part, separated by a middle dot.
fn meta(identifier: &str, parts: &[Part], icons: AgentIcons, tokens: &Tokens) -> AnyElement {
    h_flex()
        .min_w_0()
        .children(parts.iter().enumerate().map(|(index, part)| {
            let separator = (index > 0).then_some(SEPARATOR);
            h_flex()
                .flex_shrink_0()
                .children(separator)
                .child(part_element(identifier, part, icons, tokens))
        }))
        .into_any_element()
}

fn part_element(identifier: &str, part: &Part, icons: AgentIcons, tokens: &Tokens) -> AnyElement {
    match part {
        Part::Ready => div()
            .text_color(tokens.accent)
            .child("Ready for agents")
            .into_any_element(),
        Part::Text(text) => div().child(text.clone()).into_any_element(),
        Part::Assignee { name, rank } => h_flex()
            .gap_1()
            .children(rank.and_then(|rank| agent_icon(identifier, name, rank, icons)))
            .child(name.clone())
            .into_any_element(),
    }
}

/// An agent assignee's icon as the Icons setting asks: none while off.
fn agent_icon(
    identifier: &str,
    name: &str,
    rank: AgentRank,
    icons: AgentIcons,
) -> Option<AnyElement> {
    let face = avatar::agent(
        SharedString::from(format!("issue-tile-agent:{identifier}")),
        rank,
        name.to_owned(),
    )
    .small();
    avatar::agent_icon(face, icons).map(|icon| icon.into_any_element())
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;
    use subspace_framework::contracts::issues::{
        IssuePerson, IssueProgress, IssueState, IssueSummary, StateType,
    };
    use subspace_framework::dashboard::avatar::AgentRank;

    use super::{Part, parts, progress};

    fn issue() -> IssueSummary {
        IssueSummary::new(
            "NAV-1",
            "Render star chart tiles off the UI thread",
            "https://example.test/NAV-1",
            IssueState::new("In progress", StateType::Started),
        )
    }

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, 16).expect("a date")
    }

    fn text(value: &str) -> Part {
        Part::Text(value.to_owned())
    }

    #[test]
    fn a_bare_issue_says_only_its_state() {
        assert_eq!(parts(&issue(), today()), [text("In progress")]);
        assert_eq!(progress(&issue()), None, "no progress, no meter");
    }

    #[test]
    fn a_full_issue_says_ready_tracker_state_assignee_and_due_in_order() {
        let issue = issue()
            .with_markers(true, false)
            .with_ready_for_agents(Some(true))
            .with_assignee(Some("aurora-lyra"))
            .with_assignee_kind(Some("agent"))
            .with_assignee_rank(Some("captain"))
            .with_due_date(Some("2026-11-18"))
            .with_progress(Some(IssueProgress::new(1, 2)));
        assert_eq!(
            parts(&issue, today()),
            [
                Part::Ready,
                text("Tracker"),
                text("In progress"),
                Part::Assignee {
                    name: "aurora-lyra".into(),
                    rank: Some(AgentRank::Captain),
                },
                text("due Wed"),
            ]
        );
        assert_eq!(progress(&issue), Some((1, 2, "1 of 2".into())));
    }

    #[test]
    fn the_owner_stands_in_for_a_missing_assignee() {
        let owner = IssuePerson::new("owner", "Rosa Fixture", "human");
        let issue = issue().with_owner(Some(owner));
        assert_eq!(
            parts(&issue, today()),
            [text("In progress"), text("Rosa Fixture")]
        );
    }

    #[test]
    fn only_captains_and_the_rear_admiral_have_an_icon() {
        let assigned = |kind: &str, rank: Option<&str>| {
            let issue = issue()
                .with_assignee(Some("nav"))
                .with_assignee_kind(Some(kind))
                .with_assignee_rank(rank);
            match &parts(&issue, today())[1] {
                Part::Assignee { rank, .. } => *rank,
                other => panic!("an assignee, not {other:?}"),
            }
        };
        assert_eq!(
            assigned("agent", Some("rearAdmiral")),
            Some(AgentRank::RearAdmiral)
        );
        assert_eq!(assigned("agent", Some("crew")), None);
        assert_eq!(
            assigned("human", Some("captain")),
            None,
            "a person has no agent icon"
        );
    }

    #[test]
    fn not_ready_an_empty_tracker_and_a_bad_date_leave_no_gap() {
        let issue = issue()
            .with_ready_for_agents(Some(false))
            .with_due_date(Some("next week"))
            .with_progress(Some(IssueProgress::new(0, 0)));
        assert_eq!(parts(&issue, today()), [text("In progress")]);
        assert_eq!(progress(&issue), None);
    }
}
