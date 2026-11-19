//! The running app's tools for chat sessions: each session gets its own
//! loopback Streamable HTTP endpoint and token, and its writes ask that
//! chat. Independent of the user's app server in Settings > MCP.
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;

use async_trait::async_trait;
use subspace_agent_tools::{Approval, ChatToolEndpoint, ChatToolServer};
use tokio::sync::oneshot;

use crate::{ENDPOINT, Endpoint, HttpOptions, Identity, ServedCatalog, ToolServer};

/// An ephemeral port on this machine only.
const LOOPBACK: SocketAddr = SocketAddr::new(std::net::IpAddr::V4(Ipv4Addr::LOCALHOST), 0);

pub struct ChatTools {
    catalog: ServedCatalog,
    identity: Identity,
}

impl ChatTools {
    pub fn new(catalog: impl Into<ServedCatalog>, identity: Identity) -> Self {
        Self {
            catalog: catalog.into(),
            identity,
        }
    }
}

#[async_trait]
impl ChatToolServer for ChatTools {
    async fn open(&self, approval: Arc<dyn Approval>) -> Result<ChatToolEndpoint, String> {
        let token = subspace_core::mcp_token::generate();
        let server =
            ToolServer::new(self.catalog.clone(), self.identity.clone()).with_approval(approval);
        let options = HttpOptions {
            bind: LOOPBACK,
            token: Some(token.expose().to_owned()),
            origins: Vec::new(),
            hosts: Vec::new(),
        };
        let endpoint = Endpoint::bind(server, &options)
            .await
            .map_err(|error| format!("Cannot serve the chat's tools: {error}"))?;
        let address = endpoint.address()?;
        // Dropping the lease drops `stop`, which ends `serve`.
        let (stop, stopped) = oneshot::channel::<()>();
        tokio::spawn(endpoint.serve(async {
            let _ = stopped.await;
        }));
        Ok(ChatToolEndpoint {
            url: format!("http://{address}{ENDPOINT}"),
            token,
            lease: Box::new(stop),
        })
    }

    fn instructions(&self) -> String {
        self.catalog.instructions(&self.identity.title)
    }
}
