//! Only the bundled help tool may emit image content to MCP clients.

use rmcp::model::{CallToolResult, ContentBlock};
use serde_json::Value;

pub(crate) fn call_result(name: &str, outcome: Result<Value, String>) -> CallToolResult {
    if let Ok(value) = &outcome
        && let Some(image) = subspace_agent_tools::help_image_reply(name, value)
    {
        return CallToolResult::success(vec![
            ContentBlock::text(format!(
                "{} help image {}: {}",
                image.board_id, image.image_id, image.caption
            )),
            ContentBlock::image(image.data, image.media_type),
        ]);
    }
    super::server::call_result(outcome)
}
