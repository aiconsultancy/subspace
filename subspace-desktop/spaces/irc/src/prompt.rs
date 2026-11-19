//! A self-contained prompt that lets a coding agent configure IRC networks
//! in this machine's IRC settings file.

use std::fmt::Write as _;
use std::path::PathBuf;

use crate::schema::{self, FieldDoc};

const INTRODUCTION: &str = "You are helping me configure IRC networks in Subspace, a desktop dashboard app. Its Channels Space connects to IRC networks with IRCv3 support and reads its networks from its own JSON settings file.";

const RULES: &str = "\
- Edit the IRC settings file in place, or create it if it does not exist. It is JSON and must stay valid JSON: no comments and no trailing commas.
- The file's top-level object holds the IRC settings. Keep every other key and value as it is, including keys you do not recognise.
- To show the board, also set `\"boards\": { \"irc\": true }` in the Subspace config file, keeping the other entries of `boards` and every other key of that file. Do not put IRC settings in the Subspace config file.
- The IRC settings file is readable only by its owner (mode 0600) and its folder only by its owner (mode 0700) because the file holds passwords. Subspace ignores the file when other users can read or write it. Keep those modes; if your edit created or replaced the file, set them with `chmod 600` and `chmod 700`.
- Ask me for any password, or ask whether it is in a file and use `sasl.passwordFile` instead. Never print, log or repeat a password, and never put this file or a password in a commit, an issue or a chat message.
- Ask me which networks, nicks and channels I want; do not guess them.
- Close Subspace's Settings window before editing: it refuses to save over a file changed while it is open.";

const EXAMPLE: &str = r##"{
  "networks": [
    {
      "id": "example",
      "name": "Example Network",
      "host": "irc.example.net",
      "port": 6697,
      "tls": true,
      "nickname": "your-nick",
      "sasl": { "username": "your-account", "password": "ask the user" },
      "channels": ["#example"],
      "autoConnect": true
    }
  ],
  "highlights": ["your-nick"]
}"##;

const VERIFY: &str = "\
1. Check that the file still parses, for example with `python3 -m json.tool` on it.
2. Restart Subspace to apply the change. Changes saved from Settings > Channels apply without a restart.
3. Open the Channels Space. Each network is a group whose header shows its connection state. Its Server row shows the server's messages, including sign-in and connection errors. A network entry that cannot be read is skipped and named on the board while no network can be read.";

/// The files the prompt names.
pub(crate) struct Files {
    /// The IRC settings file.
    pub board: PathBuf,
    /// The Subspace config file, which enables the board.
    pub product: PathBuf,
}

/// The prompt for this computer's `files`.
pub(crate) fn agent_prompt(files: &Files) -> String {
    let mut prompt = String::new();
    _ = writeln!(prompt, "{INTRODUCTION}\n");
    _ = writeln!(prompt, "IRC settings file: {}", files.board.display());
    _ = writeln!(
        prompt,
        "Subspace config file: {}\n",
        files.product.display()
    );
    _ = writeln!(prompt, "Rules:\n{RULES}\n");
    _ = writeln!(
        prompt,
        "Schema of the IRC settings file, a top-level object:"
    );
    write_fields(&mut prompt, schema::BOARD);
    _ = writeln!(prompt, "\nEach object in `networks`:");
    write_fields(&mut prompt, schema::NETWORK);
    _ = writeln!(prompt, "\nThe `sasl` object:");
    write_fields(&mut prompt, schema::SASL);
    _ = writeln!(
        prompt,
        "\nExample of the IRC settings file:\n```json\n{EXAMPLE}\n```\n"
    );
    _ = write!(prompt, "Verify:\n{VERIFY}");
    prompt
}

fn write_fields(prompt: &mut String, fields: &[FieldDoc]) {
    for field in fields {
        _ = writeln!(
            prompt,
            "- `{}` ({}, default {}): {}",
            field.key, field.kind, field.default, field.meaning
        );
    }
}

#[cfg(test)]
mod tests {
    use super::{Files, agent_prompt};
    use crate::schema;

    #[test]
    fn the_prompt_names_both_files_and_every_documented_field() {
        let prompt = agent_prompt(&Files {
            board: "/home/fixture/.config/subspace/irc/config.json".into(),
            product: "/home/fixture/.config/subspace/config.json".into(),
        });
        assert!(
            prompt.contains("IRC settings file: /home/fixture/.config/subspace/irc/config.json")
        );
        assert!(
            prompt.contains("Subspace config file: /home/fixture/.config/subspace/config.json")
        );
        for field in schema::BOARD
            .iter()
            .chain(schema::NETWORK)
            .chain(schema::SASL)
        {
            assert!(
                prompt.contains(&format!("- `{}` (", field.key)),
                "{}",
                field.key
            );
        }
        for key in [
            "caCert",
            "passwordFile",
            "0600",
            "0700",
            "never put this file",
        ] {
            assert!(prompt.contains(key), "{key}");
        }
    }

    #[test]
    fn the_example_is_a_valid_settings_file_with_a_readable_network() {
        let example: serde_json::Value = serde_json::from_str(super::EXAMPLE).unwrap_or_default();
        let settings = crate::config::parse(example.as_object());
        assert_eq!(settings.networks.len(), 1);
        assert!(settings.error.is_none());
    }
}
