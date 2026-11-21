//! What the inspector says about a run, worked out without drawing it.

use super::{empty_words, job_note, reviews_for, shows_run, status_phrase, title, waiting_on};
use crate::graph::GraphJob;
use crate::model::{Review, View};
use crate::test_support::run;

fn review(environment: &str) -> Review {
    Review {
        environment: environment.into(),
        reviewers: vec!["ops".into()],
    }
}

#[test]
fn nothing_selected_asks_for_a_selection_in_each_views_words() {
    assert_eq!(
        empty_words(View::Runs),
        ("Run", "Select a run to see it here.")
    );
    assert_eq!(empty_words(View::Flow), empty_words(View::Runs));
    assert_eq!(empty_words(View::Releases).0, "Release");
}

#[test]
fn a_run_is_titled_by_its_id_and_reads_its_status_in_words() {
    let mut record = run("octo/app");
    assert_eq!(title(&record, View::Runs), "Run 1");
    assert_eq!(status_phrase(&record), "Success");
    record.status = "in_progress".into();
    assert_eq!(status_phrase(&record), "In progress");
    record.approval = Some((7, true));
    assert_eq!(status_phrase(&record), "Waiting for approval");
    record.run = None;
    assert_eq!(title(&record, View::Releases), "Release");
}

#[test]
fn a_waiting_run_says_what_it_waits_on_and_a_finished_one_says_nothing() {
    let mut record = run("octo/app");
    assert_eq!(waiting_on(&record, &[]), None, "it has finished");
    record.status = "waiting".into();
    let production = [review("production")];
    assert_eq!(
        waiting_on(&record, &production).as_deref(),
        Some("Waiting for a reviewer to approve the production environment.")
    );
    assert_eq!(
        waiting_on(&record, &[]).as_deref(),
        Some("Waiting for an environment's protection rules.")
    );
    record.status = "queued".into();
    assert_eq!(
        waiting_on(&record, &[]).as_deref(),
        Some("Waiting for a runner.")
    );
    record.run = None;
    assert_eq!(waiting_on(&record, &[]), None, "only a run waits");
}

#[test]
fn a_run_finds_the_reviews_loaded_for_it_among_the_deployments() {
    let record = run("octo/app");
    let mut pending = run("octo/app");
    pending.approval = Some((3, true));
    if let Some(facts) = &mut pending.facts {
        facts.review = Some(review("production"));
    }
    let mut elsewhere = pending.clone();
    if let Some(target) = &mut elsewhere.run {
        target.id = 2;
    }
    let deployments = [pending.clone(), elsewhere];
    assert_eq!(reviews_for(&record, &deployments), [review("production")]);
    assert_eq!(
        reviews_for(&pending, &[]),
        [review("production")],
        "its own"
    );
    assert!(reviews_for(&record, &[]).is_empty());
}

#[test]
fn a_job_reads_its_result_and_duration_or_where_it_stands() {
    let mut job = GraphJob {
        id: 1,
        name: "build".into(),
        status: "success".into(),
        duration: "134s".into(),
        steps: String::new(),
    };
    assert_eq!(job_note(&job), "success · 134s");
    job.duration = "Running".into();
    assert_eq!(job_note(&job), "Running");
}

#[test]
fn flow_offers_the_writes_only_for_the_run_it_shows() {
    let shown = run("octo/app");
    let mut other = run("octo/app");
    if let Some(target) = &mut other.run {
        target.id = 2;
    }
    let mut release = run("octo/app");
    release.run = None;
    let target = shown.run.as_ref();
    assert!(shows_run(Some(&shown), target));
    assert!(!shows_run(Some(&other), target), "another run");
    assert!(!shows_run(None, target), "nothing is selected");
    assert!(!shows_run(Some(&shown), None), "no run is open");
    assert!(!shows_run(Some(&release), None), "an item with no run");
}
