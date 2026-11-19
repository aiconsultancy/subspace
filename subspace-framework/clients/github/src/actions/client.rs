use serde_json::json;
use subspace_core::http::Error;
use subspace_core::http::status as http_status;

use crate::Client;
use crate::transport::{self, unexpected};

use super::request::{self, PageRequest, PostRequest};
use super::{
    ActionsApi, Definition, Deployment, DispatchBody, DispatchRequest, Environment, Job, JobLog,
    Page, PendingDeployment, Release, ReviewPendingRequest, ReviewState, RunFilters, TagRef,
    Workflow, WorkflowDefinition, WorkflowRun,
};

const MAX_LIST_PAGES: usize = 3;

impl ActionsApi for Client {
    fn actions_workflows(&self, login: &str, repo: &str) -> Result<Page<Workflow>, Error> {
        request::page(
            self,
            login,
            &PageRequest {
                repo,
                endpoint: "actions/workflows",
                key: "workflows",
                query: &[],
                max_pages: MAX_LIST_PAGES,
            },
        )
    }

    fn actions_runs(
        &self,
        login: &str,
        repo: &str,
        filters: &RunFilters,
    ) -> Result<Page<WorkflowRun>, Error> {
        let endpoint = filters.workflow.as_deref().map_or_else(
            || "actions/runs".to_owned(),
            |workflow| format!("actions/workflows/{workflow}/runs"),
        );
        let query = [
            ("branch", filters.branch.as_deref()),
            ("status", filters.status.as_deref()),
            ("actor", filters.actor.as_deref()),
        ];
        let query: Vec<_> = query
            .into_iter()
            .filter_map(|(key, value)| value.map(|value| (key, value)))
            .collect();
        request::page(
            self,
            login,
            &PageRequest {
                repo,
                endpoint: &endpoint,
                key: "workflow_runs",
                query: &query,
                max_pages: MAX_LIST_PAGES,
            },
        )
    }

    fn actions_run(&self, login: &str, repo: &str, run_id: u64) -> Result<WorkflowRun, Error> {
        request::json(self, login, repo, &format!("actions/runs/{run_id}"), &[])
    }

    fn actions_jobs(&self, login: &str, repo: &str, run_id: u64) -> Result<Page<Job>, Error> {
        let endpoint = format!("actions/runs/{run_id}/jobs");
        request::page(
            self,
            login,
            &PageRequest {
                repo,
                endpoint: &endpoint,
                key: "jobs",
                query: &[("filter", "latest")],
                max_pages: MAX_LIST_PAGES,
            },
        )
    }

    fn actions_job_log(&self, login: &str, repo: &str, job_id: u64) -> Result<JobLog, Error> {
        request::job_log(self, login, repo, job_id)
    }

    fn actions_workflow_definition(
        &self,
        login: &str,
        repo: &str,
        sha: &str,
        path: &str,
    ) -> Result<Definition<WorkflowDefinition>, Error> {
        super::workflow::definition(self, login, repo, sha, path)
    }

    fn actions_pending_deployments(
        &self,
        login: &str,
        repo: &str,
        run_id: u64,
    ) -> Result<Vec<PendingDeployment>, Error> {
        request::json(
            self,
            login,
            repo,
            &format!("actions/runs/{run_id}/pending_deployments"),
            &[],
        )
    }

    fn actions_environments(&self, login: &str, repo: &str) -> Result<Page<Environment>, Error> {
        request::page(
            self,
            login,
            &PageRequest {
                repo,
                endpoint: "environments",
                key: "environments",
                query: &[],
                max_pages: MAX_LIST_PAGES,
            },
        )
    }

    fn actions_deployments(
        &self,
        login: &str,
        repo: &str,
        environment: Option<&str>,
    ) -> Result<Page<Deployment>, Error> {
        let query: Vec<_> = environment
            .map(|value| vec![("environment", value)])
            .unwrap_or_default();
        let mut page: Page<Deployment> = request::page(
            self,
            login,
            &PageRequest {
                repo,
                endpoint: "deployments",
                key: "",
                query: &query,
                max_pages: 1,
            },
        )?;
        for deployment in &mut page.items {
            deployment.latest_status =
                request::deployment_status(self, login, repo, deployment.id)?;
        }
        Ok(page)
    }

    fn actions_latest_deployment(
        &self,
        login: &str,
        repo: &str,
        environment: &str,
    ) -> Result<Option<Deployment>, Error> {
        let page: Page<Deployment> = request::page(
            self,
            login,
            &PageRequest {
                repo,
                endpoint: "deployments",
                key: "",
                query: &[("environment", environment), ("per_page", "1")],
                max_pages: 1,
            },
        )?;
        let Some(mut deployment) = page.items.into_iter().next() else {
            return Ok(None);
        };
        deployment.latest_status = request::deployment_status(self, login, repo, deployment.id)?;
        Ok(Some(deployment))
    }

    fn actions_releases(&self, login: &str, repo: &str) -> Result<Page<Release>, Error> {
        request::page(
            self,
            login,
            &PageRequest {
                repo,
                endpoint: "releases",
                key: "",
                query: &[],
                max_pages: 1,
            },
        )
    }

    fn actions_tags(&self, login: &str, repo: &str) -> Result<Page<TagRef>, Error> {
        request::page(
            self,
            login,
            &PageRequest {
                repo,
                endpoint: "tags",
                key: "",
                query: &[],
                max_pages: 1,
            },
        )
    }

    fn dispatch_workflow(&self, login: &str, request: &DispatchRequest<'_>) -> Result<(), Error> {
        if request.workflow.trim().is_empty() || request.reference.trim().is_empty() {
            return Err(transport::status(
                http_status::BAD_REQUEST,
                "Workflow and ref are required.",
            ));
        }
        let body = serde_json::to_string(&DispatchBody {
            reference: request.reference,
            inputs: request.inputs,
        })
        .map_err(|_| unexpected("workflow dispatch request"))?;
        request::post(
            self,
            login,
            &PostRequest {
                repo: request.repo,
                endpoint: &format!("actions/workflows/{}/dispatches", request.workflow),
                body: &body,
                scope: "Actions: write or workflow",
            },
        )
    }

    fn rerun_workflow(&self, login: &str, repo: &str, run_id: u64) -> Result<(), Error> {
        request::post(
            self,
            login,
            &PostRequest {
                repo,
                endpoint: &format!("actions/runs/{run_id}/rerun"),
                body: "{}",
                scope: "Actions: write or workflow",
            },
        )
    }

    fn rerun_failed_jobs(&self, login: &str, repo: &str, run_id: u64) -> Result<(), Error> {
        request::post(
            self,
            login,
            &PostRequest {
                repo,
                endpoint: &format!("actions/runs/{run_id}/rerun-failed-jobs"),
                body: "{}",
                scope: "Actions: write or workflow",
            },
        )
    }

    fn cancel_workflow(&self, login: &str, repo: &str, run_id: u64) -> Result<(), Error> {
        request::post(
            self,
            login,
            &PostRequest {
                repo,
                endpoint: &format!("actions/runs/{run_id}/cancel"),
                body: "{}",
                scope: "Actions: write or workflow",
            },
        )
    }

    fn review_pending_deployments(
        &self,
        login: &str,
        request: &ReviewPendingRequest<'_>,
    ) -> Result<(), Error> {
        if request.environment_ids.is_empty() {
            return Err(transport::status(
                http_status::BAD_REQUEST,
                "At least one environment is required.",
            ));
        }
        if request.state == ReviewState::Rejected && request.comment.trim().is_empty() {
            return Err(transport::status(
                http_status::BAD_REQUEST,
                "A rejection comment is required.",
            ));
        }
        let body = json!({
            "environment_ids": request.environment_ids,
            "state": request.state,
            "comment": request.comment,
        })
        .to_string();
        request::post(
            self,
            login,
            &PostRequest {
                repo: request.repo,
                endpoint: &format!("actions/runs/{}/pending_deployments", request.run_id),
                body: &body,
                scope: "Deployments: write and environment reviewer access",
            },
        )
    }
}
