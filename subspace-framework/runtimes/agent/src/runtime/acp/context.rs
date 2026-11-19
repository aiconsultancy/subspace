//! Subspace's context for an ACP agent: the boards and tools its MCP server
//! serves. An agent that advertises `sessionCapabilities.systemPrompt` gets
//! it appended to its system prompt on `session/new` and `session/load`;
//! any other agent gets it once, as a marked block before the first prompt
//! of a new session.
use super::{ChatId, Value, json};
use subspace_core::log::{Level, Logger};

const OPEN: &str = "<subspace-context>";
const CLOSE: &str = "</subspace-context>";
const PREAMBLE: &str =
    "Context from Subspace, the app hosting this chat. It is not part of the user's message.";

/// How the context reached the agent, for the log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Delivery {
    SystemPrompt,
    FirstPrompt,
}

/// The `systemPrompt` of a session request, in append mode.
pub(super) fn system_prompt(text: &str) -> Value {
    json!({"mode": "append", "content": [{"type": "text", "text": text}]})
}

/// The text block that leads a session's first prompt.
pub(super) fn prompt_block(text: &str) -> Value {
    json!({"type": "text", "text": format!("{OPEN}\n{PREAMBLE}\n\n{text}\n{CLOSE}")})
}

/// A replayed user message without the context block that led it.
pub(super) fn without_context(text: &str) -> &str {
    match text
        .strip_prefix(OPEN)
        .and_then(|rest| rest.split_once(CLOSE))
    {
        Some((_, message)) => message.trim_start(),
        None => text,
    }
}

/// A debug record of the context an agent received, so it can be read in
/// the log viewer. It holds board ids, tool names and rules, never secrets.
pub(super) fn log_delivery(chat: ChatId, delivery: Delivery, text: &str) {
    let delivery = match delivery {
        Delivery::SystemPrompt => "system_prompt",
        Delivery::FirstPrompt => "first_prompt",
    };
    let chat = chat.to_string();
    Logger::new("agent").record(
        Level::Debug,
        "acp session context sent",
        &[("chat", &chat), ("delivery", delivery), ("context", text)],
    );
}

#[cfg(test)]
mod tests {
    use super::{prompt_block, system_prompt, without_context};

    #[test]
    fn the_system_prompt_appends_one_text_block() {
        let prompt = system_prompt("Boards");
        assert_eq!(prompt["mode"], "append");
        assert_eq!(prompt["content"][0]["type"], "text");
        assert_eq!(prompt["content"][0]["text"], "Boards");
        assert_eq!(prompt["content"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn a_replay_drops_the_marked_block_and_keeps_the_message() {
        let block = prompt_block("Boards");
        let block = block["text"].as_str().unwrap();
        assert!(block.starts_with("<subspace-context>\nContext from Subspace"));
        assert_eq!(without_context(block), "");
        assert_eq!(without_context(&format!("{block}\n\nHello")), "Hello");
        assert_eq!(without_context("Hello"), "Hello");
        assert_eq!(
            without_context("<subspace-context> unfinished"),
            "<subspace-context> unfinished"
        );
    }
}
