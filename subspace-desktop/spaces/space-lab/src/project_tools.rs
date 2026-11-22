//! Structured agent access to projects owned by the shared root service.

mod actions;
mod approval;
mod catalog;
mod execute;

#[cfg(test)]
mod tests;

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use subspace_framework::agent_tools::{AgentTool, Invocation, ToolEnvironment, ToolSpec};

use actions::Request;

const MAX_MESSAGE: usize = 256 * 1024;

struct ProjectTool {
    spec: ToolSpec,
    root: PathBuf,
    write: bool,
}

/// Construct the project tools for the same root used by the native UI.
pub fn tools_for_root(_: &ToolEnvironment, root: PathBuf) -> Vec<Arc<dyn AgentTool>> {
    vec![tool(root.clone(), false), tool(root, true)]
}

fn tool(root: PathBuf, write: bool) -> Arc<dyn AgentTool> {
    Arc::new(ProjectTool {
        spec: catalog::spec(write),
        root,
        write,
    })
}

#[async_trait]
impl AgentTool for ProjectTool {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    async fn call(&self, invocation: Invocation) -> Result<Value, String> {
        if invocation.arguments.to_string().len() > MAX_MESSAGE {
            return Err("Project request exceeds 256 KiB".into());
        }
        let request = Request::parse(invocation.arguments, self.write)?;
        if invocation.monitor.cancelled() {
            return Err("Cancelled".into());
        }
        let root = self.root.clone();
        let monitor = invocation.monitor.clone();
        let prepared = subspace_framework::agent_tools::blocking(move || {
            if monitor.cancelled() {
                return Err("Cancelled".into());
            }
            approval::prepare(&root, request).map_err(|_| {
                "Project request could not be prepared; check its revision and fields".into()
            })
        })
        .await?;
        if self.write
            && !invocation
                .approval
                .approve(prepared.summary.clone())
                .await?
        {
            return Err("Not approved".into());
        }
        if invocation.monitor.cancelled() {
            return Err("Cancelled".into());
        }
        let root = self.root.clone();
        let monitor = invocation.monitor.clone();
        let result = subspace_framework::agent_tools::blocking(move || {
            if monitor.cancelled() {
                return Err("Cancelled".into());
            }
            execute::run(&root, prepared.request).map_err(safe_error)
        })
        .await?;
        if result.to_string().len() > MAX_MESSAGE {
            return Err("Project result exceeds 256 KiB".into());
        }
        Ok(result)
    }
}

fn safe_error(error: anyhow::Error) -> String {
    const UNSUPPORTED: &str = "private storage permissions unavailable on this platform";
    if error.chain().any(|cause| cause.to_string() == UNSUPPORTED) {
        UNSUPPORTED.into()
    } else {
        "Project operation failed; check the revision, path and project state".into()
    }
}
