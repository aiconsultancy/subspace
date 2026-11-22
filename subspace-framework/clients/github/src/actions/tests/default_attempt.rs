//! The trait's own attempt read, for an implementation that only reads a
//! run's latest jobs.

use crate::actions::{
    ActionsApi, Definition, Deployment, DispatchRequest, Environment, Job, JobLog, Page,
    PendingDeployment, Release, ReviewPendingRequest, RunFilters, TagRef, Workflow,
    WorkflowDefinition, WorkflowRun,
};
use serde_json::json;
use subspace_core::http::Error;

/// Reads run 20's latest jobs and nothing else.
struct LatestOnly;

fn unused<T>() -> Result<T, Error> {
    Err(Error::Network("not used by this test".into()))
}

impl ActionsApi for LatestOnly {
    fn actions_workflows(&self, _: &str, _: &str) -> Result<Page<Workflow>, Error> {
        unused()
    }
    fn actions_runs(&self, _: &str, _: &str, _: &RunFilters) -> Result<Page<WorkflowRun>, Error> {
        unused()
    }
    fn actions_run(&self, _: &str, _: &str, _: u64) -> Result<WorkflowRun, Error> {
        unused()
    }
    fn actions_jobs(&self, _: &str, _: &str, run_id: u64) -> Result<Page<Job>, Error> {
        let job = json!({"id":32,"run_id":run_id,"run_attempt":2,"name":"Build"});
        let job: Job = serde_json::from_value(job).map_err(|_| Error::Network("fixture".into()))?;
        Ok(Page {
            items: vec![job],
            truncated: false,
        })
    }
    fn actions_job_log(&self, _: &str, _: &str, _: u64) -> Result<JobLog, Error> {
        unused()
    }
    fn actions_workflow_definition(
        &self,
        _: &str,
        _: &str,
        _: &str,
        _: &str,
    ) -> Result<Definition<WorkflowDefinition>, Error> {
        unused()
    }
    fn actions_pending_deployments(
        &self,
        _: &str,
        _: &str,
        _: u64,
    ) -> Result<Vec<PendingDeployment>, Error> {
        unused()
    }
    fn actions_environments(&self, _: &str, _: &str) -> Result<Page<Environment>, Error> {
        unused()
    }
    fn actions_deployments(
        &self,
        _: &str,
        _: &str,
        _: Option<&str>,
    ) -> Result<Page<Deployment>, Error> {
        unused()
    }
    fn actions_releases(&self, _: &str, _: &str) -> Result<Page<Release>, Error> {
        unused()
    }
    fn actions_tags(&self, _: &str, _: &str) -> Result<Page<TagRef>, Error> {
        unused()
    }
    fn dispatch_workflow(&self, _: &str, _: &DispatchRequest<'_>) -> Result<(), Error> {
        unused()
    }
    fn rerun_workflow(&self, _: &str, _: &str, _: u64) -> Result<(), Error> {
        unused()
    }
    fn rerun_failed_jobs(&self, _: &str, _: &str, _: u64) -> Result<(), Error> {
        unused()
    }
    fn cancel_workflow(&self, _: &str, _: &str, _: u64) -> Result<(), Error> {
        unused()
    }
    fn review_pending_deployments(
        &self,
        _: &str,
        _: &ReviewPendingRequest<'_>,
    ) -> Result<(), Error> {
        unused()
    }
}

#[test]
fn the_default_attempt_read_gives_the_latest_jobs_only_for_attempt_zero() {
    let latest = LatestOnly
        .actions_attempt_jobs("ann", "o/r", 20, 0)
        .unwrap();
    assert_eq!(latest.items[0].id, 32);
    let error = LatestOnly
        .actions_attempt_jobs("ann", "o/r", 20, 1)
        .unwrap_err();
    assert!(
        matches!(error, Error::Status { status: 501, .. }),
        "attempt 1 is not answered with the latest attempt's jobs: {error}"
    );
}
