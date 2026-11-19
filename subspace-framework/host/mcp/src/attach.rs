//! `mcp --attach <URL>`: MCP over stdio for an agent that cannot speak
//! Streamable HTTP, relayed to the running app's endpoint for one chat. It
//! serves no catalog of its own: every tool runs, and asks, in the app.
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, Implementation, ListResourceTemplatesResult,
    ListResourcesResult, ListToolsResult, PaginatedRequestParams, ReadResourceRequestParams,
    ReadResourceResponse, ServerCapabilities, ServerConfig,
};
use rmcp::service::{Peer, RequestContext, ServiceError};
use rmcp::transport::StreamableHttpClientTransport;
use rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig;
use rmcp::{ErrorData, RoleClient, RoleServer, ServerHandler, ServiceExt};
use std::sync::Arc;

/// Where the app's chat endpoint can be: this machine only, so the token
/// never leaves it.
const LOOPBACK: [&str; 3] = ["http://127.0.0.1:", "http://[::1]:", "http://localhost:"];

/// Relays stdio until standard input closes.
pub(crate) async fn serve(url: &str, token: &str) -> Result<(), String> {
    relay(url, token, rmcp::transport::stdio()).await
}

/// Relays `downstream` until it closes.
async fn relay<T, E, A>(url: &str, token: &str, downstream: T) -> Result<(), String>
where
    T: rmcp::transport::IntoTransport<RoleServer, E, A> + Send + 'static,
    E: std::error::Error + Send + Sync + 'static,
{
    let config = StreamableHttpClientTransportConfig::with_uri(url).auth_header(token);
    let upstream = ()
        .serve(StreamableHttpClientTransport::with_client(
            loopback_client()?,
            config,
        ))
        .await
        .map_err(|error| format!("Cannot reach the app's chat tools at {url}: {error}"))?;
    let relayed = Relay {
        upstream: upstream.peer().clone(),
    };
    let running = relayed
        .serve(downstream)
        .await
        .map_err(|error| format!("MCP over stdio failed: {error}"))?;
    let _ = running.waiting().await;
    let _ = upstream.cancel().await;
    Ok(())
}

/// A plain-HTTP client for this machine, without a proxy. The workspace
/// builds reqwest without a default TLS provider, so it gets one explicitly.
fn loopback_client() -> Result<reqwest::Client, String> {
    let tls = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::aws_lc_rs::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .map_err(|error| error.to_string())?
    .with_root_certificates(rustls::RootCertStore::empty())
    .with_no_client_auth();
    reqwest::Client::builder()
        .no_proxy()
        .tls_backend_preconfigured(tls)
        .build()
        .map_err(|error| format!("Cannot start the MCP relay: {error}"))
}

/// `--attach` takes only a loopback `http://` URL.
pub(crate) fn check_url(url: &str) -> Result<(), String> {
    if LOOPBACK.iter().any(|prefix| url.starts_with(prefix)) {
        Ok(())
    } else {
        Err(format!(
            "--attach needs the app's loopback endpoint, such as http://127.0.0.1:PORT/mcp, got {url:?}."
        ))
    }
}

struct Relay {
    upstream: Peer<RoleClient>,
}

impl ServerHandler for Relay {
    fn get_info(&self) -> ServerConfig {
        let info = self.upstream.peer_info();
        let capabilities = info
            .as_ref()
            .map_or_else(ServerCapabilities::default, |info| {
                info.capabilities.clone()
            });
        let mut config = ServerConfig::new(capabilities);
        if let Some(server) = info.as_ref().and_then(|info| info.server_info.clone()) {
            config = config.with_server_info(server);
        } else {
            config = config.with_server_info(Implementation::new("subspace", "attach"));
        }
        match info.as_ref().and_then(|info| info.instructions.clone()) {
            Some(instructions) => config.with_instructions(instructions),
            None => config,
        }
    }

    async fn list_tools(
        &self,
        request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        self.upstream.list_tools(request).await.map_err(relayed)
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        self.upstream.call_tool_once(request).await.map_err(relayed)
    }

    async fn list_resources(
        &self,
        request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, ErrorData> {
        self.upstream.list_resources(request).await.map_err(relayed)
    }

    async fn list_resource_templates(
        &self,
        request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourceTemplatesResult, ErrorData> {
        self.upstream
            .list_resource_templates(request)
            .await
            .map_err(relayed)
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, ErrorData> {
        self.upstream
            .read_resource_once(request)
            .await
            .map_err(relayed)
    }
}

/// The app's own error when it sent one, else the transport's.
fn relayed(error: ServiceError) -> ErrorData {
    match error {
        ServiceError::McpError(error) => error,
        other => ErrorData::internal_error(other.to_string(), None),
    }
}

#[cfg(test)]
mod tests {
    use super::{check_url, relay};
    use crate::{Board, ChatTools, GroupedCatalog, Identity};
    use serde_json::{Value, json};
    use std::sync::Arc;
    use subspace_agent_tools::test_support::RecordingApproval;
    use subspace_agent_tools::{
        Access, AgentTool, ApprovalPolicy, Arguments, ChatToolServer, Invocation, ToolEnvironment,
        ToolSpec,
    };

    struct Note;

    #[async_trait::async_trait]
    impl AgentTool for Note {
        fn spec(&self) -> &ToolSpec {
            static SPEC: std::sync::LazyLock<ToolSpec> = std::sync::LazyLock::new(|| ToolSpec {
                name: "note_write",
                title: "Write note",
                description: "Write a note",
                access: Access::Write,
                arguments: Arguments::Single { fields: Vec::new() },
                approval: ApprovalPolicy::PerCall,
            });
            &SPEC
        }

        async fn call(&self, invocation: Invocation) -> Result<Value, String> {
            let allowed = invocation.approval.approve("Write a note".into()).await?;
            Ok(json!({ "allowed": allowed }))
        }
    }

    fn tools(_: &ToolEnvironment) -> Vec<Arc<dyn AgentTool>> {
        vec![Arc::new(Note)]
    }

    fn chat_tools() -> ChatTools {
        let boards = [Board {
            id: "notes",
            title: "Notes",
            description: "Fixture notes",
            help: "",
            help_images: &[],
            tools,
        }];
        let environment = subspace_agent_tools::test_support::app_environment(Default::default());
        let settings = subspace_core::config::McpSettings::default();
        let catalog = GroupedCatalog::build(&boards, &[], &environment, &settings).unwrap();
        ChatTools::new(
            catalog.served_catalog(),
            Identity::new("subspace", "Subspace", "test"),
        )
    }

    #[tokio::test]
    async fn the_relay_reaches_the_chat_endpoint_and_its_writes_ask_the_chat() {
        let tools = chat_tools();
        let approval = Arc::new(RecordingApproval::new(true));
        let endpoint = tools.open(approval.clone()).await.unwrap();
        assert!(endpoint.url.starts_with("http://127.0.0.1:"));
        let (client_end, relay_end) = tokio::io::duplex(1 << 16);
        let (url, token) = (endpoint.url.clone(), endpoint.token.expose().to_owned());
        tokio::spawn(async move { relay(&url, &token, tokio::io::split(relay_end)).await });
        let client = rmcp::ServiceExt::serve((), tokio::io::split(client_end))
            .await
            .unwrap();
        let instructions = client
            .peer_info()
            .and_then(|info| info.instructions.clone());
        assert!(
            instructions
                .unwrap()
                .contains("- Notes (notes): Fixture notes.")
        );
        let names: Vec<_> = client
            .list_all_tools()
            .await
            .unwrap()
            .into_iter()
            .map(|tool| tool.name.to_string())
            .collect();
        assert!(names.contains(&"note_write".to_owned()), "{names:?}");
        let result = client
            .call_tool(rmcp::model::CallToolRequestParams::new("note_write"))
            .await
            .unwrap();
        assert_eq!(result.structured_content.unwrap()["allowed"], true);
        assert_eq!(approval.summaries(), ["Write a note"]);
        client.cancel().await.unwrap();
    }

    #[tokio::test]
    async fn the_relay_needs_the_endpoint_token() {
        let tools = chat_tools();
        let endpoint = tools
            .open(Arc::new(RecordingApproval::new(true)))
            .await
            .unwrap();
        let (_client_end, relay_end) = tokio::io::duplex(1 << 16);
        let refused = relay(&endpoint.url, "wrong", tokio::io::split(relay_end)).await;
        assert!(refused.unwrap_err().contains("Cannot reach"));
    }

    #[test]
    fn only_a_loopback_http_endpoint_can_be_attached() {
        assert!(check_url("http://127.0.0.1:51234/mcp").is_ok());
        assert!(check_url("http://[::1]:51234/mcp").is_ok());
        for refused in [
            "https://127.0.0.1:1/mcp",
            "http://192.0.2.4:1/mcp",
            "http://127.0.0.1.example:1/mcp",
            "http://localhost.example/mcp",
        ] {
            assert!(check_url(refused).is_err(), "{refused}");
        }
    }
}
