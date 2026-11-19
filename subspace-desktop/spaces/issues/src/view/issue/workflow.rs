//! A project's ordered flight path. The trail means position, never a calculated
//! issue status. Subspace API remains the sole authority for the current state.
//!
//! States group by type, so custom workflows keep their shape: triage, backlog,
//! unstarted, started and unknown types are path steps in workflow order, and
//! completed, canceled and duplicate states share one end slot. The slot shows
//! the issue's end state, or the first completed state while it has none.
mod drawing;
mod end;

use crate::model::StateKind;
use subspace_api_client::model::{IssueDetail, Project, WorkflowState};

pub(super) use drawing::render;

const MIN_WAYPOINT: f32 = 96.;
const CELL_CENTRE: f32 = 0.5;
const UNMEASURED: f32 = 0.;

struct FlightPath<'a> {
    /// The path steps, in workflow order.
    course: Vec<&'a WorkflowState>,
    /// Every end state: completed, then canceled, then duplicate types.
    ends: Vec<&'a WorkflowState>,
    /// The end state the slot shows.
    end: Option<&'a WorkflowState>,
    /// The current state's stop, counting the end slot last.
    current: Option<usize>,
    current_id: &'a str,
}

#[derive(Debug, PartialEq)]
enum Layout {
    Current,
    Course {
        centres: Vec<f32>,
        travelled_to: Option<f32>,
    },
}

fn path<'a>(detail: &'a IssueDetail, projects: &'a [Project]) -> Option<FlightPath<'a>> {
    let project = projects
        .iter()
        .find(|project| project.id == detail.fields.project_id && !project.states.is_empty())?;
    Some(FlightPath::new(&project.states, &detail.fields.state.id))
}

fn kind(state: &WorkflowState) -> StateKind {
    StateKind::parse(&state.kind)
}

/// The end states' order in the slot's menu, whatever order the type enum
/// declares them in.
const END_ORDER: [StateKind; 3] = [
    StateKind::Completed,
    StateKind::Canceled,
    StateKind::Duplicate,
];

/// A state type's place in [`END_ORDER`]; other types sort after it.
fn end_rank(kind: StateKind) -> usize {
    END_ORDER
        .iter()
        .position(|end| *end == kind)
        .unwrap_or(END_ORDER.len())
}

impl<'a> FlightPath<'a> {
    fn new(states: &'a [WorkflowState], current_id: &'a str) -> Self {
        let (mut ends, mut course): (Vec<_>, Vec<_>) =
            states.iter().partition(|state| kind(state).is_terminal());
        course.sort_by_key(|state| state.position);
        ends.sort_by_key(|state| (end_rank(kind(state)), state.position));
        let end = ends
            .iter()
            .find(|state| state.id == current_id)
            .or(ends.first())
            .copied();
        let current = course
            .iter()
            .chain(&end)
            .position(|state| state.id == current_id);
        Self {
            course,
            ends,
            end,
            current,
            current_id,
        }
    }

    /// The path steps followed by the end slot's state.
    fn stops(&self) -> impl Iterator<Item = &'a WorkflowState> + '_ {
        self.course.iter().copied().chain(self.end)
    }

    fn count(&self) -> usize {
        self.course.len() + usize::from(self.end.is_some())
    }

    fn selected(&self) -> Option<&'a WorkflowState> {
        self.stops().find(|state| state.id == self.current_id)
    }

    fn description(&self) -> String {
        let status = match (self.current, self.selected()) {
            (Some(ix), Some(state)) => format!("{}, {} of {}", state.name, ix + 1, self.count()),
            _ => "Current state unavailable".into(),
        };
        let course = names(self.stops(), " → ");
        let mut description = format!("Workflow: {status}. {course}");
        if self.ends.len() > 1 {
            description.push_str(&format!(
                ". End states: {}",
                names(self.ends.iter().copied(), ", ")
            ));
        }
        description
    }

    fn layout(&self, width: f32, scale: f32) -> Layout {
        let count = self.count();
        match count == 0 || (width > UNMEASURED && width < count as f32 * MIN_WAYPOINT * scale) {
            true => Layout::Current,
            false => {
                let centres = centres(count, width);
                let travelled_to = self.current.map(|ix| centres[ix]);
                Layout::Course {
                    centres,
                    travelled_to,
                }
            }
        }
    }
}

fn names<'a>(states: impl Iterator<Item = &'a WorkflowState>, separator: &str) -> String {
    states
        .map(|state| state.name.as_str())
        .collect::<Vec<_>>()
        .join(separator)
}

fn centres(count: usize, width: f32) -> Vec<f32> {
    let step = width / count as f32;
    (0..count)
        .map(|ix| (ix as f32 + CELL_CENTRE) * step)
        .collect()
}

#[cfg(test)]
mod tests;
