use subspace_api_client::issue_writes::{IssueChanges, IssueProperty};

use super::{Field, IssueEditor};

#[derive(Clone, Copy)]
pub(super) enum TrackerConfirmation {
    Assignment,
    State { assign_children: Option<bool> },
}

#[derive(Default)]
pub(super) struct ChildChanges {
    pub(super) assign_children: Option<bool>,
    pub(super) move_children: bool,
}

impl ChildChanges {
    pub(super) fn confirmation(&self, editor: &IssueEditor) -> Option<TrackerConfirmation> {
        let properties = &editor.properties;
        if crate::pickers::teams() || !editor.editing_existing() || !properties.tracker() {
            return None;
        }
        if self.assign_children.is_none()
            && properties
                .staged(Field::Assignee)
                .is_some_and(|values| !values.is_empty())
        {
            return Some(TrackerConfirmation::Assignment);
        }
        if !self.move_children && properties.staged(Field::State).is_some() {
            return Some(TrackerConfirmation::State {
                assign_children: self.assign_children,
            });
        }
        None
    }

    pub(super) fn install(self, fields: &mut IssueChanges) {
        if self.assign_children == Some(true) {
            fields.set(IssueProperty::AssignUnassignedChildren);
        }
        if self.move_children {
            fields.set(IssueProperty::MoveOpenChildren);
        }
    }
}
