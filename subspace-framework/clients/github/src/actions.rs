//! GitHub Actions, deployments and releases REST client.
//!
//! Calls are synchronous and bounded. Dashboard callers run them off the UI
//! thread. Workflow YAML parsing is pure and reports unsupported or malformed
//! definitions as [`Definition::Unreadable`].

mod client;
mod definition;
mod model;
mod request;
mod workflow;

pub use definition::{match_job_dependencies, parse_workflow_definition};
pub use model::*;

/// GitHub Actions operations using the shared client's credentials and transport.
pub trait ActionsApi {
    fn actions_workflows(
        &self,
        login: &str,
        repo: &str,
    ) -> Result<Page<Workflow>, subspace_core::http::Error>;
    fn actions_runs(
        &self,
        login: &str,
        repo: &str,
        filters: &RunFilters,
    ) -> Result<Page<WorkflowRun>, subspace_core::http::Error>;
    fn actions_run(
        &self,
        login: &str,
        repo: &str,
        run_id: u64,
    ) -> Result<WorkflowRun, subspace_core::http::Error>;
    fn actions_jobs(
        &self,
        login: &str,
        repo: &str,
        run_id: u64,
    ) -> Result<Page<Job>, subspace_core::http::Error>;
    /// The jobs of attempt `attempt` of run `run_id`. A re-run is a new
    /// attempt with jobs of its own, so a caller that read one attempt's run
    /// asks for that attempt's jobs, not whichever attempt is newest by the
    /// time the read lands. Attempt 0, which a run without `run_attempt`
    /// maps to, reads the run's latest jobs.
    ///
    /// The default, for an implementation that cannot read one attempt,
    /// reads the latest jobs for attempt 0 and fails with HTTP 501 (Not
    /// Implemented) for any other, rather than answer for one attempt with
    /// another's jobs.
    fn actions_attempt_jobs(
        &self,
        login: &str,
        repo: &str,
        run_id: u64,
        attempt: u64,
    ) -> Result<Page<Job>, subspace_core::http::Error> {
        match attempt {
            0 => self.actions_jobs(login, repo, run_id),
            _ => Err(subspace_core::http::Error::Status {
                status: ureq::http::StatusCode::NOT_IMPLEMENTED.as_u16(),
                message: "This GitHub client cannot read the jobs of one run attempt.".into(),
            }),
        }
    }
    fn actions_job_log(
        &self,
        login: &str,
        repo: &str,
        job_id: u64,
    ) -> Result<JobLog, subspace_core::http::Error>;
    fn actions_workflow_definition(
        &self,
        login: &str,
        repo: &str,
        sha: &str,
        path: &str,
    ) -> Result<Definition<WorkflowDefinition>, subspace_core::http::Error>;
    fn actions_pending_deployments(
        &self,
        login: &str,
        repo: &str,
        run_id: u64,
    ) -> Result<Vec<PendingDeployment>, subspace_core::http::Error>;
    fn actions_environments(
        &self,
        login: &str,
        repo: &str,
    ) -> Result<Page<Environment>, subspace_core::http::Error>;
    fn actions_deployments(
        &self,
        login: &str,
        repo: &str,
        environment: Option<&str>,
    ) -> Result<Page<Deployment>, subspace_core::http::Error>;
    fn actions_latest_deployment(
        &self,
        login: &str,
        repo: &str,
        environment: &str,
    ) -> Result<Option<Deployment>, subspace_core::http::Error> {
        self.actions_deployments(login, repo, Some(environment))
            .map(|page| page.items.into_iter().next())
    }
    fn actions_releases(
        &self,
        login: &str,
        repo: &str,
    ) -> Result<Page<Release>, subspace_core::http::Error>;
    fn actions_tags(
        &self,
        login: &str,
        repo: &str,
    ) -> Result<Page<TagRef>, subspace_core::http::Error>;
    fn dispatch_workflow(
        &self,
        login: &str,
        request: &DispatchRequest<'_>,
    ) -> Result<(), subspace_core::http::Error>;
    fn rerun_workflow(
        &self,
        login: &str,
        repo: &str,
        run_id: u64,
    ) -> Result<(), subspace_core::http::Error>;
    fn rerun_failed_jobs(
        &self,
        login: &str,
        repo: &str,
        run_id: u64,
    ) -> Result<(), subspace_core::http::Error>;
    fn cancel_workflow(
        &self,
        login: &str,
        repo: &str,
        run_id: u64,
    ) -> Result<(), subspace_core::http::Error>;
    fn review_pending_deployments(
        &self,
        login: &str,
        request: &ReviewPendingRequest<'_>,
    ) -> Result<(), subspace_core::http::Error>;
}

#[cfg(test)]
mod tests;
