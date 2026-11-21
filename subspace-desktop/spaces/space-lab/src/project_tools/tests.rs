mod approval;
#[cfg(unix)]
mod export;
mod inspection;

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde_json::Value;
use subspace_framework::agent_tools::{Approval, Invocation, Unmonitored};

use crate::project::{InputDeclaration, ProjectDefinition, ProjectRevision};
use crate::project_service::shared;

pub(super) struct Decision {
    allow: bool,
    summaries: Mutex<Vec<String>>,
}

#[async_trait]
impl Approval for Decision {
    async fn approve(&self, summary: String) -> Result<bool, String> {
        self.summaries.lock().unwrap().push(summary);
        Ok(self.allow)
    }
}

pub(super) fn invocation(arguments: Value, allow: bool) -> (Arc<Decision>, Invocation) {
    let decision = Arc::new(Decision {
        allow,
        summaries: Mutex::new(Vec::new()),
    });
    let call = Invocation {
        arguments,
        approval: decision.clone(),
        monitor: Arc::new(Unmonitored),
    };
    (decision, call)
}

pub(super) fn fixture(root: &std::path::Path) -> ProjectRevision {
    let service = shared(root).unwrap();
    let mut definition = ProjectDefinition::new("demo", "python-data", "main.py");
    definition.inputs.insert(
        "count".into(),
        InputDeclaration::Number {
            default: 1.0,
            min: 0.0,
            max: 10.0,
            step: 1.0,
        },
    );
    definition.environment_keys = vec!["API_KEY".into()];
    definition.destinations = vec!["https://api.example.com".into()];
    service
        .create(
            &definition,
            BTreeMap::from([("main.py".into(), b"print('example')".to_vec())]),
        )
        .unwrap()
        .revision
}
