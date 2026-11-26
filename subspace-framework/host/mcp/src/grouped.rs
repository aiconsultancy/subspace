//! A single source for Settings' group inventory and the MCP served catalog.
use std::sync::Arc;

use subspace_agent_tools::{Access, AgentTool, Catalog, ToolEnvironment};
use subspace_core::config::McpSettings;

use crate::instructions::APPLICATION;
use crate::{Board, requested_boards};

#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct GroupTool {
    pub name: &'static str,
    pub access: Access,
    pub actions: Vec<&'static str>,
    pub served: bool,
}

/// Who provides a group's tools: one board, or the Application, which
/// shares its tools with every board. Read this, not the id, to tell them
/// apart: a board may have any valid id, `application` included.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum GroupKind {
    Board,
    Application,
}

#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct ToolGroup {
    pub id: &'static str,
    pub kind: GroupKind,
    pub title: &'static str,
    /// The board's one-line purpose; empty for the Application group.
    pub description: &'static str,
    pub tools: Vec<GroupTool>,
    pub served: bool,
}

#[derive(Clone)]
pub struct GroupedCatalog {
    groups: Vec<ToolGroup>,
    served: ServedCatalog,
}

/// Tools and resource-only capabilities have different group policies.
#[derive(Clone)]
pub struct ServedCatalog {
    pub(crate) tools: Catalog,
    pub(crate) cache_reader: Option<Arc<dyn AgentTool>>,
    /// Every group, served or not, which the instructions describe.
    groups: Arc<[ToolGroup]>,
}

/// A bare catalog has no boards, so its instructions give only the rules.
impl From<Catalog> for ServedCatalog {
    fn from(tools: Catalog) -> Self {
        Self {
            cache_reader: tools.get("read_search_result").cloned(),
            tools,
            groups: Arc::from([]),
        }
    }
}

impl GroupedCatalog {
    pub fn build(
        boards: &[Board],
        requested: &[String],
        environment: &ToolEnvironment,
        settings: &McpSettings,
    ) -> Result<Self, String> {
        let selected = requested_boards(boards, requested)?;
        reject_disabled_request(requested, settings)?;
        build(&selected, environment, settings)
    }

    pub fn groups(&self) -> &[ToolGroup] {
        &self.groups
    }

    pub fn served_catalog(&self) -> ServedCatalog {
        self.served.clone()
    }
}

impl ServedCatalog {
    pub fn tools(&self) -> &Catalog {
        &self.tools
    }

    /// The enabled boards, the tools served for them and the rules, for
    /// MCP `instructions` and the context of an ACP session.
    pub fn instructions(&self, product: &str) -> String {
        crate::instructions::instructions(product, &self.groups)
    }
}

fn reject_disabled_request(requested: &[String], settings: &McpSettings) -> Result<(), String> {
    match requested.iter().find(|id| !settings.group_enabled(id)) {
        Some(id) => Err(format!("The MCP group is disabled by mcp.groups.{id}.")),
        None => Ok(()),
    }
}

fn build(
    boards: &[Board],
    environment: &ToolEnvironment,
    settings: &McpSettings,
) -> Result<GroupedCatalog, String> {
    let board_tools: Vec<_> = boards
        .iter()
        .map(|board| (board, (board.tools)(environment)))
        .collect();
    let enabled: Vec<_> = boards
        .iter()
        .filter(|board| settings.group_enabled(board.id))
        .collect();
    let docs = boards.iter().map(Board::doc).collect();
    let has_tools = board_tools.iter().any(|(_, tools)| !tools.is_empty());
    let search_boards = enabled.iter().map(|board| board.id.to_owned()).collect();
    let application = subspace_agent_tools::application_tools_for_boards(
        docs,
        environment,
        has_tools,
        search_boards,
    );
    let mut groups = Vec::new();
    let mut served = Vec::new();
    for (board, tools) in board_tools {
        let on = settings.group_enabled(board.id);
        groups.push(group(board_identity(board), GroupKind::Board, &tools, on));
        if on {
            served.extend(tools);
        }
    }
    let on = settings.group_enabled(APPLICATION);
    groups.push(group(
        (APPLICATION, "Application", ""),
        GroupKind::Application,
        &application,
        on,
    ));
    if groups.iter().all(|group| group.tools.is_empty()) {
        return Err("The selected boards advertise no agent tools.".into());
    }
    let cache_reader = application
        .iter()
        .find(|tool| !enabled.is_empty() && tool.spec().name == "read_search_result")
        .cloned();
    if on {
        served.extend(application);
    }
    Ok(GroupedCatalog {
        served: ServedCatalog {
            tools: Catalog::new(served)?,
            cache_reader,
            groups: Arc::from(groups.as_slice()),
        },
        groups,
    })
}

fn board_identity(board: &Board) -> (&'static str, &'static str, &'static str) {
    (board.id, board.title, board.description)
}

fn group(
    (id, title, description): (&'static str, &'static str, &'static str),
    kind: GroupKind,
    tools: &[Arc<dyn AgentTool>],
    served: bool,
) -> ToolGroup {
    ToolGroup {
        id,
        kind,
        title,
        description,
        served,
        tools: tools
            .iter()
            .map(|tool| GroupTool {
                name: tool.spec().name,
                access: tool.spec().access,
                actions: tool.spec().arguments.action_names(),
                served,
            })
            .collect(),
    }
}
