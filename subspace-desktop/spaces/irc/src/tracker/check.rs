//! Whether an issue's state allows a command's change, as the API would
//! decide it: checked on what the tracker last said before Return, and
//! again on the issue as it is just before the write. Pure.

use super::plan::{Known, Plan, clip};

/// The most characters of a name a refusal quotes.
const NAME_CHARS: usize = 60;

/// Whether `known`'s state allows `plan`. `assignee_id` is the user an
/// assignment names, once known.
pub(crate) fn check(plan: &Plan, known: &Known, assignee_id: Option<&str>) -> Result<(), String> {
    let issue = plan.issue();
    match plan {
        Plan::Assign { to, .. } => {
            let same = assignee_id.is_some() && known.assignee_id.as_deref() == assignee_id;
            if same {
                return Err(format!("{issue} is already assigned to {}.", to.words()));
            }
        }
        Plan::Ready { on: true, .. } => ready_allowed(issue, known)?,
        Plan::Ready { on: false, .. } if known.ready == Some(false) => {
            return Err(format!("{issue} isn't flagged Ready for agents."));
        }
        _ => {}
    }
    Ok(())
}

/// The API flags only an open, unassigned issue that is not a tracker.
fn ready_allowed(issue: &str, known: &Known) -> Result<(), String> {
    if known.tracker {
        return Err(format!(
            "{issue} is a tracker. Trackers can't be flagged Ready for agents."
        ));
    }
    if known.closed {
        return Err(format!(
            "{issue} is closed. Reopen it before flagging it Ready for agents."
        ));
    }
    if let Some(assignee) = &known.assignee {
        return Err(format!(
            "{issue} is assigned to {}. Remove the assignee before flagging it Ready for agents.",
            clip(assignee, NAME_CHARS)
        ));
    }
    if known.ready == Some(true) {
        return Err(format!("{issue} is already flagged Ready for agents."));
    }
    Ok(())
}
