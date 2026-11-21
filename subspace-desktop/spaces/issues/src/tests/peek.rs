use serde_json::json;

use super::peek;
use crate::fixtures::{issue, person, project};
use crate::model::StateKind;

#[test]
fn the_peek_shows_what_the_row_already_holds() {
    let extra = json!({
        "priority": 1,
        "summary": "  Link people when they join the sample org.  ",
        "assignee": person("Jo Fixture"),
        "labels": [
            {"id": "label-1", "name": "Needs owner", "color": "#f2994a", "state": "active"},
            {"id": "label-2", "name": "Feature", "color": "#4cb782", "state": "active"},
        ],
        "dueDate": "2026-12-17",
        "lastActivity": {
            "kind": "comment.created", "occurredAt": "2026-11-15T15:30:00Z",
            "actor": {"type": "agent", "label": "Fixture worker"},
        },
    });
    let item = issue("SUB-1141", "Design Contacts", ("Todo", "unstarted"), extra);
    let peek = peek(&item, &[project("SUB", "Subspace", false)], "2 h ago");
    assert_eq!(peek.identifier, "SUB-1141");
    assert_eq!(peek.state, "Todo");
    assert_eq!(peek.state_kind, StateKind::Unstarted);
    assert_eq!(peek.title, "Design Contacts");
    assert_eq!(
        peek.summary.as_deref(),
        Some("Link people when they join the sample org.")
    );
    assert_eq!(
        peek.facts,
        [
            ("Priority", "Urgent".to_owned()),
            ("Assignee", "Jo Fixture".to_owned()),
            ("Project", "SUB · Subspace".to_owned()),
            ("Labels", "Needs owner, Feature".to_owned()),
            ("Due date", "17 Dec 2026".to_owned()),
        ]
    );
    assert_eq!(
        peek.activity.as_deref(),
        Some("Comment added · Fixture worker (agent) · 2 h ago")
    );
}

#[test]
fn absent_values_read_plainly_or_are_left_out() {
    let item = issue(
        "SUB-9",
        "A quiet issue",
        ("Backlog", "backlog"),
        json!({"summary": "  ", "projectKey": "ZZZ"}),
    );
    let peek = peek(&item, &[project("SUB", "Subspace", false)], "");
    assert_eq!(peek.state_kind, StateKind::Backlog);
    assert_eq!(peek.summary, None, "a blank summary is no summary");
    assert_eq!(
        peek.facts,
        [
            ("Priority", "No priority".to_owned()),
            ("Assignee", "Unassigned".to_owned()),
            ("Project", "ZZZ".to_owned()),
        ],
        "no labels and no due date: no rows for them"
    );
    assert_eq!(peek.activity, None, "the row carries no latest activity");
}
