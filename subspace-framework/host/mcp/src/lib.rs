//! MCP server for the agent tools the selected boards advertise, over stdio
//! or Streamable HTTP. Runs before the GUI and needs no Copilot login.

mod attach;
mod chat;
mod grouped;
mod help_image;
mod host;
mod http;
mod instructions;
mod options;
mod prompt;
mod resources;
mod server;

use options::{Parsed, TOKEN_VARIABLE};
use rmcp::ServiceExt;
use rmcp::service::ServerInitializeError;
use std::process::ExitCode;
use subspace_agent_tools::{BoardDoc, ToolEnvironment, ToolProvider};
use subspace_product::ProductContext;

pub use chat::ChatTools;
pub use grouped::{GroupKind, GroupTool, GroupedCatalog, ServedCatalog, ToolGroup};
pub use host::{Host, HostConfig};
pub use http::{BindError, ENDPOINT, Endpoint};
pub use options::{Attach, HttpOptions, Options, bind_address};
pub use prompt::{PromptInput, agent_prompt};
pub use server::{Identity, ToolServer};

/// The options of `mcp` (`args` follow it), with `token` as the value of
/// `SUBSPACE_MCP_TOKEN`. A help request is an error here.
// qual:api (the HTTP protocol tests build endpoints from parsed options)
pub fn parse_options(
    binary: &str,
    args: &[String],
    token: Option<String>,
) -> Result<Options, String> {
    let address = bind_address(subspace_product::MCP_DEFAULT_BIND)?;
    let mut settings = subspace_core::config::McpSettings::default();
    settings.bind = address.ip();
    settings.port = address.port();
    parse_options_with_settings(
        binary,
        args,
        token,
        &settings,
        std::path::Path::new("mcp-token"),
    )
}

/// Resolves CLI overrides against saved settings without reading process state.
pub fn parse_options_with_settings(
    binary: &str,
    args: &[String],
    token: Option<String>,
    settings: &subspace_core::config::McpSettings,
    token_file: &std::path::Path,
) -> Result<Options, String> {
    match options::parse_with_settings(binary, args, token, settings, token_file)? {
        Parsed::Serve(options) => Ok(options),
        Parsed::Help(_) => Err("Help was requested.".into()),
    }
}

/// One selected board and its tools.
#[derive(Clone, Copy)]
pub struct Board {
    pub id: &'static str,
    pub title: &'static str,
    /// One line on what the board shows, empty when it has none.
    pub description: &'static str,
    /// The board's compiled-in Markdown help, empty when it has none.
    pub help: &'static str,
    pub help_images: &'static [subspace_agent_tools::HelpImage],
    pub tools: ToolProvider,
}

impl Board {
    fn doc(&self) -> BoardDoc {
        BoardDoc {
            id: self.id,
            title: self.title,
            markdown: self.help,
            images: self.help_images,
        }
    }
}

/// `<binary> mcp [options]`. `args` follow `mcp`.
pub fn run(
    context: &ProductContext,
    boards: &[Board],
    environment: &ToolEnvironment,
    args: &[String],
    settings: &subspace_core::config::McpSettings,
) -> ExitCode {
    let binary = context.product.binary_name.as_str();
    let token_file = environment
        .config_file
        .as_deref()
        .and_then(std::path::Path::parent)
        .map(subspace_core::mcp_token::path)
        .unwrap_or_else(|| std::path::PathBuf::from("mcp-token"));
    let options = match options::parse_with_settings(
        binary,
        args,
        std::env::var(TOKEN_VARIABLE).ok(),
        settings,
        &token_file,
    ) {
        Ok(Parsed::Help(page)) => {
            print!("{page}");
            return ExitCode::SUCCESS;
        }
        Ok(Parsed::Serve(options)) => options,
        Err(error) => return fail(&error, 2),
    };
    if let Some(attach) = &options.attach {
        return finish(
            runtime()
                .and_then(|runtime| runtime.block_on(attach::serve(&attach.url, &attach.token))),
        );
    }
    let catalog = match GroupedCatalog::build(boards, &options.boards, environment, settings) {
        Ok(catalog) => catalog.served_catalog(),
        Err(error) => return fail(&error, 2),
    };
    let server = ToolServer::new(catalog, served_identity(context));
    finish(runtime().and_then(|runtime| runtime.block_on(serve(server, options.http))))
}

/// The app's name and version, not the Framework's, as the desktop's MCP
/// server reports.
fn served_identity(context: &ProductContext) -> Identity {
    Identity::new(
        context.product.binary_name.as_str(),
        &context.product.display_name,
        &context.version,
    )
}

fn finish(result: Result<(), String>) -> ExitCode {
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => fail(&error, 1),
    }
}

fn fail(error: &str, code: u8) -> ExitCode {
    eprintln!("{error}");
    ExitCode::from(code)
}

fn runtime() -> Result<tokio::runtime::Runtime, String> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("Cannot start the MCP runtime: {error}"))
}

/// The unfiltered board-command catalog, grouped by the board or Application
/// group that provides each tool. MCP hosts build `GroupedCatalog` with their
/// settings instead, so MCP switches do not remove the product's generated CLI
/// commands.
pub fn served_catalog(
    boards: &[Board],
    requested: &[String],
    environment: &ToolEnvironment,
) -> Result<GroupedCatalog, String> {
    let grouped = GroupedCatalog::build(
        boards,
        requested,
        environment,
        &subspace_core::config::McpSettings::default(),
    )?;
    if grouped.served_catalog().tools().is_empty() {
        Err("The selected boards advertise no agent tools.".into())
    } else {
        Ok(grouped)
    }
}

/// `--board` chooses among the active boards only: a board the config file
/// disables is not listed and cannot be served.
fn requested_boards(boards: &[Board], requested: &[String]) -> Result<Vec<Board>, String> {
    if let Some(unknown) = requested
        .iter()
        .find(|id| !boards.iter().any(|board| board.id == id.as_str()))
    {
        let known: Vec<&str> = boards.iter().map(|board| board.id).collect();
        return Err(format!(
            "This product has no active board {unknown}; it is not built in, or the \
             config file turned it off. Choose from: {}.",
            known.join(", ")
        ));
    }
    Ok(boards
        .iter()
        .filter(|board| requested.is_empty() || requested.iter().any(|id| id == board.id))
        .copied()
        .collect())
}

async fn serve(server: ToolServer, http: Option<HttpOptions>) -> Result<(), String> {
    match http {
        None => serve_stdio(server).await,
        Some(options) => serve_http(server, &options).await,
    }
}

/// Until standard input closes. Standard output carries only MCP messages.
async fn serve_stdio(server: ToolServer) -> Result<(), String> {
    let running = match server.serve(rmcp::transport::stdio()).await {
        Ok(running) => running,
        Err(ServerInitializeError::ConnectionClosed(_)) => return Ok(()),
        Err(error) => return Err(format!("MCP over stdio failed: {error}")),
    };
    running
        .waiting()
        .await
        .map(|_| ())
        .map_err(|error| format!("MCP over stdio failed: {error}"))
}

/// Until Ctrl+C.
async fn serve_http(server: ToolServer, options: &HttpOptions) -> Result<(), String> {
    let endpoint = Endpoint::bind(server, options)
        .await
        .map_err(|error| error.to_string())?;
    let address = endpoint.address()?;
    let authentication = match options.token {
        Some(_) => "bearer token required",
        None => "no authentication",
    };
    eprintln!(
        "Serving MCP on http://{address}{ENDPOINT} ({authentication}). Press Ctrl+C to stop."
    );
    endpoint
        .serve(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{Board, served_catalog, served_identity};
    use serde_json::{Value, json};
    use std::sync::Arc;
    use subspace_agent_tools::{
        Access, AgentTool, ApprovalPolicy, Arguments, Invocation, ToolEnvironment, ToolProvider,
        ToolSpec,
    };

    struct Named(ToolSpec);

    #[async_trait::async_trait]
    impl AgentTool for Named {
        fn spec(&self) -> &ToolSpec {
            &self.0
        }
        async fn call(&self, _invocation: Invocation) -> Result<Value, String> {
            Ok(json!({}))
        }
    }

    fn tool(name: &'static str) -> Arc<dyn AgentTool> {
        Arc::new(Named(ToolSpec {
            name,
            title: name,
            description: name,
            access: Access::Read,
            arguments: Arguments::Single { fields: Vec::new() },
            approval: ApprovalPolicy::None,
        }))
    }

    fn github(_: &ToolEnvironment) -> Vec<Arc<dyn AgentTool>> {
        vec![tool("github_read")]
    }

    fn irc(_: &ToolEnvironment) -> Vec<Arc<dyn AgentTool>> {
        vec![tool("irc_read")]
    }

    fn none(_: &ToolEnvironment) -> Vec<Arc<dyn AgentTool>> {
        Vec::new()
    }

    fn board(id: &'static str, tools: ToolProvider) -> Board {
        Board {
            id,
            title: id,
            description: "",
            help: "",
            help_images: &[],
            tools,
        }
    }

    fn names(boards: &[Board], requested: &[&str]) -> Result<Vec<&'static str>, String> {
        let environment = subspace_agent_tools::test_support::environment(Default::default());
        let requested: Vec<String> = requested.iter().map(|id| (*id).to_owned()).collect();
        served_catalog(boards, &requested, &environment).map(|grouped| {
            grouped
                .served_catalog()
                .tools()
                .tools()
                .iter()
                .map(|tool| tool.spec().name)
                .collect()
        })
    }

    #[test]
    fn boards_limit_the_catalog_in_product_order_and_unknown_boards_fail() {
        let boards = [
            board("github", github),
            board("irc", irc),
            board("terminal", none),
        ];
        // The framework web reader follows the board tools on every surface.
        assert_eq!(
            names(&boards, &[]).unwrap(),
            ["github_read", "irc_read", "web_read"]
        );
        assert_eq!(
            names(&boards, &["irc", "github"]).unwrap(),
            ["github_read", "irc_read", "web_read"]
        );
        assert_eq!(names(&boards, &["irc"]).unwrap(), ["irc_read", "web_read"]);
        let unknown = names(&boards, &["missing"]).unwrap_err();
        assert!(unknown.contains("no active board missing") && unknown.contains("turned it off"));
        assert!(unknown.contains("github, irc, terminal"));
        assert!(
            names(&boards, &["terminal"])
                .unwrap_err()
                .contains("no agent tools")
        );
    }

    #[test]
    fn a_board_the_config_file_disabled_is_absent_and_cannot_be_asked_for() {
        // The shell leaves a disabled board out of `boards` before serving.
        let active = [board("github", github)];
        assert_eq!(names(&active, &[]).unwrap(), ["github_read", "web_read"]);
        let refused = names(&active, &["irc"]).unwrap_err();
        assert!(refused.contains("no active board irc"), "{refused}");
        let empty = names(&[], &[]).unwrap_err();
        assert!(empty.contains("no agent tools"), "{empty}");
    }

    #[test]
    fn grouped_inventory_matches_served_tools_and_does_not_change_chat() {
        let boards = [board("github", github), board("irc", irc)];
        let mut config = subspace_core::config::Config::default();
        config.mcp.groups.insert("github".into(), false);
        let environment = subspace_agent_tools::test_support::environment(config);
        let grouped =
            super::GroupedCatalog::build(&boards, &[], &environment, &environment.config.mcp)
                .unwrap();
        assert_eq!(grouped.groups()[0].id, "github");
        assert_eq!(grouped.groups()[0].tools[0].name, "github_read");
        assert!(!grouped.groups()[0].tools[0].served);
        let listed: Vec<_> = grouped
            .groups()
            .iter()
            .flat_map(|group| &group.tools)
            .filter(|tool| tool.served)
            .map(|tool| tool.name)
            .collect();
        let served = grouped.served_catalog();
        let names: Vec<_> = served
            .tools()
            .tools()
            .iter()
            .map(|tool| tool.spec().name)
            .collect();
        assert_eq!(listed, names);
        assert_eq!(names, ["irc_read", "web_read"]);
        let chat = subspace_agent_tools::catalog(&[github, irc], &environment).unwrap();
        assert!(chat.get("github_read").is_some());
        assert!(
            served_catalog(&boards, &[], &environment)
                .unwrap()
                .served_catalog()
                .tools()
                .get("github_read")
                .is_some()
        );
        let error = super::GroupedCatalog::build(
            &boards,
            &["github".into()],
            &environment,
            &environment.config.mcp,
        )
        .err()
        .unwrap();
        assert!(error.contains("mcp.groups.github"));
    }

    #[test]
    fn instructions_name_only_the_requested_boards_in_product_order() {
        let boards = [
            Board {
                description: "Pull requests",
                ..board("github", github)
            },
            Board {
                description: "Channels",
                ..board("irc", irc)
            },
        ];
        let environment = subspace_agent_tools::test_support::environment(Default::default());
        let settings = subspace_core::config::McpSettings::default();
        let all = super::GroupedCatalog::build(&boards, &[], &environment, &settings)
            .unwrap()
            .served_catalog()
            .instructions("Subspace");
        let github = all.find("- github (github): Pull requests.").unwrap();
        let irc = all
            .find("- irc (irc): Channels. Tools: irc_read (read).")
            .unwrap();
        assert!(github < irc);
        let only = super::GroupedCatalog::build(&boards, &["irc".into()], &environment, &settings)
            .unwrap()
            .served_catalog()
            .instructions("Subspace");
        assert!(
            only.contains("- irc (irc)") && !only.contains("github"),
            "{only}"
        );
        assert!(only.contains("Shared tools: web_read (read)."), "{only}");
    }

    #[test]
    fn application_off_keeps_board_tools_but_not_shared_tools() {
        let boards = [Board {
            help: "# Help",
            ..board("github", github)
        }];
        let environment = subspace_agent_tools::test_support::environment(Default::default());
        let mut settings = subspace_core::config::McpSettings::default();
        settings.groups.insert("application".into(), false);
        let grouped = super::GroupedCatalog::build(&boards, &[], &environment, &settings).unwrap();
        let served = grouped.served_catalog();
        assert!(served.tools().get("github_read").is_some());
        assert!(served.tools().get("board_help").is_none());
        assert!(served.tools().get("web_read").is_none());
        assert_eq!(grouped.groups()[1].tools.len(), 2);
        assert!(grouped.groups()[1].tools.iter().all(|tool| !tool.served));
    }

    /// The only test in this binary that installs former ids.
    #[test]
    fn a_former_id_never_takes_the_application_group_s_choice() {
        subspace_core::former_ids::install(subspace_core::former_ids::Renames::from([(
            "application".to_owned(),
            "github".to_owned(),
        )]));
        let (config, _) = subspace_core::config::parse(
            r#"{"mcp":{"groups":{"application":false}}}"#,
            std::path::Path::new("config.json"),
        );
        let boards = [Board {
            help: "# Help",
            ..board("github", github)
        }];
        let environment = subspace_agent_tools::test_support::environment(Default::default());
        let grouped =
            super::GroupedCatalog::build(&boards, &[], &environment, &config.mcp).unwrap();
        let served = grouped.served_catalog();
        assert!(served.tools().get("github_read").is_some());
        assert!(served.tools().get("board_help").is_none());
        assert!(served.tools().get("web_read").is_none());
    }

    #[test]
    fn the_help_tool_serves_the_documents_of_the_served_boards_only() {
        let boards = [
            Board {
                help: "# GitHub",
                ..board("github", github)
            },
            Board {
                help: "# IRC",
                ..board("irc", irc)
            },
        ];
        assert_eq!(
            names(&boards, &[]).unwrap(),
            [
                "github_read",
                "irc_read",
                subspace_agent_tools::HELP_TOOL,
                "web_read"
            ]
        );
        assert_eq!(
            names(&boards, &["github"]).unwrap(),
            ["github_read", subspace_agent_tools::HELP_TOOL, "web_read"]
        );
        let documented = [Board {
            help: "# Terminal",
            ..board("terminal", none)
        }];
        assert_eq!(
            names(&documented, &[]).unwrap(),
            [subspace_agent_tools::HELP_TOOL],
            "a board with help but no tools still answers help"
        );
    }

    #[test]
    fn the_command_line_server_reports_the_app_version() {
        let mut context = subspace_product::ProductContext::new(
            subspace_product::validate_product(&subspace_product::fixture::descriptor()).unwrap(),
            subspace_product::PlatformEnv::from_process().unwrap(),
        );
        context.version = "9.8.7".into();
        let identity = served_identity(&context);
        assert_eq!(identity.version, "9.8.7");
        assert_eq!(identity.name, context.product.binary_name.as_str());
        assert_eq!(identity.title, context.product.display_name);
    }
}
