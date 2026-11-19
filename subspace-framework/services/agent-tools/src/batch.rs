//! Batch validation, remote preflight and invocation-scoped approval for a
//! write tool. Every operation is checked and its target read before the one
//! approval; approved operations run once, in order, and are never retried.

use super::{AgentTool, Approval, CallMonitor, Invocation, ToolSpec, blocking, decoded};
use async_trait::async_trait;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use std::sync::Arc;

/// The most operations, and the most items such as risk ids, in one batch.
pub const MAX_BATCH: usize = 20;

pub const NO_RETRY: &str = "No retry was attempted. A network or response error can mean the mutation succeeded; inspect the target before retrying.";

const NOT_STARTED: &str = "The call was cancelled before this operation started.";

/// A board's write actions.
pub trait WriteService: Send + Sync + 'static {
    /// A strict, `action`-tagged enum; the preview prints it as JSON.
    type Operation: Serialize + DeserializeOwned + Clone + Send + Sync + 'static;
    /// What the preflight learned, for the preview and for the writes.
    type Prepared: Send + Sync + 'static;

    /// The checks that need no service call, such as targets in scope.
    fn validate(&self, operations: &[Self::Operation]) -> Result<(), String>;
    /// Reads each target before the approval.
    fn preflight(&self, operations: &[Self::Operation]) -> Result<Self::Prepared, String>;
    /// What the JSON preview cannot show, such as resolved titles.
    fn describe(&self, operations: &[Self::Operation], prepared: &Self::Prepared) -> String;
    /// Config edits and other sensitive operations cannot use client-side or
    /// unattended approval. Existing service writes retain normal approval.
    fn requires_app_approval(&self, _operations: &[Self::Operation]) -> bool {
        false
    }
    /// Runs operation `index` exactly as the preflight prepared it.
    fn write(
        &self,
        index: usize,
        operation: &Self::Operation,
        prepared: &Self::Prepared,
    ) -> Result<Value, String>;

    /// Cancellable preflight; existing boards keep their current behavior.
    fn preflight_monitored(
        &self,
        operations: &[Self::Operation],
        monitor: &dyn CallMonitor,
    ) -> Result<Self::Prepared, String> {
        if monitor.cancelled() {
            return Err(NOT_STARTED.into());
        }
        self.preflight(operations)
    }

    /// Cancellable active operation; called only after the invocation's approval.
    fn write_monitored(
        &self,
        index: usize,
        operation: &Self::Operation,
        prepared: &Self::Prepared,
        _monitor: &dyn CallMonitor,
    ) -> Result<Value, String> {
        self.write(index, operation, prepared)
    }
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields, bound = "T: DeserializeOwned")]
struct Batch<T> {
    operations: Vec<T>,
}

// qual:api
pub struct WriteTool<S: WriteService> {
    spec: ToolSpec,
    service: Arc<S>,
}

impl<S: WriteService> WriteTool<S> {
    pub fn new(spec: ToolSpec, service: Arc<S>) -> Self {
        Self { spec, service }
    }

    pub fn service(&self) -> &Arc<S> {
        &self.service
    }

    /// Validates, previews, asks once and runs the approved batch.
    pub async fn execute(
        &self,
        operations: Vec<S::Operation>,
        approval: Arc<dyn Approval>,
        monitor: Arc<dyn CallMonitor>,
    ) -> Result<Value, String> {
        let batch = Arc::new(operations);
        let (service, checked) = (self.service.clone(), batch.clone());
        let preflight_monitor = monitor.clone();
        let prepared =
            blocking(move || prepare(service.as_ref(), &checked, preflight_monitor.as_ref()))
                .await?;
        if monitor.cancelled() {
            return Err(NOT_STARTED.into());
        }
        let preview = summary(&batch, &self.service.describe(&batch, &prepared))?;
        let allowed = if self.service.requires_app_approval(&batch) {
            approval.approve_in_app(preview).await?
        } else {
            approval.approve(preview).await?
        };
        if !allowed {
            return Ok(denied());
        }
        let service = self.service.clone();
        blocking(move || Ok(run(service.as_ref(), &batch, &prepared, monitor.as_ref()))).await
    }
}

#[async_trait]
impl<S: WriteService> AgentTool for WriteTool<S> {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    async fn call(&self, invocation: Invocation) -> Result<Value, String> {
        let batch = decoded::<Batch<S::Operation>>(invocation.arguments)?;
        self.execute(batch.operations, invocation.approval, invocation.monitor)
            .await
    }
}

fn prepare<S: WriteService>(
    service: &S,
    operations: &[S::Operation],
    monitor: &dyn CallMonitor,
) -> Result<S::Prepared, String> {
    validate_size(operations.len())?;
    service.validate(operations)?;
    service.preflight_monitored(operations, monitor)
}

fn validate_size(len: usize) -> Result<(), String> {
    if len == 0 || len > MAX_BATCH {
        Err("Batch requires 1..20 operations.".into())
    } else {
        Ok(())
    }
}

/// Every target, revision, field and complete body. JSON escapes control
/// characters, so they cannot hide actions in the approval preview.
pub fn summary<T: Serialize>(operations: &[T], details: &str) -> Result<String, String> {
    let summary = serde_json::to_string_pretty(operations)
        .map(|body| {
            format!(
                "Apply exactly these {} service write operations once:\n{body}{details}",
                operations.len()
            )
        })
        .map_err(|error| error.to_string())?;
    if crate::secrets::looks_like_credential(&summary) {
        Err("The proposed operation includes credential-like data and cannot be approved.".into())
    } else {
        Ok(summary)
    }
}

fn denied() -> Value {
    json!({"approved":false,"results":[],"message":"No operations were executed."})
}

fn run<S: WriteService>(
    service: &S,
    operations: &[S::Operation],
    prepared: &S::Prepared,
    monitor: &dyn CallMonitor,
) -> Value {
    let total = operations.len();
    let results: Vec<Value> = operations
        .iter()
        .enumerate()
        .map(|(index, operation)| {
            step(index, total, monitor, || {
                service.write_monitored(index, operation, prepared, monitor)
            })
        })
        .collect();
    json!({"approved":true,"results":results})
}

/// One operation, unless the caller has gone: then nothing starts.
fn step(
    index: usize,
    total: usize,
    monitor: &dyn CallMonitor,
    write: impl FnOnce() -> Result<Value, String>,
) -> Value {
    if monitor.cancelled() {
        return json!({"index":index,"success":false,"started":false,"error":NOT_STARTED});
    }
    monitor.progress(index, total, &format!("Operation {} of {total}", index + 1));
    match write() {
        Ok(value) => json!({"index":index,"success":true,"value":value}),
        Err(error) => json!({"index":index,"success":false,"error":error,"warning":NO_RETRY}),
    }
}

#[cfg(test)]
mod tests {
    use super::{WriteService, WriteTool, summary};
    use crate::test_support::{self, RecordingMonitor};
    use crate::{Access, AgentTool, ApprovalPolicy, Arguments, ToolSpec};
    use serde::{Deserialize, Serialize};
    use serde_json::{Value, json};
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Serialize, Deserialize)]
    #[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
    enum Operation {
        Note { body: String },
    }

    /// Fails the operation whose body is `fail`; refuses `invalid` before
    /// the preflight.
    struct Notes {
        written: Mutex<Vec<String>>,
        preflights: Mutex<usize>,
    }

    impl WriteService for Notes {
        type Operation = Operation;
        type Prepared = usize;
        fn validate(&self, operations: &[Operation]) -> Result<(), String> {
            match operations
                .iter()
                .any(|Operation::Note { body }| body == "invalid")
            {
                true => Err("Invalid note.".into()),
                false => Ok(()),
            }
        }
        fn preflight(&self, operations: &[Operation]) -> Result<usize, String> {
            *self.preflights.lock().unwrap() += 1;
            Ok(operations.len())
        }
        fn describe(&self, _: &[Operation], prepared: &usize) -> String {
            format!("\n\nprepared {prepared}")
        }
        fn requires_app_approval(&self, operations: &[Operation]) -> bool {
            operations
                .iter()
                .any(|Operation::Note { body }| body == "sensitive")
        }
        fn write(&self, _: usize, operation: &Operation, _: &usize) -> Result<Value, String> {
            let Operation::Note { body } = operation;
            self.written.lock().unwrap().push(body.clone());
            match body.as_str() {
                "fail" => Err("Ambiguous network failure.".into()),
                _ => Ok(json!({ "body": body })),
            }
        }
    }

    fn tool() -> WriteTool<Notes> {
        WriteTool::new(
            ToolSpec {
                name: "notes_write",
                title: "Notes",
                description: "Notes",
                access: Access::Write,
                arguments: Arguments::Single { fields: Vec::new() },
                approval: ApprovalPolicy::PerCall,
            },
            Arc::new(Notes {
                written: Mutex::new(Vec::new()),
                preflights: Mutex::new(0),
            }),
        )
    }

    fn notes(bodies: &[&str]) -> Value {
        let operations: Vec<Value> = bodies
            .iter()
            .map(|body| json!({"action":"note","body":body}))
            .collect();
        json!({ "operations": operations })
    }

    fn written(tool: &WriteTool<Notes>) -> Vec<String> {
        tool.service().written.lock().unwrap().clone()
    }

    #[tokio::test]
    async fn a_denied_batch_writes_nothing_and_asks_once_with_every_body() {
        let tool = tool();
        let (approval, invocation) = test_support::invocation(notes(&["first", "second"]), false);
        let value = tool.call(invocation).await.unwrap();
        assert_eq!(value["approved"], false);
        assert!(written(&tool).is_empty());
        let summaries = approval.summaries();
        assert_eq!(summaries.len(), 1);
        assert!(summaries[0].contains("first") && summaries[0].contains("second"));
        assert!(summaries[0].ends_with("prepared 2"));
    }

    #[tokio::test]
    async fn sensitive_writes_refuse_unattended_approval_but_accept_manual_decisions() {
        let tool = tool();
        let invocation = crate::Invocation {
            arguments: notes(&["sensitive"]),
            approval: Arc::new(crate::Unattended),
            monitor: Arc::new(crate::Unmonitored),
        };
        assert_eq!(tool.call(invocation).await.unwrap()["approved"], false);
        assert!(written(&tool).is_empty());
        let (_, refused) = test_support::invocation(notes(&["sensitive"]), false);
        assert_eq!(tool.call(refused).await.unwrap()["approved"], false);
        assert!(written(&tool).is_empty());
        let (approval, accepted) = test_support::invocation(notes(&["sensitive"]), true);
        assert_eq!(tool.call(accepted).await.unwrap()["approved"], true);
        assert_eq!(written(&tool), ["sensitive"]);
        assert_eq!(approval.summaries().len(), 1);
    }

    #[tokio::test]
    async fn an_invalid_or_oversized_batch_is_refused_before_preflight_and_approval() {
        let tool = tool();
        for arguments in [
            notes(&["first", "invalid"]),
            notes(&[]),
            notes(&["x"; 21]),
            json!({"operations":[],"extra":true}),
        ] {
            let (approval, invocation) = test_support::invocation(arguments, true);
            assert!(tool.call(invocation).await.is_err());
            assert!(approval.summaries().is_empty());
        }
        assert_eq!(*tool.service().preflights.lock().unwrap(), 0);
        assert!(written(&tool).is_empty());
    }

    #[tokio::test]
    async fn a_partial_failure_is_visible_never_retried_and_approval_does_not_carry_over() {
        let tool = tool();
        let (approval, invocation) = test_support::invocation(notes(&["fail", "second"]), true);
        let value = tool.call(invocation).await.unwrap();
        assert_eq!(value["results"][0]["success"], false);
        assert!(
            value["results"][0]["warning"]
                .as_str()
                .unwrap()
                .contains("No retry")
        );
        assert_eq!(value["results"][1]["success"], true);
        assert_eq!(written(&tool), ["fail", "second"]);
        let (_, again) = test_support::invocation(notes(&["third"]), true);
        tool.call(again).await.unwrap();
        assert_eq!(
            approval.summaries().len(),
            1,
            "each call asks its own approval"
        );
        assert_eq!(written(&tool).len(), 3);
    }

    #[tokio::test]
    async fn progress_precedes_each_operation_and_cancellation_starts_no_more() {
        let tool = tool();
        let monitor = Arc::new(RecordingMonitor::cancel_after(1));
        let value = tool
            .execute(
                vec![
                    Operation::Note { body: "a".into() },
                    Operation::Note { body: "b".into() },
                    Operation::Note { body: "c".into() },
                ],
                Arc::new(crate::Unattended),
                monitor.clone(),
            )
            .await
            .unwrap();
        assert_eq!(monitor.reports(), [(0, 3)]);
        assert_eq!(written(&tool), ["a"]);
        assert_eq!(value["results"][1]["started"], false);
        assert_eq!(value["results"][2]["started"], false);
    }

    #[test]
    fn the_preview_escapes_control_characters_and_counts_operations() {
        let text = summary(
            &[Operation::Note {
                body: "exact\ncomment".into(),
            }],
            "",
        )
        .unwrap();
        assert!(text.starts_with("Apply exactly these 1 service write operations once:"));
        assert!(text.contains("exact\\ncomment"));
    }

    #[test]
    fn a_credential_like_preview_is_refused_before_any_surface_sees_it() {
        let error = summary(
            &[Operation::Note {
                body: "github_pat_not-a-real-token".into(),
            }],
            "",
        )
        .unwrap_err();
        assert!(error.contains("credential-like data"), "{error}");
    }
}
