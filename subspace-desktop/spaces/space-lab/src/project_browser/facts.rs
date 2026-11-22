//! What the pane shows and what its chrome and the status bar say about it,
//! kept apart from rendering so it can be checked without a window.

use subspace_framework::dashboard::panel::IconName;

use crate::project::ProjectRevision;
use crate::project_runtime::SessionStatus;
use crate::shell::NEW_PROJECT;

/// The pane tab's name when nothing is open.
const WELCOME: &str = "Welcome";
/// The toolbar's leading name when no project is open: the view's.
const PROJECTS: &str = "Projects";
/// The status bar's line while a project operation is pending.
const WORKING: &str = "Working…";
/// Revisions are long hashes; their first characters identify them.
const SHORT_REVISION_CHARS: usize = 12;

/// What is open in the pane, which names its tab and its toolbar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Opened {
    /// Nothing: the empty state.
    Welcome,
    /// The New project form.
    Creating,
    /// A project, by its title.
    Project(String),
}

impl Opened {
    /// The New project form covers an open project until it is cancelled or
    /// a project opens.
    pub(super) fn of(creating: bool, project: Option<&str>) -> Self {
        match (creating, project) {
            (true, _) => Self::Creating,
            (false, Some(title)) => Self::Project(title.to_owned()),
            (false, None) => Self::Welcome,
        }
    }

    /// The pane tab's name.
    pub(super) fn tab_title(&self) -> &str {
        self.chrome().0
    }

    /// The toolbar's leading name: the open project's, or the view's.
    pub(super) fn toolbar_title(&self) -> &str {
        self.chrome().1
    }

    /// The pane tab's glyph.
    pub(super) fn icon(&self) -> IconName {
        self.chrome().2
    }

    /// The tab's name, the toolbar's leading name and the tab's glyph, in
    /// one place per variant.
    fn chrome(&self) -> (&str, &str, IconName) {
        match self {
            Self::Welcome => (WELCOME, PROJECTS, IconName::FlaskConical),
            Self::Creating => (NEW_PROJECT, PROJECTS, IconName::Plus),
            Self::Project(title) => (title.as_str(), title.as_str(), IconName::LayoutDashboard),
        }
    }
}

/// What the worker's state allows. The toolbar's actions and the revision
/// lines read the same facts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct WorkerFacts {
    /// Starting or running: Start is unavailable and Stop is offered.
    pub(super) running: bool,
    pub(super) failed: bool,
    /// The working revision is not the one that runs.
    pub(super) changed: bool,
}

impl WorkerFacts {
    pub(super) fn new(
        status: SessionStatus,
        applied: Option<&ProjectRevision>,
        working: &ProjectRevision,
    ) -> Self {
        Self {
            running: matches!(status, SessionStatus::Starting | SessionStatus::Running),
            failed: status == SessionStatus::Failed,
            changed: applied.is_some_and(|revision| revision != working),
        }
    }

    /// Start becomes Restart after a failure.
    pub(super) fn start_label(self) -> &'static str {
        if self.failed {
            "Restart…"
        } else {
            "Start…"
        }
    }

    /// Apply & Restart is offered while a worker starts or runs, or when
    /// working changes are not running.
    pub(super) fn offers_apply(self) -> bool {
        self.running || self.changed
    }
}

/// The working revision's line above an open project.
pub(super) fn working_line(working: &ProjectRevision) -> String {
    format!(
        "Working · definition {} · source {}",
        short(&working.definition),
        short(&working.source)
    )
}

/// The running revision's line above an open project.
pub(super) fn running_line(applied: Option<&ProjectRevision>, status: SessionStatus) -> String {
    applied.map_or_else(
        || "Running · none (not started)".into(),
        |revision| {
            format!(
                "Running · definition {} · source {} · {status:?}",
                short(&revision.definition),
                short(&revision.source)
            )
        },
    )
}

fn short(revision: &str) -> String {
    revision.chars().take(SHORT_REVISION_CHARS).collect()
}

/// The Space's line in the shell's status bar: `Working…` while an operation
/// is pending, the latest status otherwise.
pub(super) fn status_line(busy: bool, status: &str) -> String {
    if busy { WORKING.into() } else { status.into() }
}

#[cfg(test)]
mod tests {
    use subspace_framework::dashboard::panel::IconName;

    use super::{Opened, WorkerFacts, running_line, status_line, working_line};
    use crate::project::ProjectRevision;
    use crate::project_runtime::SessionStatus;

    #[test]
    fn the_tab_names_what_is_open() {
        assert_eq!(Opened::of(false, None).tab_title(), "Welcome");
        assert_eq!(Opened::of(false, Some("Rates")).tab_title(), "Rates");
        assert_eq!(Opened::of(true, None).tab_title(), "New project");
        assert_eq!(
            Opened::of(true, Some("Rates")),
            Opened::Creating,
            "the form covers an open project"
        );
    }

    #[test]
    fn the_toolbar_names_the_open_project_or_the_view() {
        assert_eq!(Opened::Welcome.toolbar_title(), "Projects");
        assert_eq!(Opened::Creating.toolbar_title(), "Projects");
        assert_eq!(Opened::Project("Rates".into()).toolbar_title(), "Rates");
    }

    #[test]
    fn each_kind_of_tab_has_its_own_glyph() {
        let project = Opened::Project("Rates".into());
        assert_eq!(Opened::Welcome.icon(), IconName::FlaskConical);
        assert_eq!(Opened::Creating.icon(), IconName::Plus);
        assert_eq!(project.icon(), IconName::LayoutDashboard);
    }

    #[test]
    fn a_stopped_worker_offers_start_only() {
        let working = ProjectRevision::new("definition-one", "source-one");
        let facts = WorkerFacts::new(SessionStatus::Stopped, None, &working);
        assert!(!facts.running && !facts.failed && !facts.changed);
        assert_eq!(facts.start_label(), "Start…");
        assert!(!facts.offers_apply());
    }

    #[test]
    fn a_starting_or_running_worker_offers_apply_and_stop() {
        let working = ProjectRevision::new("definition-one", "source-one");
        for status in [SessionStatus::Starting, SessionStatus::Running] {
            let facts = WorkerFacts::new(status, Some(&working), &working);
            assert!(facts.running && !facts.changed);
            assert!(facts.offers_apply());
        }
    }

    #[test]
    fn working_changes_offer_apply_while_the_worker_is_stopped() {
        let applied = ProjectRevision::new("definition-one", "source-one");
        let working = ProjectRevision::new("definition-one", "source-two");
        let facts = WorkerFacts::new(SessionStatus::Stopped, Some(&applied), &working);
        assert!(facts.changed && !facts.running);
        assert!(facts.offers_apply());
    }

    #[test]
    fn a_failed_worker_offers_restart() {
        let working = ProjectRevision::new("definition-one", "source-one");
        let facts = WorkerFacts::new(SessionStatus::Failed, Some(&working), &working);
        assert!(facts.failed && !facts.running);
        assert_eq!(facts.start_label(), "Restart…");
    }

    #[test]
    fn revision_lines_show_the_first_twelve_characters() {
        let working = ProjectRevision::new("0123456789abcdef", "fedcba9876543210");
        assert_eq!(
            working_line(&working),
            "Working · definition 0123456789ab · source fedcba987654"
        );
        assert_eq!(
            running_line(Some(&working), SessionStatus::Running),
            "Running · definition 0123456789ab · source fedcba987654 · Running"
        );
        assert_eq!(
            running_line(None, SessionStatus::Stopped),
            "Running · none (not started)"
        );
    }

    #[test]
    fn the_status_bar_says_working_while_an_operation_is_pending() {
        let guidance = "Open a project or create one. Starting a worker is always explicit.";
        assert_eq!(status_line(false, guidance), guidance);
        assert_eq!(status_line(true, guidance), "Working…");
    }
}
