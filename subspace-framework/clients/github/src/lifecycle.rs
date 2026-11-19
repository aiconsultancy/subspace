//! Bounded reads of recent closed pull requests and deployment status history.

use chrono::{DateTime, Utc};
use serde::{Deserialize, de::DeserializeOwned};
use subspace_core::http::Error;

use crate::Client;
use crate::actions::{Deployment, DeploymentStatus};
use crate::validate::{check_login, check_repo, check_sha};

/// Default cadence for full background reads of PRs, Actions and deployments.
// qual:api — Used by Subspace Desktop through the Framework facade.
pub const REFRESH_INTERVAL: std::time::Duration = std::time::Duration::from_secs(10 * 60);

// qual:allow(srp, god_struct) reason: "Each reader exposes one field of the GitHub pull request response."
#[derive(Debug, Deserialize)]
pub struct ClosedPull {
    number: u32,
    merged_at: Option<DateTime<Utc>>,
    merge_commit_sha: Option<String>,
    base: Option<PullBase>,
    body: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PullBase {
    repo: Option<PullRepository>,
}

#[derive(Debug, Deserialize)]
struct PullRepository {
    full_name: String,
}

impl ClosedPull {
    // qual:api — Called from the separate Subspace Desktop workspace.
    pub fn number(&self) -> u32 {
        self.number
    }

    // qual:api — Called from the separate Subspace Desktop workspace.
    pub fn merged_at(&self) -> Option<DateTime<Utc>> {
        self.merged_at
    }

    // qual:api — Called from the separate Subspace Desktop workspace.
    pub fn merge_commit_sha(&self) -> Option<&str> {
        self.merge_commit_sha.as_deref()
    }

    // qual:api — Called from the separate Subspace Desktop workspace.
    pub fn body(&self) -> Option<&str> {
        self.body.as_deref()
    }

    /// Whether GitHub reports the pull request merged into this repository.
    // qual:api — Called from the separate Subspace Desktop workspace.
    pub fn is_merged_into(&self, repo: &str) -> bool {
        self.merged_at.is_some()
            && self
                .base
                .as_ref()
                .and_then(|base| base.repo.as_ref())
                .is_some_and(|base| base.full_name.eq_ignore_ascii_case(repo))
    }
}

// qual:allow(coupling, oi) reason: "These repository reads reuse the GitHub client transport and credentials."
impl Client {
    /// The 100 most recently updated closed pull requests.
    // qual:api — Called from the separate Subspace Desktop workspace.
    pub fn recent_closed_pulls(&self, login: &str, repo: &str) -> Result<Vec<ClosedPull>, Error> {
        self.lifecycle_page(
            login,
            repo,
            &["pulls"],
            &[
                ("state", "closed"),
                ("sort", "updated"),
                ("direction", "desc"),
            ],
        )
    }

    /// Pull requests GitHub associates with the deployment commit.
    // qual:api — Called from the separate Subspace Desktop workspace.
    pub fn commit_pulls(
        &self,
        login: &str,
        repo: &str,
        sha: &str,
    ) -> Result<Vec<ClosedPull>, Error> {
        let sha = check_sha(sha)?;
        self.lifecycle_page(login, repo, &["commits", sha, "pulls"], &[])
    }

    /// The 100 newest deployment records, without reading each one's history.
    // qual:api — Called from the separate Subspace Desktop workspace.
    pub fn recent_deployments(&self, login: &str, repo: &str) -> Result<Vec<Deployment>, Error> {
        self.lifecycle_page(login, repo, &["deployments"], &[])
    }

    /// The newest 100 status records for one deployment.
    // qual:api — Called from the separate Subspace Desktop workspace.
    pub fn deployment_history(
        &self,
        login: &str,
        repo: &str,
        id: u64,
    ) -> Result<Vec<DeploymentStatus>, Error> {
        self.lifecycle_page(
            login,
            repo,
            &["deployments", &id.to_string(), "statuses"],
            &[],
        )
    }

    fn lifecycle_page<T: DeserializeOwned>(
        &self,
        login: &str,
        repo: &str,
        path: &[&str],
        query: &[(&str, &str)],
    ) -> Result<Vec<T>, Error> {
        check_login(login)?;
        let (owner, name) = check_repo(repo)?;
        let segments = ["repos", owner.as_str(), name.as_str()]
            .into_iter()
            .chain(path.iter().copied());
        let mut query = query.to_vec();
        query.push(("per_page", "100"));
        let url = self.rest_url(segments, &query)?;
        self.rest_json(login, &url)
    }
}
