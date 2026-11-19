use std::sync::Arc;

/// An image kept in the chat draft until the user explicitly submits it.
/// Shared bytes let the UI retain a rejected draft without copying its payload.
#[derive(Clone, PartialEq, Eq)]
pub struct ImageAttachment {
    pub id: u64,
    pub data: Arc<[u8]>,
    pub mime_type: String,
    pub display_name: String,
}

impl std::fmt::Debug for ImageAttachment {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ImageAttachment")
            .field("id", &self.id)
            .field("bytes", &self.data.len())
            .field("mime_type", &self.mime_type)
            .field("display_name", &self.display_name)
            .finish()
    }
}

/// One draft's total encoded image budget, matching the existing private
/// attachment-workspace budget.
pub const DRAFT_ATTACHMENT_LIMIT: usize = 20 * 1024 * 1024;
