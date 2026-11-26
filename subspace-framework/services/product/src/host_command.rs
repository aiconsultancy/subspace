//! The root words the shell answers at start-up, before a product's command
//! runner sees the arguments. One table decides both the route and the names
//! generated commands must not use, so the two cannot drift apart.

use crate::mcp::MCP_COMMAND;
use crate::update::UPGRADE_COMMAND;

/// The hidden word the command runner answers with completion candidates
/// before it parses, when `--` follows it.
pub const COMPLETE_WORD_COMMAND: &str = "__complete-word";

/// What start-up runs instead of the product's commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostCommand {
    Metadata,
    About,
    Version,
    Mcp,
    Upgrade,
}

/// How much of the command line start-up claims with a word.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Claim {
    /// Only the word on its own, such as `example-app about`.
    Alone,
    /// The word and whatever follows it, such as `example-app mcp --http`.
    Leading,
}

/// Each word, what it starts, and how much of the line it claims.
const HOST_COMMANDS: [(&str, HostCommand, Claim); 6] = [
    ("__product_metadata", HostCommand::Metadata, Claim::Alone),
    ("about", HostCommand::About, Claim::Alone),
    ("--about", HostCommand::About, Claim::Alone),
    ("--version", HostCommand::Version, Claim::Alone),
    (MCP_COMMAND, HostCommand::Mcp, Claim::Leading),
    (UPGRADE_COMMAND, HostCommand::Upgrade, Claim::Leading),
];

/// The table's entries for a product; without an update source `upgrade` is
/// not a start-up command.
fn offered(updates: bool) -> impl Iterator<Item = &'static (&'static str, HostCommand, Claim)> {
    HOST_COMMANDS
        .iter()
        .filter(move |(_, command, _)| updates || *command != HostCommand::Upgrade)
}

/// The start-up command `args` (without the binary name) select. `updates` is
/// whether the product has an update source.
pub fn host_command(args: &[String], updates: bool) -> Option<HostCommand> {
    let first = args.first()?;
    offered(updates)
        .find(|(word, _, claim)| word == first && (*claim == Claim::Leading || args.len() == 1))
        .map(|(_, command, _)| *command)
}

/// Root words no generated leaf command or alias may use. A leaf can be
/// invoked on its own, so this is every word start-up claims, and the
/// completion word the command runner claims.
pub fn reserved_leaf_words(updates: bool) -> Vec<&'static str> {
    offered(updates)
        .map(|(word, _, _)| *word)
        .chain([COMPLETE_WORD_COMMAND])
        .collect()
}

/// Root words no generated command group, and no Space id, may use: the
/// words start-up or the command runner claim whatever follows them. A group needs a word after
/// its name, so a word claimed only alone leaves the group reachable.
pub fn reserved_group_words(updates: bool) -> Vec<&'static str> {
    offered(updates)
        .filter(|(_, _, claim)| *claim == Claim::Leading)
        .map(|(word, _, _)| *word)
        .chain([COMPLETE_WORD_COMMAND])
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        COMPLETE_WORD_COMMAND, HostCommand, host_command, reserved_group_words, reserved_leaf_words,
    };

    fn words(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    #[test]
    fn every_reserved_leaf_word_selects_a_start_up_command_alone() {
        for updates in [true, false] {
            for word in reserved_leaf_words(updates) {
                let runner = word == COMPLETE_WORD_COMMAND;
                assert!(
                    runner || host_command(&words(&[word]), updates).is_some(),
                    "{word}"
                );
            }
        }
        assert_eq!(
            reserved_leaf_words(true),
            [
                "__product_metadata",
                "about",
                "--about",
                "--version",
                "mcp",
                "upgrade",
                "__complete-word"
            ]
        );
        assert!(!reserved_leaf_words(false).contains(&"upgrade"));
    }

    #[test]
    fn reserved_group_words_are_the_ones_start_up_claims_with_arguments() {
        for updates in [true, false] {
            for word in reserved_leaf_words(updates) {
                let leading = word == COMPLETE_WORD_COMMAND
                    || host_command(&words(&[word, "x"]), updates).is_some();
                assert_eq!(
                    reserved_group_words(updates).contains(&word),
                    leading,
                    "{word}"
                );
            }
        }
        assert_eq!(
            reserved_group_words(true),
            ["mcp", "upgrade", "__complete-word"]
        );
        assert_eq!(reserved_group_words(false), ["mcp", "__complete-word"]);
    }

    #[test]
    fn lone_words_only_match_alone_and_leading_words_take_arguments() {
        assert_eq!(host_command(&words(&["about", "x"]), true), None);
        assert_eq!(
            host_command(&words(&["mcp", "--http"]), false),
            Some(HostCommand::Mcp)
        );
        assert_eq!(
            host_command(&words(&["upgrade", "--check"]), true),
            Some(HostCommand::Upgrade)
        );
        assert_eq!(host_command(&words(&["upgrade"]), false), None);
        assert_eq!(host_command(&[], true), None);
    }
}
