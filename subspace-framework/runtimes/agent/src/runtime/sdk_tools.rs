//! The selected boards' agent tools as SDK tools. Each chat registers its own
//! copies, so a write tool's approval reaches the tab that started the turn.

use async_trait::async_trait;
use github_copilot_sdk::{
    Tool, ToolResult,
    tool::ToolHandler,
    types::{ToolBinaryResult, ToolInvocation, ToolResultExpanded},
};
use serde_json::{Value, json};
use std::sync::Arc;
use subspace_agent_tools::{AgentTool, Approval, Catalog, Invocation, Unmonitored};

struct SdkTool {
    tool: Arc<dyn AgentTool>,
    approval: Arc<dyn Approval>,
}

#[async_trait]
impl ToolHandler for SdkTool {
    async fn call(
        &self,
        invocation: ToolInvocation,
    ) -> Result<ToolResult, github_copilot_sdk::Error> {
        let outcome = self
            .tool
            .call(Invocation {
                arguments: invocation.arguments,
                approval: self.approval.clone(),
                monitor: Arc::new(Unmonitored),
            })
            .await;
        Ok(result(self.tool.spec().name, outcome))
    }
}

/// Every tool skips the SDK's generic prompt: reads need none, and a write
/// tool asks for its own exact approval of the whole batch.
pub(super) fn definitions(catalog: &Catalog, approval: Arc<dyn Approval>) -> Vec<Tool> {
    catalog
        .tools()
        .iter()
        .map(|tool| {
            let spec = tool.spec();
            Tool::new(spec.name)
                .with_description(spec.description)
                .with_parameters(spec.input_schema())
                .with_skip_permission(true)
                .with_handler(Arc::new(SdkTool {
                    tool: tool.clone(),
                    approval: approval.clone(),
                }))
        })
        .collect()
}

fn result(tool_name: &str, outcome: Result<Value, String>) -> ToolResult {
    if let Ok(value) = &outcome
        && let Some(image) = subspace_agent_tools::help_image_reply(tool_name, value)
    {
        return image_result(image);
    }
    ToolResult::Text(match outcome {
        Ok(value) => value.to_string(),
        Err(error) => json!({ "error": error }).to_string(),
    })
}

fn image_result(image: subspace_agent_tools::help_image::HelpImageReply) -> ToolResult {
    ToolResult::Expanded(
        ToolResultExpanded::new(
            format!(
                "{} help image {}: {}",
                image.board_id, image.image_id, image.caption
            ),
            "success",
        )
        .with_binary_results(vec![ToolBinaryResult {
            data: image.data,
            mime_type: image.media_type,
            r#type: "image".into(),
            description: Some(image.caption),
        }]),
    )
}

#[cfg(test)]
mod tests {
    use super::{SdkTool, definitions, result};
    use crate::runtime::{
        EventQueue, batch_fixture,
        decisions::{ChatApproval, Decisions},
    };
    use github_copilot_sdk::ToolResult;
    use github_copilot_sdk::{tool::ToolHandler as _, types::ToolInvocation};
    use serde_json::{Value, json};
    use std::sync::Arc;
    use std::sync::Mutex;
    use subspace_agent_tools::test_support::RecordingApproval;
    use subspace_agent_tools::{
        Access, AgentTool, ApprovalPolicy, Arguments, Catalog, Invocation, ToolSpec,
    };

    struct Named(ToolSpec);

    #[test]
    fn bundled_help_image_uses_the_sdk_binary_envelope_only_for_help() {
        let image = subspace_agent_tools::HelpImage::new(
            "dashboard",
            "Synthetic dashboard",
            "image/png",
            b"\x89PNG\r\n\x1a\ntest",
        );
        let value = image.reply("lab").unwrap();
        let ToolResult::Expanded(expanded) = result("board_help", Ok(value.clone())) else {
            panic!("Expected SDK image result")
        };
        let binary = expanded.binary_results_for_llm.unwrap();
        assert_eq!(binary.len(), 1);
        assert_eq!(binary[0].mime_type, "image/png");
        assert_eq!(binary[0].r#type, "image");
        assert_eq!(
            binary[0].description.as_deref(),
            Some("Synthetic dashboard")
        );
        assert!(!expanded.text_result_for_llm.contains(&binary[0].data));
        assert!(matches!(
            result("ordinary_read", Ok(value)),
            ToolResult::Text(_)
        ));
    }

    #[async_trait::async_trait]
    impl AgentTool for Named {
        fn spec(&self) -> &ToolSpec {
            &self.0
        }
        async fn call(&self, _invocation: Invocation) -> Result<Value, String> {
            Ok(json!({}))
        }
    }

    #[test]
    fn each_catalog_tool_becomes_an_sdk_tool_with_its_schema() {
        let tool: Arc<dyn AgentTool> = Arc::new(Named(ToolSpec {
            name: "github_write",
            title: "Write GitHub",
            description: "Write.",
            access: Access::Write,
            arguments: Arguments::Single { fields: Vec::new() },
            approval: ApprovalPolicy::PerCall,
        }));
        let catalog = Catalog::new(vec![tool]).unwrap();
        let tools = definitions(&catalog, Arc::new(RecordingApproval::new(false)));
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "github_write");
        let ToolResult::Text(text) = result("github_write", Err("Visible service error".into()))
        else {
            panic!("Expected text")
        };
        assert_eq!(
            serde_json::from_str::<Value>(&text).unwrap()["error"],
            "Visible service error"
        );
    }

    #[tokio::test]
    async fn sdk_call_waits_for_its_chat_request_and_denial_writes_nothing() {
        let events = Arc::new(EventQueue::default());
        let decisions = Arc::new(Decisions {
            events: events.clone(),
            pending: Mutex::new(Default::default()),
            chats: Mutex::new(Default::default()),
            workspace: Default::default(),
        });
        let tool = batch_fixture::tool();
        let service = tool.service().clone();
        let sdk = SdkTool {
            tool: Arc::new(tool),
            approval: Arc::new(ChatApproval {
                decisions: decisions.clone(),
                chat: 7,
            }),
        };
        let mut invocation = ToolInvocation::default();
        invocation.session_id = "fixture-session-seven".into();
        invocation.tool_name = "fixture_write".into();
        invocation.tool_call_id = "fixture-call".into();
        invocation.arguments = batch_fixture::arguments();
        let pending = tokio::spawn(async move { sdk.call(invocation).await });
        let (chat, request, summary) = batch_fixture::approval(&events).await;
        assert_eq!(chat, 7);
        assert!(
            summary.contains("first complete body") && summary.contains("second complete body")
        );
        assert!(summary.ends_with("Resolved 2 fixture notes"));
        assert!(
            service.written().is_empty(),
            "nothing may write before confirmation"
        );
        decisions.approve("another-request", true);
        assert!(
            !pending.is_finished(),
            "another request cannot approve this call"
        );
        decisions.approve(&request, false);
        let ToolResult::Text(result) = pending.await.unwrap().unwrap() else {
            panic!("SDK text result");
        };
        assert_eq!(
            serde_json::from_str::<Value>(&result).unwrap()["approved"],
            false
        );
        assert!(service.written().is_empty());
        assert!(
            crate::runtime::lock(&events.items).is_empty(),
            "one batch asks once"
        );
    }
}
