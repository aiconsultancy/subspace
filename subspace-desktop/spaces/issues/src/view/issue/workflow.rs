//! A project's ordered flight path, drawn as a slim bar: one equal segment
//! per stop, each a thin rule with its state's name beneath. Segments up to
//! and including the current stop are filled. The bar means position, never
//! a calculated issue status. Subspace API remains the sole authority for
//! the current state.
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

/// The narrowest a stop's segment may be before the bar gives way to the
/// current state alone, at the default text size.
const MIN_SEGMENT: f32 = 96.;
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
    /// Too narrow for every stop: the current state and its place alone.
    Current,
    /// One equal segment per stop; the first `filled` are filled.
    Segments { filled: usize },
}

/// One stop's segment: whether its rule is filled, and whether it is the
/// current stop, whose name is strong.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Segment {
    filled: bool,
    current: bool,
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
        if count == 0 || (width > UNMEASURED && width < count as f32 * MIN_SEGMENT * scale) {
            return Layout::Current;
        }
        Layout::Segments {
            filled: self.current.map_or(0, |ix| ix + 1),
        }
    }

    /// Stop `ix`'s segment, counting the end slot last. Without a current
    /// stop nothing is filled: the bar never makes up progress.
    fn segment(&self, ix: usize) -> Segment {
        Segment {
            filled: self.current.is_some_and(|current| ix <= current),
            current: self.current == Some(ix),
        }
    }
}

fn names<'a>(states: impl Iterator<Item = &'a WorkflowState>, separator: &str) -> String {
    states
        .map(|state| state.name.as_str())
        .collect::<Vec<_>>()
        .join(separator)
}

#[cfg(test)]
mod tests;
