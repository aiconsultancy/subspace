//! Test doubles for tool tests: credentials that never read the environment
//! or run a CLI, and approvals and monitors that record what they saw.

use super::{Approval, CallMonitor, Invocation, ToolEnvironment, ToolHost};
use async_trait::async_trait;
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use subspace_core::config::Config;
use subspace_core::credentials::{Credentials, Environment, Runner};

struct NoEnvironment;

impl Environment for NoEnvironment {
    fn var(&self, _: &str) -> Option<String> {
        None
    }
}

struct NoRunner;

impl Runner for NoRunner {
    fn run(&self, _: &str, _: &[&str], _: &[&str], _: Duration) -> Result<String, String> {
        Err("No CLI calls in tool tests.".into())
    }
}

/// Credentials from `config` alone.
pub fn credentials(config: &Config) -> Arc<Credentials> {
    Arc::new(Credentials::new(config, NoEnvironment, NoRunner))
}

/// A headless environment, as `mcp` and the generated commands build.
pub fn environment(config: Config) -> ToolEnvironment {
    let credentials = credentials(&config);
    ToolEnvironment {
        config: Arc::new(config),
        credentials,
        search: None,
        config_file: None,
        host: ToolHost::Headless,
    }
}

/// The running app's environment, which also builds the app-only tools.
pub fn app_environment(config: Config) -> ToolEnvironment {
    ToolEnvironment {
        host: ToolHost::App,
        ..environment(config)
    }
}

/// Answers every approval with `allow` and keeps each preview.
pub struct RecordingApproval {
    allow: bool,
    summaries: Mutex<Vec<String>>,
}

impl RecordingApproval {
    pub fn new(allow: bool) -> Self {
        Self {
            allow,
            summaries: Mutex::new(Vec::new()),
        }
    }

    pub fn summaries(&self) -> Vec<String> {
        lock(&self.summaries).clone()
    }
}

#[async_trait]
impl Approval for RecordingApproval {
    async fn approve(&self, summary: String) -> Result<bool, String> {
        lock(&self.summaries).push(summary);
        Ok(self.allow)
    }

    async fn approve_in_app(&self, summary: String) -> Result<bool, String> {
        self.approve(summary).await
    }
}

/// Keeps each progress report and cancels once it has seen `limit` of them.
// qual:api (board and consumer tests drive write tools with it)
pub struct RecordingMonitor {
    limit: Option<usize>,
    reports: Mutex<Vec<(usize, usize)>>,
}

impl RecordingMonitor {
    // qual:api (board and consumer tests drive write tools with it)
    pub fn cancel_after(limit: usize) -> Self {
        Self {
            limit: Some(limit),
            reports: Mutex::new(Vec::new()),
        }
    }

    pub fn reports(&self) -> Vec<(usize, usize)> {
        lock(&self.reports).clone()
    }
}

impl CallMonitor for RecordingMonitor {
    fn progress(&self, done: usize, total: usize, _message: &str) {
        lock(&self.reports).push((done, total));
    }

    fn cancelled(&self) -> bool {
        self.limit
            .is_some_and(|limit| lock(&self.reports).len() >= limit)
    }
}

/// A call with `arguments` and a recording approval answering `allow`.
pub fn invocation(arguments: Value, allow: bool) -> (Arc<RecordingApproval>, Invocation) {
    let approval = Arc::new(RecordingApproval::new(allow));
    let invocation = Invocation {
        arguments,
        approval: approval.clone(),
        monitor: Arc::new(super::Unmonitored),
    };
    (approval, invocation)
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// `Err` unless the tool table under `heading` in `markdown` names exactly
/// the tools and actions `providers` and `docs` compile to. `absent` names
/// documented tools the test environment cannot build, such as the search
/// tools that need an index. The catalog is the running app's, which also
/// holds the app-only tools.
// qual:api (product crates compare their compiled catalog with docs/mcp.md)
pub fn check_documented_catalog(
    providers: &[super::ToolProvider],
    docs: Vec<super::BoardDoc>,
    markdown: &str,
    heading: &str,
    absent: &[&str],
) -> Result<(), String> {
    let environment = app_environment(Config::default());
    let catalog = super::catalog_with_help(providers, docs, &environment)?;
    let compiled = catalog_inventory(&catalog);
    let mut documented = documented_tools(markdown, heading);
    for tool in absent {
        if documented.remove(*tool).is_none() {
            return Err(format!("{tool} is not documented under {heading}"));
        }
    }
    if documented != compiled {
        return Err(format!(
            "the {heading} table drifted from the catalog:\n documented {documented:?}\n compiled {compiled:?}"
        ));
    }
    Ok(())
}

/// Tool name to its action names (`action` enum of a read schema or of a
/// write batch item), or an empty list for a schema without actions.
fn catalog_inventory(catalog: &super::Catalog) -> BTreeMap<String, Vec<String>> {
    catalog
        .tools()
        .iter()
        .map(|tool| {
            let actions = tool
                .spec()
                .arguments
                .action_names()
                .into_iter()
                .map(str::to_string)
                .collect();
            (tool.spec().name.to_string(), actions)
        })
        .collect()
}

/// The tool table under `heading` in a Markdown document: rows of
/// `| Board | \`tool\` | \`a\`, \`b\` |` until the next heading. A tool with
/// no actions documents `—`.
fn documented_tools(markdown: &str, heading: &str) -> BTreeMap<String, Vec<String>> {
    markdown
        .split("\n## ")
        .find(|section| section.starts_with(heading))
        .map(|section| {
            section
                .lines()
                .filter(|line| line.starts_with("| ") && !line.starts_with("| ---"))
                .filter_map(table_row)
                .collect()
        })
        .unwrap_or_default()
}

fn table_row(line: &str) -> Option<(String, Vec<String>)> {
    let cells: Vec<&str> = line.trim_matches('|').split('|').map(str::trim).collect();
    let tool = backticked(cells.get(1)?).into_iter().next()?;
    let actions = backticked(cells.get(2)?);
    Some((tool, actions))
}

fn backticked(cell: &str) -> Vec<String> {
    cell.split('`')
        .skip(1)
        .step_by(2)
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{check_documented_catalog, documented_tools};
    use crate::{
        Access, AgentTool, ApprovalPolicy, Arguments, Invocation, ToolEnvironment, ToolSpec, schema,
    };
    use async_trait::async_trait;
    use serde_json::Value;
    use std::sync::Arc;

    struct Fixture(ToolSpec);

    #[async_trait]
    impl AgentTool for Fixture {
        fn spec(&self) -> &ToolSpec {
            &self.0
        }

        async fn call(&self, _: Invocation) -> Result<Value, String> {
            Ok(Value::Null)
        }
    }

    fn fixture(_: &ToolEnvironment) -> Vec<Arc<dyn AgentTool>> {
        vec![Arc::new(Fixture(ToolSpec {
            name: "fixture_read",
            title: "Fixture",
            description: "Fixture",
            access: Access::Read,
            arguments: Arguments::Actions {
                actions: vec![
                    schema::action("first", "First.", vec![]),
                    schema::action("second", "Second.", vec![]),
                ],
                batched: false,
            },
            approval: ApprovalPolicy::None,
        }))]
    }

    #[test]
    fn documented_tools_read_one_section_and_ignore_header_rows() {
        let markdown = "# Title\n\n## Tools\n\n| Board | Tool | Actions |\n| --- | --- | --- |\n| GitHub | `github_read` | `scope`, `github_item` |\n| Shared | `search_content` | — |\n\n## Other\n\n| X | `other_read` | `a` |\n";
        let tools = documented_tools(markdown, "Tools");
        assert_eq!(tools["github_read"], ["scope", "github_item"]);
        assert!(tools["search_content"].is_empty());
        assert!(!tools.contains_key("other_read"));
        assert_eq!(documented_tools(markdown, "Other")["other_read"], ["a"]);
        assert!(documented_tools(markdown, "Missing").is_empty());
    }

    #[test]
    fn catalog_check_names_an_action_removed_from_documentation() {
        let markdown = "## Tools\n\n| Board | Tool | Actions |\n| --- | --- | --- |\n| Fixture | `fixture_read` | `first` |\n";
        let error =
            check_documented_catalog(&[fixture], Vec::new(), markdown, "Tools", &[]).unwrap_err();
        assert!(error.contains("second"), "{error}");
    }
}
