//! Properties of the project or the milestone shown in the pane.

use gpui_kit::{App, Context, ParentElement as _, Styled as _, TestSupportExt as _, div};
use subspace_api_client::projects::{CompletionSource, Milestone};
use subspace_framework::dashboard::{Dashboard as _, Tokens, feedback, workspace};

use super::{date_label, milestones, overview};
use crate::Projects;

pub(super) fn render(
    this: &Projects,
    tokens: &Tokens,
    cx: &mut Context<Projects>,
) -> workspace::Inspector {
    let detail = this.milestone_pages.detail.as_ref();
    let title = if detail.is_some() {
        "Milestone"
    } else {
        "Project"
    };
    let inspector = workspace::Inspector::new("projects-inspector", title).on_close(
        "Close inspector",
        cx.listener(|this, _, window, cx| {
            this.toggle_dock(workspace::SpaceDock::Inspector, window, cx);
            this.focus.focus(window, cx);
        }),
    );
    let record = this.record.as_ref().and_then(|result| result.as_ref().ok());
    let Some(record) = record.filter(|record| this.in_scope(&record.project)) else {
        return inspector.child(feedback::empty_line(
            tokens,
            "projects-inspector-empty",
            empty_message(this),
        ));
    };
    if let Some(detail) = detail {
        return match this.milestone(&detail.milestone) {
            Some(row) => milestone_inspector(inspector, row, &record.project.name, tokens, cx),
            None => inspector.child(feedback::empty_line(
                tokens,
                "projects-inspector-empty",
                "Milestone unavailable.",
            )),
        };
    }
    inspector.child(
        workspace::section("Properties", cx).children(
            overview::properties(&record.project)
                .map(|(label, value)| workspace::fact(label, value, cx).test_support()),
        ),
    )
}

fn empty_message(this: &Projects) -> &'static str {
    if this.record_task.is_some() || this.directory_task.is_some() {
        "Loading project…"
    } else if this.selected.is_some() {
        "Project unavailable."
    } else {
        "Select a project to see its properties."
    }
}

fn milestone_inspector(
    inspector: workspace::Inspector,
    row: &Milestone,
    project: &str,
    tokens: &Tokens,
    cx: &App,
) -> workspace::Inspector {
    let (status, colour) = milestones::status_label(row.status, tokens);
    let done_by = row.completion_source.map(|source| match source {
        CompletionSource::Manual => "Manual",
        CompletionSource::Issues => "Linked issues",
        CompletionSource::ManualAndIssues => "Manual and linked issues",
    });
    let properties = workspace::section("Properties", cx)
        .child(workspace::fact("Project", project.to_owned(), cx).test_support())
        .child(workspace::fact("Status", div().text_color(colour).child(status), cx).test_support())
        .child(workspace::fact(
            "Target date",
            date_label(row.target_date.as_deref()),
            cx,
        ))
        .children(done_by.map(|source| workspace::fact("Completed by", source, cx)));
    inspector
        .child(properties)
        .children(row.issue_progress.as_ref().map(|progress| {
            workspace::section("Issue progress", cx)
                .child(milestones::issue_progress(&row.id, progress, tokens))
        }))
}
