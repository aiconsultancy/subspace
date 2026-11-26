use std::process::ExitCode;

use subspace_agent_tools::ToolEnvironment;
use subspace_product::ProductContext;

use crate::{cli, fail, tree};

/// Product-owned commands merged with generated capability routes.
/// Completion receives the merged tree. Fallback receives the original
/// arguments for product commands and unknown root commands. Parse errors on
/// declared commands never reach fallback, with or without a catalogue.
pub struct Composition {
    tree: fn(&ProductContext) -> Result<usage::Spec, String>,
    completion: fn(&usage::Spec, &[String]) -> Vec<String>,
    fallback: fn(&ProductContext, &[String]) -> ExitCode,
    specialized_tools: &'static [&'static str],
}

impl Composition {
    pub const fn new(
        tree: fn(&ProductContext) -> Result<usage::Spec, String>,
        completion: fn(&usage::Spec, &[String]) -> Vec<String>,
        fallback: fn(&ProductContext, &[String]) -> ExitCode,
    ) -> Self {
        Self {
            tree,
            completion,
            fallback,
            specialized_tools: &[],
        }
    }

    /// Exclude exact tool identities from CLI generation, never from MCP.
    // qual:api (products replace selected tools with their own CLI handlers)
    pub fn with_specialized_tools(mut self, names: &'static [&'static str]) -> Self {
        self.specialized_tools = names;
        self
    }

    /// Headless product callback using the same served catalogue as MCP.
    /// Without one, the product's own tree still parses, with no generated
    /// commands, after a notice on standard error.
    // qual:api (products supply this callback to shell::launch)
    pub fn run(
        &self,
        context: &ProductContext,
        boards: &[subspace_mcp::Board],
        environment: &ToolEnvironment,
        args: &[String],
    ) -> ExitCode {
        let (capabilities, notice) = available(cli::capabilities(context, boards, environment));
        notice.iter().for_each(|notice| eprintln!("{notice}"));
        self.run_catalog(context, &capabilities, args)
    }

    pub(crate) fn commands(
        &self,
        context: &ProductContext,
        capabilities: &tree::Capabilities,
    ) -> Result<tree::Commands, String> {
        tree::Commands::new(context, capabilities, self.tree, self.specialized_tools)
    }

    pub(crate) fn run_catalog(
        &self,
        context: &ProductContext,
        capabilities: &tree::Capabilities,
        args: &[String],
    ) -> ExitCode {
        let commands = match self.commands(context, capabilities) {
            Ok(commands) => commands,
            Err(error) => return fail(&error, 1),
        };
        cli::run_commands(context, &commands, args, self.completion, self.fallback)
    }
}

/// The served capabilities, or none with the notice that explains why a
/// generated command is missing.
pub(crate) fn available(
    capabilities: Result<tree::Capabilities, String>,
) -> (tree::Capabilities, Option<String>) {
    match capabilities {
        Ok(capabilities) => (capabilities, None),
        Err(error) => (
            tree::Capabilities::default(),
            Some(format!("Space commands unavailable: {error}")),
        ),
    }
}
