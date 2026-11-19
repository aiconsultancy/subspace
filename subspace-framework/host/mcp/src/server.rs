//! One MCP server over a tool catalog. Protocol revisions, discovery and
//! version errors come from `rmcp`; this maps tools, results, progress and
//! cancellation.

use crate::help_image::call_result as named_call_result;
use crate::resources::{
    resource, resource_call, resource_descriptors, resource_template, resource_template_descriptors,
};
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, Implementation,
    ListResourceTemplatesResult, ListResourcesResult, ListToolsResult, PaginatedRequestParams,
    ProgressNotificationParam, ProgressToken, ReadResourceRequestParams, ReadResourceResponse,
    ReadResourceResult, Resource, ResourceContents, ResourceTemplate, ServerCapabilities,
    ServerConfig, Tool, ToolAnnotations,
};
use rmcp::service::{Peer, RequestContext};
use rmcp::{ErrorData, RoleServer, ServerHandler};
use serde_json::{Map, Value};
use std::sync::Arc;
use std::sync::RwLock;
use subspace_agent_tools::{
    Access, AgentTool, Approval, CallMonitor, Invocation, ToolSpec, Unattended, Unmonitored,
};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// The identity a client sees, such as `subspace` and `Subspace`.
#[derive(Clone)]
pub struct Identity {
    pub name: String,
    pub title: String,
    pub version: String,
}

impl Identity {
    pub fn new(name: &str, title: &str, version: &str) -> Self {
        Self {
            name: name.to_owned(),
            title: title.to_owned(),
            version: version.to_owned(),
        }
    }
}

#[derive(Clone)]
pub struct ToolServer {
    catalog: Arc<RwLock<crate::ServedCatalog>>,
    identity: Identity,
    /// Who decides a write: nobody for an external client, which asks its
    /// own user; the chat's approval card for a chat endpoint.
    approval: Arc<dyn Approval>,
    cancel: CancellationToken,
}

impl ToolServer {
    pub fn new(catalog: impl Into<crate::ServedCatalog>, identity: Identity) -> Self {
        Self {
            catalog: Arc::new(RwLock::new(catalog.into())),
            identity,
            approval: Arc::new(Unattended),
            cancel: CancellationToken::new(),
        }
    }

    pub fn with_approval(mut self, approval: Arc<dyn Approval>) -> Self {
        self.approval = approval;
        self
    }

    pub fn replace_catalog(&self, catalog: impl Into<crate::ServedCatalog>) {
        *self
            .catalog
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = catalog.into();
    }

    fn snapshot(&self) -> crate::ServedCatalog {
        self.catalog
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    pub(crate) fn cancel(&self) {
        self.cancel.cancel();
    }
}

impl ServerHandler for ToolServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .build(),
        )
        .with_server_info(
            Implementation::new(&self.identity.name, &self.identity.version)
                .with_title(&self.identity.title),
        )
        .with_instructions(self.snapshot().instructions(&self.identity.title))
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        Ok(ListToolsResult::with_all_items(
            self.snapshot()
                .tools
                .tools()
                .iter()
                .map(|tool| mcp_tool(tool.spec()))
                .collect(),
        ))
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, ErrorData> {
        Ok(ListResourcesResult::with_all_items(resources(self)))
    }

    async fn list_resource_templates(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourceTemplatesResult, ErrorData> {
        Ok(ListResourceTemplatesResult::with_all_items(
            resource_templates(self),
        ))
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, ErrorData> {
        let uri = request.uri;
        let (name, arguments) = resource_call(&uri)
            .ok_or_else(|| ErrorData::invalid_params(format!("Unknown resource: {uri}"), None))?;
        let tool = self.snapshot().tools.get(name).cloned().ok_or_else(|| {
            ErrorData::invalid_params(format!("Resource capability is unavailable: {name}"), None)
        })?;
        let value = call_unmonitored(tool, arguments)
            .await
            .map_err(|error| ErrorData::internal_error(error, None))?;
        let body = serde_json::to_string(&value)
            .map_err(|error| ErrorData::internal_error(error.to_string(), None))?;
        Ok(ReadResourceResult::new(vec![ResourceContents::text(body, uri)]).into())
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        self.snapshot()
            .tools
            .get(name)
            .map(|tool| mcp_tool(tool.spec()))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let tool = self
            .snapshot()
            .tools
            .get(&request.name)
            .cloned()
            .ok_or_else(|| {
                ErrorData::invalid_params(format!("Unknown tool: {}", request.name), None)
            })?;
        let arguments = Value::Object(request.arguments.unwrap_or_default());
        let outcome = call(
            tool,
            arguments,
            &context,
            self.approval.clone(),
            self.cancel.clone(),
        )
        .await;
        Ok(CallToolResponse::Complete(named_call_result(
            &request.name,
            outcome,
        )))
    }
}

fn resources(server: &ToolServer) -> Vec<Resource> {
    resource_descriptors(
        server
            .snapshot()
            .tools
            .get(subspace_agent_tools::HELP_TOOL)
            .is_some(),
    )
    .into_iter()
    .map(resource)
    .collect()
}

fn resource_templates(server: &ToolServer) -> Vec<ResourceTemplate> {
    let catalog = server.snapshot();
    resource_template_descriptors(
        catalog.tools.get("search_content").is_some(),
        catalog.cache_reader.is_some(),
    )
    .into_iter()
    .map(resource_template)
    .collect()
}

/// Runs the call, then waits until every progress report it made is sent,
/// so none follows the response.
async fn call(
    tool: Arc<dyn AgentTool>,
    arguments: Value,
    context: &RequestContext<RoleServer>,
    approval: Arc<dyn Approval>,
    shutdown: CancellationToken,
) -> Result<Value, String> {
    let (monitor, forwarder) = Monitor::start(context, shutdown);
    let outcome = tool
        .call(Invocation {
            arguments,
            approval,
            monitor,
        })
        .await;
    if let Some(forwarder) = forwarder {
        let _ = forwarder.await;
    }
    outcome
}

async fn call_unmonitored(tool: Arc<dyn AgentTool>, arguments: Value) -> Result<Value, String> {
    tool.call(Invocation {
        arguments,
        approval: Arc::new(Unattended),
        monitor: Arc::new(Unmonitored),
    })
    .await
}

/// Progress reports from a tool, in order, as `notifications/progress`.
struct Monitor {
    cancel: CancellationToken,
    shutdown: CancellationToken,
    reports: Option<mpsc::UnboundedSender<ProgressNotificationParam>>,
    token: Option<ProgressToken>,
}

impl Monitor {
    /// The monitor, and the task that sends its reports while the call runs.
    /// Without a progress token there is nothing to send.
    fn start(
        context: &RequestContext<RoleServer>,
        shutdown: CancellationToken,
    ) -> (Arc<dyn CallMonitor>, Option<tokio::task::JoinHandle<()>>) {
        let token = context.meta.get_progress_token();
        let (reports, forwarder) = match token {
            Some(_) => {
                let (sender, receiver) = mpsc::unbounded_channel();
                let task = tokio::spawn(forward(receiver, context.peer.clone()));
                (Some(sender), Some(task))
            }
            None => (None, None),
        };
        let monitor = Monitor {
            cancel: context.ct.clone(),
            shutdown,
            reports,
            token,
        };
        (Arc::new(monitor), forwarder)
    }
}

impl CallMonitor for Monitor {
    fn progress(&self, done: usize, total: usize, message: &str) {
        if let (Some(reports), Some(token)) = (&self.reports, &self.token) {
            let report = ProgressNotificationParam::new(token.clone(), done as f64)
                .with_total(total as f64)
                .with_message(message);
            let _ = reports.send(report);
        }
    }

    fn cancelled(&self) -> bool {
        self.cancel.is_cancelled() || self.shutdown.is_cancelled()
    }
}

async fn forward(
    mut receiver: mpsc::UnboundedReceiver<ProgressNotificationParam>,
    peer: Peer<RoleServer>,
) {
    while let Some(report) = receiver.recv().await {
        if peer.notify_progress(report).await.is_err() {
            break;
        }
    }
}

pub fn mcp_tool(spec: &ToolSpec) -> Tool {
    let schema = match spec.input_schema() {
        Value::Object(schema) => schema,
        _ => Map::new(),
    };
    Tool::new(spec.name, spec.description, Arc::new(schema))
        .with_title(spec.title)
        .with_annotations(annotations(spec))
}

fn annotations(spec: &ToolSpec) -> ToolAnnotations {
    let read = spec.access == Access::Read;
    ToolAnnotations::from_raw(
        Some(spec.title.to_owned()),
        Some(read),
        (!read).then_some(true),
        (!read).then_some(false),
        Some(true),
    )
}

/// JSON text for every result, and `structuredContent` for an object, which
/// every protocol revision accepts there.
pub fn call_result(outcome: Result<Value, String>) -> CallToolResult {
    match outcome {
        Ok(value @ Value::Object(_)) => CallToolResult::structured(value),
        Ok(value) => CallToolResult::success(vec![ContentBlock::text(value.to_string())]),
        Err(error) => CallToolResult::error(vec![ContentBlock::text(error)]),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Identity, ToolServer, call_result, call_unmonitored, mcp_tool, named_call_result,
        resource_call, resource_descriptors, resource_template, resource_template_descriptors,
        resource_templates,
    };
    use async_trait::async_trait;
    use serde_json::{Value, json};
    use std::sync::Arc;
    use subspace_agent_tools::{
        Access, AgentTool, ApprovalPolicy, Arguments, Catalog, Invocation, ToolSpec,
    };

    struct Echo;

    #[test]
    fn bundled_help_image_uses_mcp_image_content_only_for_help() {
        let image = subspace_agent_tools::HelpImage::new(
            "dashboard",
            "Synthetic dashboard",
            "image/png",
            b"\x89PNG\r\n\x1a\ntest",
        );
        let value = image.reply("lab").unwrap();
        let result = named_call_result("board_help", Ok(value.clone()));
        let encoded = serde_json::to_value(result).unwrap();
        assert_eq!(encoded["content"][1]["type"], "image");
        assert_eq!(encoded["content"][1]["mimeType"], "image/png");
        assert!(encoded["content"][1]["data"].as_str().unwrap().len() > 8);
        let ordinary = serde_json::to_value(named_call_result("ordinary_read", Ok(value))).unwrap();
        assert!(
            ordinary["content"]
                .as_array()
                .unwrap()
                .iter()
                .all(|block| block["type"] == "text")
        );
    }

    #[async_trait]
    impl AgentTool for Echo {
        fn spec(&self) -> &ToolSpec {
            static SPEC: ToolSpec = ToolSpec {
                name: "echo",
                title: "Echo",
                description: "Echo",
                access: Access::Read,
                arguments: Arguments::Single { fields: Vec::new() },
                approval: ApprovalPolicy::None,
            };
            &SPEC
        }

        async fn call(&self, invocation: Invocation) -> Result<Value, String> {
            Ok(invocation.arguments)
        }
    }

    fn spec(access: Access) -> ToolSpec {
        ToolSpec {
            name: "github_write",
            title: "Write GitHub",
            description: "Write.",
            access,
            arguments: Arguments::Single { fields: Vec::new() },
            approval: if access == Access::Write {
                ApprovalPolicy::PerCall
            } else {
                ApprovalPolicy::None
            },
        }
    }

    #[test]
    fn tools_carry_their_schema_title_and_access_hints() {
        let write = serde_json::to_value(mcp_tool(&spec(Access::Write))).unwrap();
        assert_eq!(write["name"], "github_write");
        assert_eq!(write["title"], "Write GitHub");
        assert_eq!(write["inputSchema"]["type"], "object");
        assert_eq!(write["annotations"]["readOnlyHint"], false);
        assert_eq!(write["annotations"]["destructiveHint"], true);
        assert_eq!(write["annotations"]["idempotentHint"], false);
        let read = serde_json::to_value(mcp_tool(&spec(Access::Read))).unwrap();
        assert_eq!(read["annotations"]["readOnlyHint"], true);
        assert!(read["annotations"].get("destructiveHint").is_none());
    }

    #[test]
    fn objects_are_structured_other_values_are_text_and_failures_are_errors() {
        let object = call_result(Ok(json!({"id":7})));
        assert_eq!(object.structured_content, Some(json!({"id":7})));
        assert_eq!(object.is_error, Some(false));
        let list = call_result(Ok(json!([1, 2])));
        assert!(list.structured_content.is_none());
        let failed = call_result(Err("Repository is not configured: x/y".into()));
        assert_eq!(failed.is_error, Some(true));
        assert!(
            serde_json::to_string(&failed)
                .unwrap()
                .contains("not configured")
        );
    }

    #[tokio::test]
    async fn unmonitored_resource_calls_forward_arguments_and_result() {
        let result = call_unmonitored(Arc::new(Echo), json!({"ok": true}))
            .await
            .unwrap();
        assert_eq!(result, json!({"ok": true}));
    }

    #[test]
    fn resource_uris_map_to_the_matching_read_capability() {
        assert_eq!(
            resource_call("subspace://help"),
            Some(("board_help", json!({"action": "list"})))
        );
        assert_eq!(
            resource_call("subspace://search?q=needle&boards=irc,github"),
            Some((
                "search_content",
                json!({"query": "needle", "boards": ["irc", "github"]})
            ))
        );
        assert!(resource_call("subspace://unknown").is_none());
    }

    #[test]
    fn resource_descriptors_include_only_available_read_surfaces() {
        let resources = resource_descriptors(true);
        assert_eq!(resources.len(), 1);
        assert_eq!(resources[0].0, "subspace://help");
    }

    #[test]
    fn resource_template_descriptors_use_rfc6570_uris() {
        let templates = resource_template_descriptors(true, true);
        assert_eq!(templates.len(), 2);
        assert_eq!(templates[0].0, "subspace://search{?q,boards}");
        assert_eq!(templates[1].0, "subspace://cache/{board}/{account}/{id}");
        assert!(templates.iter().all(|template| !template.3.is_empty()));
    }

    #[test]
    fn resource_template_conversion_preserves_the_protocol_fields() {
        let template = resource_template((
            "subspace://search{?q,boards}",
            "search",
            "Cached search",
            "Search cached board content.",
        ));
        assert_eq!(template.uri_template, "subspace://search{?q,boards}");
        assert_eq!(template.name, "search");
        assert_eq!(template.title.as_deref(), Some("Cached search"));
        assert_eq!(
            template.description.as_deref(),
            Some("Search cached board content.")
        );
        assert_eq!(template.mime_type.as_deref(), Some("application/json"));
    }

    #[test]
    fn resource_templates_are_empty_when_indexed_capabilities_are_unavailable() {
        let server = ToolServer::new(
            Catalog::new(Vec::new()).unwrap(),
            Identity::new("subspace", "Subspace", "0.1.0"),
        );
        assert!(resource_templates(&server).is_empty());
    }

    #[test]
    fn cache_resources_follow_boards_independently_of_application_resources() {
        use crate::{Board, GroupedCatalog};
        let dir = tempfile::tempdir().unwrap();
        let mut environment = subspace_agent_tools::test_support::environment(Default::default());
        environment.search = Some(
            subspace_search::Service::open(dir.path().into(), "test".into(), vec!["test".into()])
                .unwrap(),
        );
        let boards = [Board {
            id: "test",
            title: "Test",
            description: "",
            help: "# Test",
            help_images: &[],
            tools: |_| Vec::new(),
        }];
        let mut settings = subspace_core::config::McpSettings::default();
        settings.groups.insert("application".into(), false);
        let catalog = GroupedCatalog::build(&boards, &[], &environment, &settings).unwrap();
        let server = ToolServer::new(
            catalog.served_catalog(),
            Identity::new("test", "Test", "test"),
        );
        assert!(super::resources(&server).is_empty());
        let templates = resource_templates(&server);
        assert_eq!(templates.len(), 1);
        assert_eq!(
            templates[0].uri_template,
            "subspace://cache/{board}/{account}/{id}"
        );
        settings.groups.insert("test".into(), false);
        let catalog = GroupedCatalog::build(&boards, &[], &environment, &settings).unwrap();
        server.replace_catalog(catalog.served_catalog());
        assert!(resource_templates(&server).is_empty());
    }

    #[test]
    fn host_cancellation_reaches_the_inflight_write_monitor() {
        use subspace_agent_tools::CallMonitor;
        let server = ToolServer::new(Catalog::default(), Identity::new("test", "Test", "test"));
        let monitor = super::Monitor {
            cancel: tokio_util::sync::CancellationToken::new(),
            shutdown: server.cancel.clone(),
            reports: None,
            token: None,
        };
        assert!(!monitor.cancelled());
        server.cancel();
        assert!(monitor.cancelled());
    }
}
