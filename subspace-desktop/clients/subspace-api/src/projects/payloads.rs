//! Request bodies for versioned milestone changes.

use serde_json::{Value, json};
use ureq::http::StatusCode;

use super::{Milestone, MilestoneChange, MilestoneFields, Move};

pub(super) fn change_body(milestone: &Milestone, change: MilestoneChange) -> (Value, StatusCode) {
    let version = milestone.version;
    let mut body = match change {
        MilestoneChange::Edit(fields) => edited_fields(milestone, fields),
        MilestoneChange::Complete(completed) => json!({ "completed": completed }),
        MilestoneChange::Move(Move::Up) => json!({ "move": "up" }),
        MilestoneChange::Move(Move::Down) => json!({ "move": "down" }),
        MilestoneChange::Delete => return (json!({ "version": version }), StatusCode::NO_CONTENT),
    };
    body["version"] = json!(version);
    (body, StatusCode::OK)
}

fn edited_fields(before: &Milestone, fields: MilestoneFields) -> Value {
    let mut body = json!({});
    if before.name != fields.name {
        body["name"] = json!(fields.name);
    }
    if before.description.as_deref().unwrap_or_default()
        != fields.description.as_deref().unwrap_or_default()
    {
        body["description"] = json!(fields.description);
    }
    if before.target_date != fields.target_date {
        body["targetDate"] = json!(fields.target_date);
    }
    body
}
