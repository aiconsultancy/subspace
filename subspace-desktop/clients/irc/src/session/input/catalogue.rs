//! The `/commands` the composer parser handles: their arguments and
//! descriptions, for completion and usage errors.

/// What a command's first argument names, so a composer can complete it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FirstArgument {
    /// Free text, or nothing to complete.
    Text,
    Nick,
    /// Any channel the network knows.
    Channel,
    /// A channel we are in.
    JoinedChannel,
}

/// One command the composer understands, for completion and usage errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandHelp {
    /// Without the leading slash, such as `join`.
    pub name: &'static str,
    pub aliases: &'static [&'static str],
    /// The arguments, such as `<#channel> [key]`.
    pub args: &'static str,
    pub description: &'static str,
    pub first: FirstArgument,
}

impl CommandHelp {
    /// Such as `/join <#channel> [key]`.
    pub fn usage(&self) -> String {
        format!("/{} {}", self.name, self.args)
            .trim_end()
            .to_string()
    }

    /// Whether `name`, in lower case, is this command or one of its aliases.
    pub fn answers(&self, name: &str) -> bool {
        self.name == name || self.aliases.contains(&name)
    }
}

const fn help(
    name: &'static str,
    args: &'static str,
    description: &'static str,
    first: FirstArgument,
) -> CommandHelp {
    CommandHelp {
        name,
        aliases: &[],
        args,
        description,
        first,
    }
}

/// Every `/command` [`super::parse_input`] handles itself, most used first. Any
/// other command is sent to the server as typed.
pub const COMMANDS: &[CommandHelp] = &[
    CommandHelp {
        aliases: &["j"],
        ..help(
            "join",
            "<#channel> [key]",
            "Join a channel",
            FirstArgument::Channel,
        )
    },
    CommandHelp {
        aliases: &["leave"],
        ..help(
            "part",
            "[#channel] [reason]",
            "Leave a channel",
            FirstArgument::JoinedChannel,
        )
    },
    help(
        "msg",
        "<nick> <text>",
        "Send a private message",
        FirstArgument::Nick,
    ),
    help(
        "query",
        "<nick> [text]",
        "Open a private conversation",
        FirstArgument::Nick,
    ),
    help(
        "me",
        "<action>",
        "Describe what you are doing",
        FirstArgument::Text,
    ),
    help(
        "notice",
        "<target> <text>",
        "Send a notice",
        FirstArgument::Nick,
    ),
    help(
        "topic",
        "[#channel] [text]",
        "Show or set the topic",
        FirstArgument::JoinedChannel,
    ),
    help("nick", "<nick>", "Change your nick", FirstArgument::Text),
    help(
        "away",
        "[message]",
        "Set or clear your away message",
        FirstArgument::Text,
    ),
    help("whois", "<nick>", "Look up a user", FirstArgument::Nick),
    help(
        "invite",
        "<nick> [#channel]",
        "Invite someone to a channel",
        FirstArgument::Nick,
    ),
    help(
        "kick",
        "[#channel] <nick> [reason]",
        "Remove someone from a channel",
        FirstArgument::Nick,
    ),
    help(
        "mode",
        "[target] <modes> [arguments]",
        "Show or change modes",
        FirstArgument::Text,
    ),
    help(
        "quit",
        "[reason]",
        "Disconnect from the network",
        FirstArgument::Text,
    ),
    CommandHelp {
        aliases: &["quote"],
        ..help(
            "raw",
            "<line>",
            "Send a raw protocol line",
            FirstArgument::Text,
        )
    },
];
