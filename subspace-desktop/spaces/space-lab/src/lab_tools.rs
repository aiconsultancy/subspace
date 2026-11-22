//! Board-owned fixture and native project tools for Copilot and stdio MCP.
//! No model request is made here. General Copilot built-ins retain their own
//! SDK policy; these tools do not claim to sandbox an entire chat session.

mod approval;
mod catalog;
mod edit;
mod environment;
mod helpers;
mod inspect;
mod run;
mod schema;

#[cfg(test)]
mod tests;

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use sha2::{Digest, Sha256};
use subspace_framework::agent_tools::{AgentTool, Invocation, ToolEnvironment, ToolSpec};

use crate::environments::EnvironmentManager;
use crate::studio::Studio;
use schema::Input;

const MAX_INPUT: usize = 256 * 1024;

#[derive(Clone, Copy)]
enum Group {
    Inspect,
    Edit,
    Run,
    Environment,
}

type Handler = fn(&Studio, &EnvironmentManager, &std::path::Path, Input) -> Result<Value>;

struct LabTool {
    spec: ToolSpec,
    group: Group,
    root: PathBuf,
    handler: Handler,
}

pub fn tools_for_root(environment: &ToolEnvironment, root: PathBuf) -> Vec<Arc<dyn AgentTool>> {
    let mut tools = catalog::tools(root.clone());
    tools.extend(crate::project_tools::tools_for_root(environment, root));
    tools
}

/// Compatibility entry point for callers that use the product-owned root.
pub fn tools(environment: &ToolEnvironment) -> Vec<Arc<dyn AgentTool>> {
    crate::workbench_root()
        .map(|root| tools_for_root(environment, root))
        .unwrap_or_default()
}

#[async_trait]
impl AgentTool for LabTool {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    async fn call(&self, invocation: Invocation) -> Result<Value, String> {
        if invocation.arguments.to_string().len() > MAX_INPUT {
            return Err("Lab request exceeds 256 KiB".into());
        }
        let input: Input =
            serde_json::from_value(invocation.arguments).map_err(|e| e.to_string())?;
        if invocation.monitor.cancelled() {
            return Err("Cancelled".into());
        }
        if approval::needs_approval(self.group, &input.action) && !invocation.approval.approve(format!(
            "{}: {}. Environment {:?}; preset {:?}; package {:?}; tool {:?}; version {:?}; destinations {:?}; recipe {:?}; code {} bytes, SHA256 {:?}; network requested {}. No private service credentials. Networked execution uses only configured destinations.",
            self.spec.title, input.action, input.environment,input.preset.as_ref().map(|preset|(&preset.id,&preset.recipe)),input.package,input.tool,input.version,input.destinations,input.id,input.source.as_ref().map_or(0,String::len),input.source.as_ref().map(|source|hex::encode(Sha256::digest(source.as_bytes()))),input.network.unwrap_or(false),
        )).await? { return Err("Not approved".into()); }
        invocation.monitor.progress(0, 1, self.spec.title);
        let handler = self.handler;
        let root = self.root.clone();
        let output = subspace_framework::agent_tools::blocking(move || {
            execute(handler, input, root).map_err(|e| format!("{e:#}"))
        })
        .await?;
        if output.to_string().len() > MAX_INPUT {
            return Err("Lab result exceeds 256 KiB".into());
        }
        invocation.monitor.progress(1, 1, "Finished");
        Ok(output)
    }
}

fn execute(handler: Handler, input: Input, root: PathBuf) -> Result<Value> {
    let studio = Studio::new(root.clone());
    let manager = EnvironmentManager::new(root.join("environments"));
    handler(&studio, &manager, &root, input)
}
