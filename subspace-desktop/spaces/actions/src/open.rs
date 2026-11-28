//! An opened item: a second, closable tab beside the view tab. A run's tab
//! holds what opening a run always showed in full: its facts, commit, every
//! job with its steps, and a failed job's log when one is asked for.

use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, ClickEvent, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    SharedString, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, div, px,
};
use subspace_framework::dashboard::list::{self, Body};
use subspace_framework::dashboard::tabs::{self, TabSpec};
use subspace_framework::dashboard::{Tokens, feedback, workspace};

use crate::graph::GraphJob;
use crate::model::{Record, View};
use crate::panel::{LogState, RunDetail};
use crate::{Actions, inspector, lanes};

/// The item open beside the view.
pub(crate) struct OpenTab {
    /// The opened record's key, in `view`'s records.
    pub key: String,
    pub view: View,
    pub label: String,
    /// Whether this tab, rather than the view's, is showing.
    pub active: bool,
    /// The run's jobs and log, kept apart from the selection's.
    pub detail: RunDetail,
}

/// The tab's name: "Run <id>" for a run, else the item's title.
pub(crate) fn tab_label(record: &Record) -> String {
    match &record.run {
        Some(run) => format!("Run {}", run.id),
        None => record.title.clone(),
    }
}

/// Which tab a click on tab `index` shows: the view's (0) or the opened one.
pub(crate) fn shows_opened(index: usize) -> bool {
    index > 0
}

impl Actions {
    /// Opens record `key` of the current view in the second tab and shows
    /// it. A run whose jobs the selection has not read yet reads them now.
    /// Reopening the same attempt of the same item keeps its log.
    pub(crate) fn open_tab(&mut self, key: &str, cx: &mut Context<Self>) {
        let Some(record) = self
            .records()
            .iter()
            .find(|record| record.key == key)
            .cloned()
        else {
            return;
        };
        if let Some(target) = &record.run
            && self.detail.target.as_ref() != Some(target)
        {
            self.load_detail(target.clone(), cx);
        }
        let kept = self
            .open
            .take()
            .filter(|open| open.key == key && open.detail.target == record.run)
            .map(|open| (open.detail.log, open.detail.log_request));
        let detail = match &record.run {
            Some(_) => {
                let (log, log_request) = kept.unwrap_or_default();
                RunDetail {
                    log,
                    log_request,
                    selected_job: None,
                    ..self.detail.clone()
                }
            }
            None => RunDetail::default(),
        };
        self.open = Some(OpenTab {
            key: record.key.clone(),
            view: self.view,
            label: tab_label(&record),
            active: true,
            detail,
        });
        cx.notify();
    }

    pub(crate) fn close_tab(&mut self, cx: &mut Context<Self>) {
        self.open = None;
        cx.notify();
    }

    /// Shows the view's tab (index 0) or the opened item's. Showing the
    /// opened item selects it again, in its own view, so the inspector and
    /// the writes act on the item the page shows.
    pub(crate) fn select_tab(&mut self, index: usize, cx: &mut Context<Self>) {
        let shows = shows_opened(index);
        if let Some((key, view)) = self
            .open
            .as_ref()
            .filter(|_| shows)
            .map(|open| (open.key.clone(), open.view))
        {
            self.follow_opened(&key, view, cx);
        }
        if let Some(open) = &mut self.open {
            open.active = shows;
        }
        cx.notify();
    }

    /// Makes the opened item, still loaded in `view`, the selection.
    fn follow_opened(&mut self, key: &str, view: View, cx: &mut Context<Self>) {
        if !self.records_in(view).iter().any(|record| record.key == key) {
            return;
        }
        if self.view != view {
            self.set_view(view, cx);
        }
        if self.list.selected().map(AsRef::as_ref) != Some(key) {
            let key = SharedString::from(key.to_owned());
            list::update(self, cx, |list| list.select(key));
        }
    }

    /// The opened item, while its tab is the one showing.
    pub(crate) fn opened(&self) -> Option<&OpenTab> {
        self.open.as_ref().filter(|open| open.active)
    }
}

/// The tab strip: the view, then the opened item with its Close.
pub(crate) fn tab_bar(this: &Actions, cx: &mut Context<Actions>) -> tabs::TabBar {
    let bar = tabs::pane_tab_bar("actions-open-tabs")
        .child(tabs::tab(TabSpec::new(this.view.title(), this.view.icon())));
    let Some(open) = &this.open else {
        return bar.selected_index(0);
    };
    let close = cx.listener(|this, _: &ClickEvent, _, cx| this.close_tab(cx));
    let tab = tabs::tab(TabSpec::new(open.label.clone(), open.view.icon())).on_close(close);
    bar.child(tab)
        .selected_index(usize::from(open.active))
        .on_click(cx.listener(|this, index: &usize, _, cx| this.select_tab(*index, cx)))
}

/// The opened item's page, at reading width.
pub(crate) fn page(this: &Actions, open: &OpenTab, cx: &App) -> AnyElement {
    let tokens = Tokens::current(cx);
    let record = this
        .records_in(open.view)
        .iter()
        .find(|record| record.key == open.key);
    let column = v_flex()
        .w_full()
        .max_w(px(workspace::READING_WIDTH_PX))
        .min_w_0()
        .px(workspace::text_edge(&tokens))
        .py_4()
        .gap_6();
    let column = match record {
        Some(record) => column.children(sections(record, &open.detail, cx)),
        None => column.child(feedback::empty_line(
            &tokens,
            "actions-run-page-gone",
            "This item is no longer loaded. Refresh, or close its tab.",
        )),
    };
    v_flex()
        .id("actions-run-page")
        .test_support()
        .size_full()
        .min_h_0()
        .overflow_y_scroll()
        .child(h_flex().w_full().justify_center().child(column))
        .into_any_element()
}

/// Whether `detail` was read for `record`'s current attempt. A run's facts
/// and its jobs and log show together only then: after a re-run the tab
/// waits for the new attempt rather than mixing the two.
pub(crate) fn reads_current_attempt(record: &Record, detail: &RunDetail) -> bool {
    record.run.is_some() && detail.target == record.run
}

/// The lead and facts, the record's own sections, then a run's jobs and log.
fn sections(record: &Record, detail: &RunDetail, cx: &App) -> Vec<AnyElement> {
    let tokens = Tokens::current(cx);
    let waiting = RunDetail {
        loading: true,
        ..RunDetail::default()
    };
    let detail = if reads_current_attempt(record, detail) {
        detail
    } else {
        &waiting
    };
    let mut parts = vec![inspector::lead(record, None, cx), properties(record, cx)];
    parts.extend(
        record
            .sections
            .iter()
            .enumerate()
            .map(|(at, (heading, text))| {
                let body = list::render_body(
                    SharedString::from(format!("actions-run-page-section-{at}")),
                    &Body::Text(text.clone().into()),
                    &tokens,
                );
                workspace::section(heading.clone(), cx)
                    .child(body)
                    .into_any_element()
            }),
    );
    if record.run.is_some() {
        parts.push(jobs(detail, cx));
    }
    parts.extend(job_log(&detail.log, &tokens, cx));
    parts
}

/// The failed job's log once asked for: loading, the log, or why it did
/// not load. A failed log leaves the jobs above it as they were.
fn job_log(log: &LogState, tokens: &Tokens, cx: &App) -> Option<AnyElement> {
    let body = match log {
        LogState::None => return None,
        LogState::Loading => {
            feedback::loading_line(tokens, "actions-job-log-loading", "Loading the job log…")
                .into_any_element()
        }
        LogState::Loaded(text) => {
            list::render_body("actions-job-log", &Body::Text(text.clone().into()), tokens)
                .into_any_element()
        }
        LogState::Failed(error) => feedback::empty_line(
            tokens,
            "actions-job-log-error",
            format!("The job log did not load: {error}"),
        )
        .into_any_element(),
    };
    Some(
        workspace::section("Job log", cx)
            .child(body)
            .test_support()
            .into_any_element(),
    )
}

/// Every property the service gave, with the run's branch, actor and
/// workflow first.
fn properties(record: &Record, cx: &App) -> AnyElement {
    let mut facts = vec![("Repository".to_owned(), record.repo.clone())];
    if let Some(run) = &record.run {
        facts.extend([
            ("Workflow".to_owned(), run.workflow.clone()),
            ("Branch".to_owned(), run.branch.clone()),
            ("Actor".to_owned(), run.actor.clone()),
            ("Attempt".to_owned(), run.attempt.to_string()),
        ]);
    }
    facts.extend(record.properties.iter().cloned());
    workspace::section("Details", cx)
        .children(
            facts
                .into_iter()
                .filter(|(_, value)| !value.is_empty())
                .map(|(label, value)| workspace::fact(label, value, cx)),
        )
        .into_any_element()
}

/// Each job with its result and its steps.
fn jobs(detail: &RunDetail, cx: &App) -> AnyElement {
    let tokens = Tokens::current(cx);
    let section = workspace::section("Jobs", cx);
    let section = if detail.loading {
        section.child(feedback::loading_line(
            &tokens,
            "actions-run-page-loading",
            "Loading jobs…",
        ))
    } else if let Some(error) = &detail.error {
        section.child(feedback::empty_line(
            &tokens,
            "actions-run-page-error",
            format!("Load failed: {error}"),
        ))
    } else if detail.jobs.is_empty() {
        section.child(feedback::empty_line(
            &tokens,
            "actions-run-page-empty",
            "No jobs yet. A queued run may not have started.",
        ))
    } else {
        section.children(detail.jobs.iter().map(|job| job_block(job, &tokens)))
    };
    section.test_support().into_any_element()
}

fn job_block(job: &GraphJob, tokens: &Tokens) -> AnyElement {
    let head = h_flex()
        .gap_2()
        .text_color(tokens.fg_strong)
        .child(lanes::status_icon(&job.status, tokens))
        .child(div().flex_1().min_w_0().child(job.name.clone()))
        .child(
            div()
                .text_color(tokens.muted)
                .child(inspector::job_note(job)),
        );
    let steps = list::render_body(
        SharedString::from(format!("actions-run-page-steps-{}", job.id)),
        &Body::Text(job.steps.clone().into()),
        tokens,
    );
    v_flex()
        .id(SharedString::from(format!(
            "actions-run-page-job:{}",
            job.id
        )))
        .test_support()
        .gap_1()
        .child(head)
        .child(div().pl(tokens.indent).child(steps))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::{reads_current_attempt, shows_opened, tab_label};
    use crate::panel::RunDetail;
    use crate::test_support::run;

    #[test]
    fn a_run_page_shows_jobs_only_from_the_attempt_it_names() {
        let mut record = run("octo/app");
        let read = RunDetail {
            target: record.run.clone(),
            ..RunDetail::default()
        };
        assert!(reads_current_attempt(&record, &read));
        if let Some(run) = record.run.as_mut() {
            run.attempt = 2;
        }
        assert!(
            !reads_current_attempt(&record, &read),
            "attempt 1's jobs never show beside attempt 2's facts"
        );
        record.run = None;
        assert!(!reads_current_attempt(&record, &RunDetail::default()));
    }

    #[test]
    fn a_run_tab_is_named_by_its_id_and_another_item_by_its_title() {
        let mut record = run("octo/app");
        assert_eq!(tab_label(&record), "Run 1");
        record.run = None;
        assert_eq!(tab_label(&record), "CI");
        assert!(!shows_opened(0), "the view's tab");
        assert!(shows_opened(1));
    }
}
