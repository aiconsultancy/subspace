//! Composer input: plain text or a `/command`.

mod catalogue;
mod recipients;

pub use catalogue::{COMMANDS, CommandHelp, FirstArgument};
pub use recipients::private_recipients;

/// What composer input asks for. Targets left `None` mean the buffer's own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Input {
    /// Text for the buffer, possibly several lines.
    Say(String),
    Me(String),
    Msg {
        target: String,
        text: String,
    },
    Notice {
        target: String,
        text: String,
    },
    Query {
        nick: String,
        text: Option<String>,
    },
    Join {
        channel: String,
        key: Option<String>,
    },
    Part {
        channel: Option<String>,
        reason: String,
    },
    Topic {
        channel: Option<String>,
        text: Option<String>,
    },
    Kick {
        channel: Option<String>,
        nick: String,
        reason: String,
    },
    Invite {
        nick: String,
        channel: Option<String>,
    },
    /// MODE arguments; a leading `+`/`-` applies to the buffer's channel.
    Mode(Vec<String>),
    Nick(String),
    Quit(String),
    /// A raw protocol line.
    Raw(String),
}

/// A command's arguments: all of them, the first word, and the rest.
struct Args {
    name: String,
    all: String,
    first: String,
    rest: String,
    /// The first word is a channel.
    channel: bool,
}

/// A parsed `/command`: known (`None` when its arguments are wrong) or
/// unknown (sent raw).
enum Parsed {
    Known(Option<Input>),
    Unknown(Input),
}

/// Parses composer input. `//text` sends the literal `/text`; an unknown
/// `/command args` is sent raw as `COMMAND args`.
pub fn parse_input(text: &str) -> Result<Input, String> {
    let text = text.trim_end_matches(['\r', '\n']);
    command_line(text).map_or_else(|| Ok(Input::Say(literal(text))), |line| run(args(line)))
}

/// Whether composer input starts a command rather than literal text.
pub(super) fn is_command(text: &str) -> bool {
    command_line(text).is_some()
}

fn command_line(text: &str) -> Option<&str> {
    text.strip_prefix('/').filter(|rest| !rest.starts_with('/'))
}

fn literal(text: &str) -> String {
    match text.starts_with("//") {
        true => text[1..].to_string(),
        false => text.to_string(),
    }
}

fn args(command_line: &str) -> Args {
    let (name, all) = command_line.split_once(' ').unwrap_or((command_line, ""));
    let all = all.trim();
    let (first, rest) = all.split_once(' ').unwrap_or((all, ""));
    Args {
        name: name.to_ascii_lowercase(),
        all: all.to_string(),
        first: first.to_string(),
        rest: rest.trim().to_string(),
        channel: first.starts_with(['#', '&', '!', '+']),
    }
}

fn run(args: Args) -> Result<Input, String> {
    let name = args.name.clone();
    let parsed = parse_command(args);
    finish(&name, parsed)
}

fn parse_command(args: Args) -> Parsed {
    let input = match args.name.as_str() {
        "me" => me(args),
        "msg" => msg(args),
        "notice" => notice(args),
        "query" => query(args),
        "join" | "j" => join(args),
        "part" | "leave" => part(args),
        "topic" => topic(args),
        "kick" => kick(args),
        "invite" => invite(args),
        "mode" => mode(args),
        "nick" => nick(args),
        "quit" => quit(args),
        "whois" => whois(args),
        "away" => away(args),
        "raw" | "quote" => raw(args),
        _ => return Parsed::Unknown(unknown(args)),
    };
    Parsed::Known(input)
}

fn finish(name: &str, parsed: Parsed) -> Result<Input, String> {
    if name.is_empty() {
        return Err("Type a command after /".to_string());
    }
    let usage = COMMANDS
        .iter()
        .find(|command| command.answers(name))
        .map_or_else(String::new, CommandHelp::usage);
    match parsed {
        Parsed::Known(input) => input.ok_or_else(|| format!("Usage: {usage}")),
        Parsed::Unknown(input) => Ok(input),
    }
}

fn unknown(args: Args) -> Input {
    let line = format!("{} {}", args.name.to_ascii_uppercase(), args.all);
    Input::Raw(line.trim_end().to_string())
}

fn me(args: Args) -> Option<Input> {
    Some(Input::Me(args.all))
}

fn msg(args: Args) -> Option<Input> {
    (!args.rest.is_empty()).then_some(Input::Msg {
        target: args.first,
        text: args.rest,
    })
}

fn notice(args: Args) -> Option<Input> {
    (!args.rest.is_empty()).then_some(Input::Notice {
        target: args.first,
        text: args.rest,
    })
}

fn query(args: Args) -> Option<Input> {
    (!args.first.is_empty()).then(|| Input::Query {
        nick: args.first,
        text: (!args.rest.is_empty()).then_some(args.rest),
    })
}

fn join(args: Args) -> Option<Input> {
    let channel = match args.channel {
        true => args.first,
        false => format!("#{}", args.first),
    };
    (channel.len() > 1).then(|| Input::Join {
        channel,
        key: (!args.rest.is_empty()).then_some(args.rest),
    })
}

fn part(args: Args) -> Option<Input> {
    Some(match args.channel {
        true => Input::Part {
            channel: Some(args.first),
            reason: args.rest,
        },
        false => Input::Part {
            channel: None,
            reason: args.all,
        },
    })
}

fn topic(args: Args) -> Option<Input> {
    Some(match args.channel {
        true => Input::Topic {
            channel: Some(args.first),
            text: (!args.rest.is_empty()).then_some(args.rest),
        },
        false => Input::Topic {
            channel: None,
            text: (!args.all.is_empty()).then_some(args.all),
        },
    })
}

fn kick(args: Args) -> Option<Input> {
    let (channel, nick, reason) = match args.channel {
        true => {
            let (nick, reason) = args.rest.split_once(' ').unwrap_or((&args.rest, ""));
            (
                Some(args.first.clone()),
                nick.to_string(),
                reason.trim().to_string(),
            )
        }
        false => (None, args.first, args.rest),
    };
    (!nick.is_empty()).then_some(Input::Kick {
        channel,
        nick,
        reason,
    })
}

fn invite(args: Args) -> Option<Input> {
    (!args.first.is_empty()).then(|| Input::Invite {
        nick: args.first,
        channel: (!args.rest.is_empty()).then_some(args.rest),
    })
}

fn mode(args: Args) -> Option<Input> {
    Some(Input::Mode(
        args.all.split_whitespace().map(str::to_string).collect(),
    ))
}

fn nick(args: Args) -> Option<Input> {
    (!args.first.is_empty()).then_some(Input::Nick(args.first))
}

fn quit(args: Args) -> Option<Input> {
    Some(Input::Quit(args.all))
}

fn whois(args: Args) -> Option<Input> {
    (!args.first.is_empty()).then(|| Input::Raw(format!("WHOIS {}", args.first)))
}

fn away(args: Args) -> Option<Input> {
    Some(Input::Raw(match args.all.is_empty() {
        true => "AWAY".to_string(),
        false => format!("AWAY :{}", args.all),
    }))
}

fn raw(args: Args) -> Option<Input> {
    (!args.all.is_empty()).then_some(Input::Raw(args.all))
}

#[cfg(test)]
mod tests;
