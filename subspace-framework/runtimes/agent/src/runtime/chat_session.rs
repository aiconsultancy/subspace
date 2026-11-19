use super::{ChatId, ChatPreferences, Event, Session, SessionSetup};

use super::session::{current_yolo, ensure_managed_policy, set_yolo, verify_local_session};

pub(super) async fn prepare_chat_session(
    session: &Session,
    setup: &SessionSetup,
    chat: ChatId,
    preferences: Option<&ChatPreferences>,
) -> Result<bool, String> {
    let persisted = preferences.is_none();
    if persisted {
        verify_local_session(session, &setup.workspace).await?;
        ensure_managed_policy(session).await?;
        return current_yolo(session).await;
    }
    super::startup::prepare_local_session(session, &setup.workspace).await?;
    if let Some(model) = preferences.and_then(|preferences| preferences.copilot_model.as_deref())
        && let Err(error) = super::selection::select_model(super::selection::ModelSelection {
            session,
            events: &setup.decisions.events,
            chat,
            model,
            effort: None,
            user_selected: false,
        })
        .await
    {
        setup.decisions.events
            .push(Event::Notice(format!(
                "Saved model '{model}' is unavailable; the SDK fallback is active. The preference was kept. ({error})"
            )))
            .await;
    }
    if !preferences.is_some_and(|preferences| preferences.approve_all) {
        return Ok(false);
    }
    if let Err(error) = set_yolo(session, true).await {
        super::startup::prepare_local_session(session, &setup.workspace)
            .await
            .map_err(|restore| {
                format!("{error} Manual approval could not be restored: {restore}")
            })?;
        setup
            .decisions
            .events
            .push(Event::Notice(
                "Approve all could not be enabled for this chat; manual approval remains active."
                    .into(),
            ))
            .await;
        return Ok(false);
    }
    Ok(true)
}
