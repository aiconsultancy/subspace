//! Recording a decision: one `POST /v1/decisions`, attempted once, as the
//! signed-in account. The server records that account as both the maker
//! and the recorder; this crate never sends maker provenance, so a person
//! can only record their own decision.
use serde::Serialize;
use ureq::http::StatusCode;

use crate::{Backend, Client, model::DecisionDetail, transport};

const TITLE_LIMIT: usize = 200;
const TEXT_LIMIT: usize = 50_000;
const ISSUE_LIMIT: usize = 100;
const REFERENCE_LIMIT: usize = 2_000;
const EXCERPT_LIMIT: usize = 4_000;

/// A decision to record. Built through [`NewDecision::new`], which checks
/// the API's limits before anything is sent.
#[derive(Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NewDecision {
    project_id: String,
    title: String,
    decided_on: String,
    decision: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    context: Option<String>,
    issue_ids: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    evidence_kind: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    evidence_reference: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    evidence_excerpt: Option<String>,
}

impl std::fmt::Debug for NewDecision {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("NewDecision([redacted])")
    }
}

/// What to record: the project and the issue ids are the API's UUIDs.
pub struct DecisionText<'a> {
    pub title: &'a str,
    pub decision: &'a str,
    pub context: Option<&'a str>,
    /// `YYYY-MM-DD`.
    pub decided_on: &'a str,
    /// The IRC message the decision was taken from, when it was.
    pub message: Option<IrcEvidence<'a>>,
}

/// Where in IRC a decision was taken: a reference back to the message,
/// such as its network, channel and message ID, and its words.
pub struct IrcEvidence<'a> {
    pub reference: &'a str,
    pub excerpt: &'a str,
}

impl NewDecision {
    pub fn new(
        project_id: &str,
        issue_ids: &[&str],
        text: &DecisionText<'_>,
    ) -> Result<Self, DecisionWriteError> {
        let title = text.title.trim();
        let decision = text.decision.trim();
        let context = text.context.map(str::trim).filter(|text| !text.is_empty());
        if invalid_line(title)
            || invalid_text(decision, TEXT_LIMIT)
            || context.is_some_and(|context| invalid_text(context, TEXT_LIMIT))
            || text.message.as_ref().is_some_and(invalid_evidence)
            || issue_ids.len() > ISSUE_LIMIT
            || chrono::NaiveDate::parse_from_str(text.decided_on, "%Y-%m-%d").is_err()
        {
            return Err(DecisionWriteError::Invalid);
        }
        let message = text.message.as_ref();
        Ok(Self {
            project_id: project_id.to_owned(),
            title: title.to_owned(),
            decided_on: text.decided_on.to_owned(),
            decision: decision.to_owned(),
            context: context.map(str::to_owned),
            issue_ids: issue_ids.iter().map(|id| (*id).to_owned()).collect(),
            evidence_kind: message.map(|_| "ircMessage"),
            evidence_reference: message.map(|message| message.reference.to_owned()),
            evidence_excerpt: message.map(|message| message.excerpt.to_owned()),
        })
    }

    pub fn title(&self) -> &str {
        &self.title
    }
}

/// Limits count UTF-16 units, as the API does.
fn invalid_line(text: &str) -> bool {
    text.is_empty()
        || text.contains(['\r', '\n', '\0'])
        || text.encode_utf16().count() > TITLE_LIMIT
}

fn invalid_text(text: &str, limit: usize) -> bool {
    text.trim().is_empty() || text.contains('\0') || text.encode_utf16().count() > limit
}

fn invalid_evidence(message: &IrcEvidence<'_>) -> bool {
    invalid_text(message.reference, REFERENCE_LIMIT) || invalid_text(message.excerpt, EXCERPT_LIMIT)
}

/// Why a decision was not recorded, in plain words.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum DecisionWriteError {
    #[error("Write a decision with a title of 1 to 200 characters on one line.")]
    Invalid,
    #[error("This tracker doesn't record decisions from here.")]
    Unsupported,
    #[error("Sign in under Settings › Account to record decisions.")]
    SignIn,
    #[error("You don't have permission to record a decision in this project.")]
    Forbidden,
    #[error("This project or issue is no longer available.")]
    Unavailable,
    #[error("This project is archived. Unarchive it before recording decisions.")]
    ProjectArchived,
    #[error("Subspace API refused this decision. Check the issue and the text.")]
    Refused,
    #[error("The service is busy. Wait before trying again.")]
    RateLimited,
    #[error("Couldn't send the decision. Try again when connected.")]
    NotSent,
    #[error(
        "Couldn't confirm the decision was recorded. Check the issue's decisions before trying again."
    )]
    Unconfirmed,
}

// qual:allow(coupling, oi) reason: "The HTTP boundary composes encoding, single-attempt transport and safe response classification."
impl Client {
    pub fn log_decision(
        &self,
        decision: &NewDecision,
    ) -> Result<DecisionDetail, DecisionWriteError> {
        if self.backend != Backend::Subspace {
            return Err(DecisionWriteError::Unsupported);
        }
        let url = self
            .endpoint(&["decisions"], &[])
            .map_err(|_| DecisionWriteError::NotSent)?;
        let body = serde_json::to_value(decision).map_err(|_| DecisionWriteError::NotSent)?;
        let response = transport::post_json(self, &url, &body, transport::READ_TIMEOUT)
            .map_err(|_| DecisionWriteError::Unconfirmed)?;
        decision_response(self, response)
    }
}

fn decision_response(
    client: &Client,
    response: ureq::http::Response<ureq::Body>,
) -> Result<DecisionDetail, DecisionWriteError> {
    let status = response.status();
    if transport::is_access_challenge(&response) || status == StatusCode::UNAUTHORIZED {
        return Err(DecisionWriteError::SignIn);
    }
    if status == StatusCode::CREATED {
        return client
            .read(response)
            .map_err(|_| DecisionWriteError::Unconfirmed);
    }
    Err(match status.as_u16() {
        400 | 422 => DecisionWriteError::Refused,
        403 => DecisionWriteError::Forbidden,
        404 => DecisionWriteError::Unavailable,
        409 => DecisionWriteError::ProjectArchived,
        429 => DecisionWriteError::RateLimited,
        _ => DecisionWriteError::Unconfirmed,
    })
}
