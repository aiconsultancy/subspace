//! The app-wide chat panel. The shell retains it; boards use dashboard chat requests.
mod agent_chat;

#[cfg(feature = "test-support")]
pub use agent_chat::test_support;
pub use agent_chat::{
    ChatAttentionKind, ChatPanel, ChatPanelEvent, ChatToolsProvider, ContextProvider, OPEN_KEY,
};
/// Whether this build has the Copilot chat engine. Settings offers it only then.
pub use subspace_agent::COPILOT_BUILT_IN;

use subspace_dashboard::{
    CloudSession, Tokens, action, anchored, chart, chat_bubble, chat_input as input,
    chat_sessions as shared, images, links, measure, prefs, rich, services, tokens, truncated,
};
