//! The board's agent tools: `github_copilot_agents_read` lists Copilot cloud agent tasks,
//! and `github_copilot_agents_write` starts them in the configured repositories and records
//! them so this board follows them. Copilot chat and the MCP server register
//! both.

mod read;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;
use subspace_framework::agent_tools::schema::{Action, action, batch, text};
use subspace_framework::agent_tools::{
    Access, AgentTool, ReadTool, ToolEnvironment, ToolSpec, WriteService, WriteTool,
};
use subspace_framework::client_github::Client;
use subspace_framework::client_github::copilot_agents::{StartTask, Task, TaskScope};
use subspace_framework::core::config::Config;
use subspace_framework::core::copilot_agent_tasks::{TaskRef, TaskStore};

const DESCRIPTION: &str = "Start 1..20 GitHub Copilot cloud agent tasks after one approval for this call. A task hands the work to an autonomous agent that writes code in a configured repository and opens a pull request; it is not a local edit and it keeps running afterwards. Every task is validated and its base branch resolved before approval; denial starts nothing. Tasks start once, in order; never start a task again after an ambiguous result, read the Copilot Agents board first.";

const UNSAVED_TASK: &str = "The task was created, but its local reference could not be saved. Keep its GitHub link; do not start it again.";

pub(crate) fn agent_tools(environment: &ToolEnvironment) -> Vec<Arc<dyn AgentTool>> {
    let service = Arc::new(CopilotAgentsTools {
        client: Client::new(environment.credentials.clone()),
        config: environment.config.clone(),
        tasks: TaskStore::from_system(),
    });
    vec![
        Arc::new(ReadTool::new(
            read::spec(),
            Arc::new(read::github(environment)),
        )),
        Arc::new(WriteTool::new(spec(), service)),
    ]
}

struct CopilotAgentsTools {
    config: Arc<Config>,
    client: Client,
    /// `copilot-agent-tasks.json` beside the state file; `None` without one.
    tasks: Option<TaskStore>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
enum Write {
    GithubAgentTask {
        repo: String,
        prompt: String,
        #[serde(default)]
        base_ref: Option<String>,
        #[serde(default)]
        model: Option<String>,
        #[serde(default = "pull_request")]
        create_pull_request: bool,
    },
}

/// A task opens a pull request unless the caller says otherwise, as the
/// dashboards do.
fn pull_request() -> bool {
    true
}

impl WriteService for CopilotAgentsTools {
    type Operation = Write;
    /// The branch each task starts from, so the preview names it and the task
    /// cannot resolve another one later.
    type Prepared = Vec<String>;

    fn validate(&self, operations: &[Write]) -> Result<(), String> {
        operations
            .iter()
            .try_for_each(|operation| validate_task(&self.config, operation))
    }

    fn preflight(&self, operations: &[Write]) -> Result<Vec<String>, String> {
        let login = self
            .client
            .work_login()
            .map_err(|error| error.to_string())?;
        operations
            .iter()
            .map(
                |Write::GithubAgentTask { repo, base_ref, .. }| match base_ref {
                    Some(base) => Ok(base.clone()),
                    None => self
                        .client
                        .default_branch(&login, repo)
                        .map_err(|error| error.to_string()),
                },
            )
            .collect()
    }

    fn describe(&self, operations: &[Write], branches: &Self::Prepared) -> String {
        operations
            .iter()
            .zip(branches)
            .map(|(operation, branch)| task_line(operation, branch))
            .collect()
    }

    fn write(
        &self,
        index: usize,
        operation: &Write,
        branches: &Self::Prepared,
    ) -> Result<Value, String> {
        let branch = branches.get(index).ok_or("The task was not prepared.")?;
        let task = start_task(operation.clone(), Some(branch.clone()));
        let started = self
            .client
            .start_copilot_agent_task(&task, &task_scope(&self.config))
            .map_err(|error| error.to_string())?;
        Ok(task_result(&started, self.record(&started)))
    }
}

impl CopilotAgentsTools {
    /// Records the started task so this board can follow it. A confirmed
    /// remote task must never look like a failure because the local reference
    /// could not be written: retrying would start a second task.
    fn record(&self, task: &Task) -> bool {
        let Some(store) = &self.tasks else {
            return false;
        };
        store
            .record(
                &format!("chat:{}", task.id),
                TaskRef {
                    repo: task.repo.clone(),
                    task_id: task.id.clone(),
                    created_at: task.created_at.clone(),
                    monitor: true,
                },
            )
            .is_ok()
    }
}

fn task_result(task: &Task, saved: bool) -> Value {
    let mut value = json!({"id":task.id,"repo":task.repo,"state":task.state,"url":task.html_url,
        "pullRequest":task.pull_request.as_ref().map(|pull| json!({"number":pull.number,"url":pull.url}))});
    if !saved {
        value["warning"] = json!(UNSAVED_TASK);
    }
    value
}

fn start_task(operation: Write, base_ref: Option<String>) -> StartTask {
    let Write::GithubAgentTask {
        repo,
        prompt,
        model,
        create_pull_request,
        ..
    } = operation;
    StartTask {
        repo,
        prompt,
        model,
        base_ref,
        create_pull_request,
        subject: None,
    }
}

/// Only the configured repositories.
fn task_scope(config: &Config) -> TaskScope {
    TaskScope {
        configured_repos: config.repos.clone(),
    }
}

/// The client's own rules: prompt, repository scope, model and branch name.
fn validate_task(config: &Config, operation: &Write) -> Result<(), String> {
    let Write::GithubAgentTask { base_ref, .. } = operation;
    subspace_framework::client_github::copilot_agents::validate_start(
        &start_task(operation.clone(), base_ref.clone()),
        &task_scope(config),
    )
    .map(|_| ())
    .map_err(|error| error.to_string())
}

/// An autonomous agent is not a local edit, so the preview says so first.
fn task_line(operation: &Write, base_ref: &str) -> String {
    let Write::GithubAgentTask {
        repo,
        prompt,
        create_pull_request,
        ..
    } = operation;
    let pull = if *create_pull_request {
        "opens a pull request"
    } else {
        "pushes a branch without a pull request"
    };
    format!(
        "\n\nGitHub Copilot cloud agent task:\n- This starts an autonomous agent that writes code in {repo} on its own and {pull}. It is not a local edit and it keeps running after this call.\n- repository: {repo}\n- base branch: {base_ref}\n- prompt:\n{prompt}"
    )
}

fn spec() -> ToolSpec {
    ToolSpec {
        name: "github_copilot_agents_write",
        title: "Start Copilot cloud agent tasks",
        description: DESCRIPTION,
        access: Access::Write,
        arguments: batch(&actions()),
        approval: subspace_framework::agent_tools::ApprovalPolicy::PerCall,
    }
}

fn actions() -> Vec<Action> {
    vec![action(
        "github_agent_task",
        "Start a GitHub Copilot cloud agent task on a configured repository. This hands the work to an autonomous agent that writes code and opens a pull request; it is not a local edit. prompt is the full instruction, base_ref defaults to the repository's default branch.",
        vec![
            ("repo", text(), true),
            ("prompt", json!({"type":"string","maxLength":65536}), true),
            ("base_ref", text(), false),
            ("model", text(), false),
            (
                "create_pull_request",
                json!({"type":"boolean","default":true}),
                false,
            ),
        ],
    )]
}

#[cfg(test)]
mod tests {
    use super::{
        Write, agent_tools, pull_request, start_task, task_line, task_result, task_scope,
        validate_task,
    };
    use serde_json::json;
    use subspace_framework::agent_tools::test_support;
    use subspace_framework::core::config::Config;

    fn config() -> Config {
        Config {
            repos: vec!["work/repo".into()],
            ..Default::default()
        }
    }

    fn task(repo: &str, prompt: &str, base_ref: Option<&str>) -> Write {
        Write::GithubAgentTask {
            repo: repo.into(),
            prompt: prompt.into(),
            base_ref: base_ref.map(str::to_owned),
            model: None,
            create_pull_request: true,
        }
    }

    #[test]
    fn a_task_runs_only_in_a_configured_repository_with_a_usable_prompt() {
        let config = config();
        assert!(validate_task(&config, &task("work/repo", "Fix the bug", None)).is_ok());
        assert!(validate_task(&config, &task("work/repo", "Fix it", Some("main"))).is_ok());
        for invalid in [
            task("other/repo", "Fix the bug", None),
            task("work/repo", "   ", None),
            task("work/repo", "Fix the bug", Some("../evil")),
            Write::GithubAgentTask {
                repo: "work/repo".into(),
                prompt: "Fix the bug".into(),
                base_ref: None,
                model: Some("not-a-model".into()),
                create_pull_request: true,
            },
        ] {
            assert!(validate_task(&config, &invalid).is_err());
        }
        assert_eq!(task_scope(&config).configured_repos, ["work/repo"]);
    }

    #[test]
    fn a_task_opens_a_pull_request_unless_the_caller_says_otherwise() {
        let operation: Write = serde_json::from_value(json!({
            "action": "github_agent_task",
            "repo": "work/repo",
            "prompt": "Fix the bug",
        }))
        .unwrap();
        let Write::GithubAgentTask {
            create_pull_request,
            ..
        } = operation;
        assert!(create_pull_request);
        assert_eq!(create_pull_request, pull_request());
    }

    #[test]
    fn the_task_request_carries_the_approved_branch_and_no_subject() {
        let started = start_task(
            task("work/repo", "Fix the bug", None),
            Some("release/1.2".into()),
        );
        assert_eq!(started.base_ref.as_deref(), Some("release/1.2"));
        assert_eq!(started.repo, "work/repo");
        assert!(started.create_pull_request);
        assert_eq!(started.subject, None);
    }

    #[test]
    fn the_preview_says_plainly_that_an_autonomous_agent_starts() {
        let text = task_line(
            &task("work/repo", "Fix the SQL injection in db.rs", None),
            "main",
        );
        assert!(text.contains("starts an autonomous agent"));
        assert!(text.contains("opens a pull request"));
        assert!(text.contains("- repository: work/repo"));
        assert!(text.contains("- base branch: main"));
        assert!(text.contains("Fix the SQL injection in db.rs"));
        let no_pull = Write::GithubAgentTask {
            repo: "work/repo".into(),
            prompt: "Fix it".into(),
            base_ref: None,
            model: None,
            create_pull_request: false,
        };
        assert!(task_line(&no_pull, "main").contains("without a pull request"));
    }

    #[test]
    fn an_unsaved_reference_warns_but_still_reports_the_started_task() {
        let started = subspace_framework::client_github::copilot_agents::Task {
            id: "task-1".into(),
            repo: "work/repo".into(),
            name: "Fix the bug".into(),
            state: "queued".into(),
            html_url: "https://github.com/work/repo/agents/tasks/task-1".into(),
            created_at: "2026-10-07T09:00:00Z".into(),
            updated_at: "2026-10-07T09:00:00Z".into(),
            pull_request: None,
            subject: None,
        };
        assert_eq!(task_result(&started, true).get("warning"), None);
        assert!(
            task_result(&started, false)["warning"]
                .as_str()
                .is_some_and(|text| text.contains("do not start it again"))
        );
    }

    #[test]
    fn the_board_advertises_read_and_write_tools() {
        let tools = agent_tools(&test_support::environment(config()));
        let names: Vec<&str> = tools.iter().map(|tool| tool.spec().name).collect();
        assert_eq!(
            names,
            ["github_copilot_agents_read", "github_copilot_agents_write"]
        );
        assert_eq!(
            tools[0].spec().input_schema()["properties"]["action"]["enum"],
            json!(["scope", "tasks", "task", "refresh"])
        );
        assert_eq!(
            tools[1].spec().input_schema()["properties"]["operations"]["items"]["properties"]["action"]
                ["enum"],
            json!(["github_agent_task"])
        );
    }
}
