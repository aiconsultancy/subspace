//! The app-wide chat panel. The shell retains it; boards use dashboard chat requests.
mod agent_chat;

#[cfg(feature = "test-support")]
pub use agent_chat::test_support;
pub use agent_chat::{
    ChatAttentionKind, ChatPanel, ChatPanelEvent, ChatToolsProvider, ContextProvider, OPEN_KEY,
};

use subspace_dashboard::{
    CloudSession, Tokens, action, anchored, chart, chat_bubble, chat_input as input,
    chat_sessions as shared, images, links, measure, prefs, rich, services, tokens, truncated,
};
