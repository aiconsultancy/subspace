//! The draft's limits on its connection: how many lines and bytes it may
//! hold, and whether it is over them.

use gpui_kit::Context;
use subspace_client_irc::{MAX_INLINE_BYTES, check_draft, connection_batches, draft_line_limit};
use subspace_framework::dashboard::chat_input::DraftLimits;

use super::Draft;
use crate::conversation::Conversation;

/// Whether the conversation's connection sends multi-line text as batches,
/// which lets a draft hold more lines.
pub(crate) fn batches_lines(conversation: &Conversation, cx: &gpui_kit::App) -> bool {
    conversation
        .hub
        .read(cx)
        .state
        .network(&conversation.buffer.network)
        .is_some_and(|network| connection_batches(&network.caps))
}

/// Re-checks the draft against the connection's current limits, for caps
/// that arrive or change while a draft is open.
pub(crate) fn revalidate(conversation: &mut Conversation, cx: &mut Context<Conversation>) {
    let text = conversation.draft.entity.read(cx).text(cx);
    let batches = batches_lines(conversation, cx);
    validate_draft(&mut conversation.draft, &text, batches, cx);
}

/// Marks a draft over the limit for its connection: `batches` is whether
/// that connection sends multi-line text as one batch.
pub(super) fn validate_draft(
    draft: &mut Draft,
    text: &str,
    batches: bool,
    cx: &mut Context<Conversation>,
) {
    let oversized = check_draft(text, batches).is_err();
    let limits = DraftLimits::new(draft_line_limit(text, batches), MAX_INLINE_BYTES);
    draft
        .entity
        .update(cx, |input, cx| input.set_limits(Some(limits), cx));
    if oversized != draft.oversized {
        draft.oversized = oversized;
        draft.entity.update(cx, |input, cx| {
            input.set_submit_blocked(oversized, cx);
        });
        cx.notify();
    }
}
