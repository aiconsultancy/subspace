//! The board's agent tools: `irc_read` reads the live networks, buffers,
//! messages and members, and `irc_write` sends messages, joins and leaves
//! channels and connects or disconnects networks, as the user. Both act on
//! the board's hub: the same connections, buffers and nick the user sees.
//! Only the running app builds them. The headless `mcp` command and the
//! generated commands have no hub, and a second connection would collide
//! with the user's nick and session.

mod bridge;
mod lookup;
mod read;
mod write;

#[cfg(test)]
mod tests;

use std::sync::Arc;

use subspace_framework::agent_tools::{AgentTool, ReadTool, ToolEnvironment, ToolHost, WriteTool};
use subspace_framework::dashboard::full_text::Service;

pub(crate) use bridge::{Request, close, open, refuse, serve};

pub(crate) fn agent_tools(environment: &ToolEnvironment) -> Vec<Arc<dyn AgentTool>> {
    match environment.host {
        ToolHost::App => tools(bridge::Link::Process, environment.search.clone()),
        ToolHost::Headless => Vec::new(),
    }
}

/// The tools over `link`; `search` is the store `irc_read search` also
/// reads older messages from.
fn tools(link: bridge::Link, search: Option<Service>) -> Vec<Arc<dyn AgentTool>> {
    vec![
        Arc::new(ReadTool::new(
            read::spec(),
            Arc::new(read::Reader {
                link: link.clone(),
                search,
            }),
        )),
        Arc::new(WriteTool::new(write::spec(), Arc::new(write::Writer(link)))),
    ]
}
