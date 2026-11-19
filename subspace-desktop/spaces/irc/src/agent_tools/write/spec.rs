//! The `irc_write` schema and the guide each action carries.

use serde_json::json;
use subspace_framework::agent_tools::schema::{Action, action, batch, text};
use subspace_framework::agent_tools::{Access, ApprovalPolicy, ToolSpec};

use super::DESCRIPTION;
use super::validate::MAX_TEXT_CHARS;

pub(crate) fn spec() -> ToolSpec {
    ToolSpec {
        name: "irc_write",
        title: "Send on IRC",
        description: DESCRIPTION,
        access: Access::Write,
        arguments: batch(&actions()),
        approval: ApprovalPolicy::PerCall,
    }
}

fn actions() -> Vec<Action> {
    vec![
        action(
            "message",
            "Send text to a joined channel or to a nick, as the user. Line breaks send several lines, as one draft/multiline batch when the server supports it; long lines are split as the composer splits them. At most 2000 characters, 4096 UTF-8 bytes and 10 lines, with no control characters but line breaks and tabs. me true sends one line as a /me action. Messages to network services such as NickServ are refused.",
            vec![
                ("network", text(), true),
                ("target", text(), true),
                (
                    "text",
                    json!({"type":"string","maxLength":MAX_TEXT_CHARS}),
                    true,
                ),
                ("me", json!({"type":"boolean","default":false}), false),
            ],
        ),
        action(
            "join",
            "Join a channel on a connected network. key is the channel key, sent with this join and never saved.",
            vec![
                ("network", text(), true),
                ("channel", text(), true),
                ("key", text(), false),
            ],
        ),
        action(
            "part",
            "Leave a joined channel, with an optional one-line reason.",
            vec![
                ("network", text(), true),
                ("channel", text(), true),
                ("reason", text(), false),
            ],
        ),
        action(
            "connect",
            "Connect a configured network that is disconnected or failed.",
            vec![("network", text(), true)],
        ),
        action(
            "disconnect",
            "Disconnect a connected or connecting network.",
            vec![("network", text(), true)],
        ),
    ]
}
