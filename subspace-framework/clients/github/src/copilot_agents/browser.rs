//! The browser routes used by `gh agent-task view --web`.

use serde_json::Value;
use subspace_core::http::plain_https_url;

use super::{PullRequestArtifact, validate_task_id};

// qual:api
pub const COPILOT_AGENTS_HOME_URL: &str = "https://github.com/copilot/agents";

/// The pull request's sessions, optionally focused on one known run.
pub fn session_browser_url(pull_url: &str, session_id: Option<&str>) -> Option<String> {
    let pull = plain_https_url(pull_url).ok()?;
    let suffix = session_id.filter(|id| validate_task_id(id).is_ok());
    Some(match suffix {
        Some(id) => format!("{pull}/agent-sessions/{id}"),
        None => format!("{pull}/agent-sessions"),
    })
}

pub(super) fn task_browser_url(raw: &Value, pull: Option<&PullRequestArtifact>) -> String {
    raw.get("html_url")
        .and_then(Value::as_str)
        .and_then(|url| plain_https_url(url).ok())
        .or_else(|| session_browser_url(&pull?.url, last_session_id(raw)))
        .unwrap_or_default()
}

fn last_session_id(raw: &Value) -> Option<&str> {
    raw.get("sessions")?.as_array()?.last()?.get("id")?.as_str()
}

#[cfg(test)]
mod tests {
    use super::session_browser_url;

    #[test]
    fn session_routes_do_not_append_untrusted_paths_or_use_unsafe_origins() {
        let pull = "https://github.com/o/r/pull/7/";
        assert_eq!(
            session_browser_url(pull, Some("../other")).as_deref(),
            Some("https://github.com/o/r/pull/7/agent-sessions")
        );
        for url in ["javascript:alert(1)", "https://user@github.com/o/r/pull/7"] {
            assert!(session_browser_url(url, Some("s1")).is_none());
        }
    }
}
