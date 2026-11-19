use super::{FlightPath, Layout};
use serde_json::json;
use subspace_api_client::model::WorkflowState;

pub(super) fn states() -> Vec<WorkflowState> {
    with([
        ("done", "Done", "completed", 40),
        ("cancel", "Cancelled", "canceled", 50),
        ("todo", "Todo", "unstarted", 20),
        ("duplicate", "Duplicate", "duplicate", 60),
        ("started", "In Progress", "started", 30),
        ("backlog", "Backlog", "backlog", 10),
    ])
}

fn with<const N: usize>(rows: [(&str, &str, &str, i32); N]) -> Vec<WorkflowState> {
    rows.into_iter()
        .map(|(id, name, kind, position)| {
            serde_json::from_value(json!({
                "id": id, "name": name, "type": kind, "position": position,
                "projectId": "project", "isDefault": false, "issueCount": 0
            }))
            .unwrap()
        })
        .collect()
}

fn ids<'a>(states: impl IntoIterator<Item = &'a &'a WorkflowState>) -> Vec<&'a str> {
    states.into_iter().map(|state| state.id.as_str()).collect()
}

#[test]
fn flight_path_keeps_workflow_order_and_ends_in_one_slot() {
    let states = states();
    let path = FlightPath::new(&states, "started");
    assert_eq!(ids(&path.course), ["backlog", "todo", "started"]);
    assert_eq!(ids(&path.ends), ["done", "cancel", "duplicate"]);
    assert_eq!(path.end.map(|s| s.id.as_str()), Some("done"));
    assert_eq!(path.current, Some(2));
    assert_eq!(
        path.description(),
        "Workflow: In Progress, 3 of 4. Backlog → Todo → In Progress → Done. End states: Done, Cancelled, Duplicate"
    );
}

#[test]
fn geometry_centres_waypoints_and_narrow_fallback_keeps_current_state() {
    let states = states();
    let path = FlightPath::new(&states, "started");
    let layout = path.layout(400., 1.);
    assert_eq!(
        layout,
        Layout::Course {
            centres: vec![50., 150., 250., 350.],
            travelled_to: Some(250.)
        }
    );
    assert_eq!(path.layout(180., 1.), Layout::Current);
    assert_eq!(
        path.layout(400., 2.),
        Layout::Current,
        "zoom uses the same fallback"
    );
    assert!(path.description().contains("In Progress, 3 of 4"));
}

#[test]
fn cancelled_and_duplicate_issues_take_the_end_slot_as_their_position() {
    let states = states();
    for (current, name) in [("cancel", "Cancelled"), ("duplicate", "Duplicate")] {
        let path = FlightPath::new(&states, current);
        assert_eq!(path.end.map(|s| s.name.as_str()), Some(name));
        assert_eq!(path.count(), 4, "one slot, not extra stops");
        assert_eq!(path.current, Some(3));
        assert!(
            path.description().starts_with(&format!(
                "Workflow: {name}, 4 of 4. Backlog → Todo → In Progress → {name}."
            )),
            "{}",
            path.description()
        );
        assert_eq!(
            path.layout(400., 1.),
            Layout::Course {
                centres: vec![50., 150., 250., 350.],
                travelled_to: Some(350.)
            }
        );
    }
}

#[test]
fn missing_current_never_fabricates_travelled_progress() {
    let states = states();
    let unknown = FlightPath::new(&states, "unknown");
    assert_eq!(unknown.current, None);
    assert_eq!(unknown.end.map(|s| s.id.as_str()), Some("done"));
    assert!(
        unknown
            .description()
            .starts_with("Workflow: Current state unavailable.")
    );
    assert!(matches!(
        unknown.layout(400., 1.),
        Layout::Course {
            travelled_to: None,
            ..
        }
    ));
    assert_eq!(
        FlightPath::new(&[], "unknown").layout(400., 1.),
        Layout::Current
    );
}

#[test]
fn cancelled_spelling_still_ends_the_path() {
    let mut states = states();
    states[1].kind = "cancelled".into();
    let path = FlightPath::new(&states, "cancel");
    assert_eq!(path.count(), 4);
    assert_eq!(path.end.map(|s| s.id.as_str()), Some("cancel"));
    assert!(path.description().contains("Cancelled, 4 of 4"));
}

#[test]
fn custom_workflows_group_by_state_type_not_by_name_or_position() {
    let states = with([
        ("shipped", "Shipped", "completed", 9),
        ("review", "In review", "started", 4),
        ("dropped", "Dropped", "canceled", 1),
        ("merged", "Merged", "completed", 7),
        ("parked", "Parked", "custom", 6),
        ("inbox", "Inbox", "triage", 0),
    ]);
    let path = FlightPath::new(&states, "review");
    assert_eq!(ids(&path.course), ["inbox", "review", "parked"]);
    assert_eq!(
        ids(&path.ends),
        ["merged", "shipped", "dropped"],
        "completed types come first, each type in workflow order"
    );
    assert_eq!(path.end.map(|s| s.id.as_str()), Some("merged"));
    assert_eq!(
        FlightPath::new(&states, "shipped")
            .end
            .map(|s| s.id.as_str()),
        Some("shipped")
    );
    let without_completed = with([
        ("todo", "Todo", "unstarted", 0),
        ("dropped", "Dropped", "canceled", 1),
    ]);
    let path = FlightPath::new(&without_completed, "todo");
    assert_eq!(path.end.map(|s| s.id.as_str()), Some("dropped"));
    assert_eq!(
        path.description(),
        "Workflow: Todo, 1 of 2. Todo → Dropped",
        "a single end state needs no list"
    );
    let only_path = with([("todo", "Todo", "unstarted", 0)]);
    let path = FlightPath::new(&only_path, "todo");
    assert_eq!((path.end.is_none(), path.count()), (true, 1));
}

#[test]
fn unmeasured_width_starts_with_the_full_course() {
    let states = states();
    assert!(matches!(
        FlightPath::new(&states, "started").layout(0., 1.),
        Layout::Course { .. }
    ));
    assert_eq!(
        FlightPath::new(&[], "unknown").layout(0., 1.),
        Layout::Current
    );
}

#[test]
fn end_states_rank_completed_then_canceled_then_duplicate_whatever_their_positions() {
    let states = with([
        ("dup", "Duplicate", "duplicate", 0),
        ("dropped", "Dropped", "canceled", 1),
        ("cancel", "Cancelled", "cancelled", 2),
        ("shipped", "Shipped", "completed", 9),
        ("todo", "Todo", "unstarted", 5),
    ]);
    let path = FlightPath::new(&states, "todo");
    assert_eq!(ids(&path.ends), ["shipped", "dropped", "cancel", "dup"]);
    assert_eq!(path.end.map(|s| s.id.as_str()), Some("shipped"));
}
