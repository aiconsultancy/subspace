//! What the inspector says about a run, a deployment or a release, and
//! about Flow's selected job: worked out without drawing, so the tests read
//! it directly.

use crate::graph::GraphJob;
use crate::model::{Record, Review, RunTarget, View};
use crate::rows;

/// The inspector's title, and what it says, while nothing is selected.
pub(crate) fn empty_words(view: View) -> (&'static str, &'static str) {
    match view {
        View::Runs | View::Flow => ("Run", "Select a run to see it here."),
        View::Deployments => (
            "Deployment",
            "Select a deployment or review to see it here.",
        ),
        View::Releases => ("Release", "Select a release to see it here."),
    }
}

/// The header's title: "Run <id>" for a run, else the kind of item.
pub(crate) fn title(record: &Record, view: View) -> String {
    match &record.run {
        Some(run) => format!("Run {}", run.id),
        None => empty_words(view).0.to_owned(),
    }
}

/// The status line over the title, such as "Waiting for approval".
pub(crate) fn status_phrase(record: &Record) -> String {
    if record.approval.is_some() {
        return "Waiting for approval".into();
    }
    let word = rows::status_word(&record.status);
    let mut chars = word.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

/// The reviews `record` waits on: its own, or those of the pending
/// deployments loaded for the same run.
pub(crate) fn reviews_for(record: &Record, deployments: &[Record]) -> Vec<Review> {
    if let Some(review) = record.facts.as_ref().and_then(|facts| facts.review.clone()) {
        return vec![review];
    }
    let Some(run) = &record.run else {
        return Vec::new();
    };
    deployments
        .iter()
        .filter(|other| other.run.as_ref().is_some_and(|other| other.id == run.id))
        .filter_map(|other| other.facts.as_ref()?.review.clone())
        .collect()
}

/// One line saying what a waiting run waits on; none for any other run.
pub(crate) fn waiting_on(record: &Record, reviews: &[Review]) -> Option<String> {
    record.run.as_ref()?;
    let names = reviews
        .iter()
        .map(|review| review.environment.as_str())
        .collect::<Vec<_>>();
    match record.status.as_str() {
        "waiting" if !names.is_empty() => Some(format!(
            "Waiting for a reviewer to approve the {} environment.",
            names.join(" and ")
        )),
        "waiting" => Some("Waiting for an environment's protection rules.".into()),
        "queued" => Some("Waiting for a runner.".into()),
        "pending" | "requested" => Some("Waiting to start.".into()),
        "action_required" => Some("Waiting for a maintainer to approve the run.".into()),
        _ => None,
    }
}

/// A job's result and duration, or where it stands while it runs.
pub(crate) fn job_note(job: &GraphJob) -> String {
    match job.duration.as_str() {
        "Running" | "Not started" => job.duration.clone(),
        duration => format!("{} · {duration}", rows::status_word(&job.status)),
    }
}

/// Whether `selected` is the run whose jobs Flow shows. The writes act on
/// the list's selection, so Flow's job inspector offers them only for its run.
pub(crate) fn shows_run(selected: Option<&Record>, shown: Option<&RunTarget>) -> bool {
    let selected = selected.and_then(|record| record.run.as_ref());
    selected.is_some() && selected == shown
}
