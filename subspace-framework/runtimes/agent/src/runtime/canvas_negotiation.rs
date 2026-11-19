//! Retry only an explicit unsupported-canvas rejection, with the feature off.
use super::{ChatId, Client, Event, Session, SessionId, SessionSetup, session};

pub(super) async fn create(
    client: &Client,
    setup: &SessionSetup,
    chat: ChatId,
) -> Result<Session, String> {
    let mut config = session::creation_config(setup, chat);
    match client.create_session(config.clone()).await {
        Ok(session) => return Ok(session),
        Err(error) if unsupported(&error.to_string()) => {}
        Err(_) => return Err(
            "Cannot create the SDK session. Check identity, Copilot access and enterprise policy."
                .into(),
        ),
    }
    config.request_canvas_renderer = None;
    config.canvases = None;
    config.canvas_provider = None;
    config.canvas_handler = None;
    remove_tools(&mut config.available_tools);
    let session = client
        .create_session(config)
        .await
        .map_err(|_| "Cannot create the SDK session with canvases disabled.".to_string())?;
    unavailable(setup, chat).await;
    Ok(session)
}

pub(super) async fn resume(
    client: &Client,
    setup: &SessionSetup,
    chat: ChatId,
    id: SessionId,
) -> Result<Session, String> {
    let mut config = session::resume_config(setup, chat, id);
    let handler = std::sync::Arc::new(super::canvas::Host::new(
        chat,
        setup.decisions.events.clone(),
    ));
    config.canvas_handler = Some(handler.clone());
    match client.resume_session(config.clone()).await {
        Ok(session) => {
            handler.restore_ids(&session);
            return Ok(session);
        },
        Err(error) if unsupported(&error.to_string()) => {},
        Err(_) => return Err("Cannot resume the saved SDK session. Check identity/access or close the tab to start again. No replacement session was silently created.".into()),
    }
    config.request_canvas_renderer = None;
    config.canvases = None;
    config.canvas_provider = None;
    config.canvas_handler = None;
    remove_tools(&mut config.available_tools);
    let session = client
        .resume_session(config)
        .await
        .map_err(|_| "Cannot resume this SDK session with canvases disabled.".to_string())?;
    unavailable(setup, chat).await;
    Ok(session)
}

fn remove_tools(tools: &mut Option<Vec<String>>) {
    if let Some(tools) = tools {
        tools.retain(|tool| !["list_canvas_capabilities", "open_canvas"].contains(&tool.as_str()));
    }
}

async fn unavailable(setup: &SessionSetup, _chat: ChatId) {
    setup.decisions.events.push(Event::Notice("Native canvases are unavailable: the installed Copilot runtime rejected experimental canvas configuration. Chat continues with canvases disabled.".into())).await;
}

fn unsupported(error: &str) -> bool {
    let error = error.to_ascii_lowercase();
    error.contains("canvas")
        && [
            "unknown field",
            "unrecognized",
            "unsupported",
            "unexpected property",
        ]
        .iter()
        .any(|marker| error.contains(marker))
}

#[cfg(test)]
mod tests {
    use super::unsupported;
    #[test]
    fn only_explicit_canvas_schema_rejection_allows_feature_fallback() {
        assert!(unsupported("unknown field requestCanvasRenderer"));
        assert!(!unsupported("authentication failed"));
        assert!(!unsupported("canvas transport disconnected"));
        assert!(!unsupported("unsupported sandbox policy"));
    }
}
