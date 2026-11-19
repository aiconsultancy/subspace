//! Relation identities recorded after a Copilot Agents session log is published.

use subspace_framework::client_github::copilot_agents::{TaskDetail, TaskSession};
use subspace_framework::dashboard::full_text::{
    IndexMetadata, Reference, ReferenceIdentity, ReferenceMatch,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TaskLinks {
    task_id: String,
    repo: Option<String>,
    pull_number: Option<u32>,
    branch: Option<String>,
}

impl TaskLinks {
    pub(crate) fn from_detail(detail: &TaskDetail) -> Self {
        Self {
            task_id: detail.task.id.clone(),
            repo: supplied(&detail.task.repo),
            pull_number: detail.task.pull_request.as_ref().map(|pull| pull.number),
            branch: session_branch(detail.sessions.last()),
        }
    }

    pub(crate) fn from_task_key(task_key: &str) -> Self {
        Self {
            task_id: task_key
                .strip_prefix("cloud:")
                .unwrap_or(task_key)
                .to_owned(),
            repo: None,
            pull_number: None,
            branch: None,
        }
    }
}

pub(crate) fn metadata(links: &TaskLinks) -> IndexMetadata {
    IndexMetadata {
        project: None,
        references: references(links),
    }
}

fn references(links: &TaskLinks) -> Vec<Reference> {
    let mut refs = Vec::new();
    push_exact(
        &mut refs,
        ReferenceIdentity::github_copilot_agents(&links.task_id),
        "task",
    );
    if let (Some(repo), Some(number)) = (&links.repo, links.pull_number) {
        push_exact(
            &mut refs,
            ReferenceIdentity::github(repo, u64::from(number)),
            "resulting_pull_request",
        );
    }
    if let Some(repo) = &links.repo {
        push_exact(&mut refs, ReferenceIdentity::repository(repo), "repository");
        if let Some(branch) = &links.branch {
            push_exact(&mut refs, ReferenceIdentity::branch(repo, branch), "branch");
        }
    }
    refs
}

fn push_exact(
    refs: &mut Vec<Reference>,
    identity: Result<ReferenceIdentity, String>,
    relation_type: &str,
) {
    if let Ok(identity) = identity {
        refs.push(Reference {
            identity,
            relation_type: relation_type.into(),
            matched: ReferenceMatch::Exact,
        });
    }
}

/// The task's own branch. The base branch is the merge target (usually
/// `main`) and would relate the task to every item on it.
fn session_branch(session: Option<&TaskSession>) -> Option<String> {
    session.and_then(|session| supplied(&session.head_ref))
}

fn supplied(value: &str) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

#[cfg(test)]
mod tests {
    use subspace_framework::client_github::copilot_agents::{
        PullRequestArtifact, Task, TaskDetail, TaskSession,
    };
    use subspace_framework::client_github::model::PrState;
    use subspace_framework::dashboard::full_text::ReferenceMatch;

    use super::{TaskLinks, metadata};

    fn task(id: &str, repo: &str, pr: Option<u32>) -> Task {
        Task {
            id: id.into(),
            repo: repo.into(),
            name: format!("Fix {id}"),
            state: "completed".into(),
            html_url: String::new(),
            created_at: String::new(),
            updated_at: String::new(),
            pull_request: pr.map(|number| PullRequestArtifact {
                number,
                url: format!("https://github.com/{repo}/pull/{number}"),
                state: PrState::Open,
            }),
            subject: None,
        }
    }

    fn session(head_ref: &str, base_ref: &str) -> TaskSession {
        TaskSession {
            id: "session-1".into(),
            name: String::new(),
            state: "completed".into(),
            created_at: String::new(),
            updated_at: String::new(),
            completed_at: None,
            prompt: String::new(),
            head_ref: head_ref.into(),
            base_ref: base_ref.into(),
            model: String::new(),
            usage: None,
            error: None,
        }
    }

    fn identities(detail: &TaskDetail) -> Vec<String> {
        metadata(&TaskLinks::from_detail(detail))
            .references
            .into_iter()
            .map(|reference| {
                assert!(matches!(reference.matched, ReferenceMatch::Exact));
                format!(
                    "{}:{}",
                    reference.relation_type,
                    reference.identity.as_str()
                )
            })
            .collect()
    }

    #[test]
    fn a_task_with_a_pull_request_writes_exact_github_and_copilot_agents_identities() {
        let detail = TaskDetail {
            task: task("task-1", "work/repo", Some(7)),
            sessions: vec![session("copilot/fix-1", "main")],
        };
        let metadata = metadata(&TaskLinks::from_detail(&detail));
        assert_eq!(metadata.project, None);
        assert_eq!(
            identities(&detail),
            [
                "task:github-copilot-agents:task-1",
                "resulting_pull_request:github:work/repo:7",
                "repository:repository:work/repo",
                "branch:branch:work/repo:copilot/fix-1",
            ]
        );
    }

    #[test]
    fn a_task_without_a_pull_request_omits_the_github_identity() {
        let detail = TaskDetail {
            task: task("task-2", "work/repo", None),
            sessions: Vec::new(),
        };
        let listed = identities(&detail);
        assert_eq!(
            listed,
            [
                "task:github-copilot-agents:task-2",
                "repository:repository:work/repo"
            ]
        );
        assert!(
            !listed
                .iter()
                .any(|value| value.contains("resulting_pull_request") || value.contains("github:"))
        );
    }

    #[test]
    fn the_base_branch_is_not_recorded_as_the_task_branch() {
        let detail = TaskDetail {
            task: task("task-3", "work/repo", None),
            sessions: vec![session("", "main")],
        };
        assert!(
            !identities(&detail)
                .iter()
                .any(|value| value.contains("branch:"))
        );
    }

    #[test]
    fn a_task_key_alone_does_not_invent_a_pull_request() {
        let metadata = metadata(&TaskLinks::from_task_key("cloud:task-9"));
        assert_eq!(metadata.project, None);
        assert_eq!(metadata.references.len(), 1);
        assert_eq!(
            metadata.references[0].identity.as_str(),
            "github-copilot-agents:task-9"
        );
        assert_eq!(metadata.references[0].relation_type, "task");
    }
}
