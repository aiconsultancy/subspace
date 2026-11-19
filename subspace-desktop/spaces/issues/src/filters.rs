//! Memory-only toolbar choices, applied through the hub's existing query.

use subspace_api_client::ListQuery;

pub(crate) const MAX_PRIORITY: u8 = 4;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Filters {
    pub(crate) assignee: Option<String>,
    pub(crate) priorities: Vec<u8>,
    pub(crate) states: Vec<String>,
    pub(crate) state_types: Vec<String>,
    pub(crate) kind: Option<String>,
    pub(crate) due: Option<String>,
    pub(crate) archived: bool,
}

pub(crate) fn is_active(filters: &Filters) -> bool {
    filters != &Filters::default()
}

impl Filters {
    pub(crate) fn apply(&self, query: ListQuery) -> ListQuery {
        query
            .assignee(self.assignee.as_deref())
            .priorities(&self.priorities)
            .states(&self.states.iter().map(String::as_str).collect::<Vec<_>>())
            .state_types(
                &self
                    .state_types
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>(),
            )
            .kind(self.kind.as_deref())
            .due(self.due.as_deref())
            .include_archived(self.archived)
    }

    pub(crate) fn clear_status(&mut self) {
        self.states.clear();
        self.state_types.clear();
    }

    pub(crate) fn has_more(&self) -> bool {
        !self.states.is_empty()
            || !self.state_types.is_empty()
            || self.kind.is_some()
            || self.due.is_some()
            || self.archived
    }

    pub(crate) fn pick(&mut self, value: &str) {
        let (key, value) = value.split_once(':').unwrap_or((value, ""));
        match key {
            "assignee" => self.assignee = optional(value),
            "priority" => {
                if let Ok(priority @ 0..=MAX_PRIORITY) = value.parse::<u8>() {
                    toggle(&mut self.priorities, priority);
                }
            }
            "priorities" => self.priorities.clear(),
            "state" => toggle(&mut self.states, value.to_owned()),
            "stateType" => toggle(&mut self.state_types, value.to_owned()),
            "status" => self.clear_status(),
            "type" => self.kind = optional(value),
            "due" => self.due = optional(value),
            "archived" => self.archived = !self.archived,
            _ => {}
        }
    }
}

fn optional(value: &str) -> Option<String> {
    (!value.is_empty()).then(|| value.to_owned())
}

fn toggle<T: PartialEq>(values: &mut Vec<T>, value: T) {
    if let Some(index) = values.iter().position(|found| *found == value) {
        values.remove(index);
    } else {
        values.push(value);
    }
}
