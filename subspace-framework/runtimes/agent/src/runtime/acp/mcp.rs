//! The running app's tools for one session: an endpoint the app serves on
//! loopback for this chat, whose writes ask this chat's approval card. The
//! agent gets it as an HTTP MCP server when it advertises
//! `mcpCapabilities.http`, else as `<this app> mcp --attach`, a stdio relay
//! to the same endpoint. The token goes only into `session/new` and
//! `session/load`, and the event queue treats it as a secret.
use super::{Arc, EventQueue, Value, json};
use subspace_agent_tools::{Approval, ChatToolEndpoint, ChatToolServer};
use subspace_core::mcp_token::TOKEN_VARIABLE;

/// The MCP server name the agent sees.
const SERVER: &str = "subspace";

pub(super) struct SessionTools {
    server: Option<Arc<dyn ChatToolServer>>,
    approval: Arc<dyn Approval>,
    endpoint: Option<ChatToolEndpoint>,
    servers: Value,
}

impl SessionTools {
    pub(super) fn new(
        server: Option<Arc<dyn ChatToolServer>>,
        approval: Arc<dyn Approval>,
    ) -> Self {
        Self {
            server,
            approval,
            endpoint: None,
            servers: json!([]),
        }
    }

    /// What the endpoint serves, for the session's context.
    pub(super) fn context(&self) -> Option<Arc<str>> {
        self.server
            .as_ref()
            .map(|server| Arc::from(server.instructions()))
    }

    /// Opens the endpoint once; a second session request reuses it.
    pub(super) async fn open(&mut self, http: bool, events: &EventQueue) -> Result<(), String> {
        let Some(server) = self.server.clone().filter(|_| self.endpoint.is_none()) else {
            return Ok(());
        };
        let endpoint = server.open(self.approval.clone()).await?;
        events.remember_secrets([endpoint.token.clone()]);
        let entry = match http {
            true => http_server(&endpoint),
            false => relay_server(&endpoint, &relay_executable()?),
        };
        self.servers = json!([entry]);
        self.endpoint = Some(endpoint);
        Ok(())
    }

    /// The `mcpServers` of a session request.
    pub(super) fn servers(&self) -> &Value {
        &self.servers
    }
}

fn http_server(endpoint: &ChatToolEndpoint) -> Value {
    let authorization = format!("Bearer {}", endpoint.token.expose());
    json!({
        "type": "http",
        "name": SERVER,
        "url": endpoint.url,
        "headers": [{"name": "Authorization", "value": authorization}],
    })
}

fn relay_server(endpoint: &ChatToolEndpoint, executable: &str) -> Value {
    json!({
        "name": SERVER,
        "command": executable,
        "args": ["mcp", "--attach", endpoint.url],
        "env": [{"name": TOKEN_VARIABLE, "value": endpoint.token.expose()}],
    })
}

fn relay_executable() -> Result<String, String> {
    std::env::current_exe()
        .ok()
        .and_then(|path| path.to_str().map(str::to_owned))
        .ok_or_else(|| "Cannot find this app's executable for the MCP relay.".to_string())
}

#[cfg(test)]
mod tests {
    use super::{http_server, relay_server};
    use subspace_agent_tools::ChatToolEndpoint;
    use subspace_core::Secret;

    fn endpoint() -> ChatToolEndpoint {
        ChatToolEndpoint {
            url: "http://127.0.0.1:9/mcp".into(),
            token: Secret::new("test-only-endpoint-token"),
            lease: Box::new(()),
        }
    }

    #[test]
    fn http_agents_get_the_endpoint_and_others_the_relay() {
        let http = http_server(&endpoint());
        assert_eq!(http["type"], "http");
        assert_eq!(http["url"], "http://127.0.0.1:9/mcp");
        assert_eq!(
            http["headers"][0]["value"],
            "Bearer test-only-endpoint-token"
        );
        let relay = relay_server(&endpoint(), "/app/subspace");
        assert!(relay.get("type").is_none());
        assert_eq!(relay["command"], "/app/subspace");
        assert_eq!(
            relay["args"],
            serde_json::json!(["mcp", "--attach", "http://127.0.0.1:9/mcp"])
        );
        assert_eq!(relay["env"][0]["name"], "SUBSPACE_MCP_TOKEN");
    }
}
