//! Session setup: `initialize`, then `session/new` or `session/load`, after
//! which the chat is ready.
use super::chat::{AcpChat, Pending};
use super::context::{self, Delivery};
use super::protocol::{RpcError, capabilities, model_options};
use super::transcript::fold;
use super::{ChatEvent, Event, EventQueue, Value, json};
use crate::runtime::names::{NEW_CHAT, clean_title, first_message};

impl AcpChat {
    pub(super) fn failure(&self, pending: &Pending, error: &RpcError) -> String {
        let agent = self.agent;
        let message = &error.message;
        match pending {
            Pending::NewSession
            | Pending::LoadSession
            | Pending::Authenticate
            | Pending::Prompt
                if error.needs_sign_in() =>
            {
                let how =
                    self.capabilities.sign_in.as_deref().unwrap_or(
                        "Sign in with the agent's own command line, then restart Subspace",
                    );
                format!("{agent} needs you to sign in: {message} {how}.")
            }
            Pending::Initialize => format!("{agent} did not start: {message}"),
            Pending::NewSession | Pending::LoadSession => {
                format!("{agent} did not open a session: {message}")
            }
            Pending::Authenticate => format!("{agent} could not sign in: {message}"),
            _ => format!("{agent} rejected the request: {message}"),
        }
    }

    pub(super) async fn initialized(
        &mut self,
        result: &Value,
        events: &EventQueue,
    ) -> Result<(), String> {
        self.capabilities = capabilities(result)?;
        self.tools.open(self.capabilities.mcp_http, events).await?;
        if self.saved.is_some() && !self.capabilities.load_session {
            self.saved = None;
            let notice = format!(
                "{} cannot reopen saved chats, so this tab starts a new one.",
                self.agent
            );
            events.push(Event::Notice(notice)).await;
        }
        self.open_session().await
    }

    /// Reopens the saved session when there is one, else starts a new one.
    /// Both carry Subspace's context as a `systemPrompt` when the agent
    /// accepts one; otherwise a new session's first prompt carries it, and
    /// a reopened session already received it with its own first prompt.
    pub(super) async fn open_session(&mut self) -> Result<(), String> {
        let system_prompt = self.system_prompt();
        let Some(session) = self.saved.clone() else {
            self.context_pending = system_prompt.is_none() && self.context.is_some();
            let params = with_system_prompt(
                json!({"cwd": self.cwd, "mcpServers": self.tools.servers()}),
                system_prompt,
            );
            return self
                .request(Pending::NewSession, "session/new", params)
                .await;
        };
        self.replaying = true;
        self.session_id = Some(session.clone());
        let params = with_system_prompt(
            json!({"sessionId": session, "cwd": self.cwd, "mcpServers": self.tools.servers()}),
            system_prompt,
        );
        self.request(Pending::LoadSession, "session/load", params)
            .await
    }

    fn system_prompt(&self) -> Option<Value> {
        self.context
            .as_deref()
            .filter(|_| self.capabilities.system_prompt)
            .map(context::system_prompt)
    }

    /// Records the context the opened session's system prompt carries.
    fn log_system_prompt(&self) {
        if let Some(text) = self
            .context
            .as_deref()
            .filter(|_| self.capabilities.system_prompt)
        {
            context::log_delivery(self.id, Delivery::SystemPrompt, text);
        }
    }

    /// Signs in once with the agent's first method, which reuses the agent's
    /// own saved login, then opens the session again.
    pub(super) async fn authenticate(&mut self, events: &EventQueue) -> Result<bool, String> {
        let Some(method) = self.capabilities.auth_method.clone() else {
            return Ok(false);
        };
        if self.authenticated {
            return Ok(false);
        }
        self.authenticated = true;
        self.replaying = false;
        let notice = format!("Signing in to {}…", self.agent);
        events.push(Event::Notice(notice)).await;
        self.request(
            Pending::Authenticate,
            "authenticate",
            json!({"methodId": method}),
        )
        .await?;
        Ok(true)
    }

    pub(super) async fn session_opened(
        &mut self,
        result: &Value,
        events: &EventQueue,
    ) -> Result<(), String> {
        let session = result["sessionId"]
            .as_str()
            .ok_or_else(|| format!("{} did not report a session id.", self.agent))?;
        self.session_id = Some(session.to_owned());
        self.models = model_options(&result["configOptions"]);
        self.log_system_prompt();
        self.apply_preferred_model(events).await
    }

    pub(super) async fn session_loaded(
        &mut self,
        result: &Value,
        events: &EventQueue,
    ) -> Result<(), String> {
        self.replaying = false;
        let finished = self.transcript.finish(false);
        fold(&mut self.entries, &finished);
        if self.title == NEW_CHAT
            && let Some(title) = first_message(&self.entries).and_then(clean_title)
        {
            self.title = title;
        }
        self.models = model_options(&result["configOptions"]);
        self.log_system_prompt();
        self.apply_preferred_model(events).await
    }

    pub(super) async fn publish_ready(&mut self, events: &EventQueue) {
        self.ready = true;
        let id = self.id;
        let session_id = self.session_id.clone().unwrap_or_default();
        if !self.recovering {
            events
                .push(Event::of(id, ChatEvent::History(self.entries.clone())))
                .await;
        }
        self.recovering = false;
        let initial = [
            ChatEvent::Title(self.title.clone()),
            ChatEvent::Mode(false),
            ChatEvent::Ready { session_id },
        ];
        for event in initial {
            events.push(Event::of(id, event)).await;
        }
        events.push(Event::Agents(Vec::new())).await;
        self.publish_models(events).await;
    }
}

/// `params` with `systemPrompt` set, when there is one to send.
fn with_system_prompt(mut params: Value, system_prompt: Option<Value>) -> Value {
    if let Some(prompt) = system_prompt {
        params["systemPrompt"] = prompt;
    }
    params
}
