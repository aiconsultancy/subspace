//! Creating and reviewing proposed decisions in the standalone Space.

mod draft;
mod entry;
mod evidence;
mod form;
mod job;
mod picker;
mod submission;
pub(crate) mod view;

#[cfg(test)]
mod tests;

use gpui_kit::{App, FocusHandle, KeyBinding, Task, actions};
use subspace_api_client::Backend;
use subspace_api_client::decision_writes::DecisionReview;
use subspace_api_client::model::DecisionDetail;

use crate::load::Lease;
use draft::Draft;
use job::{Fields, Job, Problem, Target};

#[cfg(test)]
pub(crate) type TestWriter =
    std::sync::Arc<dyn Fn(Job) -> Result<DecisionDetail, Problem> + Send + Sync>;

actions!(decisions_author, [CancelDecisionDialog]);
const CONTEXT: &str = "DecisionDialog";

pub(crate) fn bind_keys(cx: &mut App) {
    cx.bind_keys([KeyBinding::new(
        "escape",
        CancelDecisionDialog,
        Some(CONTEXT),
    )]);
}

enum Mode {
    Draft {
        draft: Box<Draft>,
        target: Option<Target>,
    },
    Review {
        target: Target,
        title: String,
        review: DecisionReview,
    },
}

enum Phase {
    Ready,
    Sending,
    Failed(Problem),
    ReadingCurrent,
    Current(Box<DecisionDetail>),
    ReadFailed,
    Unconfirmed,
}

pub(crate) struct Editor {
    mode: Mode,
    phase: Phase,
    lease: Lease,
    focus: FocusHandle,
    previous_focus: Option<FocusHandle>,
    task: Option<Task<()>>,
}

impl Editor {
    fn target(&self) -> Option<&Target> {
        match &self.mode {
            Mode::Draft { target, .. } => target.as_ref(),
            Mode::Review { target, .. } => Some(target),
        }
    }

    fn job(&self, cx: &App) -> Result<Job, Problem> {
        let (draft, target) = match &self.mode {
            Mode::Draft { draft, target } => (draft, target),
            Mode::Review { target, review, .. } => {
                return Ok(Job::Review {
                    target: target.clone(),
                    review: *review,
                });
            }
        };
        let project = draft.project_id(cx).ok_or(Problem::ChooseProject)?;
        let (changes, issues) = draft.changes(cx);
        let fields = Box::new(Fields { changes, issues });
        match target {
            Some(target) if fields.changed() => Ok(Job::Edit {
                target: target.clone(),
                project,
                fields,
            }),
            Some(_) => Err(Problem::Empty),
            None => Ok(Job::Create {
                project,
                fields,
                attribution: draft
                    .evidence
                    .as_ref()
                    .map(|evidence| evidence.attribution(cx))
                    .transpose()
                    .map_err(Problem::Write)?
                    .flatten(),
            }),
        }
    }
}

pub(crate) fn available() -> bool {
    subspace_api_client::has_backend() && subspace_api_client::backend() == Backend::Subspace
}
