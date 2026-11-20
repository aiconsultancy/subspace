//! Versioned project moves: preview first, then one explicitly confirmed write.
use crate::issue_writes::IssueSaveError;
use crate::{Client, model::IssueDetail, transport};
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::{Value, json};
mod reconcile;
pub use reconcile::{Unreconciled, UnreconciledReason};

const MAX_MOVE_ISSUES: usize = 500;
const TOKEN_HEX_LEN: usize = 64;

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct MoveProject {
    pub id: String,
    pub key: String,
    pub name: String,
    #[serde(default)]
    pub archived: bool,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct MoveItem {
    pub id: String,
    pub version: u32,
    pub old_identifier: String,
    /// The number the issue receives. NETT allocates it when the move is
    /// confirmed, so a NETT preview has none.
    pub new_identifier: Option<String>,
    pub state_id: String,
    pub state_name: String,
    pub cycle_cleared: bool,
    pub milestone_cleared: bool,
    pub estimate_cleared: bool,
    #[serde(default)]
    pub estimate_after: Option<f64>,
    pub parent_detached: bool,
    pub archived: bool,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct MovePreview {
    pub token: String,
    pub source: MoveProject,
    pub destination: MoveProject,
    pub issues: Vec<MoveItem>,
    /// NETT's snapshot of the root's review context, sent back unchanged
    /// with the confirmation.
    #[serde(default)]
    pub(crate) review_context: Option<Value>,
}

/// Whether `identifier` is a number in the team keyed `key`.
fn preview_number(identifier: &str, key: &str) -> bool {
    identifier.rsplit_once('-').is_some_and(|(prefix, number)| {
        prefix == key && !number.is_empty() && number.bytes().all(|b| b.is_ascii_digit())
    })
}

impl MovePreview {
    /// Whether the preview moves an archived sub-issue, which NETT refuses.
    pub fn moves_archived_sub_issue(&self) -> bool {
        self.issues.iter().skip(1).any(|item| item.archived)
    }

    /// Whether `identifier` is the number the root issue has after the move:
    /// the previewed number, or for NETT, which numbers on confirmation,
    /// any number in the destination.
    pub fn root_moved_to(&self, identifier: &str) -> bool {
        match self
            .issues
            .first()
            .and_then(|root| root.new_identifier.as_deref())
        {
            Some(expected) => expected == identifier,
            None => identifier.rsplit_once('-').is_some_and(|(key, number)| {
                key == self.destination.key
                    && !number.is_empty()
                    && number.bytes().all(|b| b.is_ascii_digit())
            }),
        }
    }

    /// Whether this preview answers a request to move `id`, at `version`,
    /// to `destination`: a bounded plan with a full token, rooted at that
    /// issue and version.
    fn answers(&self, id: &str, destination: &str, version: u32) -> bool {
        self.issues.len() <= MAX_MOVE_ISSUES
            && self.token.len() == TOKEN_HEX_LEN
            && self.issues.first().is_some_and(|root| {
                (root.id == id || root.old_identifier == id) && root.version == version
            })
            && self.destination.id == destination
    }

    /// Whether the move can be confirmed as previewed: NETT needs the
    /// review context back, and Subspace API previews every new number.
    fn confirmable(&self, nett: bool) -> bool {
        if nett {
            self.review_context.is_some()
        } else {
            self.issues.iter().all(|item| item.new_identifier.is_some())
        }
    }
}
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct MoveResult {
    pub issue: IssueDetail,
    /// The previewed issues read back in the destination, with their numbers
    /// there, the root first.
    pub moved: Vec<MoveItem>,
    /// Previewed sub-issues that a NETT move which committed could not
    /// confirm in the destination when they were read back. The move itself
    /// succeeded; the Issues Space lists these for the person to check.
    /// Worked out here, never read from a reply.
    #[serde(skip)]
    pub unreconciled: Vec<Unreconciled>,
}

// qual:allow(coupling, oi) reason: "The bounded HTTP seam owns single-attempt writes, access challenges and validated responses."
impl Client {
    pub fn preview_issue_move(
        &self,
        id: &str,
        destination: &str,
        version: u32,
    ) -> Result<MovePreview, IssueSaveError> {
        let nett = self.backend == crate::Backend::Nett;
        let body = if nett {
            json!({"destinationTeamId":destination,"version":version})
        } else {
            json!({"destinationProjectId":destination,"version":version})
        };
        let preview: MovePreview = self.move_request(id, "move-preview", body)?;
        if !preview.answers(id, destination, version) || !preview.confirmable(nett) {
            return Err(IssueSaveError::Unavailable);
        }
        Ok(preview)
    }
    pub fn move_issue(&self, preview: &MovePreview) -> Result<MoveResult, IssueSaveError> {
        if self.backend == crate::Backend::Nett {
            return self.nett_move_issue(preview);
        }
        let root = preview.issues.first().ok_or(IssueSaveError::Invalid)?;
        let result: MoveResult = self.move_request(&root.id, "move", json!({"destinationProjectId":preview.destination.id,"version":root.version,"token":preview.token}))?;
        if result.issue.fields.id != root.id
            || !preview.root_moved_to(&result.issue.fields.identifier)
            || result.issue.fields.project_id != preview.destination.id
            || result.moved.len() != preview.issues.len()
            || !result
                .moved
                .iter()
                .zip(&preview.issues)
                .all(|(a, b)| a.id == b.id && a.new_identifier == b.new_identifier)
        {
            return Err(IssueSaveError::Unconfirmed);
        }
        Ok(result)
    }
    /// NETT confirms the previewed plan with a versioned PATCH of the root
    /// carrying the preview's token and review context; the whole subtree
    /// moves at once and only then receives its numbers, read back here.
    fn nett_move_issue(&self, preview: &MovePreview) -> Result<MoveResult, IssueSaveError> {
        if !self.serves(crate::backend::Feature::IssueMoves) {
            return Err(IssueSaveError::Unavailable);
        }
        let root = preview.issues.first().ok_or(IssueSaveError::Invalid)?;
        if preview.moves_archived_sub_issue() {
            return Err(IssueSaveError::ArchivedSubIssue);
        }
        let context = preview
            .review_context
            .clone()
            .ok_or(IssueSaveError::Invalid)?;
        let url = self
            .endpoint(&["issues", &root.id], &[])
            .map_err(|_| IssueSaveError::NotSent)?;
        let body = json!({
            "teamId": preview.destination.id,
            "version": root.version,
            "reviewContext": context,
            "movePreviewToken": preview.token,
        });
        transport::prepare(&url);
        let mut request = self
            .agent
            .patch(url.as_str())
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
        let issue: IssueDetail = self.move_response(response, true)?;
        self.nett_verify_move(preview, Some(issue))
    }

    /// Reads a move reply. Previews keep NETT's own field names; an issue
    /// detail is mapped like any other read.
    fn move_response<T: DeserializeOwned>(
        &self,
        response: ureq::http::Response<ureq::Body>,
        detail: bool,
    ) -> Result<T, IssueSaveError> {
        if transport::is_access_challenge(&response)
            || response.status() == ureq::http::StatusCode::UNAUTHORIZED
        {
            return Err(IssueSaveError::SignIn);
        }
        match response.status().as_u16() {
            200 if detail => self.read(response).map_err(|_| IssueSaveError::Unconfirmed),
            200 => {
                let reply =
                    transport::read_json(response).map_err(|_| IssueSaveError::Unconfirmed)?;
                // A NETT preview read after the account or organisation
                // changed belongs to no one.
                if self.backend == crate::Backend::Nett {
                    self.current().map_err(|_| IssueSaveError::SignIn)?;
                }
                Ok(reply)
            }
            400 | 422 => Err(IssueSaveError::Invalid),
            403 => Err(IssueSaveError::Forbidden),
            404 => Err(IssueSaveError::Unavailable),
            409 => Err(IssueSaveError::Conflict),
            429 => Err(IssueSaveError::RateLimited),
            // A NETT preview's 503 asks for a wait like a 429. A move's 503
            // may hide a move that committed, on either backend, so it is
            // left unconfirmed, to be checked without moving again; any wait
            // NETT asked for already holds the account's requests.
            503 if self.backend == crate::Backend::Nett && !detail => {
                Err(IssueSaveError::RateLimited)
            }
            _ => Err(IssueSaveError::Unconfirmed),
        }
    }
    fn move_request<T: DeserializeOwned>(
        &self,
        id: &str,
        action: &str,
        body: Value,
    ) -> Result<T, IssueSaveError> {
        if !self.serves(crate::backend::Feature::IssueMoves) {
            return Err(IssueSaveError::Unavailable);
        }
        let url = self
            .endpoint(&["issues", id, action], &[])
            .map_err(|_| IssueSaveError::NotSent)?;
        let response = transport::post_json(self, &url, &body, transport::READ_TIMEOUT)
            .map_err(|_| IssueSaveError::Unconfirmed)?;
        self.move_response(response, false)
    }
}

#[cfg(test)]
mod tests {
    use super::{MovePreview, Unreconciled, UnreconciledReason};
    use serde_json::{Value, json};
    use subspace_framework::client_test_support::Mock;

    const ROOT: &str = "00000000-0000-4000-8000-0000000000e1";
    const CHILD: &str = "00000000-0000-4000-8000-0000000000e2";

    fn issue(id: &str, identifier: &str, (team, key): (&str, &str), version: i64) -> Value {
        let mut issue = crate::tests::detail(id);
        issue["identifier"] = json!(identifier);
        issue["projectId"] = json!(team);
        issue["projectKey"] = json!(key);
        issue["version"] = json!(version);
        issue
    }

    fn item(id: &str, identifier: &str) -> Value {
        json!({"id": id, "version": 2, "oldIdentifier": identifier, "newIdentifier": null,
            "stateId": "state-1", "stateName": "Started", "cycleCleared": false,
            "milestoneCleared": false, "estimateCleared": false, "parentDetached": false,
            "archived": false})
    }

    /// Confirms a move of ROOT with its sub-issue CHILD to WEB, which NETT
    /// commits, then reads CHILD back with `child`'s reply. Returns the
    /// result and the PATCHes sent.
    fn committed_move(
        child: impl Fn() -> (u16, String) + Send + Sync + 'static,
    ) -> (super::MoveResult, usize) {
        let mock = Mock::new(move |request| {
            let path = request.path.as_str();
            if path.ends_with("/desktop/capabilities") {
                (200, r#"{"features":["issueMoves"]}"#.into())
            } else if request.method == "PATCH" && path.ends_with(&format!("/issues/{ROOT}")) {
                // NETT moved the whole subtree and returns the root there.
                let root = issue(ROOT, "WEB-7", ("team-web", "WEB"), 3);
                (200, root.to_string())
            } else if path.ends_with(&format!("/issues/{CHILD}")) {
                child()
            } else {
                (404, "{}".into())
            }
        });
        let client = crate::Client::nett_loopback(
            &format!("{}/api", mock.base_url()),
            Some("00000000-0000-4000-8000-0000000000e0"),
            Some(subspace_framework::core::Secret::new("nett-move-reconcile")),
        )
        .unwrap()
        .without_live();
        let preview: MovePreview = serde_json::from_value(json!({
            "token": "a".repeat(64),
            "source": {"id": "team-ops", "key": "OPS", "name": "Operations"},
            "destination": {"id": "team-web", "key": "WEB", "name": "Web"},
            "issues": [item(ROOT, "OPS-2"), item(CHILD, "OPS-3")],
            "reviewContext": {"snapshot": 1},
        }))
        .unwrap();
        let result = client
            .move_issue(&preview)
            .unwrap_or_else(|error| panic!("the move committed, not {error:?}"));
        let patches = mock
            .requests()
            .iter()
            .filter(|request| request.method == "PATCH")
            .count();
        (result, patches)
    }

    #[test]
    fn a_committed_move_stays_committed_when_a_sub_issue_moves_again_before_it_is_read() {
        // Someone else moved the sub-issue on before it was read.
        let (result, patches) = committed_move(|| {
            (
                200,
                issue(CHILD, "OPS-9", ("team-ops", "OPS"), 4).to_string(),
            )
        });
        assert_eq!(result.issue.fields.identifier, "WEB-7");
        assert_eq!(
            result.unreconciled,
            [Unreconciled {
                id: CHILD.into(),
                old_identifier: "OPS-3".into(),
                reason: UnreconciledReason::Elsewhere,
            }],
            "the sub-issue is listed to check"
        );
        assert_eq!(result.moved.len(), 1);
        assert_eq!(patches, 1, "the move is never sent again");
    }

    #[test]
    fn a_committed_move_keeps_its_proof_when_a_sub_issue_cannot_be_read_back() {
        // Moved on at once into a team the person cannot read.
        let (result, patches) = committed_move(|| (404, "{}".into()));
        assert_eq!(
            result.issue.fields.identifier, "WEB-7",
            "the move committed"
        );
        assert_eq!(
            result.unreconciled,
            [Unreconciled {
                id: CHILD.into(),
                old_identifier: "OPS-3".into(),
                reason: UnreconciledReason::Unread(crate::Error::NotFound),
            }],
            "the sub-issue is listed with what its read found"
        );
        assert_eq!(result.moved.len(), 1);
        assert_eq!(patches, 1, "the move is never sent again");
    }
}
