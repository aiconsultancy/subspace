//! GitHub cloud-agent task API: validation and transport mapping.

use serde_json::{Value, json};

use crate::Client;
use crate::PinnedWork;
use crate::transport;
use subspace_core::http::Error;

use crate::MAX_BODY_BYTES;
use std::collections::HashMap;

pub const AGENT_API_VERSION: &str = "2026-03-10";
pub const FALLBACK_API_VERSION: &str = "2022-11-28";
pub const MAX_PROMPT_UTF16: usize = 60_000;
const MAX_SUBJECT_ID_CHARS: usize = 200;
const MAX_REF_CHARS: usize = 255;
const MAX_TASK_ID_CHARS: usize = 100;
const MAX_TASKS: usize = 50;
const TASK_WINDOW_DAYS: i64 = 30;

mod browser;
mod lookup;
mod map;
mod model;
mod submission;
mod validate;

pub use browser::{COPILOT_AGENTS_HOME_URL, session_browser_url};

pub use model::{
    PullRequestArtifact, SessionError, SessionUsage, StartTask, Subject, SubjectKind, Task,
    TaskDetail, TaskList, TaskScope, TaskSession,
};
pub use validate::{
    validate_prompt, validate_ref, validate_start, validate_subject, validate_task_id,
};

use model::{RawArtifact, RawPull, RawSessions};

use map::{
    PULL_NODES_QUERY, PULLS_PER_PAGE, PullNodes, artifacts_from_nodes, linked_task,
    needs_rest_lookup, raw_artifact, since_30_days, to_task,
};
pub const MODELS: &[Option<&str>] = &[
    None,
    Some("claude-sonnet-4.6"),
    Some("claude-opus-4.6"),
    Some("gpt-5.2-codex"),
    Some("gpt-5.3-codex"),
    Some("gpt-5.4"),
    Some("claude-sonnet-4.5"),
    Some("claude-opus-4.5"),
];

impl Client {
    // qual:api
    pub fn start_copilot_agent_task(
        &self,
        input: &StartTask,
        scope: &TaskScope,
    ) -> Result<Task, Error> {
        self.start_copilot_agent_task_with_submission(
            input,
            scope,
            &subspace_core::copilot_agent_tasks::Submission::default(),
        )
    }

    pub fn start_copilot_agent_task_with_submission(
        &self,
        input: &StartTask,
        scope: &TaskScope,
        submission: &subspace_core::copilot_agent_tasks::Submission,
    ) -> Result<Task, Error> {
        submission::start(self, input, scope, submission)
    }

    /// The authenticated work user's recent tasks across every repository,
    /// read with one pinned work credential ([`Client::pin_work`]).
    // qual:api
    pub fn list_my_copilot_agent_tasks(&self) -> Result<TaskList, Error> {
        self.pin_work()?.list_my_copilot_agent_tasks()
    }

    /// One task and its sessions, by id, without a repository path, read
    /// with one pinned work credential ([`Client::pin_work`]).
    // qual:api
    pub fn get_copilot_agent_task_detail(
        &self,
        task_id: &str,
        subject: Option<Subject>,
    ) -> Result<TaskDetail, Error> {
        validate_task_id(task_id)?;
        self.pin_work()?
            .get_copilot_agent_task_detail(task_id, subject)
    }
}

impl PinnedWork<'_> {
    /// The work account's recent tasks across every repository. Missing
    /// repository names and pull node ids are resolved once per repository,
    /// not once per task, and every request goes out with the pinned
    /// credential.
    // qual:api
    pub fn list_my_copilot_agent_tasks(&self) -> Result<TaskList, Error> {
        let value = self.agent_get(&format!(
            "/agents/tasks?per_page={MAX_TASKS}&sort=updated_at&direction=desc&since={}",
            since_30_days()
        ))?;
        let mut values: Vec<Value> = value
            .get("tasks")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .take(MAX_TASKS)
            .collect();
        let pulls = lookup::resolve_task_pulls(self, &mut values);
        Ok(TaskList {
            tasks: values
                .iter()
                .map(|raw| linked_task(raw, None, &pulls))
                .collect(),
        })
    }

    /// One task and its sessions, by id, with the pinned credential.
    // qual:api
    pub fn get_copilot_agent_task_detail(
        &self,
        task_id: &str,
        subject: Option<Subject>,
    ) -> Result<TaskDetail, Error> {
        validate_task_id(task_id)?;
        let mut raw = self.agent_get(&format!("/agents/tasks/{task_id}"))?;
        let pulls = lookup::resolve_task_pulls(self, std::slice::from_mut(&mut raw));
        let sessions = serde_json::from_value::<RawSessions>(raw.clone())
            .map(|raw| raw.sessions)
            .unwrap_or_default();
        Ok(TaskDetail {
            task: linked_task(&raw, subject, &pulls),
            sessions,
        })
    }

    fn agent_get(&self, path: &str) -> Result<Value, Error> {
        let url = format!("{}{}", self.client.api_url, path);
        let response = self.get(&url, Some(AGENT_API_VERSION)).or_else(|error| {
            if transport::is_api_version_rejection(&error) {
                self.get(&url, Some(FALLBACK_API_VERSION))
            } else {
                Err(error)
            }
        })?;
        let body = transport::read_limited(response, MAX_BODY_BYTES)?;
        serde_json::from_slice(&body).map_err(|_| transport::unexpected("response"))
    }

    /// The pull requests the GraphQL node ids of `artifacts` resolve to.
    fn pulls_by_node_id(&self, artifacts: &[RawArtifact]) -> HashMap<u64, PullRequestArtifact> {
        let node_ids: Vec<String> = artifacts
            .iter()
            .filter_map(|artifact| artifact.data.as_ref()?.global_id.clone())
            .filter(|id| !id.is_empty())
            .collect();
        if node_ids.is_empty() {
            return HashMap::new();
        }
        self.graphql::<PullNodes>(PULL_NODES_QUERY, json!({ "ids": node_ids }))
            .map(|data| artifacts_from_nodes(data.nodes))
            .unwrap_or_default()
    }

    /// Fills the gaps in `pulls` from the repository's recent pull requests.
    fn fill_pulls_from_rest(&self, repo: &str, pulls: &mut HashMap<u64, PullRequestArtifact>) {
        let url = format!(
            "{}/repos/{repo}/pulls?state=all&sort=created&direction=desc&per_page={PULLS_PER_PAGE}",
            self.client.api_url
        );
        let Ok(rows) = self.rest_json::<Vec<RawPull>>(&url) else {
            return;
        };
        for row in rows {
            let state = pull_state(Some(&row.state), row.draft, row.merged_at.is_some());
            pulls.entry(row.id).or_insert(PullRequestArtifact {
                number: row.number,
                url: row.html_url,
                state,
            });
        }
    }

    fn resolve_pull_artifacts(
        &self,
        repo: &str,
        tasks: &[Value],
    ) -> HashMap<u64, PullRequestArtifact> {
        let artifacts: Vec<RawArtifact> = tasks.iter().filter_map(raw_artifact).collect();
        let mut pulls = self.pulls_by_node_id(&artifacts);
        if needs_rest_lookup(&artifacts, &pulls) {
            self.fill_pulls_from_rest(repo, &mut pulls);
        }
        pulls
    }
}
pub(super) fn pull_state(state: Option<&str>, draft: bool, merged: bool) -> crate::model::PrState {
    if merged || state == Some("MERGED") {
        crate::model::PrState::Merged
    } else if state == Some("CLOSED") {
        crate::model::PrState::Closed
    } else if draft {
        crate::model::PrState::Draft
    } else {
        crate::model::PrState::Open
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::PrState;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::Duration;
    use subspace_core::config::Config;
    use subspace_core::credentials::{Credentials, Environment, Runner};

    struct Env;
    impl Environment for Env {
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

    fn mock_agent() -> (String, Arc<Mutex<Vec<(String, String, String)>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let copy = seen.clone();
        thread::spawn(move || {
            for (index, stream) in listener.incoming().enumerate().take(2) {
                let mut stream = stream.unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                let mut version = String::new();
                let mut length = 0;
                loop {
                    let mut header = String::new();
                    reader.read_line(&mut header).unwrap();
                    if header == "\r\n" {
                        break;
                    }
                    let (name, value) = header.trim_end().split_once(": ").unwrap_or(("", ""));
                    if name.eq_ignore_ascii_case("x-github-api-version") {
                        version = value.into();
                    }
                    if name.eq_ignore_ascii_case("content-length") {
                        length = value.parse().unwrap();
                    }
                }
                let mut body = vec![0; length];
                reader.read_exact(&mut body).unwrap();
                copy.lock().unwrap().push((
                    line.trim_end().into(),
                    version,
                    String::from_utf8(body).unwrap(),
                ));
                let (status, body) = if index == 0 {
                    (400, r#"{"message":"Unsupported API version"}"#)
                } else {
                    (201, r#"{"id":"task-1","state":"queued"}"#)
                };
                let response = format!(
                    "HTTP/1.1 {status} X\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(response.as_bytes()).unwrap();
            }
        });
        (format!("http://{address}"), seen)
    }

    fn scope() -> TaskScope {
        TaskScope {
            configured_repos: vec!["Owner/one".into()],
        }
    }
    fn input(prompt: String) -> StartTask {
        StartTask {
            repo: "owner/one".into(),
            prompt,
            model: None,
            base_ref: None,
            create_pull_request: true,
            subject: None,
        }
    }

    #[test]
    fn prompt_limit_counts_utf16_units() {
        assert!(validate_start(&input("😀".repeat(30_000)), &scope()).is_ok());
        assert!(validate_start(&input(format!("x{}", "😀".repeat(30_000))), &scope()).is_err());
    }

    #[test]
    fn model_ref_and_repo_rules_are_explicit() {
        let mut value = input("ok".into());
        value.model = Some("gpt-5.4".into());
        assert!(validate_start(&value, &scope()).is_ok());
        value.model = Some("future-model".into());
        assert!(validate_start(&value, &scope()).is_err());
        value.model = None;
        value.base_ref = Some("feature/x".into());
        assert!(validate_start(&value, &scope()).is_ok());
        value.base_ref = Some("../bad".into());
        assert!(validate_start(&value, &scope()).is_err());
        value.repo = "other/nope".into();
        assert!(validate_start(&value, &scope()).is_err());
    }

    #[test]
    fn future_task_states_and_pull_artifacts_are_preserved() {
        let raw = serde_json::json!({"id":"t1","state":"future_state","artifacts":[{"type":"pull","data":{"id":42}}]});
        let mut pulls = HashMap::new();
        pulls.insert(
            42,
            PullRequestArtifact {
                number: 7,
                url: "https://github.com/o/r/pull/7".into(),
                state: PrState::Open,
            },
        );
        let task = to_task("o/r", &raw, None, &pulls);
        assert_eq!(task.state, "future_state");
        assert_eq!(task.pull_request.as_ref().map(|pull| pull.number), Some(7));
    }

    #[test]
    fn agent_post_sends_bearer_body_and_only_falls_back_for_api_version() {
        let (url, seen) = mock_agent();
        let credentials = Credentials::new(&Config::default(), Env, NoRunner);
        let client = Client::new(std::sync::Arc::new(credentials)).with_api_url(url);
        let result = submission::post(
            &client,
            "work_emu",
            "/agents/repos/o/r/tasks",
            json!({"prompt":"fix","create_pull_request":true}),
            &subspace_core::copilot_agent_tasks::Submission::default(),
        )
        .unwrap();
        assert_eq!(result["id"], "task-1");
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 2);
        assert_eq!(seen[0].0, "POST /agents/repos/o/r/tasks HTTP/1.1");
        assert_eq!(seen[0].1, AGENT_API_VERSION);
        assert_eq!(seen[1].1, FALLBACK_API_VERSION);
        assert!(seen[0].2.contains("\"prompt\":\"fix\""));
    }

    #[test]
    fn since_30_days_is_an_rfc3339_time_about_a_month_back() {
        let since = super::since_30_days();
        let parsed = chrono::DateTime::parse_from_rfc3339(&since).unwrap();
        let age = chrono::Utc::now().signed_duration_since(parsed.to_utc());
        assert_eq!(age.num_days(), super::TASK_WINDOW_DAYS);
        assert!(since.ends_with('Z') && !since.contains('+'), "{since}");
    }

    #[test]
    fn artifacts_from_nodes_keeps_complete_pull_nodes_only() {
        let nodes = serde_json::from_value::<super::map::PullNodes>(json!({
            "nodes": [
                {"databaseId": 1, "number": 7, "url": "u1", "state": "OPEN", "isDraft": true},
                {"databaseId": 2, "number": null, "url": "u2", "state": "OPEN", "isDraft": false},
                null
            ]
        }))
        .unwrap();
        let pulls = super::artifacts_from_nodes(nodes.nodes);
        assert_eq!(pulls.len(), 1);
        assert_eq!(pulls[&1].number, 7);
        assert_eq!(pulls[&1].state, PrState::Draft);
    }

    /// One row of `GET /agents/tasks`: a repository id, no repository name.
    fn list_row() -> Value {
        json!({
            "id": "task-9",
            "name": "Fix the login button",
            "state": "in_progress",
            "html_url": "https://github.com/Owner/one/agents/tasks/task-9",
            "created_at": "2026-10-01T09:00:00Z",
            "updated_at": "2026-10-02T09:00:00Z",
            "repository": {"id": 4242},
            "artifacts": [{"provider": "github", "type": "pull", "data": {"id": 42, "global_id": "PR_1"}}]
        })
    }

    fn resolved_pulls() -> HashMap<u64, PullRequestArtifact> {
        HashMap::from([(
            42,
            PullRequestArtifact {
                number: 7,
                url: "https://github.com/Owner/one/pull/7".into(),
                state: PrState::Open,
            },
        )])
    }

    #[test]
    fn list_rows_take_their_repository_from_the_task_page() {
        let task = super::linked_task(&list_row(), None, &resolved_pulls());
        assert_eq!(task.repo, "Owner/one");
        assert_eq!(task.id, "task-9");
        assert_eq!(task.state, "in_progress");
        assert_eq!(task.pull_request.map(|pull| pull.number), Some(7));
    }

    #[test]
    fn a_row_without_a_usable_page_falls_back_to_its_pull_request() {
        let mut raw = list_row();
        raw["html_url"] = json!("https://github.com/copilot/agents/task-9");
        assert_eq!(
            super::linked_task(&raw, None, &resolved_pulls()).repo,
            "Owner/one"
        );
        raw["html_url"] = json!("");
        assert_eq!(super::linked_task(&raw, None, &HashMap::new()).repo, "");
    }

    #[test]
    fn missing_task_pages_use_the_supported_pull_session_route() {
        let mut raw = list_row();
        raw["html_url"] = Value::Null;
        raw["sessions"] = json!([{"id": "session-first"}, {"id": "session-last"}]);
        let task = super::linked_task(&raw, None, &resolved_pulls());
        assert_eq!(
            task.html_url,
            "https://github.com/Owner/one/pull/7/agent-sessions/session-last"
        );
        raw["sessions"] = json!([]);
        assert_eq!(
            super::linked_task(&raw, None, &resolved_pulls()).html_url,
            "https://github.com/Owner/one/pull/7/agent-sessions"
        );
    }

    #[test]
    fn task_repository_names_survive_missing_pages_and_unresolved_artifacts() {
        let mut raw = list_row();
        raw["html_url"] = Value::Null;
        raw["repository"]["full_name"] = json!("Owner/one");
        assert_eq!(
            super::linked_task(&raw, None, &HashMap::new()).repo,
            "Owner/one"
        );
    }

    #[test]
    fn repo_urls_need_two_plain_segments_under_a_real_owner() {
        for url in [
            "https://github.com/copilot/agents/x",
            "https://github.com/settings/profile",
            "https://github.com/Owner",
            "https://github.com//one/x",
            "not a url",
        ] {
            assert_eq!(super::map::repo_from_url(url), None, "{url}");
        }
        assert_eq!(
            super::map::repo_from_url("https://ghe.example.com/Owner/one.js/pull/7"),
            Some("Owner/one.js".into())
        );
    }

    #[test]
    fn task_sessions_keep_every_documented_field() {
        let raw = json!({
            "id": "task-9",
            "state": "completed",
            "sessions": [{
                "id": "session-1",
                "name": "first attempt",
                "state": "failed",
                "created_at": "2026-10-01T09:00:00Z",
                "updated_at": "2026-10-01T09:30:00Z",
                "completed_at": "2026-10-01T09:30:00Z",
                "prompt": "fix it",
                "head_ref": "copilot/fix-1",
                "base_ref": "main",
                "model": "gpt-5.4",
                "usage": {"type": "premium_requests", "amount": 1.5},
                "error": {"message": "the build failed"}
            }, {
                "id": "session-2",
                "state": "completed"
            }]
        });
        let sessions = serde_json::from_value::<super::RawSessions>(raw)
            .unwrap()
            .sessions;
        assert_eq!(sessions.len(), 2);
        assert_eq!(sessions[0].model, "gpt-5.4");
        assert_eq!(sessions[0].base_ref, "main");
        assert_eq!(
            sessions[0].completed_at.as_deref(),
            Some("2026-10-01T09:30:00Z")
        );
        assert_eq!(
            sessions[0].usage.as_ref().map(|usage| usage.kind.as_str()),
            Some("premium_requests")
        );
        assert_eq!(
            sessions[0]
                .error
                .as_ref()
                .map(|error| error.message.as_str()),
            Some("the build failed")
        );
        assert_eq!(sessions[1].prompt, "");
        assert_eq!(sessions[1].usage, None);
    }

    #[test]
    fn task_ids_are_opaque_but_bounded() {
        assert!(validate_task_id("task-9").is_ok());
        assert!(validate_task_id("").is_err());
        assert!(validate_task_id("../other").is_err());
        assert!(validate_task_id(&"a".repeat(MAX_TASK_ID_CHARS + 1)).is_err());
    }
}
