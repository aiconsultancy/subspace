//! The agent tools a board advertises. Copilot chat and the MCP server
//! register the same tools, so a board's operations behave the same through
//! either consumer. No GPUI and no SDK dependency.

mod batch;
mod chat_server;
pub mod envelope;
pub mod help;
mod read;
pub mod refresh;
pub mod schema;
mod search;
pub mod secrets;
#[cfg(any(test, feature = "test-support"))]
pub mod test_support;
pub mod web;

use async_trait::async_trait;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::collections::HashSet;
use std::sync::Arc;
use subspace_core::config::Config;
use subspace_core::credentials::Credentials;

pub use batch::{MAX_BATCH, NO_RETRY, WriteService, WriteTool, summary};
pub use chat_server::{ChatToolEndpoint, ChatToolServer};
pub use help::{BoardDoc, HELP_TOOL, help_tool};
pub mod help_image;
pub use help_image::{HelpImage, help_image_reply};
pub use read::{ReadService, ReadTool};

/// What a board needs to build its tools: the loaded configuration and the
/// credential sources, as the dashboards receive them.
#[derive(Clone)]
pub struct ToolEnvironment {
    pub config: Arc<Config>,
    pub credentials: Arc<Credentials>,
    pub search: Option<subspace_search::Service>,
    /// The file `config` was read from, for a tool that edits its board's
    /// own section. `None` when the consumer has no file, as in tests.
    pub config_file: Option<std::path::PathBuf>,
    pub host: ToolHost,
}

/// The process that builds the tools. A board whose tools act on state only
/// the running app holds, such as live connections, builds them only for
/// [`ToolHost::App`]: the headless `mcp` command and the generated commands
/// cannot reach that state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolHost {
    /// The desktop app: chat and the app's own MCP server.
    App,
    /// `mcp` and the generated commands, without the GUI.
    Headless,
}

/// Whether a tool only reads its service or can change it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Access {
    Read,
    Write,
}

/// Whether a surface must obtain one explicit decision before a call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApprovalPolicy {
    None,
    PerCall,
}

/// The declarative arguments shared by chat, MCP and generated commands.
#[derive(Clone, Debug)]
pub enum Arguments {
    Actions {
        actions: Vec<schema::Action>,
        batched: bool,
    },
    Single {
        fields: Vec<schema::Field>,
    },
}

impl Arguments {
    /// Derives the JSON Schema exposed to model-facing surfaces.
    pub fn input_schema(&self) -> Value {
        match self {
            Self::Actions { actions, batched } => schema::actions_schema(actions, *batched),
            Self::Single { fields } => schema::single_schema(fields),
        }
    }

    /// Stable action names used by documentation and generated command inventories.
    pub fn action_names(&self) -> Vec<&'static str> {
        match self {
            Self::Actions { actions, .. } => actions.iter().map(|action| action.name).collect(),
            Self::Single { .. } => Vec::new(),
        }
    }
}

/// How a consumer lists a tool.
#[derive(Clone, Debug)]
pub struct ToolSpec {
    /// Unique within a product, such as `github_read`.
    pub name: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub access: Access,
    pub arguments: Arguments,
    pub approval: ApprovalPolicy,
}

impl ToolSpec {
    pub fn input_schema(&self) -> Value {
        self.arguments.input_schema()
    }
}

fn validate_spec(spec: &ToolSpec) -> Result<(), String> {
    if spec.access == Access::Write && spec.approval != ApprovalPolicy::PerCall {
        return Err(format!(
            "Write tool {} must require per-call approval.",
            spec.name
        ));
    }
    Ok(())
}

/// Asks for one decision about one write batch. Chat shows an approval
/// card; the MCP server leaves the decision to its client.
#[async_trait]
pub trait Approval: Send + Sync {
    async fn approve(&self, summary: String) -> Result<bool, String>;

    /// Requires a fresh decision in the app. External/unattended consumers
    /// cannot grant it by inheriting their normal approval behavior.
    async fn approve_in_app(&self, _summary: String) -> Result<bool, String> {
        Ok(false)
    }
}

/// Approves every batch. For a consumer whose client asks its user itself.
pub struct Unattended;

#[async_trait]
impl Approval for Unattended {
    async fn approve(&self, _summary: String) -> Result<bool, String> {
        Ok(true)
    }
}

/// Watches one call: progress reports out, cancellation in. Both are
/// optional for a consumer.
pub trait CallMonitor: Send + Sync {
    /// `done` operations of `total` have finished; the next one starts.
    fn progress(&self, _done: usize, _total: usize, _message: &str) {}
    /// True once the caller no longer wants the result.
    fn cancelled(&self) -> bool {
        false
    }
}

/// A monitor that reports nowhere and never cancels.
pub struct Unmonitored;

impl CallMonitor for Unmonitored {}

/// One call of a tool.
pub struct Invocation {
    pub arguments: Value,
    pub approval: Arc<dyn Approval>,
    pub monitor: Arc<dyn CallMonitor>,
}

/// One tool a board advertises.
#[async_trait]
pub trait AgentTool: Send + Sync {
    fn spec(&self) -> &ToolSpec;
    /// The result, or an error message the caller can read and act on.
    async fn call(&self, invocation: Invocation) -> Result<Value, String>;
}

/// A board's tools for one environment.
pub type ToolProvider = fn(&ToolEnvironment) -> Vec<Arc<dyn AgentTool>>;

/// The tools of the selected boards, in product order.
#[derive(Clone, Default)]
pub struct Catalog {
    tools: Vec<Arc<dyn AgentTool>>,
}

impl Catalog {
    /// Refuses two tools with one name: a consumer could not tell them apart.
    pub fn new(tools: Vec<Arc<dyn AgentTool>>) -> Result<Self, String> {
        for tool in &tools {
            validate_spec(tool.spec())?;
        }
        match duplicate_name(&tools) {
            Some(name) => Err(format!("Two boards advertise the agent tool {name}.")),
            None => Ok(Self { tools }),
        }
    }

    pub fn tools(&self) -> &[Arc<dyn AgentTool>] {
        &self.tools
    }

    pub fn get(&self, name: &str) -> Option<&Arc<dyn AgentTool>> {
        self.tools.iter().find(|tool| tool.spec().name == name)
    }

    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }
}

fn duplicate_name(tools: &[Arc<dyn AgentTool>]) -> Option<&'static str> {
    let mut seen = HashSet::new();
    tools
        .iter()
        .map(|tool| tool.spec().name)
        .find(|name| !seen.insert(*name))
}

/// The tools of `providers`, built for `environment`, in order.
pub fn catalog(
    providers: &[ToolProvider],
    environment: &ToolEnvironment,
) -> Result<Catalog, String> {
    catalog_with_help(providers, Vec::new(), environment)
}

/// The tools of `providers`, then the board help tool for `docs`. Every
/// consumer builds its catalog here, so chat and MCP list the same help.
pub fn catalog_with_help(
    providers: &[ToolProvider],
    docs: Vec<BoardDoc>,
    environment: &ToolEnvironment,
) -> Result<Catalog, String> {
    let mut tools: Vec<Arc<dyn AgentTool>> = providers
        .iter()
        .flat_map(|provider| provider(environment))
        .collect();
    // The web reader supports board work; a catalog with no board tools stays
    // empty so MCP still reports "no agent tools" for those boards.
    let serves_board_tools = !tools.is_empty();
    tools.extend(application_tools(docs, environment, serves_board_tools));
    Catalog::new(tools)
}

/// Builds the Application group separately without changing chat's catalog path.
pub fn application_tools(
    docs: Vec<BoardDoc>,
    environment: &ToolEnvironment,
    serves_board_tools: bool,
) -> Vec<Arc<dyn AgentTool>> {
    let search_boards = docs.iter().map(|board| board.id.to_owned()).collect();
    application_tools_for_boards(docs, environment, serves_board_tools, search_boards)
}

/// Restricts retained content independently of the Application tools' own switch.
pub fn application_tools_for_boards(
    docs: Vec<BoardDoc>,
    environment: &ToolEnvironment,
    serves_board_tools: bool,
    search_boards: Vec<String>,
) -> Vec<Arc<dyn AgentTool>> {
    let has_boards = !docs.is_empty();
    let mut tools: Vec<_> = help_tool(docs).into_iter().collect();
    if has_boards {
        tools.extend(search::tools(environment.search.clone(), search_boards));
    }
    if serves_board_tools {
        tools.extend(web::tools(environment, None));
    }
    tools
}

pub fn encoded(value: impl Serialize) -> Result<Value, String> {
    serde_json::to_value(value).map_err(|error| error.to_string())
}

pub fn decoded<T: DeserializeOwned>(value: Value) -> Result<T, String> {
    serde_json::from_value(value).map_err(|error| error.to_string())
}

/// Runs a blocking client call off the async runtime.
pub async fn blocking<T: Send + 'static>(
    task: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tokio::task::spawn_blocking(task)
        .await
        .map_err(|error| format!("Service worker failed: {error}"))?
}

/// Only the configured repositories, compared without case and never by
/// prefix.
// qual:api
pub fn configured_repo(config: &Config, repo: &str) -> Result<(), String> {
    if config
        .repos
        .iter()
        .any(|known| known.eq_ignore_ascii_case(repo))
    {
        Ok(())
    } else {
        Err(format!("Repository is not configured: {repo}"))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Access, AgentTool, ApprovalPolicy, Catalog, Invocation, ToolEnvironment, ToolProvider,
        ToolSpec, catalog, configured_repo, decoded, encoded,
    };
    use crate::test_support;
    use async_trait::async_trait;
    use serde_json::{Value, json};
    use std::sync::Arc;
    use subspace_core::config::Config;

    struct Named(ToolSpec);

    #[async_trait]
    impl AgentTool for Named {
        fn spec(&self) -> &ToolSpec {
            &self.0
        }
        async fn call(&self, _invocation: Invocation) -> Result<Value, String> {
            Ok(json!(self.0.name))
        }
    }

    fn tool(name: &'static str) -> Arc<dyn AgentTool> {
        Arc::new(Named(ToolSpec {
            name,
            title: name,
            description: "test",
            access: Access::Read,
            arguments: crate::Arguments::Single { fields: Vec::new() },
            approval: ApprovalPolicy::None,
        }))
    }

    fn first(_: &ToolEnvironment) -> Vec<Arc<dyn AgentTool>> {
        vec![tool("one_read"), tool("one_write")]
    }

    fn second(_: &ToolEnvironment) -> Vec<Arc<dyn AgentTool>> {
        vec![tool("two_read")]
    }

    #[test]
    fn the_catalog_keeps_provider_order_and_refuses_duplicate_names() {
        let environment = test_support::environment(Config::default());
        let providers: [ToolProvider; 2] = [first, second];
        let tools = catalog(&providers, &environment).unwrap();
        let names: Vec<&str> = tools.tools().iter().map(|tool| tool.spec().name).collect();
        assert_eq!(names, ["one_read", "one_write", "two_read", "web_read"]);
        assert!(tools.get("two_read").is_some());
        assert!(tools.get("missing").is_none());
        let error = catalog(&[first, first], &environment).err().unwrap();
        assert!(error.contains("one_read"), "{error}");
        assert!(Catalog::default().is_empty());
    }

    #[test]
    fn the_help_tool_follows_the_board_tools_when_a_board_documents_itself() {
        let environment = test_support::environment(Config::default());
        let docs = vec![crate::BoardDoc {
            id: "one",
            title: "One",
            markdown: "# One",
            images: &[],
        }];
        let catalog = crate::catalog_with_help(&[first], docs, &environment).unwrap();
        let names: Vec<&str> = catalog
            .tools()
            .iter()
            .map(|tool| tool.spec().name)
            .collect();
        assert_eq!(
            names,
            ["one_read", "one_write", crate::HELP_TOOL, "web_read"]
        );
        let without = crate::catalog_with_help(&[first], Vec::new(), &environment).unwrap();
        assert!(without.get(crate::HELP_TOOL).is_none());
    }

    #[test]
    fn configured_boundaries_are_case_insensitive_but_not_prefixes() {
        let config = Config {
            repos: vec!["work/repo".into()],
            ..Default::default()
        };
        assert!(configured_repo(&config, "WORK/REPO").is_ok());
        for repo in ["work/repo-extra", "other/repo", "work"] {
            assert!(configured_repo(&config, repo).is_err(), "{repo}");
        }
    }

    #[test]
    fn values_round_trip_and_decoding_errors_are_visible() {
        assert_eq!(encoded(vec!["field"]).unwrap(), json!(["field"]));
        assert_eq!(decoded::<Vec<u32>>(json!([1, 2])).unwrap(), [1, 2]);
        assert!(decoded::<Vec<u32>>(json!("text")).is_err());
    }

    #[test]
    fn a_write_tool_cannot_disable_approval() {
        let invalid: Arc<dyn AgentTool> = Arc::new(Named(ToolSpec {
            name: "invalid_write",
            title: "Invalid",
            description: "Invalid",
            access: Access::Write,
            arguments: crate::Arguments::Single { fields: Vec::new() },
            approval: ApprovalPolicy::None,
        }));
        let error = Catalog::new(vec![invalid]).err().unwrap();
        assert!(error.contains("per-call approval"), "{error}");
    }
}
