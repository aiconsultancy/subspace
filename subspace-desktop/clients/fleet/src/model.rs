//! The part of Stargate's Fleet API v1 (`src/api/v1/types.ts`) the desktop
//! shows. Every field has a default and unknown fields and values are
//! ignored, so a newer Stargate, whose changes are additive, still reads.
//! Times are epoch milliseconds.

use std::collections::BTreeMap;

use serde::Deserialize;

/// `GET /api/v1/fleet`, and the `fleet` event.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct FleetOverview {
    pub api_version: u32,
    pub generated_at: i64,
    /// True while Stargate retains an earlier overview after a failed read.
    pub overview_stale: bool,
    /// The configured fleet name, or the host name.
    pub name: String,
    pub host: Option<Host>,
    pub gateway: Option<Gateway>,
    /// The rear admiral, when a registration has that role.
    pub admiral: Option<Captain>,
    /// The captains' ships, without the rear admiral's.
    pub ships: Vec<Ship>,
    pub longest_idle: Option<LongestIdle>,
}

impl FleetOverview {
    /// The ship with registration `id`.
    pub fn ship(&self, id: &str) -> Option<&Ship> {
        self.ships.iter().find(|ship| ship.id == id)
    }
}

/// The crew member idle the longest.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct LongestIdle {
    pub ship: String,
    pub member: String,
    pub since: i64,
}

/// A captain or the rear admiral: one registration and its job queue.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct Captain {
    pub id: String,
    pub name: String,
    pub role: Role,
    /// `irc`, `slack` or `teams`.
    pub transport: String,
    pub backend: String,
    pub model: Option<String>,
    /// Jobs by status, such as `queued`, `running` and `done`.
    pub queue: BTreeMap<String, u64>,
    pub activity: Activity,
    /// When `activity` last changed while Stargate was running.
    pub since: Option<i64>,
}

impl Captain {
    /// How many of the captain's jobs have `status`.
    // qual:api
    pub fn queued(&self, status: &str) -> u64 {
        self.queue.get(status).copied().unwrap_or_default()
    }
}

/// A registration's role.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Captain,
    RearAdmiral,
    /// A role this version does not know.
    #[default]
    #[serde(other)]
    Other,
}

/// What a captain, crew member or project context is doing.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Activity {
    Running,
    Uncertain,
    Queued,
    Idle,
    Unprovisioned,
    /// A state this version does not know.
    #[default]
    #[serde(other)]
    Unknown,
}

/// One captain's ship in the overview.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct Ship {
    /// The captain's registration ID, also the route key.
    pub id: String,
    pub name: String,
    pub captain: Captain,
    /// `owner/name` of the captain's project on GitHub.
    pub repository: Option<String>,
    /// False when the ship's crew or backend could not be read.
    pub available: bool,
    /// A safe reason when `available` is false.
    pub error: Option<String>,
    /// `None` when the backend has no crew.
    pub crew: Option<CrewTotals>,
    pub features: Vec<Feature>,
}

/// A ship's crew in numbers.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct CrewTotals {
    pub people: u32,
    pub busy: u32,
    pub queued: u32,
    /// True when the counts are lower bounds.
    pub partial: bool,
}

/// `GET /api/v1/ships/{id}`, and the `ship` event: the ship with its crew.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct ShipDetail {
    #[serde(flatten)]
    pub ship: Ship,
    pub crew_members: Vec<CrewMember>,
    /// In a `ship` event, when the crew was observed.
    pub detail_updated_at: Option<i64>,
    /// In a `ship` event, true while the last good detail is retained.
    pub detail_stale: bool,
}

/// One crew member and its work in each project.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct CrewMember {
    pub id: String,
    pub name: String,
    /// The configured specialism.
    pub role: Option<String>,
    pub model: Option<String>,
    pub backend: String,
    /// The most active state across its contexts.
    pub activity: Activity,
    pub since: Option<i64>,
    pub idle_since: Option<i64>,
    pub contexts: Vec<CrewContext>,
    pub subagents: Vec<Subagent>,
}

impl CrewMember {
    /// The task the member is working on now, in its most active context.
    pub fn current_task(&self) -> Option<&Task> {
        self.contexts
            .iter()
            .filter(|context| context.current.is_some())
            .min_by_key(|context| context.activity)
            .and_then(|context| context.current.as_ref())
    }
}

/// A crew member's work in one project.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct CrewContext {
    /// The project key; `None` for the default project.
    pub project: Option<String>,
    pub activity: Activity,
    /// `unprovisioned`, `pending` or `ready`.
    pub provisioned: String,
    pub current: Option<Task>,
    pub queued_count: u32,
    pub queued: Vec<Task>,
    pub last_outcome: Option<Task>,
    pub recent: Vec<Task>,
    pub recent_count: u32,
    pub git: Option<Git>,
    pub pull_request: Option<PullRequest>,
}

/// One task of a crew member. Text is bounded and redacted by Stargate.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct Task {
    pub id: String,
    pub status: TaskStatus,
    pub summary: String,
    pub phase: String,
    pub progress: String,
    pub created_at: Option<i64>,
    pub started_at: Option<i64>,
    pub updated_at: Option<i64>,
    pub finished_at: Option<i64>,
}

/// A task's state.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Queued,
    Dispatching,
    Running,
    Cancelling,
    Completed,
    Cancelled,
    Uncertain,
    Abandoned,
    /// A state this version does not know.
    #[default]
    #[serde(other)]
    Unknown,
}

/// The git state of a crew checkout.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct Git {
    pub branch: Option<String>,
    pub head: Option<String>,
    pub base: Option<String>,
    pub ahead: Option<u32>,
    pub behind: Option<u32>,
    pub changed_files: Option<u32>,
    pub repository: Option<String>,
    pub checked_at: i64,
    /// A safe reason when the read failed.
    pub error: Option<String>,
}

/// The pull request for a checkout's branch.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct PullRequest {
    pub number: u64,
    pub url: String,
    pub title: String,
    /// `draft`, `open`, `merged` or `closed`.
    pub state: String,
    /// `passing`, `running`, `failing` or `none`.
    pub checks: String,
}

/// A child session of a crew member's work.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct Subagent {
    pub id: String,
    pub title: String,
    pub updated_at: Option<i64>,
}

/// A gateway or registration switch.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct Feature {
    pub key: String,
    pub label: String,
    pub enabled: bool,
    pub value: Option<String>,
}

/// The fleet VM, and the `host` event.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct Host {
    pub hostname: String,
    pub platform: String,
    pub arch: String,
    pub uptime_sec: u64,
    pub cpu: Cpu,
    pub memory: Memory,
    pub disks: Vec<Disk>,
    pub sampled_at: i64,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct Cpu {
    pub cores: u32,
    /// The busy fraction, 0 to 1; `None` before Stargate's second sample.
    pub usage: Option<f64>,
    /// The one, five and fifteen minute load averages.
    pub load: Vec<f64>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct Memory {
    pub total_bytes: u64,
    pub available_bytes: u64,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct Disk {
    /// `/` for the root filesystem, `data` for the data directory's.
    pub path: String,
    pub total_bytes: u64,
    pub free_bytes: u64,
}

/// Stargate itself.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct Gateway {
    pub version: String,
    pub opencode_version: Option<String>,
    pub started_at: i64,
    pub features: Vec<Feature>,
}

/// One line of the fleet's activity, from the `activity` event.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct ActivityEvent {
    pub ship: String,
    pub member: Option<String>,
    /// `dispatch`, `phase`, `outcome`, `queued`, `git` or `availability`.
    pub kind: String,
    /// One plain sentence, such as `Starbuck started “…”`.
    pub text: String,
    pub at: i64,
    pub branch: Option<String>,
    /// The final status of an `outcome`.
    pub status: Option<TaskStatus>,
}

#[cfg(test)]
mod tests {
    use super::{Activity, FleetOverview, Role, ShipDetail, TaskStatus};

    #[test]
    fn unknown_fields_and_values_are_ignored() {
        let fleet: FleetOverview = serde_json::from_value(serde_json::json!({
            "apiVersion": 1, "generatedAt": 5, "name": "Example", "future": {"x": 1},
            "admiral": {"id": "a", "name": "Admiral", "role": "rear_admiral",
                        "queue": {"queued": 2, "teleporting": 1}, "activity": "warping"},
            "ships": [{"id": "s", "name": "Lyra", "available": true,
                       "captain": {"id": "s", "role": "navigator", "activity": "running"},
                       "crew": {"people": 3, "busy": 1, "queued": 2, "partial": true,
                                "robots": 9}}]
        }))
        .unwrap();
        let admiral = fleet.admiral.as_ref().unwrap();
        assert_eq!(admiral.role, Role::RearAdmiral);
        assert_eq!(admiral.activity, Activity::Unknown);
        assert_eq!((admiral.queued("queued"), admiral.queued("done")), (2, 0));
        let ship = fleet.ship("s").unwrap();
        assert_eq!(ship.captain.role, Role::Other);
        assert_eq!(ship.captain.activity, Activity::Running);
        assert_eq!(
            ship.crew.map(|crew| (crew.people, crew.partial)),
            Some((3, true))
        );
        assert!(fleet.host.is_none() && fleet.longest_idle.is_none());
    }

    #[test]
    fn a_ship_detail_carries_its_ship_and_crew_work() {
        let detail: ShipDetail = serde_json::from_value(serde_json::json!({
            "id": "s", "name": "Lyra", "available": true, "detailUpdatedAt": 9,
            "crewMembers": [{"id": "m", "name": "Wren", "activity": "running", "contexts": [
                {"project": null, "activity": "idle", "current": null},
                {"project": "nav", "activity": "running",
                 "current": {"id": "t", "status": "running", "summary": "Chart", "kind": "new"}}
            ]}]
        }))
        .unwrap();
        assert_eq!(
            (detail.ship.name.as_str(), detail.detail_updated_at),
            ("Lyra", Some(9))
        );
        let task = detail.crew_members[0].current_task().unwrap();
        assert_eq!(
            (task.summary.as_str(), task.status),
            ("Chart", TaskStatus::Running)
        );
    }
}
