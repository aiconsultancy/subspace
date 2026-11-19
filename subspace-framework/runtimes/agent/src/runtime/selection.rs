use super::choices::{chat_mode, session_mode};
use super::{
    AgentSelectRequest, ChatEvent, ChatId, ChatMode, Event, EventQueue, ModeSetRequest, Session,
};
use github_copilot_sdk::session_events::SessionMode;
use github_copilot_sdk::types::SetModelOptions;

pub(super) struct ModelSelection<'a> {
    pub(super) session: &'a Session,
    pub(super) events: &'a EventQueue,
    pub(super) chat: ChatId,
    pub(super) model: &'a str,
    pub(super) effort: Option<String>,
    pub(super) user_selected: bool,
}

/// Sets the model and returns the selection the runtime confirmed.
pub(super) async fn select_model(selection: ModelSelection<'_>) -> Result<ChatEvent, String> {
    let ModelSelection {
        session,
        events,
        chat,
        model,
        effort,
        user_selected,
    } = selection;
    session
        .set_model(model, effort.map(model_options))
        .await
        .map_err(|_| {
            "The SDK rejected this model or reasoning level. Check model availability and enterprise policy.".to_string()
        })?;
    let confirmed = current_model(session).await?;
    let confirmed = confirmed.ok_or("The SDK has not confirmed the chosen model.")?;
    events.push(Event::of(chat, confirmed.clone())).await;
    if user_selected && let ChatEvent::SelectedModel { model, .. } = &confirmed {
        events
            .push(Event::of(
                chat,
                ChatEvent::ModelConfirmed {
                    model: model.clone(),
                },
            ))
            .await;
    }
    Ok(confirmed)
}

fn model_options(reasoning_effort: String) -> SetModelOptions {
    SetModelOptions {
        reasoning_effort: Some(reasoning_effort),
        ..Default::default()
    }
}

pub(super) async fn select_mode(
    session: &Session,
    events: &EventQueue,
    chat: ChatId,
    mode: ChatMode,
) -> Result<(), String> {
    session
        .rpc()
        .mode()
        .set(ModeSetRequest {
            mode: session_mode(mode),
            ..Default::default()
        })
        .await
        .map_err(|_| {
            "The SDK rejected this chat mode. The previous mode is still active.".to_string()
        })?;
    publish_mode(session, chat, events).await
}

pub(super) async fn select_agent(
    session: &Session,
    events: &EventQueue,
    chat: ChatId,
    name: String,
) -> Result<(), String> {
    let result = session
        .rpc()
        .agent()
        .select(AgentSelectRequest { name })
        .await
        .map_err(|_| {
            "The SDK rejected this agent. The conversation was not replaced.".to_string()
        })?;
    events
        .push(Event::of(chat, confirmed_agent(Some(result.agent.name))?))
        .await;
    Ok(())
}

/// Publishes the chat's model, mode and agent. Returns the confirmed model
/// selection, which the chat keeps for its failure reports.
pub(super) async fn publish_current(
    session: &Session,
    chat: ChatId,
    events: &EventQueue,
) -> Option<ChatEvent> {
    let model = publish_model(session, chat, events).await;
    let selected = model.clone().unwrap_or_default();
    report(model.map(|_| ()), chat, events).await;
    report(publish_mode(session, chat, events).await, chat, events).await;
    report(publish_agent(session, chat, events).await, chat, events).await;
    selected
}

async fn report(result: Result<(), String>, chat: ChatId, events: &EventQueue) {
    if let Err(error) = result {
        events.push(Event::of(chat, ChatEvent::Error(error))).await;
    }
}

async fn publish_agent(session: &Session, chat: ChatId, events: &EventQueue) -> Result<(), String> {
    let result = session
        .rpc()
        .agent()
        .get_current()
        .await
        .map_err(|_| "Cannot confirm the current SDK agent.".to_string())?;
    events
        .push(Event::of(
            chat,
            confirmed_agent(result.agent.map(|agent| agent.name))?,
        ))
        .await;
    Ok(())
}

/// A new session has no model until Copilot picks one; that is not an error.
async fn publish_model(
    session: &Session,
    chat: ChatId,
    events: &EventQueue,
) -> Result<Option<ChatEvent>, String> {
    let selected = current_model(session).await?;
    if let Some(event) = selected.clone() {
        events.push(Event::of(chat, event)).await;
    }
    Ok(selected)
}

async fn current_model(session: &Session) -> Result<Option<ChatEvent>, String> {
    let result = session
        .rpc()
        .model()
        .get_current()
        .await
        .map_err(|_| "Cannot confirm the current SDK model.".to_string())?;
    Ok(confirmed_model(result.model_id, result.reasoning_effort))
}

async fn publish_mode(session: &Session, chat: ChatId, events: &EventQueue) -> Result<(), String> {
    let mode = session
        .rpc()
        .mode()
        .get()
        .await
        .map_err(|_| "Cannot confirm the current SDK chat mode.".to_string())?;
    events.push(Event::of(chat, confirmed_mode(&mode)?)).await;
    Ok(())
}

fn confirmed_model(name: Option<String>, effort: Option<String>) -> Option<ChatEvent> {
    name.filter(|model| !model.is_empty())
        .map(|model| ChatEvent::SelectedModel { model, effort })
}

fn confirmed_mode(mode: &SessionMode) -> Result<ChatEvent, String> {
    chat_mode(mode)
        .map(ChatEvent::SelectedMode)
        .ok_or_else(|| "The SDK reported a chat mode Subspace does not offer.".into())
}

fn confirmed_agent(name: Option<String>) -> Result<ChatEvent, String> {
    name.filter(|agent| !agent.is_empty())
        .map(ChatEvent::SelectedAgent)
        .ok_or_else(|| "The SDK has not confirmed an active custom agent.".into())
}

#[cfg(test)]
mod tests {
    use super::{confirmed_agent, confirmed_mode, confirmed_model, model_options};
    use crate::{ChatEvent, ChatMode};
    use github_copilot_sdk::session_events::SessionMode;

    #[test]
    fn confirmations_use_authoritative_names_and_do_not_guess_missing_values() {
        assert!(matches!(
            confirmed_model(Some("sdk-model".into()), Some("high".into())),
            Some(ChatEvent::SelectedModel { model, effort })
                if model == "sdk-model" && effort.as_deref() == Some("high")
        ));
        assert!(
            matches!(confirmed_agent(Some("sdk-agent".into())), Ok(ChatEvent::SelectedAgent(name)) if name == "sdk-agent")
        );
        assert!(confirmed_model(None, None).is_none(), "no model yet");
        assert!(confirmed_model(Some(String::new()), None).is_none());
        assert!(confirmed_agent(Some(String::new())).is_err());
        assert!(confirmed_agent(None).is_err(), "the default agent");
    }

    #[test]
    fn mode_confirmation_requires_a_mode_subspace_offers() {
        assert!(matches!(
            confirmed_mode(&SessionMode::Plan),
            Ok(ChatEvent::SelectedMode(ChatMode::Plan))
        ));
        assert!(confirmed_mode(&SessionMode::Unknown).is_err());
    }

    #[test]
    fn reasoning_effort_is_the_only_model_override_sent() {
        let options = model_options("xhigh".into());
        assert_eq!(options.reasoning_effort.as_deref(), Some("xhigh"));
        assert!(options.context_tier.is_none());
        assert!(options.model_capabilities.is_none());
    }
}
