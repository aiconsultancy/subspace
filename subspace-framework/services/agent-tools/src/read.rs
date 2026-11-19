//! A read tool: one typed request, one blocking service call, no approval.

use super::{AgentTool, CallMonitor, Invocation, ToolSpec, blocking, decoded};
use async_trait::async_trait;
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::sync::Arc;

/// A board's read actions. `Request` is a strict, `action`-tagged enum, so an
/// unknown action or field is refused before any service call.
pub trait ReadService: Send + Sync + 'static {
    type Request: DeserializeOwned + Send + 'static;
    fn read(&self, request: Self::Request) -> Result<Value, String>;

    /// Boards with cancellable clients override this without changing older tools.
    fn read_monitored(
        &self,
        request: Self::Request,
        monitor: &dyn CallMonitor,
    ) -> Result<Value, String> {
        if monitor.cancelled() {
            return Err("The read was cancelled before it started.".into());
        }
        self.read(request)
    }
}

pub struct ReadTool<S: ReadService> {
    spec: ToolSpec,
    service: Arc<S>,
}

impl<S: ReadService> ReadTool<S> {
    pub fn new(spec: ToolSpec, service: Arc<S>) -> Self {
        Self { spec, service }
    }
}

#[async_trait]
impl<S: ReadService> AgentTool for ReadTool<S> {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    async fn call(&self, invocation: Invocation) -> Result<Value, String> {
        let request = decoded::<S::Request>(invocation.arguments)?;
        let service = self.service.clone();
        blocking(move || service.read_monitored(request, invocation.monitor.as_ref())).await
    }
}

#[cfg(test)]
mod tests {
    use super::{ReadService, ReadTool};
    use crate::{Access, AgentTool, ApprovalPolicy, Arguments, ToolSpec, test_support};
    use serde::Deserialize;
    use serde_json::{Value, json};
    use std::sync::Arc;

    struct Cancelled;

    impl crate::CallMonitor for Cancelled {
        fn cancelled(&self) -> bool {
            true
        }
    }

    #[derive(Deserialize)]
    #[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
    enum Request {
        Echo { text: String },
    }

    struct Echo;

    impl ReadService for Echo {
        type Request = Request;
        fn read(&self, request: Request) -> Result<Value, String> {
            let Request::Echo { text } = request;
            Ok(json!({ "text": text }))
        }
    }

    #[tokio::test]
    async fn a_read_decodes_strictly_and_never_asks_for_approval() {
        let tool = ReadTool::new(
            ToolSpec {
                name: "echo_read",
                title: "Echo",
                description: "Echo",
                access: Access::Read,
                arguments: Arguments::Single { fields: Vec::new() },
                approval: ApprovalPolicy::None,
            },
            Arc::new(Echo),
        );
        let (approval, invocation) =
            test_support::invocation(json!({"action":"echo","text":"hi"}), true);
        assert_eq!(tool.call(invocation).await.unwrap()["text"], "hi");
        let (_, unknown) = test_support::invocation(json!({"action":"raw_http"}), true);
        assert!(tool.call(unknown).await.is_err());
        let (_, extra) =
            test_support::invocation(json!({"action":"echo","text":"hi","token":"x"}), true);
        assert!(tool.call(extra).await.is_err());
        assert!(approval.summaries().is_empty());
    }

    #[tokio::test]
    async fn a_cancelled_read_does_not_start_a_service_call() {
        let tool = ReadTool::new(
            ToolSpec {
                name: "echo_read",
                title: "Echo",
                description: "Echo",
                access: Access::Read,
                arguments: Arguments::Single { fields: Vec::new() },
                approval: ApprovalPolicy::None,
            },
            Arc::new(Echo),
        );
        let (_, mut invocation) =
            test_support::invocation(json!({"action":"echo","text":"hi"}), true);
        invocation.monitor = Arc::new(Cancelled);
        assert!(tool.call(invocation).await.is_err());
    }
}
