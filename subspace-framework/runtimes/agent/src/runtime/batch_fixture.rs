//! A harmless service behind the real shared batch implementation.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use subspace_agent_tools::{Access, ApprovalPolicy, Arguments, ToolSpec};
use subspace_agent_tools::{WriteService, WriteTool};

#[derive(Clone, Deserialize, Serialize)]
pub struct Note {
    body: String,
}

#[derive(Default)]
pub struct Notes(Mutex<Vec<String>>);

impl Notes {
    pub fn written(&self) -> Vec<String> {
        self.0.lock().unwrap().clone()
    }
}

impl WriteService for Notes {
    type Operation = Note;
    type Prepared = usize;

    fn validate(&self, _: &[Note]) -> Result<(), String> {
        Ok(())
    }

    fn preflight(&self, operations: &[Note]) -> Result<usize, String> {
        Ok(operations.len())
    }

    fn describe(&self, _: &[Note], count: &usize) -> String {
        format!("\nResolved {count} fixture notes")
    }

    fn write(&self, _: usize, operation: &Note, _: &usize) -> Result<Value, String> {
        self.0.lock().unwrap().push(operation.body.clone());
        Ok(json!({"body": operation.body}))
    }
}

pub fn tool() -> WriteTool<Notes> {
    WriteTool::new(
        ToolSpec {
            name: "fixture_write",
            title: "Fixture notes",
            description: "Write test-only notes.",
            access: Access::Write,
            arguments: Arguments::Single { fields: Vec::new() },
            approval: ApprovalPolicy::PerCall,
        },
        Arc::new(Notes::default()),
    )
}

pub fn arguments() -> Value {
    json!({"operations":[{"body":"first complete body"},{"body":"second complete body"}]})
}

pub async fn approval(events: &super::EventQueue) -> (u64, String, String) {
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let event = super::lock(&events.items).pop_front();
            if let Some(crate::Event::Chat {
                chat,
                event:
                    crate::ChatEvent::Approval {
                        request_id,
                        summary,
                    },
            }) = event
            {
                return (chat, request_id, summary);
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("fixture approval")
}
