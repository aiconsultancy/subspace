//! Skill-backed slash commands. Only listed skills can be invoked.
use super::choices::{allowed_command, slash_commands};
use super::{Session, SlashCommand};
use github_copilot_sdk::rpc::{CommandsInvokeRequest, SlashCommandInvocationResult};

/// What the runtime returned for an invoked skill command.
pub(super) enum CommandOutcome {
    /// A prompt the host must submit as the next turn.
    Prompt(String),
    /// Text the host shows without starting a turn.
    Note(String),
}

pub(super) async fn list_skill_commands(session: &Session) -> Result<Vec<SlashCommand>, String> {
    let list = session
        .rpc()
        .commands()
        .list()
        .await
        .map_err(|_| "Skill command list is unavailable from Copilot.".to_string())?;
    Ok(slash_commands(list.commands))
}

pub(super) async fn invoke_command(
    session: &Session,
    name: &str,
    args: String,
) -> Result<CommandOutcome, String> {
    let listed = list_skill_commands(session).await?;
    let request = invoke_request(allowed_command(&listed, name)?, args);
    let result = session
        .rpc()
        .commands()
        .invoke(request)
        .await
        .map_err(|_| "The SDK rejected this skill command.".to_string())?;
    Ok(outcome(result))
}

fn invoke_request(name: String, args: String) -> CommandsInvokeRequest {
    CommandsInvokeRequest {
        input: Some(args).filter(|args| !args.trim().is_empty()),
        name,
        origin: None,
    }
}

fn outcome(result: SlashCommandInvocationResult) -> CommandOutcome {
    match result {
        SlashCommandInvocationResult::AgentPrompt(prompt) => CommandOutcome::Prompt(prompt.prompt),
        SlashCommandInvocationResult::Text(text) => CommandOutcome::Note(text.text),
        SlashCommandInvocationResult::Completed(completed) => CommandOutcome::Note(
            completed
                .message
                .unwrap_or_else(|| "The skill command finished.".to_owned()),
        ),
        _ => CommandOutcome::Note(
            "The skill command asked for a response Subspace does not offer. Nothing was run."
                .to_owned(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::{CommandOutcome, invoke_request, outcome};
    use github_copilot_sdk::rpc::{
        SlashCommandAgentPromptResult, SlashCommandCompletedResult, SlashCommandInvocationResult,
        SlashCommandSelectSubcommandResult, SlashCommandTextResult,
    };

    #[test]
    fn blank_arguments_are_not_sent_as_input() {
        assert!(
            invoke_request("review".into(), "   ".into())
                .input
                .is_none()
        );
        assert_eq!(
            invoke_request("review".into(), "this diff".into())
                .input
                .as_deref(),
            Some("this diff")
        );
        assert_eq!(
            invoke_request("review".into(), String::new()).name,
            "review"
        );
    }

    #[test]
    fn only_prompt_and_text_results_produce_chat_output() {
        let prompt = SlashCommandInvocationResult::AgentPrompt(SlashCommandAgentPromptResult {
            prompt: "Review the diff".into(),
            display_prompt: "Review".into(),
            ..Default::default()
        });
        assert!(
            matches!(outcome(prompt), CommandOutcome::Prompt(text) if text == "Review the diff")
        );
        let text = SlashCommandInvocationResult::Text(SlashCommandTextResult {
            text: "Output".into(),
            ..Default::default()
        });
        assert!(matches!(outcome(text), CommandOutcome::Note(note) if note == "Output"));
        let completed = SlashCommandInvocationResult::Completed(SlashCommandCompletedResult {
            message: None,
            ..Default::default()
        });
        assert!(
            matches!(outcome(completed), CommandOutcome::Note(note) if note.contains("finished"))
        );
        let other = SlashCommandInvocationResult::SelectSubcommand(
            SlashCommandSelectSubcommandResult::default(),
        );
        assert!(
            matches!(outcome(other), CommandOutcome::Note(note) if note.contains("Nothing was run"))
        );
    }
}
