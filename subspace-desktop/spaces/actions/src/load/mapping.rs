//! Service facts to text records. No UI or network work.

use subspace_framework::client_github::actions::{
    Deployment, Environment, PendingDeployment, Release, WorkflowRun,
};
use subspace_framework::dashboard::dates;

use crate::model::{Record, RunTarget, active};

pub(crate) fn run(repo: &str, run: &WorkflowRun) -> Record {
    let status = run.conclusion.as_deref().unwrap_or(&run.status).to_owned();
    let branch = run.head_branch.clone().unwrap_or_default();
    let actor = run
        .actor
        .as_ref()
        .map(|actor| actor.login.clone())
        .unwrap_or_default();
    let age = dates::age(&run.updated_at, chrono::Utc::now());
    Record {
        key: format!("{repo}#run#{}", run.id),
        repo: repo.into(),
        title: format!(
            "{} #{} · {branch} · {actor} · {} · {age}",
            run.name, run.run_number, run.event
        ),
        status,
        active: active(&run.status),
        link: run.html_url.clone(),
        approval: None,
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
    }
}
