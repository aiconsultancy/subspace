//! The Copilot agent session API: the session document and its log.
//!
//! These endpoints are undocumented and read-only. They live on the Copilot
//! API host, which GraphQL names (`viewer { copilotEndpoints { api } }`), so
//! nothing here hardcodes a host. They accept the gh CLI OAuth token only,
//! so a run with `GH_TOKEN`, `GITHUB_TOKEN` or a config-file token is told
//! what it needs rather than being sent to GitHub with a token that fails.

use std::io::Read as _;

use serde::Deserialize;
use serde_json::json;
use ureq::Body;
use ureq::http::Response;

use subspace_core::credentials::{Credential, Source};
use subspace_core::http::status as http_status;
use subspace_core::http::{Error, plain_https_url};

use crate::Client;
use crate::copilot_agents::validate_task_id;
use crate::transport;

mod log;
mod model;

pub use log::{LogEntry, SessionLog, parse_log};
pub use model::CopilotAgentSession;

/// The read limit for the session document, which is about 3 KB.
const MAX_SESSION_BYTES: usize = 256 * 1024;
/// The most of one log Subspace ever reads. A finished session's log is
/// 240-730 KB, so this leaves room without letting one answer grow freely.
pub const MAX_LOG_BYTES: usize = 2 * 1024 * 1024;

const ENDPOINT_QUERY: &str = "query { viewer { copilotEndpoints { api } } }";

/// What the user must do when the work credential is not an OAuth token,
/// naming Sign in with GitHub only when the app offers it.
pub fn needs_oauth_login(sign_in_offered: bool) -> String {
    let lead = if sign_in_offered {
        "Sign in with GitHub in Settings or run"
    } else {
        "Run"
    };
    format!(
        "The session log needs a GitHub sign-in. {lead} `gh auth login` for the work account, \
         and unset GH_TOKEN and GITHUB_TOKEN."
    )
}

#[derive(Debug, Deserialize)]
struct Endpoints {
    viewer: Viewer,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Viewer {
    copilot_endpoints: CopilotEndpoints,
}

#[derive(Debug, Deserialize)]
struct CopilotEndpoints {
    #[serde(default)]
    api: String,
}

/// A body read up to a cap, and whether the cap cut it.
struct Limited {
    bytes: Vec<u8>,
    truncated: bool,
}

/// Reads at most `max` bytes. More than that is reported as a cut body
/// rather than as an error, because a long log is still worth showing.
fn read_capped(mut response: Response<Body>, max: usize) -> Result<Limited, Error> {
    let mut bytes = Vec::new();
    response
        .body_mut()
        .as_reader()
        .take(max as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| Error::Network("The Copilot session could not be read.".into()))?;
    let truncated = bytes.len() > max;
    bytes.truncate(max);
    Ok(Limited { bytes, truncated })
}

impl Client {
    /// The Copilot API base for the work account. GraphQL answers it, and
    /// the answer is kept for the run, so one process asks once.
    pub fn copilot_api_url(&self) -> Result<String, Error> {
        match self.copilot_api.get() {
            Some(url) => Ok(url.clone()),
            None => self.resolve_copilot_api(),
        }
    }

    fn resolve_copilot_api(&self) -> Result<String, Error> {
        let login = self.work_login()?;
        let data: Endpoints = self.graphql(Some(&login), ENDPOINT_QUERY, json!({}), false)?;
        let url = plain_https_url(&data.viewer.copilot_endpoints.api)
            .map_err(|_| transport::unexpected("Copilot API URL"))?;
        Ok(self.copilot_api.get_or_init(|| url).clone())
    }

    /// The OAuth token of `login`: its Subspace sign-in or gh CLI token. An
    /// environment or config-file token is refused here, before any request
    /// leaves the process.
    fn oauth_credential(&self, login: &str) -> Result<Credential, Error> {
        let credential = self.credentials.github(Some(login))?;
        match credential.source {
            Source::GhCli { .. } | Source::SignedIn { .. } => Ok(credential),
            _ => Err(Error::Status {
                status: http_status::FORBIDDEN,
                message: needs_oauth_login(self.credentials.sign_in_offered()),
            }),
        }
    }

    /// One session document. It carries the pull request number and state
    /// the REST task artifact may be missing.
    // qual:api
    pub fn copilot_agent_session(&self, session_id: &str) -> Result<CopilotAgentSession, Error> {
        let body = self.copilot_read(session_id, "", transport::JSON, MAX_SESSION_BYTES)?;
        serde_json::from_slice(&body.bytes).map_err(|_| transport::unexpected("session"))
    }

    /// One session's log, parsed. `max_bytes` never goes above
    /// [`MAX_LOG_BYTES`]; a log the cap cut says so.
    // qual:api
    pub fn copilot_agent_session_log(
        &self,
        session_id: &str,
        max_bytes: usize,
    ) -> Result<SessionLog, Error> {
        let body = self.copilot_read(
            session_id,
            "/logs",
            transport::EVENT_STREAM,
            max_bytes.min(MAX_LOG_BYTES),
        )?;
        Ok(parse_log(
            &String::from_utf8_lossy(&body.bytes),
            body.truncated,
        ))
    }

    fn copilot_read(
        &self,
        session_id: &str,
        suffix: &str,
        accept: &str,
        max: usize,
    ) -> Result<Limited, Error> {
        validate_task_id(session_id)?;
        let login = self.work_login()?;
        let credential = self.oauth_credential(&login)?;
        let base = self.copilot_api_url()?;
        let url = format!("{base}/agents/sessions/{session_id}{suffix}");
        let account = transport::account_label(&credential, Some(login.as_str()));
        let response = transport::get_copilot(&url, &credential, &account, accept)?;
        read_capped(response, max)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use subspace_core::config::Config;
    use subspace_core::credentials::{Credentials, Environment, Runner};
    use subspace_core::http::Error;

    use super::{MAX_LOG_BYTES, needs_oauth_login};
    use crate::Client;

    struct EnvToken;
    impl Environment for EnvToken {
        fn var(&self, name: &str) -> Option<String> {
            (name == "GH_TOKEN").then(|| "token".into())
        }
    }

    struct NoRunner;
    impl Runner for NoRunner {
        fn run(&self, _: &str, _: &[&str], _: &[&str], _: Duration) -> Result<String, String> {
            Err("unused".into())
        }
    }

    #[test]
    fn an_environment_token_is_refused_with_what_to_do_about_it() {
        let credentials = Credentials::new(&Config::default(), EnvToken, NoRunner);
        let client = Client::new(Arc::new(credentials));
        let error = client.oauth_credential("work_emu").unwrap_err();
        assert!(matches!(error, Error::Status { status: 403, .. }));
        assert_eq!(error.to_string(), needs_oauth_login(false));
        assert!(!needs_oauth_login(false).contains("Sign in with GitHub"));
        assert!(needs_oauth_login(true).contains("Sign in with GitHub in Settings"));
    }

    #[test]
    fn a_body_over_the_cap_is_cut_and_reported_rather_than_refused() {
        let body = ureq::Body::builder().data("0123456789");
        let cut = super::read_capped(ureq::http::Response::new(body), 4).unwrap();
        assert_eq!(cut.bytes, b"0123");
        assert!(cut.truncated);
        let body = ureq::Body::builder().data("0123");
        let whole = super::read_capped(ureq::http::Response::new(body), 4).unwrap();
        assert_eq!(whole.bytes, b"0123");
        assert!(!whole.truncated, "a body that just fits is not cut");
        assert_eq!(MAX_LOG_BYTES, 2 * 1024 * 1024);
    }
}
