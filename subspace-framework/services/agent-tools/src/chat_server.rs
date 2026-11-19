//! The running app's tools for an agent that speaks MCP itself, such as an
//! ACP chat: one loopback endpoint per chat session, whose write approvals
//! reach that chat. The app implements it; a chat runtime only opens it.
use std::sync::Arc;

use async_trait::async_trait;
use subspace_core::Secret;

use crate::Approval;

#[async_trait]
pub trait ChatToolServer: Send + Sync {
    /// Starts serving on the caller's async runtime. Every write asks
    /// `approval`; the endpoint stops when the returned value is dropped.
    async fn open(&self, approval: Arc<dyn Approval>) -> Result<ChatToolEndpoint, String>;

    /// What the endpoints serve, as their MCP `instructions` say it.
    fn instructions(&self) -> String;
}

/// One open endpoint: Streamable HTTP at `url`, which needs
/// `Authorization: Bearer <token>`.
pub struct ChatToolEndpoint {
    pub url: String,
    pub token: Secret,
    /// Stops the endpoint when dropped.
    pub lease: Box<dyn Send + Sync>,
}
