//! The `irc_read` schema and the guide each action carries.

use serde_json::json;
use subspace_framework::agent_tools::schema::{Action, action, flat, text};
use subspace_framework::agent_tools::{Access, ApprovalPolicy, ToolSpec};

use super::{DESCRIPTION, MAX_MESSAGES};

pub(crate) fn spec() -> ToolSpec {
    ToolSpec {
        name: "irc_read",
        title: "Read IRC",
        description: DESCRIPTION,
        access: Access::Read,
        arguments: flat(&actions()),
        approval: ApprovalPolicy::None,
    }
}

fn actions() -> Vec<Action> {
    let limit = || json!({"type":"integer","minimum":1,"maximum":MAX_MESSAGES});
    vec![
        action(
            "networks",
            "The configured networks: id, name, connection status, nick, and whether the server offers chat history.",
            vec![],
        ),
        action(
            "buffers",
            "The server buffer, channels and private conversations, of every network or one: kind, name, joined, topic, member, unread and mention counts, and lines held.",
            vec![("network", text(), false)],
        ),
        action(
            "messages",
            "Recent lines of one buffer, oldest first. target is a channel or nick; omit it for the server buffer. limit is 1..200 (default 50). Pass next_before from a reply as before to page back. Past the oldest line held, the board asks the server for older chat history (server_history \"requested\"); read again shortly to see it.",
            vec![
                ("network", text(), true),
                ("target", text(), false),
                ("limit", limit(), false),
                ("before", json!({"type":"integer"}), false),
            ],
        ),
        action(
            "members",
            "The members of one joined channel with their mode prefixes, such as @ for operators and + for voiced.",
            vec![("network", text(), true), ("channel", text(), true)],
        ),
        action(
            "search",
            "Messages whose text contains query, ignoring case, in the lines the board holds, newest first. limit is 1..100 (default 20).",
            vec![
                ("query", text(), true),
                ("network", text(), false),
                ("limit", limit(), false),
            ],
        ),
    ]
}
