//! Pure mappings between SDK protocol values and the chat contract.
use super::{ChatMode, ModelChoice, SlashCommand};
use github_copilot_sdk::rpc::{Model, SlashCommandInfo, SlashCommandKind};
use github_copilot_sdk::session_events::SessionMode;

pub(super) fn model_choice(model: Model) -> ModelChoice {
    ModelChoice {
        id: model.id,
        label: model.name,
        efforts: model.supported_reasoning_efforts.unwrap_or_default(),
        default_effort: model.default_reasoning_effort,
    }
}

pub(super) fn session_mode(mode: ChatMode) -> SessionMode {
    match mode {
        ChatMode::Interactive => SessionMode::Interactive,
        ChatMode::Plan => SessionMode::Plan,
        ChatMode::Autopilot => SessionMode::Autopilot,
    }
}

pub(super) fn chat_mode(mode: &SessionMode) -> Option<ChatMode> {
    [ChatMode::Interactive, ChatMode::Plan, ChatMode::Autopilot]
        .into_iter()
        .find(|candidate| &session_mode(*candidate) == mode)
}

/// Only skill-backed commands are offered: runtime built-ins drive the CLI
/// terminal UI and can change CLI configuration.
pub(super) fn slash_commands(commands: Vec<SlashCommandInfo>) -> Vec<SlashCommand> {
    commands
        .into_iter()
        .filter(|command| command.kind == SlashCommandKind::Skill)
        .map(|command| SlashCommand {
            name: command.name,
            description: command.description,
            aliases: command.aliases.unwrap_or_default(),
        })
        .collect()
}

/// Resolves a requested command against the listed skills. An unlisted name is
/// rejected instead of being passed to the runtime.
pub(super) fn allowed_command(commands: &[SlashCommand], name: &str) -> Result<String, String> {
    let requested = name.trim().trim_start_matches('/');
    commands
        .iter()
        .find(|command| {
            command.name.eq_ignore_ascii_case(requested)
                || command
                    .aliases
                    .iter()
                    .any(|alias| alias.eq_ignore_ascii_case(requested))
        })
        .map(|command| command.name.clone())
        .ok_or_else(|| "That command is not an available skill. Type / to list skills.".into())
}

#[cfg(test)]
mod tests {
    use super::{
        ChatMode, ModelChoice, allowed_command, chat_mode, model_choice, session_mode,
        slash_commands,
    };
    use crate::SlashCommand;
    use github_copilot_sdk::rpc::{Model, SlashCommandInfo, SlashCommandKind};
    use github_copilot_sdk::session_events::SessionMode;

    fn command(
        name: &str,
        kind: SlashCommandKind,
        aliases: Option<Vec<String>>,
    ) -> SlashCommandInfo {
        SlashCommandInfo {
            aliases,
            allow_during_agent_execution: false,
            description: format!("{name} description"),
            experimental: None,
            input: None,
            kind,
            name: name.into(),
            schedulable: None,
        }
    }

    #[test]
    fn model_efforts_come_from_the_model_not_from_a_fixed_list() {
        let mut model = Model {
            id: "claude-sonnet-4.5".into(),
            name: "Claude Sonnet 4.5".into(),
            ..Default::default()
        };
        model.supported_reasoning_efforts = Some(vec!["low".into(), "high".into()]);
        model.default_reasoning_effort = Some("low".into());
        let choice = model_choice(model);
        assert_eq!(choice.id, "claude-sonnet-4.5");
        assert_eq!(choice.efforts, vec!["low".to_owned(), "high".to_owned()]);
        assert_eq!(choice.default_effort.as_deref(), Some("low"));
        let plain = model_choice(Model {
            id: "gpt".into(),
            name: "GPT".into(),
            ..Default::default()
        });
        assert!(plain.efforts.is_empty());
        assert!(plain.default_effort.is_none());
    }

    #[test]
    fn an_unsupported_effort_is_not_sent_to_the_runtime() {
        let choice = ModelChoice {
            id: "model".into(),
            label: "Model".into(),
            efforts: vec!["high".into()],
            default_effort: None,
        };
        assert_eq!(
            choice.supported_effort(Some("high")).as_deref(),
            Some("high")
        );
        assert!(choice.supported_effort(Some("max")).is_none());
        assert!(choice.supported_effort(None).is_none());
    }

    #[test]
    fn modes_map_both_ways_and_an_unknown_sdk_mode_is_not_guessed() {
        for mode in [ChatMode::Interactive, ChatMode::Plan, ChatMode::Autopilot] {
            assert_eq!(chat_mode(&session_mode(mode)), Some(mode));
        }
        assert_eq!(session_mode(ChatMode::Plan), SessionMode::Plan);
        assert!(chat_mode(&SessionMode::Unknown).is_none());
    }

    #[test]
    fn only_skill_commands_are_offered_and_invocable() {
        let listed = slash_commands(vec![
            command("review", SlashCommandKind::Skill, Some(vec!["rv".into()])),
            command("model", SlashCommandKind::Builtin, None),
            command("widget", SlashCommandKind::Client, None),
        ]);
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, "review");
        assert_eq!(listed[0].aliases, vec!["rv".to_owned()]);
        assert_eq!(allowed_command(&listed, "/Review").unwrap(), "review");
        assert_eq!(allowed_command(&listed, "rv").unwrap(), "review");
        assert!(allowed_command(&listed, "model").is_err());
        assert!(allowed_command(&Vec::<SlashCommand>::new(), "review").is_err());
    }
}
