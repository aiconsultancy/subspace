//! The words and marks for a decision's facts. Pure, so the tests read them
//! without a window. Several repeat the Issues Space's
//! `decision/words.rs` and `listing.rs`, which this Space may not depend on.

use std::collections::HashMap;
use std::time::Instant;

use chrono::{Datelike as _, NaiveDate};
use subspace_api_client::model::{
    AgentRank, DecisionEvidenceKind, DecisionListItem, DecisionMakerType, DecisionMember,
    DecisionStatus, Project,
};
use subspace_framework::dashboard::Tone;
use subspace_framework::lucide::{IconName, SvgIcon};

use crate::model::Choice;

/// A person's display name by user ID, from the account's people.
pub(crate) type People = HashMap<String, String>;

const SECONDS_PER_MINUTE: u64 = 60;
const MINUTES_PER_HOUR: u64 = 60;

/// The word for a status, also its group's name.
pub(crate) fn status(value: DecisionStatus) -> &'static str {
    match value {
        DecisionStatus::Proposed => "Proposed",
        DecisionStatus::Accepted => "Accepted",
        DecisionStatus::Rejected => "Rejected",
        DecisionStatus::Superseded => "Superseded",
        _ => "Unknown status",
    }
}

/// The glyph a status wears in the navigator, its group header and the
/// inspector.
pub(crate) fn status_icon(value: DecisionStatus) -> SvgIcon {
    match value {
        DecisionStatus::Proposed => IconName::CircleDot,
        DecisionStatus::Accepted => IconName::CircleCheck,
        DecisionStatus::Rejected => IconName::CircleX,
        DecisionStatus::Superseded => IconName::ArrowLeftRight,
        _ => IconName::CircleDashed,
    }
}

/// The glyph's tone. The word always goes with it, so the status does not
/// rest on colour.
pub(crate) fn status_tone(value: DecisionStatus) -> Tone {
    STATUS_TONES
        .iter()
        .find(|(status, _)| *status == value)
        .map_or(Tone::Muted, |(_, tone)| *tone)
}

/// The statuses whose glyph has a tone of its own; the others are muted.
const STATUS_TONES: [(DecisionStatus, Tone); 3] = [
    (DecisionStatus::Proposed, Tone::Yellow),
    (DecisionStatus::Accepted, Tone::Green),
    (DecisionStatus::Rejected, Tone::Red),
];

/// An agent's rank in words, such as "rear admiral".
pub(crate) fn rank(rank: Option<AgentRank>) -> Option<&'static str> {
    match rank? {
        AgentRank::RearAdmiral => Some("rear admiral"),
        AgentRank::Captain => Some("captain"),
        AgentRank::Crew => Some("crew"),
        _ => None,
    }
}

/// Where the evidence for a decision came from.
pub(crate) fn evidence(kind: DecisionEvidenceKind) -> &'static str {
    match kind {
        DecisionEvidenceKind::IrcMessage => "IRC message",
        DecisionEvidenceKind::TeamsMessage => "Teams message",
        DecisionEvidenceKind::SubspaceComment => "Subspace comment",
        DecisionEvidenceKind::DocumentOrUrl => "Document or URL",
        DecisionEvidenceKind::Meeting => "Meeting",
        _ => "Other evidence",
    }
}

/// A person's name from the account's people, or "A person" when they are
/// not among them.
pub(crate) fn person(id: Option<&str>, people: &People) -> String {
    id.and_then(|id| people.get(id))
        .cloned()
        .unwrap_or_else(|| "A person".to_owned())
}

/// Who recorded a decision, from the list read: a name from the account's
/// people when the API's answer names no one.
pub(crate) fn recorder_id(id: &str, people: &People) -> String {
    people
        .get(id)
        .cloned()
        .unwrap_or_else(|| "Not known".to_owned())
}

/// Who recorded a decision, from its detail: the name, and whether that is
/// an agent or no longer active.
pub(crate) fn recorder(member: &DecisionMember) -> String {
    let agent = if member.kind == "agent" {
        " (agent)"
    } else {
        ""
    };
    let inactive = if member.available { "" } else { " (inactive)" };
    format!("{}{agent}{inactive}", member.display_name)
}

/// Who made the decision, as a row shows it: a person's name, or an agent's
/// name with its rank.
pub(crate) fn maker_short(item: &DecisionListItem, people: &People) -> String {
    match item.made_by_type {
        DecisionMakerType::Human => person(item.made_by_user_id.as_deref(), people),
        DecisionMakerType::Agent => agent(item, false),
        _ => "Not recorded".to_owned(),
    }
}

/// Who made the decision, as the inspector shows it: an agent's model too,
/// and that it is an agent.
pub(crate) fn maker_full(item: &DecisionListItem, people: &People) -> String {
    match item.made_by_type {
        DecisionMakerType::Human => person(item.made_by_user_id.as_deref(), people),
        DecisionMakerType::Agent => format!("{} (agent)", agent(item, true)),
        _ => "Not recorded".to_owned(),
    }
}

/// An agent's name and rank, then its model when `model` asks for it.
fn agent(item: &DecisionListItem, model: bool) -> String {
    let name = item.made_by_agent_name.as_deref().unwrap_or("An agent");
    let model = item.made_by_agent_model.as_deref().filter(|_| model);
    [Some(name), rank(item.made_by_agent_rank), model]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" · ")
}

/// Whether a person or an agent made the decision: the maker glyph's
/// spoken name.
pub(crate) fn maker_kind(item: &DecisionListItem) -> &'static str {
    match item.made_by_type {
        DecisionMakerType::Human => "Person",
        DecisionMakerType::Agent => "Agent",
        _ => "Maker not recorded",
    }
}

pub(crate) fn maker_icon(item: &DecisionListItem) -> SvgIcon {
    match item.made_by_type {
        DecisionMakerType::Human => IconName::User,
        DecisionMakerType::Agent => IconName::Bot,
        _ => IconName::CircleDashed,
    }
}

/// The decided-on date in a row: the day and month this year, with the year
/// otherwise.
pub(crate) fn decided_short(item: &DecisionListItem, today: NaiveDate) -> String {
    match item.decided_on() {
        Some(date) if date.year() == today.year() => date.format("%-d %b").to_string(),
        Some(date) => date.format("%-d %b %Y").to_string(),
        None => "No date".to_owned(),
    }
}

/// The decided-on date in full, as the inspector and the page show it.
pub(crate) fn decided_long(item: &DecisionListItem) -> String {
    item.decided_on().map_or_else(
        || "Not recorded".to_owned(),
        |date| date.format("%-d %B %Y").to_string(),
    )
}

/// The supersession marks a row shows, such as "supersedes".
pub(crate) fn supersession(item: &DecisionListItem) -> Option<String> {
    let marks: Vec<&str> = [
        (item.supersedes_id.is_some(), "supersedes"),
        (item.superseded_by_id.is_some(), "superseded"),
    ]
    .into_iter()
    .filter_map(|(shown, mark)| shown.then_some(mark))
    .collect();
    (!marks.is_empty()).then(|| marks.join(" · "))
}

/// "1 decision", "8 decisions", or "100+ decisions" when there are more than
/// the list read.
pub(crate) fn decisions(count: usize, more: bool) -> String {
    let plus = if more { "+" } else { "" };
    if count == 1 && !more {
        "1 decision".to_owned()
    } else {
        format!("{count}{plus} decisions")
    }
}

/// "1 linked issue", "3 linked issues".
pub(crate) fn linked_issues(count: i32) -> String {
    if count == 1 {
        "1 linked issue".to_owned()
    } else {
        format!("{count} linked issues")
    }
}

/// A project as "NAV · Navigation", or its key alone when its name is not
/// known.
pub(crate) fn project(key: &str, name: Option<&str>) -> String {
    name.map_or_else(|| key.to_owned(), |name| format!("{key} · {name}"))
}

/// A decision's project, with the name its read carries or the account's
/// projects give, and whether the project is archived.
pub(crate) fn project_of(item: &DecisionListItem, projects: &[Project]) -> String {
    let scope = item.project.as_ref();
    let name = scope.map(|scope| scope.name.as_str()).or_else(|| {
        projects
            .iter()
            .find(|project| project.key == item.project_key)
            .map(|project| project.name.as_str())
    });
    let label = project(&item.project_key, name);
    if scope.is_some_and(|scope| scope.archived) {
        format!("{label} (archived)")
    } else {
        label
    }
}

/// A row's spoken name: the title, then the facts its lanes show.
pub(crate) fn row_label(item: &DecisionListItem, people: &People) -> String {
    let mut parts = vec![
        item.title.clone(),
        status(item.status).to_owned(),
        item.project_key.clone(),
        format!("made by {}", maker_short(item, people)),
    ];
    parts.extend(
        item.decided_on()
            .map(|date| date.format("decided %-d %B %Y").to_string()),
    );
    parts.extend(supersession(item));
    parts.push(linked_issues(item.issues.count));
    parts.join(", ")
}

/// The empty list's words, for the search or the choices that left nothing.
pub(crate) fn empty(choice: &Choice, query: &str) -> (&'static str, &'static str) {
    if !query.is_empty() {
        return (
            "No decisions match your search",
            "Try other words, or clear the search.",
        );
    }
    if choice.narrows() {
        return (
            "No decisions match these choices",
            "Try another view, maker, project or tag.",
        );
    }
    (
        "No decisions yet",
        "Decisions logged against issues appear here.",
    )
}

/// How long ago the list was read, as the status bar says it.
pub(crate) fn updated(read_at: Instant, now: Instant) -> String {
    let minutes = now.saturating_duration_since(read_at).as_secs() / SECONDS_PER_MINUTE;
    match minutes {
        0 => "updated just now".to_owned(),
        1..MINUTES_PER_HOUR => format!("updated {minutes} min ago"),
        _ => format!("updated {} h ago", minutes / MINUTES_PER_HOUR),
    }
}

#[cfg(test)]
#[path = "tests/words.rs"]
mod tests;
