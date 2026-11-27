//! Reconcile the ship feed's bounded task previews with fetched detail.

use std::collections::HashMap;

use subspace_client_fleet::model::{CrewMember, Task};

// The Fleet ship feed takes heads and tails in code points, with no ellipsis.
const SUMMARY_LIMIT: usize = 180;
const PHASE_LIMIT: usize = 80;
const PROGRESS_LIMIT: usize = 400;

#[derive(Clone, Copy)]
pub(in super::super) enum Field {
    Summary,
    Phase,
    Progress,
}

impl Field {
    pub const ALL: [Self; 3] = [Self::Summary, Self::Phase, Self::Progress];

    fn text(self, task: &Task) -> &str {
        match self {
            Self::Summary => &task.summary,
            Self::Phase => &task.phase,
            Self::Progress => &task.progress,
        }
    }

    fn text_mut(self, task: &mut Task) -> &mut String {
        match self {
            Self::Summary => &mut task.summary,
            Self::Phase => &mut task.phase,
            Self::Progress => &mut task.progress,
        }
    }

    pub fn limit(self) -> usize {
        match self {
            Self::Summary => SUMMARY_LIMIT,
            Self::Phase => PHASE_LIMIT,
            Self::Progress => PROGRESS_LIMIT,
        }
    }

    pub fn is_tail(self) -> bool {
        matches!(self, Self::Progress)
    }

    fn at_limit(self, text: &str) -> bool {
        text.chars().take(self.limit() + 1).count() == self.limit()
    }

    fn is_projection(self, full: &str, preview: &str) -> bool {
        full == preview
            || (full.len() > preview.len()
                && self.at_limit(preview)
                && if self.is_tail() {
                    full.ends_with(preview)
                } else {
                    full.starts_with(preview)
                })
    }
}

#[derive(Clone, Copy, Default)]
pub(in super::super) struct PartialFields([bool; Field::ALL.len()]);

impl PartialFields {
    pub fn contains(self, field: Field) -> bool {
        self.0[field as usize]
    }

    pub(super) fn from_roster(task: Option<&Task>) -> Self {
        Self(Field::ALL.map(|field| task.is_some_and(|task| field.at_limit(field.text(task)))))
    }
}

/// Only fields taken from the roster can be marked as capped previews.
#[derive(Default)]
pub(in super::super) struct TaskPreviews<'a>(HashMap<Option<&'a str>, PartialFields>);

impl<'a> TaskPreviews<'a> {
    pub fn fields(&self, project: Option<&str>) -> PartialFields {
        self.0.get(&project).copied().unwrap_or_default()
    }

    pub(super) fn insert(&mut self, project: Option<&'a str>, fields: PartialFields) {
        if fields.0.into_iter().any(|partial| partial) {
            self.0.insert(project, fields);
        }
    }

    pub(super) fn from_roster(member: &'a CrewMember) -> Self {
        let mut previews = Self::default();
        for context in &member.contexts {
            previews.insert(
                context.project.as_deref(),
                PartialFields::from_roster(context.current.as_ref()),
            );
        }
        previews
    }
}

pub(super) fn diverged(old: &Task, new: &Task, full: Option<&Task>) -> bool {
    let full = full.filter(|task| task.id == new.id);
    Field::ALL.into_iter().any(|field| {
        let preview = field.text(new);
        field.text(old) != preview
            && !full.is_some_and(|task| field.is_projection(field.text(task), preview))
    })
}

pub(super) fn overlay_text(shown: &mut Task, baseline: Option<&Task>, new: &Task) -> PartialFields {
    PartialFields(Field::ALL.map(|field| {
        let preview = field.text(new);
        if baseline.is_some_and(|old| field.text(old) == preview)
            || field.is_projection(field.text(shown), preview)
        {
            return false;
        }
        *field.text_mut(shown) = preview.to_owned();
        field.at_limit(preview)
    }))
}
