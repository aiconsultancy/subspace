//! Service facts to text records. No UI or network work.

use subspace_framework::client_github::actions::{
    Deployment, Environment, PendingDeployment, Release, WorkflowRun,
};
use subspace_framework::dashboard::dates;

use crate::model::{Record, Review, RunFacts, RunTarget, active};

pub(crate) fn run(repo: &str, run: &WorkflowRun) -> Record {
    let status = run.conclusion.as_deref().unwrap_or(&run.status).to_owned();
    let branch = run.head_branch.clone().unwrap_or_default();
    let actor = run
        .actor
        .as_ref()
        .map(|actor| actor.login.clone())
        .unwrap_or_default();
    let name = format!("{} #{}", run.name, run.run_number);
    Record {
        key: format!("{repo}#run#{}", run.id),
        repo: repo.into(),
        title: name,
        status,
        active: active(&run.status),
        link: run.html_url.clone(),
        approval: None,
        facts: Some(RunFacts {
            event: run.event.clone(),
            age: dates::age(&run.updated_at, chrono::Utc::now()),
            started: run.run_started_at.clone(),
            review: None,
        }),
        properties: vec![
            ("Event".into(), run.event.clone()),
            ("SHA".into(), run.head_sha.clone()),
            ("Started".into(), run.run_started_at.clone()),
            ("Updated".into(), run.updated_at.clone()),
        ],
        sections: run
            .head_commit
            .as_ref()
            .map(|commit| vec![("Commit".into(), commit.message.clone())])
            .unwrap_or_default(),
        run: Some(RunTarget {
            repo: repo.into(),
            id: run.id,
            workflow_id: run.workflow_id,
            workflow: run.name.clone(),
            path: run.path.clone(),
            sha: run.head_sha.clone(),
            branch,
            actor,
            attempt: run.run_attempt,
        }),
    }
}

pub(crate) fn approval(repo: &str, run: &WorkflowRun, pending: &PendingDeployment) -> Record {
    let mut record = self::run(repo, run);
    record.key = format!("{repo}#approval#{}#{}", run.id, pending.environment.id);
    record.title = format!(
        "Review {} · {} #{}",
        pending.environment.name, run.name, run.run_number
    );
    record.status = "waiting".into();
    record.approval = Some((pending.environment.id, pending.current_user_can_approve));
    let reviewers = pending
        .reviewers
        .iter()
        .filter_map(|reviewer| reviewer.reviewer.as_ref())
        .filter_map(|who| {
            who.login
                .clone()
                .or_else(|| who.name.clone())
                .or_else(|| who.slug.clone())
        })
        .collect();
    if let Some(facts) = &mut record.facts {
        facts.review = Some(Review {
            environment: pending.environment.name.clone(),
            reviewers,
        });
    }
    record.properties.extend([
        ("Environment".into(), pending.environment.name.clone()),
        (
            "Allowed reviewer".into(),
            pending.current_user_can_approve.to_string(),
        ),
        (
            "Requester".into(),
            record
                .run
                .as_ref()
                .map(|run| run.actor.clone())
                .unwrap_or_default(),
        ),
        (
            "Wait timer".into(),
            format!("{} minutes", pending.wait_timer),
        ),
        (
            "Wait timer started".into(),
            pending.wait_timer_started_at.clone().unwrap_or_default(),
        ),
    ]);
    record
}

pub(super) fn environment(
    repo: &str,
    environment: &Environment,
    deployment: Option<&Deployment>,
) -> Record {
    let state = deployment
        .and_then(|deployment| deployment.latest_status.as_ref())
        .map(|status| status.state.clone())
        .unwrap_or_else(|| "No deployment".into());
    let reference = deployment
        .map(|deployment| deployment.reference.clone())
        .unwrap_or_default();
    Record {
        key: format!("{repo}#environment#{}", environment.id),
        repo: repo.into(),
        title: format!("{} · {reference} · {state}", environment.name),
        status: state,
        active: false,
        link: environment.html_url.clone(),
        run: None,
        approval: None,
        facts: None,
        properties: vec![
            ("Environment".into(), environment.name.clone()),
            ("Ref".into(), reference),
            (
                "Protected branches".into(),
                environment
                    .deployment_branch_policy
                    .as_ref()
                    .map(|policy| policy.protected_branches.to_string())
                    .unwrap_or_else(|| "No branch policy".into()),
            ),
            (
                "Updated".into(),
                deployment.map(|d| d.updated_at.clone()).unwrap_or_default(),
            ),
        ],
        sections: Vec::new(),
    }
}

pub(super) fn release(repo: &str, release: &Release, completed: bool) -> Record {
    let status = if release.draft {
        "draft"
    } else if release.prerelease {
        "prerelease"
    } else {
        "published"
    };
    Record {
        key: format!("{repo}#release#{}", release.id),
        repo: repo.into(),
        title: format!(
            "{} · {}{}",
            release.name.as_deref().unwrap_or(&release.tag_name),
            release.tag_name,
            if completed {
                ""
            } else {
                " · no matching completed run in loaded history"
            }
        ),
        status: status.into(),
        active: false,
        link: release.html_url.clone(),
        run: None,
        approval: None,
        facts: None,
        properties: vec![
            ("Tag".into(), release.tag_name.clone()),
            (
                "Published".into(),
                release.published_at.clone().unwrap_or_default(),
            ),
            (
                "Author".into(),
                release
                    .author
                    .as_ref()
                    .map(|actor| actor.login.clone())
                    .unwrap_or_default(),
            ),
        ],
        sections: vec![(
            "Release notes".into(),
            release.body.clone().unwrap_or_default(),
        )],
    }
}

/// The notice key of a repository whose run history was cut at the client's
/// page limit.
pub(crate) const RUNS_TRUNCATED: &str = "runs-truncated";

/// Whether `record` is some repository's notice `key`.
pub(crate) fn is_notice(record: &Record, key: &str) -> bool {
    record.run.is_none() && record.key.ends_with(&format!("#notice#{key}"))
}

pub(crate) fn notice(repo: &str, key: &str, message: &str) -> Record {
    Record {
        key: format!("{repo}#notice#{key}"),
        repo: repo.into(),
        title: message.into(),
        status: "notice".into(),
        active: false,
        link: String::new(),
        properties: Vec::new(),
        sections: Vec::new(),
        run: None,
        approval: None,
        facts: None,
    }
}

#[cfg(test)]
mod tests {
    use subspace_framework::client_github::actions::{PendingDeployment, WorkflowRun};

    use super::{RUNS_TRUNCATED, approval, is_notice, notice, run};
    use crate::model::Review;

    #[test]
    fn a_run_is_named_briefly_and_a_review_carries_its_reviewers() {
        let source: WorkflowRun = serde_json::from_str(
            r#"{"id":7,"name":"Deploy","run_number":12,"event":"push","status":"waiting",
                "head_branch":"main","actor":{"login":"ann"}}"#,
        )
        .unwrap();
        let pending: PendingDeployment = serde_json::from_str(
            r#"{"environment":{"id":3,"name":"production"},"current_user_can_approve":true,
                "reviewers":[{"type":"User","reviewer":{"login":"bob"}},
                             {"type":"Team","reviewer":{"slug":"ops"}}]}"#,
        )
        .unwrap();
        let record = run("octo/app", &source);
        assert_eq!(record.title, "Deploy #12", "branch and actor have lanes");
        let facts = record.facts.unwrap();
        assert_eq!(facts.event, "push");
        assert_eq!(facts.review, None, "a run alone waits on no known review");
        let review = approval("octo/app", &source, &pending)
            .facts
            .and_then(|facts| facts.review);
        let expected = Review {
            environment: "production".into(),
            reviewers: vec!["bob".into(), "ops".into()],
        };
        assert_eq!(review, Some(expected));
    }

    #[test]
    fn a_truncation_notice_is_told_from_runs_and_other_notices() {
        let cut = notice("octo/app", RUNS_TRUNCATED, "Run history is truncated");
        assert!(is_notice(&cut, RUNS_TRUNCATED));
        let other = notice("octo/app", "no-environments", "No environments");
        assert!(!is_notice(&other, RUNS_TRUNCATED));
        let run = crate::test_support::run("octo/app");
        assert!(!is_notice(&run, "run-1"), "a record with a run is a run");
    }
}
