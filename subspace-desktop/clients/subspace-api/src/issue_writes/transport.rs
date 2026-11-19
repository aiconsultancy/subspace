use super::{IssueChanges, IssueSave, IssueSaveError, SaveTarget};
use crate::{Client, Error, model::IssueDetail, transport};
use serde_json::{Value, json};
use ureq::http::StatusCode;

// qual:allow(coupling, oi) reason: "The HTTP boundary composes validation, encoding, pacing and single-attempt transport."
impl Client {
    pub fn create_issue(
        &self,
        project: &str,
        changes: &IssueChanges,
    ) -> Result<IssueSave, IssueSaveError> {
        self.write_issue(SaveTarget::Create { project }, changes)
    }

    pub fn update_issue(
        &self,
        issue: &str,
        version: u32,
        changes: &IssueChanges,
    ) -> Result<IssueSave, IssueSaveError> {
        self.write_issue(SaveTarget::Update { issue, version }, changes)
    }

    pub(super) fn write_issue(
        &self,
        target: SaveTarget<'_>,
        changes: &IssueChanges,
    ) -> Result<IssueSave, IssueSaveError> {
        changes.validate(matches!(target, SaveTarget::Create { .. }))?;
        if changes.is_tracker(false) && changes.tracker_properties() {
            return Err(IssueSaveError::Invalid);
        }
        let (reference, body) = encode(target, changes)?;
        let mut path = vec!["issues"];
        path.extend(reference);
        let url =
            transport::endpoint(&self.base, &path, &[]).map_err(|_| IssueSaveError::NotSent)?;
        transport::prepare(&url);
        let mut request = if reference.is_none() {
            self.agent.post(url.as_str())
        } else {
            self.agent.patch(url.as_str())
        }
        .header("User-Agent", transport::USER_AGENT)
        .header("Accept", "application/json")
        .header("Content-Type", "application/json");
        if let Some(jwt) = &self.access_jwt {
            request = request.header("Authorization", format!("Bearer {}", jwt.expose()));
        }
        let response = request
            .config()
            .timeout_global(Some(transport::READ_TIMEOUT))
            .max_redirects(0)
            .max_redirects_will_error(false)
            .build()
            .send(body.to_string().as_bytes())
            .map_err(|_| IssueSaveError::Unconfirmed)?;
        decode(response, reference)
    }
}

fn encode<'a>(
    target: SaveTarget<'a>,
    changes: &IssueChanges,
) -> Result<(Option<&'a str>, Value), IssueSaveError> {
    let mut body = serde_json::to_value(changes).map_err(|_| IssueSaveError::NotSent)?;
    let reference = match target {
        SaveTarget::Create { project } => {
            body["projectId"] = json!(project);
            None
        }
        SaveTarget::Update { issue, version } => {
            body["version"] = json!(version);
            Some(issue)
        }
    };
    Ok((reference, body))
}

fn decode(
    response: ureq::http::Response<ureq::Body>,
    reference: Option<&str>,
) -> Result<IssueSave, IssueSaveError> {
    let status = response.status();
    if transport::is_access_challenge(&response) || status == StatusCode::UNAUTHORIZED {
        return Err(IssueSaveError::SignIn);
    }
    let expected = if reference.is_none() {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    if status == expected {
        let detail: IssueDetail =
            transport::read_json(response).map_err(|_| IssueSaveError::Unconfirmed)?;
        return saved(detail, reference);
    }
    Err(match status.as_u16() {
        400 | 422 => IssueSaveError::Invalid,
        403 => IssueSaveError::Forbidden,
        404 => IssueSaveError::Unavailable,
        409 if reference.is_none() => IssueSaveError::ProjectArchived,
        409 => IssueSaveError::Conflict,
        429 => IssueSaveError::RateLimited,
        _ => IssueSaveError::Unconfirmed,
    })
}

fn saved(detail: IssueDetail, reference: Option<&str>) -> Result<IssueSave, IssueSaveError> {
    let fields = &detail.fields;
    if fields.id.is_empty()
        || fields.identifier.is_empty()
        || fields.version <= 0
        || reference.is_some_and(|r| r != fields.id && r != fields.identifier)
    {
        return Err(IssueSaveError::Unconfirmed);
    }
    Ok(IssueSave::Saved(Box::new(detail)))
}

impl From<Error> for IssueSaveError {
    fn from(error: Error) -> Self {
        match error {
            Error::SignInNeeded => Self::SignIn,
            Error::Forbidden => Self::Forbidden,
            Error::NotFound => Self::Unavailable,
            _ => Self::NotSent,
        }
    }
}
