//! Read GitHub's actual merges and deployment status history.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use subspace_framework::client_github::{
    Client,
    actions::{Deployment, DeploymentStatus},
    lifecycle::ClosedPull,
};

use super::{
    Scope,
    notice::{Notice, Phase},
    route::{Route, Routes},
};

pub(super) fn read(
    client: &Client,
    api: &subspace_api_client::Client,
    scope: &Scope,
    since: DateTime<Utc>,
) -> Vec<Notice> {
    let Some(mut routes) = Routes::load(api) else {
        return Vec::new();
    };
    let Some(login) = client
        .accounts()
        .ok()
        .and_then(|accounts| accounts.pick(scope.account.as_deref()))
    else {
        return Vec::new();
    };
    let mut notices: Vec<_> = scope
        .repos
        .iter()
        .flat_map(|repo| {
            Repository {
                client,
                login: &login,
                repo,
                since,
            }
            .notices(&mut routes)
        })
        .collect();
    notices.sort_by_key(|notice| (notice.occurred_at, notice.key.phase));
    notices
}

struct Repository<'a> {
    client: &'a Client,
    login: &'a str,
    repo: &'a str,
    since: DateTime<Utc>,
}

type PullRoute = (u32, Route);

impl Repository<'_> {
    fn notices(&self, routes: &mut Routes) -> Vec<Notice> {
        let pulls = self
            .client
            .recent_closed_pulls(self.login, self.repo)
            .unwrap_or_default();
        let mut notices: Vec<_> = pulls
            .iter()
            .filter_map(|pull| self.merge(pull, routes))
            .collect();
        notices.extend(self.deployments(routes));
        notices
    }

    fn merge(&self, pull: &ClosedPull, routes: &mut Routes) -> Option<Notice> {
        let time = pull.merged_at().filter(|&time| time > self.since)?;
        let (number, route) = self.pull_route(pull, routes)?;
        Some(Notice::merged(self.repo, number, route, time))
    }

    fn pull_route(&self, pull: &ClosedPull, routes: &mut Routes) -> Option<PullRoute> {
        if !pull.is_merged_into(self.repo) {
            return None;
        }
        Some((pull.number(), routes.resolve(pull.body()?)?))
    }

    fn deployments(&self, routes: &mut Routes) -> Vec<Notice> {
        let mut owners = HashMap::new();
        self.client
            .recent_deployments(self.login, self.repo)
            .unwrap_or_default()
            .into_iter()
            .filter(|deployment| recent(&deployment.updated_at, self.since).is_some())
            .flat_map(|deployment| {
                let target = owners
                    .entry(deployment.sha.clone())
                    .or_insert_with(|| self.deployment_route(&deployment.sha, routes));
                target
                    .as_ref()
                    .map_or_else(Vec::new, |target| self.deployment(&deployment, target))
            })
            .collect()
    }

    fn deployment_route(&self, sha: &str, routes: &mut Routes) -> Option<PullRoute> {
        let pulls = self.client.commit_pulls(self.login, self.repo, sha).ok()?;
        let mut merged = pulls
            .iter()
            .filter(|pull| pull.is_merged_into(self.repo) && pull.merge_commit_sha() == Some(sha));
        let pull = merged.next()?;
        if merged.next().is_some() {
            return None;
        }
        self.pull_route(pull, routes)
    }

    fn deployment(&self, deployment: &Deployment, target: &PullRoute) -> Vec<Notice> {
        self.client
            .deployment_history(self.login, self.repo, deployment.id)
            .unwrap_or_default()
            .iter()
            .filter_map(|status| self.status(deployment, status, target))
            .collect()
    }

    fn status(
        &self,
        deployment: &Deployment,
        status: &DeploymentStatus,
        target: &PullRoute,
    ) -> Option<Notice> {
        let time = recent(&status.created_at, self.since)?;
        let phase = Phase::deployment(&status.state)?;
        Some(Notice::deployment(
            self.repo,
            target.clone(),
            deployment,
            phase,
            time,
        ))
    }
}

fn recent(value: &str, since: DateTime<Utc>) -> Option<DateTime<Utc>> {
    value.parse().ok().filter(|&time| time > since)
}
