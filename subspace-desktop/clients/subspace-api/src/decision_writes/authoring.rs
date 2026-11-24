//! Versioned decisions written from the Decisions Space.

use serde::Serialize;
use serde_json::{Value, json};
use ureq::http::StatusCode;

use crate::{
    Backend, Client,
    model::{DecisionDetail, DecisionEvidenceKind},
    transport,
};

use super::{
    DecisionWriteError, EXCERPT_LIMIT, ISSUE_LIMIT, REFERENCE_LIMIT, TEXT_LIMIT, invalid_line,
    invalid_text,
};

const DATE_LENGTH: usize = "YYYY-MM-DD".len();

/// Fields editable on a proposed decision. An omitted field keeps its current
/// value. The replacement link can be kept, removed, or set to a different decision.
#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DecisionChanges {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decided_on: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub why: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub implications: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alternatives: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub issue_ids: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supersedes_id: Option<Option<String>>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DecisionAttribution {
    #[serde(skip_serializing_if = "Option::is_none")]
    made_by_type: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    made_by_user_id: Option<String>,
    evidence_kind: &'static str,
    evidence_reference: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    evidence_excerpt: Option<String>,
}

impl DecisionAttribution {
    /// Leave the maker to the server for one's own decision. An attributed
    /// decision identifies a human maker and includes their supporting evidence.
    pub fn new(
        human_maker: Option<String>,
        kind: crate::model::DecisionEvidenceKind,
        reference: String,
        excerpt: Option<String>,
    ) -> Result<Self, DecisionWriteError> {
        let evidence_kind = match kind {
            DecisionEvidenceKind::IrcMessage => "ircMessage",
            DecisionEvidenceKind::TeamsMessage => "teamsMessage",
            DecisionEvidenceKind::SubspaceComment => "subspaceComment",
            DecisionEvidenceKind::DocumentOrUrl => "documentOrUrl",
            DecisionEvidenceKind::Meeting => "meeting",
            DecisionEvidenceKind::Other => return Err(DecisionWriteError::Evidence),
        };
        if invalid_text(&reference, REFERENCE_LIMIT)
            || excerpt
                .as_deref()
                .is_some_and(|text| invalid_text(text, EXCERPT_LIMIT))
        {
            return Err(DecisionWriteError::Evidence);
        }
        Ok(Self {
            made_by_type: human_maker.as_ref().map(|_| "human"),
            made_by_user_id: human_maker,
            evidence_kind,
            evidence_reference: reference,
            evidence_excerpt: excerpt,
        })
    }
}

#[derive(Clone, Copy)]
pub enum DecisionReview {
    Accept,
    Reject,
}

impl DecisionReview {
    fn status(self) -> &'static str {
        match self {
            Self::Accept => "accepted",
            Self::Reject => "rejected",
        }
    }
}

impl DecisionChanges {
    fn body(&self) -> Result<Value, DecisionWriteError> {
        if self.decided_on.as_deref().is_some_and(|date| {
            date.len() != DATE_LENGTH
                || chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").is_err()
        }) {
            return Err(DecisionWriteError::Date);
        }
        if self.title.as_deref().is_some_and(invalid_line)
            || self
                .issue_ids
                .as_ref()
                .is_some_and(|issues| issues.len() > ISSUE_LIMIT)
        {
            return Err(DecisionWriteError::Invalid);
        }
        if self
            .decision
            .as_deref()
            .is_some_and(|text| invalid_text(text, TEXT_LIMIT))
            || [
                &self.context,
                &self.why,
                &self.implications,
                &self.alternatives,
            ]
            .into_iter()
            .flatten()
            .any(|text| text.contains('\0') || text.encode_utf16().count() > TEXT_LIMIT)
        {
            return Err(DecisionWriteError::Text);
        }
        serde_json::to_value(self).map_err(|_| DecisionWriteError::NotSent)
    }
}

// qual:allow(coupling, oi) reason: "The versioned write boundary encodes the API payload and decodes one response."
impl Client {
    pub fn propose_decision(
        &self,
        project: &str,
        changes: &DecisionChanges,
        attribution: Option<&DecisionAttribution>,
    ) -> Result<DecisionDetail, DecisionWriteError> {
        let mut body = changes.body()?;
        body["projectId"] = json!(project);
        if let Some(attribution) = attribution {
            let Value::Object(fields) =
                serde_json::to_value(attribution).map_err(|_| DecisionWriteError::NotSent)?
            else {
                return Err(DecisionWriteError::NotSent);
            };
            body.as_object_mut()
                .ok_or(DecisionWriteError::NotSent)?
                .extend(fields);
        }
        self.write_decision(&["decisions"], &body, StatusCode::CREATED)
    }

    pub fn edit_decision(
        &self,
        id: &str,
        version: u32,
        changes: &DecisionChanges,
    ) -> Result<DecisionDetail, DecisionWriteError> {
        let mut body = changes.body()?;
        body["version"] = json!(version);
        self.write_decision(&["decisions", id], &body, StatusCode::OK)
    }

    pub fn review_decision(
        &self,
        id: &str,
        version: u32,
        review: DecisionReview,
    ) -> Result<DecisionDetail, DecisionWriteError> {
        let body = json!({"version": version, "status": review.status()});
        self.write_decision(&["decisions", id, "status"], &body, StatusCode::OK)
    }

    fn write_decision(
        &self,
        path: &[&str],
        body: &Value,
        success: StatusCode,
    ) -> Result<DecisionDetail, DecisionWriteError> {
        if self.backend != Backend::Subspace {
            return Err(DecisionWriteError::Unsupported);
        }
        let url = self
            .endpoint(path, &[])
            .map_err(|_| DecisionWriteError::NotSent)?;
        let response = if success == StatusCode::CREATED {
            transport::post_json(self, &url, body, transport::READ_TIMEOUT)
        } else {
            transport::put_json(self, &url, body, transport::READ_TIMEOUT)
        }
        .map_err(|_| DecisionWriteError::Unconfirmed)?;
        super::decode_response(self, response, success, DecisionWriteError::Conflict)
    }
}
