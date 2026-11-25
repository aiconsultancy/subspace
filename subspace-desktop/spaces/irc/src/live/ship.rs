//! A Fleet API ship and its crew as Channels shows them in the roster.
//! The one-line roster bounds names and tasks for layout and strips
//! control characters. The separate crew detail view draws the full text
//! from the crew detail route. This mapping is pure and tested on its own.

use std::cmp::Reverse;

use subspace_client_fleet::FleetSnapshot;
use subspace_client_fleet::model::{Activity, CrewMember, Ship, ShipDetail, Task};

/// The most characters of a ship's, captain's or crew member's name shown.
pub(crate) const MAX_NAME_CHARS: usize = 32;
/// The most characters of a task's summary shown.
pub(crate) const MAX_TASK_CHARS: usize = 120;
/// The most characters of a task's phase or a model's name shown.
pub(crate) const MAX_WORD_CHARS: usize = 40;
/// The most crew lines shown before expanding the Ship tab's roster.
pub(crate) const MAX_CREW: usize = 24;

/// One ship, as the Fleet API reports it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ShipLive {
    /// The captain's registration ID, also the Fleet API's route key.
    pub id: String,
    pub name: String,
    /// The captain's model, when the API names one.
    pub model: Option<String>,
    /// What the captain is doing.
    pub activity: Activity,
    /// `busy` of `people` crew, when the API counts the ship's crew.
    pub crew: Option<(usize, usize)>,
    /// How many of the crew have work queued, when the API counts them.
    pub queued: Option<usize>,
    /// How many of the captain's jobs are done, when the API says.
    pub done: Option<u64>,
    /// Each crew member, the working first, once the ship's detail arrived.
    pub members: Vec<CrewLine>,
    /// Why the API could not read the ship's crew, when it could not.
    pub unavailable: Option<String>,
    /// Whether the crew's detail is one Stargate kept after a failed read,
    /// so it is not current and its lines are not shown.
    pub crew_kept: bool,
}

/// One crew member and what they are on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CrewLine {
    /// The member's Fleet API route key, not their display name.
    pub id: String,
    pub name: String,
    /// Whether the member has a current task, as Stargate counts busy.
    pub working: bool,
    /// The current task's summary.
    pub task: Option<String>,
    /// The current task's phase, such as `Running tests`.
    pub phase: Option<String>,
    /// Since when, in epoch milliseconds: the task's start while working,
    /// else when the member went idle.
    pub since: Option<i64>,
}

impl ShipLive {
    /// The crew at work.
    pub(crate) fn working(&self) -> impl Iterator<Item = &CrewLine> {
        self.members.iter().filter(|member| member.working)
    }
}

/// The ship in `snapshot` whose captain has the roster nick `captain`,
/// matched to its registration ID ignoring ASCII case.
#[cfg(test)]
pub(crate) fn of(snapshot: &FleetSnapshot, captain: &str) -> Option<ShipLive> {
    let ship = snapshot
        .overview
        .as_ref()?
        .ships
        .iter()
        .find(|ship| ship.id.eq_ignore_ascii_case(captain))?;
    Some(live(ship, snapshot.ships.get(&ship.id)))
}

/// Every ship in `snapshot` whose registration ID `is_captain` accepts;
/// `None` while Stargate says its overview is an earlier one it kept
/// after a failed read, as that is not live.
pub(crate) fn all(
    snapshot: &FleetSnapshot,
    is_captain: impl Fn(&str) -> bool,
) -> Option<Vec<ShipLive>> {
    let overview = snapshot
        .overview
        .as_ref()
        .filter(|overview| !overview.overview_stale)?;
    Some(
        overview
            .ships
            .iter()
            .filter(|ship| is_captain(&ship.id))
            .map(|ship| live(ship, snapshot.ships.get(&ship.id)))
            .collect(),
    )
}

/// `ship` from the overview with its `detail`, when one arrived and is
/// current: not while either says the ship is unavailable, nor while
/// Stargate keeps an earlier detail after a failed read, so no crew member
/// shows on a task they were last seen on. The overview's totals stand
/// in.
fn live(ship: &Ship, detail: Option<&ShipDetail>) -> ShipLive {
    let current =
        |detail: &&ShipDetail| ship.available && detail.ship.available && !detail.detail_stale;
    let members = detail
        .filter(current)
        .map(|detail| detail.crew_members.as_slice());
    let readable = ship.available;
    ShipLive {
        id: ship.id.clone(),
        name: name_or(&ship.name, &ship.id),
        model: present(ship.captain.model.as_deref(), MAX_WORD_CHARS),
        activity: ship.captain.activity,
        crew: counts(ship, members).filter(|_| readable),
        queued: ship
            .crew
            .map(|crew| crew.queued as usize)
            .filter(|_| readable),
        done: ship.captain.queue.get("done").copied(),
        members: members.map(lines).unwrap_or_default(),
        unavailable: (!readable).then(|| {
            present(ship.error.as_deref(), MAX_TASK_CHARS)
                .unwrap_or_else(|| "Crew status unavailable.".to_owned())
        }),
        crew_kept: detail
            .is_some_and(|detail| readable && detail.ship.available && detail.detail_stale),
    }
}

/// `busy` of `people`: from the crew's detail when it arrived, else the
/// overview's totals; `None` when the ship has no crew.
fn counts(ship: &Ship, members: Option<&[CrewMember]>) -> Option<(usize, usize)> {
    match members {
        Some(members) => Some((
            members.iter().filter(|member| busy(member)).count(),
            members.len(),
        )),
        None => ship
            .crew
            .map(|crew| (crew.busy as usize, crew.people as usize)),
    }
}

/// Whether `member` has a current task, as Stargate counts a busy member.
fn busy(member: &CrewMember) -> bool {
    member.current_task().is_some()
}

/// The complete roster, working members first, with bounded row text.
fn lines(members: &[CrewMember]) -> Vec<CrewLine> {
    let mut chosen: Vec<&CrewMember> = members.iter().collect();
    chosen.sort_by_key(|member| Reverse(busy(member)));
    chosen.into_iter().map(line).collect()
}

fn line(member: &CrewMember) -> CrewLine {
    let task = member.current_task();
    CrewLine {
        id: member.id.clone(),
        name: name_or(&member.name, &member.id),
        working: task.is_some(),
        task: task.and_then(|task| present(Some(&task.summary), MAX_TASK_CHARS)),
        phase: task.and_then(|task| present(Some(&task.phase), MAX_WORD_CHARS)),
        since: task.map_or(member.idle_since.or(member.since), started),
    }
}

/// When `task` started, else when it was created.
fn started(task: &Task) -> Option<i64> {
    task.started_at.or(task.created_at)
}

/// `name`, else `id`, bounded.
fn name_or(name: &str, id: &str) -> String {
    present(Some(name), MAX_NAME_CHARS)
        .or_else(|| present(Some(id), MAX_NAME_CHARS))
        .unwrap_or_default()
}

/// `text` bounded to `max` characters, when anything is left of it.
fn present(text: Option<&str>, max: usize) -> Option<String> {
    text.map(|text| bounded(text, max))
        .filter(|text| !text.is_empty())
}

/// The explicit bidirectional formatting characters: marks, embeddings,
/// overrides and isolates, which could reorder the text drawn beside an
/// untrusted name.
const BIDI_CONTROLS: [char; 12] = [
    '\u{061C}', '\u{200E}', '\u{200F}', '\u{202A}', '\u{202B}', '\u{202C}', '\u{202D}', '\u{202E}',
    '\u{2066}', '\u{2067}', '\u{2068}', '\u{2069}',
];

pub(crate) fn is_bidi_control(ch: char) -> bool {
    BIDI_CONTROLS.contains(&ch)
}

/// `text` on one line, without control or bidirectional formatting
/// characters, trimmed and cut to at most `max` characters, the last an
/// ellipsis when cut.
pub(crate) fn bounded(text: &str, max: usize) -> String {
    let flat: String = text
        .chars()
        .filter(|ch| !is_bidi_control(*ch))
        .map(|ch| if ch.is_control() { ' ' } else { ch })
        .collect();
    let words: Vec<&str> = flat.split_whitespace().collect();
    let line = words.join(" ");
    if line.chars().count() <= max {
        return line;
    }
    let kept: String = line.chars().take(max.saturating_sub(1)).collect();
    format!("{}…", kept.trim_end())
}

/// Each state the captain can be in, in a word, as a pill says it.
const STATE_WORDS: [(Activity, &str); 5] = [
    (Activity::Running, "Working"),
    (Activity::Queued, "Queued"),
    (Activity::Idle, "Idle"),
    (Activity::Uncertain, "Uncertain"),
    (Activity::Unprovisioned, "Not started"),
];

/// The captain's state in a word; `None` for a state this build does not
/// know.
pub(crate) fn state_word(activity: Activity) -> Option<&'static str> {
    STATE_WORDS
        .iter()
        .find(|(state, _)| *state == activity)
        .map(|(_, word)| *word)
}
