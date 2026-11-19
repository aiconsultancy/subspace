//! Shared fixtures: a read tool, a slow write tool, and request builders.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Duration;
use subspace_agent_tools::{
    Access, AgentTool, Catalog, ReadService, ReadTool, ToolSpec, WriteService, WriteTool,
};
use subspace_mcp::{Identity, ToolServer};

pub const MODERN: &str = "2026-07-28";
pub const LEGACY: &str = "2025-11-25";

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
enum Read {
    Echo { text: String },
}

struct Echo;

impl ReadService for Echo {
    type Request = Read;
    fn read(&self, request: Read) -> Result<Value, String> {
        let Read::Echo { text } = request;
        Ok(json!({ "text": text }))
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
enum Note {
    Note { body: String },
}

/// Each note takes a moment, so a streamed response arrives in pieces.
struct Notes;

impl WriteService for Notes {
    type Operation = Note;
    type Prepared = ();
    fn validate(&self, _: &[Note]) -> Result<(), String> {
        Ok(())
    }
    fn preflight(&self, _: &[Note]) -> Result<(), String> {
        Ok(())
    }
    fn describe(&self, _: &[Note], _: &()) -> String {
        String::new()
    }
    fn write(&self, _: usize, note: &Note, _: &()) -> Result<Value, String> {
        std::thread::sleep(Duration::from_millis(150));
        let Note::Note { body } = note;
        Ok(json!({ "body": body }))
    }
}

fn spec(name: &'static str, title: &'static str, access: Access) -> ToolSpec {
    ToolSpec {
        name,
        title,
        description: title,
        access,
        arguments: subspace_agent_tools::Arguments::Single { fields: Vec::new() },
        approval: if access == Access::Write {
            subspace_agent_tools::ApprovalPolicy::PerCall
        } else {
            subspace_agent_tools::ApprovalPolicy::None
        },
    }
}

pub fn server() -> ToolServer {
    ToolServer::new(
        Catalog::new(tools(&subspace_agent_tools::test_support::environment(
            Default::default(),
        )))
        .unwrap(),
        Identity::new("subspace", "Subspace", "0.1.0"),
    )
}

fn tools(_: &subspace_agent_tools::ToolEnvironment) -> Vec<Arc<dyn AgentTool>> {
    let read: Arc<dyn AgentTool> = Arc::new(ReadTool::new(
        spec("echo_read", "Echo", Access::Read),
        Arc::new(Echo),
    ));
    let write: Arc<dyn AgentTool> = Arc::new(WriteTool::new(
        spec("notes_write", "Notes", Access::Write),
        Arc::new(Notes),
    ));
    vec![read, write]
}

pub fn grouped(settings: &subspace_core::config::McpSettings) -> subspace_mcp::GroupedCatalog {
    grouped_with_search(settings, None)
}

pub fn grouped_with_search(
    settings: &subspace_core::config::McpSettings,
    search: Option<subspace_search::Service>,
) -> subspace_mcp::GroupedCatalog {
    let mut environment = subspace_agent_tools::test_support::environment(Default::default());
    environment.search = search;
    let boards = [subspace_mcp::Board {
        id: "test",
        title: "Test board",
        description: "Fixture notes",
        help: "# Test help",
        help_images: &[],
        tools,
    }];
    subspace_mcp::GroupedCatalog::build(&boards, &[], &environment, settings).unwrap()
}

pub fn meta(extra: Value) -> Value {
    let mut meta = json!({
        "io.modelcontextprotocol/protocolVersion": MODERN,
        "io.modelcontextprotocol/clientCapabilities": {},
        "io.modelcontextprotocol/clientInfo": {"name":"protocol-test","version":"1.0.0"},
    });
    if let (Some(meta), Value::Object(extra)) = (meta.as_object_mut(), extra) {
        meta.extend(extra);
    }
    meta
}

pub fn request(id: u32, method: &str, params: Value) -> Value {
    json!({"jsonrpc":"2.0","id":id,"method":method,"params":params})
}

pub fn two_notes() -> Value {
    json!({"name":"notes_write","arguments":{"operations":[
        {"action":"note","body":"first"},{"action":"note","body":"second"}]}})
}
