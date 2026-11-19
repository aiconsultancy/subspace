//! One-shot creation. Browser and clipboard actions belong to the host.

use serde::Deserialize;
use serde_json::{Value, json};
use std::time::Instant;
use subspace_core::credentials::Credential;
use subspace_core::http::{TIMEOUT, status};

use super::{BrowserFallback, Error, ErrorKind, IssueRepository, PreparedReport, http};
use crate::Client;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportKind {
    Authenticated,
    Browser,
}

/// Owns the reviewed snapshot and captured credential. Intentionally not Clone.
pub struct ReportPlan {
    report: PreparedReport,
    credential: Option<Credential>,
    fallback: BrowserFallback,
}

impl ReportPlan {
    pub fn report(&self) -> &PreparedReport {
        &self.report
    }
    pub fn transport(&self) -> TransportKind {
        if self.credential.is_some() {
            TransportKind::Authenticated
        } else {
            TransportKind::Browser
        }
    }
    pub fn browser_fallback(&self) -> &BrowserFallback {
        &self.fallback
    }
    /// Effective body for the selected transport, including diagnostic trimming.
    pub fn body(&self) -> &str {
        if self.credential.is_some() {
            self.report.body()
        } else {
            self.fallback.body()
        }
    }
    pub fn title(&self) -> &str {
        self.report.title()
    }
}

#[derive(Debug)]
pub struct CreatedIssue {
    url: String,
    number: u64,
    applied_labels: Vec<String>,
    skipped_labels: Vec<String>,
}

impl CreatedIssue {
    // qual:allow(dry, boilerplate) reason: "Only a verified created-issue URL can leave the client for browser use."
    pub fn url(&self) -> &str {
        &self.url
    }
    // qual:allow(dry, boilerplate) reason: "The positive number stays bound to its verified repository URL."
    pub fn number(&self) -> u64 {
        self.number
    }
    // qual:api
    // qual:allow(dry, boilerplate) reason: "Label results are readonly post-creation evidence, not a mutable retry request."
    pub fn applied_labels(&self) -> &[String] {
        &self.applied_labels
    }
    // qual:api
    // qual:allow(dry, boilerplate) reason: "The host can explain omitted labels while retaining the confirmed issue link."
    pub fn skipped_labels(&self) -> &[String] {
        &self.skipped_labels
    }
}

#[derive(Debug)]
pub enum Outcome {
    Created(CreatedIssue),
    /// An editor still needs to be opened, and the user still needs to submit.
    Browser(BrowserFallback),
}

// qual:allow(coupling, oi) reason: "This child module keeps the headless reporting methods on the existing credential-owning GitHub client."
impl Client {
    /// Resolve credentials in the existing priority, using gh's active account.
    /// Performs no HTTP request and never switches or refreshes an account.
    pub fn prepare_issue_report(&self, report: PreparedReport) -> ReportPlan {
        let fallback = report.browser_fallback();
        ReportPlan {
            report,
            credential: self.credential(None).ok(),
            fallback,
        }
    }

    /// Consume the review plan. No automatic retry, including parse failures.
    pub fn submit_issue_report(&self, plan: ReportPlan) -> Result<Outcome, Error> {
        let Some(credential) = plan.credential else {
            return Ok(Outcome::Browser(plan.fallback));
        };
        let deadline = Instant::now() + TIMEOUT;
        let mut created = create(self, &credential, &plan.report, deadline)?;
        let repository = plan.report.repository();
        apply_labels((self, &credential), repository, &mut created, deadline);
        Ok(Outcome::Created(created))
    }
}

fn create(
    client: &Client,
    credential: &Credential,
    report: &PreparedReport,
    deadline: Instant,
) -> Result<CreatedIssue, Error> {
    let repository = report.repository();
    let path = format!("/repos/{}/issues", repository.as_str());
    // Labels are a separate best-effort operation: they cannot reject creation.
    let body = json!({"title":report.title(), "body":report.body()});
    let response = http::send(client, credential, &path, Some(&body), deadline)
        .map_err(|()| Error::from(ErrorKind::UncertainCreation))?;
    if response.status != ureq::http::StatusCode::CREATED.as_u16() {
        return Err(creation_failure(&response).into());
    }
    let mut created = parse_created(&response.body, repository)?;
    created.skipped_labels = report.requested_labels().into();
    Ok(created)
}

fn parse_created(bytes: &[u8], repository: &IssueRepository) -> Result<CreatedIssue, Error> {
    #[derive(Deserialize)]
    struct Created {
        number: u64,
        html_url: String,
    }
    let response: Created =
        serde_json::from_slice(bytes).map_err(|_| Error::from(ErrorKind::UncertainCreation))?;
    if !repository.is_created_issue_url(&response.html_url)
        || !response
            .html_url
            .ends_with(&format!("/{}", response.number))
    {
        return Err(ErrorKind::UncertainCreation.into());
    }
    Ok(CreatedIssue {
        url: response.html_url,
        number: response.number,
        applied_labels: Vec::new(),
        skipped_labels: Vec::new(),
    })
}

fn creation_failure(response: &http::Response) -> ErrorKind {
    if response.rate_limited {
        return ErrorKind::RateLimited;
    }
    match response.status {
        status::UNAUTHORIZED => ErrorKind::Authentication,
        status::FORBIDDEN => ErrorKind::AccessDenied,
        status::NOT_FOUND => ErrorKind::RepositoryUnavailable,
        status::BAD_REQUEST | 422 => ErrorKind::Rejected,
        _ => ErrorKind::UncertainCreation,
    }
}

fn apply_labels(
    (client, credential): (&Client, &Credential),
    repository: &IssueRepository,
    created: &mut CreatedIssue,
    deadline: Instant,
) {
    let available: Vec<String> = created
        .skipped_labels
        .iter()
        .filter(|label| label_exists((client, credential), repository, label, deadline))
        .cloned()
        .collect();
    if available.is_empty() {
        return;
    }
    let path = format!(
        "/repos/{}/issues/{}/labels",
        repository.as_str(),
        created.number
    );
    let body = json!({"labels":available});
    let Ok(response) = http::send(client, credential, &path, Some(&body), deadline) else {
        return;
    };
    // A success must confirm each applied label; an uncertain label response
    // leaves a usable issue link and never triggers another create request.
    let confirmed = confirmed_labels(&response);
    created
        .skipped_labels
        .retain(|label| !confirmed.contains(label));
    created.applied_labels = available
        .into_iter()
        .filter(|label| confirmed.contains(label))
        .collect();
}

fn label_exists(
    (client, credential): (&Client, &Credential),
    repository: &IssueRepository,
    label: &str,
    deadline: Instant,
) -> bool {
    let encoded: String = url::form_urlencoded::byte_serialize(label.as_bytes()).collect();
    let path = format!("/repos/{}/labels/{encoded}", repository.as_str());
    let Ok(response) = http::send(client, credential, &path, None, deadline) else {
        return false;
    };
    response.status == ureq::http::StatusCode::OK.as_u16()
        && serde_json::from_slice::<Value>(&response.body)
            .is_ok_and(|value| value["name"].as_str() == Some(label))
}

fn confirmed_labels(response: &http::Response) -> Vec<String> {
    if !status::is_success(response.status) {
        return Vec::new();
    }
    serde_json::from_slice::<Vec<Value>>(&response.body)
        .unwrap_or_default()
        .iter()
        .filter_map(|value| value["name"].as_str().map(str::to_owned))
        .collect()
}
