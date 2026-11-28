//! The inspector: the trailing dock that shows the selected run, or the
//! selected deployment or release, with the writes on it in its footer. One
//! click on a row fills it; Open, a double click or Enter opens the run in a
//! tab beside the view. In Flow, a selected job takes its place.

use chrono::Utc;
use gpui_kit::StatefulInteractiveElement as _;
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, ClickEvent, Context, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, SharedString, Styled as _, TestSupportExt as _, div,
};
use subspace_framework::dashboard::action::{Intent, action_button, icon_button};
use subspace_framework::dashboard::rich::plain_text;
use subspace_framework::dashboard::{
    Tokens, dates, feedback, heading_font, links, list, workspace,
};
use subspace_framework::lucide::IconName;

use crate::graph::GraphJob;
use crate::model::{Record, Review, View};
use crate::{Actions, lanes, rows, toolbar, writes};

mod job;
mod words;

use job::job_inspector;

pub(crate) use words::{
    empty_words, job_note, reviews_for, shows_run, status_phrase, title, waiting_on,
};

/// The inspector for this frame, unless it is hidden.
pub(crate) fn render(this: &Actions, cx: &mut Context<Actions>) -> Option<workspace::Inspector> {
    if !this.docks.inspector {
        return None;
    }
    let job = this
        .detail
        .selected_job
        .filter(|_| this.view == View::Flow)
        .and_then(|id| this.detail.jobs.iter().find(|job| job.id == id));
    if let Some(job) = job {
        return Some(job_inspector(this, job, cx));
    }
    let close = cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_inspector(cx));
    let Some(record) = this.selected() else {
        let (title, message) = empty_words(this.view);
        let tokens = Tokens::current(cx);
        let empty = div()
            .id("actions-inspector-empty")
            .test_support()
            .child(feedback::empty_line(
                &tokens,
                "actions-inspector-empty-text",
                message,
            ));
        return Some(
            workspace::Inspector::new("actions-inspector", title)
                .on_close("Close inspector", close)
                .child(empty),
        );
    };
    let mut inspector = workspace::Inspector::new("actions-inspector", title(record, this.view));
    if this.opened().is_none() {
        inspector = inspector.action(open_button(&record.key, cx));
    }
    let inspector = match github_button(record) {
        Some(github) => inspector.action(github),
        None => inspector,
    };
    let inspector = inspector
        .on_close("Close inspector", close)
        .children(body(this, record, cx));
    Some(match record.run {
        Some(_) => inspector.footer(footer(this, this.write_target())),
        None => inspector,
    })
}

/// Open: the record in a tab beside the view, as Enter or a double click.
fn open_button(key: &str, cx: &mut Context<Actions>) -> AnyElement {
    let key = key.to_owned();
    action_button(
        "actions-inspector-open",
        Intent::Neutral,
        Some(IconName::ArrowUpRight),
        "Open",
    )
    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
        list::update(this, cx, |list| {
            list.reveal(&key);
        });
    }))
    .into_any_element()
}

/// The record's page on GitHub, when it has one.
fn github_button(record: &Record) -> Option<AnyElement> {
    let link = record.link.clone();
    (!link.is_empty()).then(|| {
        icon_button(
            "actions-inspector-github",
            Intent::Quiet,
            IconName::ExternalLink,
            "Open on GitHub",
        )
        .on_click(move |_, _, cx| {
            _ = links::open(&link, cx);
        })
        .into_any_element()
    })
}

/// The status, title and wait, the facts, the jobs and the environment.
fn body(this: &Actions, record: &Record, cx: &App) -> Vec<AnyElement> {
    let deployments = this
        .snapshot()
        .map_or(&[][..], |data| data.deployments.as_slice());
    let reviews = reviews_for(record, deployments);
    let mut parts = vec![
        lead(record, waiting_on(record, &reviews), cx),
        facts(record, cx),
    ];
    if record.run.is_some() {
        parts.push(jobs(this, record, cx));
    }
    parts.extend(reviews.iter().map(|review| environment(review, cx)));
    parts
}

/// The status in its tone, the title and, for a waiting run, what it
/// waits on. The opened tab leads with it too.
pub(crate) fn lead(record: &Record, waiting: Option<String>, cx: &App) -> AnyElement {
    let tokens = Tokens::current(cx);
    let phrase = status_phrase(record);
    let (_, tone) = rows::status_look(&record.status);
    let status = h_flex()
        .id("actions-inspector-status")
        .aria_label(phrase.clone())
        .test_support()
        .gap_1p5()
        .text_size(tokens.secondary_size())
        .font_weight(FontWeight::MEDIUM)
        .text_color(tokens.tone(tone))
        .child(lanes::status_icon(&record.status, &tokens))
        .child(phrase);
    let title = div()
        .id("actions-inspector-title")
        .aria_label(record.title.clone())
        .test_support()
        .font_family(heading_font(cx))
        .text_size(tokens.heading_font)
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(tokens.fg_strong)
        .child(plain_text("actions-inspector-title-text", &record.title));
    let waiting = waiting.map(|line| {
        div()
            .id("actions-inspector-waiting")
            .test_support()
            .text_color(tokens.fg)
            .child(line)
    });
    v_flex()
        .w_full()
        .min_w_0()
        .gap_2()
        .child(status)
        .child(title)
        .children(waiting)
        .into_any_element()
}

/// A run's repository, branch, actor, event and start; another record's
/// repository and properties.
fn facts(record: &Record, cx: &App) -> AnyElement {
    let section = workspace::section("Details", cx).child(workspace::fact(
        "Repository",
        record.repo.clone(),
        cx,
    ));
    let section = match record.run.as_ref().zip(record.facts.as_ref()) {
        Some((run, facts)) => {
            let started = dates::relative_time(&facts.started, Utc::now());
            let started = if started.is_empty() {
                "Not started".into()
            } else {
                started
            };
            let branch = div()
                .font_family(cx.theme().mono_font_family.clone())
                .child(run.branch.clone());
            section
                .child(workspace::fact("Branch", branch, cx))
                .child(workspace::fact("Actor", run.actor.clone(), cx))
                .child(workspace::fact("Event", facts.event.clone(), cx))
                .child(workspace::fact("Started", started, cx))
        }
        None => section.children(
            record
                .properties
                .iter()
                .filter(|(_, value)| !value.is_empty())
                .map(|(label, value)| workspace::fact(label.clone(), value.clone(), cx)),
        ),
    };
    section.test_support().into_any_element()
}

/// One row per job, once the selection's jobs have loaded.
fn jobs(this: &Actions, record: &Record, cx: &App) -> AnyElement {
    let tokens = Tokens::current(cx);
    let section = workspace::section("Jobs", cx);
    let detail = &this.detail;
    // The same run, whatever its attempt: a re-run keeps its jobs showing.
    let current = detail
        .target
        .as_ref()
        .zip(record.run.as_ref())
        .is_some_and(|(shown, run)| shown.repo == run.repo && shown.id == run.id);
    let section = if !current || detail.loading {
        section.child(feedback::loading_line(
            &tokens,
            "actions-jobs-loading",
            "Loading jobs…",
        ))
    } else if let Some(error) = &detail.error {
        section.child(feedback::empty_line(
            &tokens,
            "actions-jobs-error",
            format!("Jobs did not load: {error}"),
        ))
    } else if detail.jobs.is_empty() {
        section.child(feedback::empty_line(
            &tokens,
            "actions-jobs-empty",
            "No jobs yet. A queued run may not have started.",
        ))
    } else {
        section.children(detail.jobs.iter().map(|job| job_row(job, &tokens)))
    };
    section.test_support().into_any_element()
}

fn job_row(job: &GraphJob, tokens: &Tokens) -> AnyElement {
    h_flex()
        .id(SharedString::from(format!("actions-job:{}", job.id)))
        .aria_label(format!("{}, {}", job.name, job_note(job)))
        .test_support()
        .w_full()
        .min_w_0()
        .gap_2()
        .text_size(tokens.secondary_size())
        .child(lanes::status_icon(&job.status, tokens))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_color(tokens.fg_strong)
                .child(job.name.clone()),
        )
        .child(
            div()
                .flex_shrink_0()
                .text_color(tokens.muted)
                .child(job_note(job)),
        )
        .into_any_element()
}

/// The environment a review waits on, and who may give it.
fn environment(review: &Review, cx: &App) -> AnyElement {
    let reviewers = if review.reviewers.is_empty() {
        "Not listed".to_owned()
    } else {
        review.reviewers.join(", ")
    };
    workspace::section("Environment", cx)
        .child(workspace::fact("Name", review.environment.clone(), cx))
        .child(workspace::fact("Reviewers", reviewers, cx))
        .test_support()
        .into_any_element()
}

/// The writes on the selection: Approve first, primary while a review
/// waits, then Reject, Re-run, Re-run failed and Cancel run. Each is
/// disabled, with the reason as its tooltip, when it does not apply.
fn footer(this: &Actions, selected: Option<&Record>) -> AnyElement {
    let buttons = writes::Operation::FOOTER.map(|operation| {
        let reason = writes::unavailable(operation, selected);
        let intent = if writes::is_primary(operation, selected) {
            Intent::Primary
        } else {
            Intent::Neutral
        };
        toolbar::write_button(this.weak.clone(), operation, reason, intent)
    });
    h_flex()
        .id("actions-run-actions")
        .test_support()
        .w_full()
        .min_w_0()
        .flex_wrap()
        .gap_2()
        .children(buttons)
        .into_any_element()
}

#[cfg(test)]
mod tests;
