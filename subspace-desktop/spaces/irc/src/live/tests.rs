//! Mapping the Fleet API to what Channels shows: a ship matched to the
//! roster captain, its figures, its crew and what each is on, untrusted
//! text bounded, and nothing while the fleet is not live.

use serde_json::json;
use subspace_client_fleet::model::{Activity, FleetOverview, ShipDetail};
use subspace_client_fleet::{FleetSnapshot, FleetState, Link};

use super::ship::{self, MAX_CREW, MAX_NAME_CHARS, bounded};
use super::{FleetLive, ShipLive};

/// Minutes before the fixture's "now", in epoch milliseconds.
pub(crate) const NOW: i64 = 1_800_000_000_000;
const MINUTE: i64 = 60_000;

/// The aurora fleet's overview: Lyra with 3 of 4 crew busy, Orion with 1
/// of 3, and a ship no roster names.
fn overview() -> FleetOverview {
    serde_json::from_value(json!({
        "apiVersion": 1, "name": "Aurora",
        "ships": [
            {"id": "aurora-lyra", "name": "Lyra", "available": true,
             "captain": {"id": "aurora-lyra", "name": "aurora-lyra", "role": "captain",
                         "model": "claude-opus-5-5", "activity": "running",
                         "queue": {"queued": 2, "running": 3, "done": 128}},
             "crew": {"people": 4, "busy": 3, "queued": 2}},
            {"id": "aurora-orion", "name": "Orion", "available": true,
             "captain": {"id": "aurora-orion", "activity": "idle", "queue": {}},
             "crew": {"people": 3, "busy": 1, "queued": 0}},
            {"id": "elsewhere-ghost", "name": "Ghost", "available": true,
             "captain": {"id": "elsewhere-ghost", "activity": "idle"}, "crew": null}
        ]
    }))
    .unwrap()
}

fn running(name: &str, summary: &str, phase: &str, minutes: i64) -> serde_json::Value {
    json!({"id": name.to_lowercase(), "name": name, "activity": "running", "contexts": [
        {"project": "nav", "activity": "running", "current": {
            "id": format!("t-{name}"), "status": "running", "summary": summary, "phase": phase,
            "startedAt": NOW - minutes * MINUTE}}
    ]})
}

/// Lyra's crew: Talon idle (listed first by the API), then Wren, Quill and
/// Juniper at work.
fn lyra_detail() -> ShipDetail {
    serde_json::from_value(json!({
        "id": "aurora-lyra", "name": "Lyra", "available": true,
        "crewMembers": [
            {"id": "talon", "name": "Talon", "activity": "idle", "idleSince": NOW - 180 * MINUTE,
             "contexts": [{"project": "nav", "activity": "idle", "current": null}]},
            running("Wren", "Cancel tile decodes that leave the viewport (NAV-7)", "Reviewing diff", 35),
            running("Quill", "Tab from a chart marker to the burn summary (NAV-11)", "Running tests", 19),
            running("Juniper", "Course correction preview (NAV-2)", "", 1)
        ]
    }))
    .unwrap()
}

/// The aurora fleet's snapshot with Lyra's detail.
pub(crate) fn snapshot() -> FleetSnapshot {
    let mut snapshot = FleetSnapshot::default();
    snapshot.link = Link::Live;
    snapshot.overview = Some(overview());
    snapshot.ships.insert("aurora-lyra".into(), lyra_detail());
    snapshot
}

/// The aurora fleet as the hub holds it in `state`, with the snapshot.
pub(crate) fn aurora(state: FleetState) -> FleetLive {
    FleetLive::new(state, Some(&snapshot()))
}

/// The aurora fleet, live.
pub(crate) fn live() -> FleetLive {
    aurora(FleetState::Followed(Link::Live))
}

/// Lyra, as the live fleet reports it.
pub(crate) fn lyra() -> ShipLive {
    live().ship("aurora-lyra").unwrap()
}

#[test]
fn a_ship_is_its_captain_s_registration_and_carries_the_api_s_figures() {
    let ship = live().ship("Aurora-Lyra").expect("matched ignoring case");
    assert_eq!(
        (ship.name.as_str(), ship.model.as_deref(), ship.activity),
        ("Lyra", Some("claude-opus-5-5"), Activity::Running)
    );
    assert_eq!(ship.crew, Some((3, 4)), "counted from the crew's detail");
    assert_eq!((ship.queued, ship.done), (Some(2), Some(128)));
    let names: Vec<&str> = ship.members.iter().map(|m| m.name.as_str()).collect();
    assert_eq!(
        names,
        ["Wren", "Quill", "Juniper", "Talon"],
        "working first"
    );
    let wren = &ship.members[0];
    assert!(wren.working);
    assert_eq!(wren.phase.as_deref(), Some("Reviewing diff"));
    assert_eq!(wren.since, Some(NOW - 35 * MINUTE));
    assert_eq!(ship.members[2].phase, None, "an empty phase is none");
    let talon = &ship.members[3];
    assert_eq!((talon.working, talon.task.as_deref()), (false, None));
    assert_eq!(talon.since, Some(NOW - 180 * MINUTE), "idle since");
    assert!(live().ship("aurora-vega").is_none(), "not in the API");
    assert!(live().ship("lyra").is_none(), "a name is no registration");
}

#[test]
fn without_the_crew_s_detail_the_overview_s_totals_count() {
    let orion = live().ship("aurora-orion").unwrap();
    assert_eq!(
        (orion.crew, orion.queued, orion.done),
        (Some((1, 3)), Some(0), None)
    );
    assert!(orion.members.is_empty());
    let ghost = ship::of(&snapshot(), "elsewhere-ghost").unwrap();
    assert_eq!(
        (ghost.crew, ghost.queued),
        (None, None),
        "no crew, no figures"
    );
}

#[test]
fn nothing_shows_while_the_fleet_is_not_live() {
    for state in [
        FleetState::NotConfigured,
        FleetState::InvalidAddress,
        FleetState::Checking,
        FleetState::NoToken,
        FleetState::AddressChanged { saved_for: None },
        FleetState::StoreUnavailable,
        FleetState::Ready,
        FleetState::Followed(Link::Connecting),
        FleetState::Followed(Link::Reconnecting),
        FleetState::Followed(Link::TokenRefused),
        FleetState::Followed(Link::Forbidden),
        FleetState::Followed(Link::Unreachable),
    ] {
        let fleet = aurora(state.clone());
        assert!(fleet.ship("aurora-lyra").is_none(), "{state:?}");
        assert!(fleet.ships(|_| true).is_none(), "{state:?}");
    }
    assert!(
        aurora(FleetState::Followed(Link::Polling))
            .ship("aurora-lyra")
            .is_some()
    );
    let empty = FleetLive::new(FleetState::Followed(Link::Live), None);
    assert!(
        empty.ship("aurora-lyra").is_none(),
        "before the first event"
    );
}

#[test]
fn the_ships_list_keeps_the_roster_s_captains_in_the_api_s_order() {
    let ships = live().ships(|id| id.starts_with("aurora-")).unwrap();
    let names: Vec<&str> = ships.iter().map(|ship| ship.name.as_str()).collect();
    assert_eq!(names, ["Lyra", "Orion"]);
}

#[test]
fn untrusted_names_and_tasks_are_bounded_one_line_text() {
    assert_eq!(
        bounded("  Wren\u{7}\n reviews\t NAV-7 ", 40),
        "Wren reviews NAV-7"
    );
    let long = "x".repeat(100);
    let cut = bounded(&long, MAX_NAME_CHARS);
    assert_eq!(cut.chars().count(), MAX_NAME_CHARS);
    assert!(cut.ends_with('…'));
    let mut detail = lyra_detail();
    detail.crew_members[1].name = format!("<b>{}</b>\u{1b}[31m", "Wren".repeat(20));
    let crowd: Vec<_> = (0..MAX_CREW + 5)
        .map(|_| detail.crew_members[2].clone())
        .collect();
    let mut snapshot = snapshot();
    detail.crew_members.extend(crowd);
    snapshot.ships.insert("aurora-lyra".into(), detail);
    let ship = ship::of(&snapshot, "aurora-lyra").unwrap();
    assert_eq!(ship.members.len(), MAX_CREW, "the list is bounded");
    assert_eq!(
        ship.crew.map(|(_, people)| people),
        Some(MAX_CREW + 9),
        "counts are not"
    );
    let wren = &ship.members[0];
    assert!(wren.name.starts_with("<b>WrenWren"), "markup stays text");
    assert!(wren.name.chars().count() <= MAX_NAME_CHARS);
    assert!(!wren.name.chars().any(char::is_control), "no escapes");
}

#[test]
fn a_ship_whose_crew_cannot_be_read_says_so_and_counts_nothing() {
    let mut overview = overview();
    overview.ships[1].available = false;
    overview.ships[1].error = Some("Crew status unavailable.".into());
    let mut snapshot = snapshot();
    snapshot.overview = Some(overview);
    let orion = ship::of(&snapshot, "aurora-orion").unwrap();
    assert_eq!((orion.crew, orion.queued), (None, None));
    assert_eq!(
        orion.unavailable.as_deref(),
        Some("Crew status unavailable.")
    );
    assert_eq!(ship::state_word(Activity::Running), Some("Working"));
    assert_eq!(ship::state_word(Activity::Unknown), None);
}

#[test]
fn news_channels_does_not_draw_leaves_the_fleet_equal() {
    // A busy fleet's activity lines and sample times must not re-sync the
    // navigator, the search index and every open transcript.
    let before = snapshot();
    let mut after = snapshot();
    after.activity.push_back(
        serde_json::from_value(json!({"ship": "aurora-lyra", "member": "wren",
            "kind": "phase", "text": "Wren started reviewing", "at": NOW}))
        .unwrap(),
    );
    after
        .ships
        .get_mut("aurora-lyra")
        .unwrap()
        .detail_updated_at = Some(NOW);
    let lyra = &mut after.overview.as_mut().unwrap().ships[0];
    lyra.repository = Some("acme/nav".into());
    let state = FleetState::Followed(Link::Live);
    assert_eq!(
        FleetLive::new(state.clone(), Some(&before)),
        FleetLive::new(state.clone(), Some(&after))
    );
    // What it draws still counts: a crew member going idle.
    let mut idle = snapshot();
    idle.overview.as_mut().unwrap().ships[0]
        .crew
        .as_mut()
        .unwrap()
        .busy = 2;
    idle.ships.remove("aurora-lyra");
    assert_ne!(
        FleetLive::new(state.clone(), Some(&before)),
        FleetLive::new(state, Some(&idle))
    );
}

#[test]
fn a_kept_or_unavailable_crew_detail_never_shows_as_live() {
    // Stargate keeps the last good detail after a failed read: no crew
    // member shows on the task they were last seen on.
    let mut kept = snapshot();
    kept.ships.get_mut("aurora-lyra").unwrap().detail_stale = true;
    let lyra = ship::of(&kept, "aurora-lyra").unwrap();
    assert!(lyra.members.is_empty(), "no crew lines from a kept detail");
    assert_eq!(lyra.crew, Some((3, 4)), "the overview's totals stand in");
    // The overview says the ship is unavailable while its detail stays.
    let mut down = snapshot();
    let overview = down.overview.as_mut().unwrap();
    overview.ships[0].available = false;
    let lyra = ship::of(&down, "aurora-lyra").unwrap();
    assert!(lyra.members.is_empty());
    assert_eq!(lyra.crew, None);
    assert!(lyra.unavailable.is_some());
    // The detail itself says the ship is unavailable.
    let mut detail_down = snapshot();
    detail_down
        .ships
        .get_mut("aurora-lyra")
        .unwrap()
        .ship
        .available = false;
    assert!(
        ship::of(&detail_down, "aurora-lyra")
            .unwrap()
            .members
            .is_empty()
    );
    // A kept overview is not live at all.
    let mut kept_overview = snapshot();
    kept_overview.overview.as_mut().unwrap().overview_stale = true;
    let fleet = FleetLive::new(FleetState::Followed(Link::Live), Some(&kept_overview));
    assert!(fleet.ship("aurora-lyra").is_none());
    assert!(fleet.ships(|_| true).is_none());
}

#[test]
fn bidirectional_controls_are_stripped_from_untrusted_text() {
    let name = "Ly\u{202E}ar\u{202C} \u{2067}x\u{2069}\u{200F}";
    assert_eq!(bounded(name, MAX_NAME_CHARS), "Lyar x");
    let mut detail = lyra_detail();
    detail.crew_members[1].name = "\u{202E}nerW".into();
    let mut snapshot = snapshot();
    snapshot.ships.insert("aurora-lyra".into(), detail);
    let ship = ship::of(&snapshot, "aurora-lyra").unwrap();
    assert_eq!(ship.members[0].name, "nerW");
}
