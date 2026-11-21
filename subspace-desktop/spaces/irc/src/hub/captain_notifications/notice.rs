//! Short, bounded status messages with repository and deployment identities.

use chrono::{DateTime, Utc};
use subspace_framework::client_github::actions::Deployment;

use super::route::Route;

const MAX_ENVIRONMENT_LENGTH: usize = 36;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) enum Phase {
    Merged,
    Started,
    Failed,
    Succeeded,
}

impl Phase {
    pub(super) fn deployment(state: &str) -> Option<Self> {
        match state {
            "in_progress" => Some(Self::Started),
            "failure" | "error" => Some(Self::Failed),
            "success" => Some(Self::Succeeded),
            _ => None,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Merged => "PR merged",
            Self::Started => "Deployment started",
            Self::Failed => "Deployment failed",
            Self::Succeeded => "Deployment succeeded",
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub(super) struct NoticeKey {
    pub(super) repo: String,
    pub(super) number: u32,
    deployment: Option<u64>,
    pub(super) phase: Phase,
}

pub(super) struct Notice {
    pub(super) key: NoticeKey,
    pub(super) route: Route,
    environment: Option<String>,
    pub(super) occurred_at: DateTime<Utc>,
}

impl Notice {
    pub(super) fn merged(
        repo: &str,
        number: u32,
        route: Route,
        occurred_at: DateTime<Utc>,
    ) -> Self {
        Self {
            key: NoticeKey {
                repo: repo.into(),
                number,
                deployment: None,
                phase: Phase::Merged,
            },
            route,
            environment: None,
            occurred_at,
        }
    }

    pub(super) fn deployment(
        repo: &str,
        (number, route): (u32, Route),
        deployment: &Deployment,
        phase: Phase,
        occurred_at: DateTime<Utc>,
    ) -> Self {
        Self {
            key: NoticeKey {
                repo: repo.into(),
                number,
                deployment: Some(deployment.id),
                phase,
            },
            route,
            environment: Some(environment_label(&deployment.environment)),
            occurred_at,
        }
    }

    pub(super) fn text(&self) -> String {
        let environment = self
            .environment
            .as_deref()
            .map_or_else(String::new, |name| format!(" ({name})"));
        format!(
            "@{} [Subspace] {} · {}{environment} — https://github.com/{}/pull/{}",
            self.route.ship,
            self.route.issue,
            self.key.phase.label(),
            self.key.repo,
            self.key.number
        )
    }
}

fn environment_label(name: &str) -> String {
    name.chars()
        .take(MAX_ENVIRONMENT_LENGTH)
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || "._-".contains(ch) {
                ch
            } else {
                '_'
            }
        })
        .collect()
}
