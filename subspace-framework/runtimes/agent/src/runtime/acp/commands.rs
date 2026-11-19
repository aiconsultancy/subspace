//! The commands the panel sends, routed to the actor or to one chat.
use super::chat::AcpChat;
use super::{AcpActor, ChatCommand, ChatId, ChatPreferences, Command, EventQueue};

impl AcpActor {
    pub(super) async fn apply(
        &mut self,
        command: Command,
        preferences: &ChatPreferences,
    ) -> Result<(), String> {
        match command {
            Command::Chat { chat, command } => self.chat_command(chat, command).await,
            Command::OpenChat {
                chat,
                session,
                preferences: requested,
            } => {
                let preferences =
                    requested.or_else(|| session.is_none().then(|| preferences.clone()));
                self.open_requested(chat, session, preferences).await
            }
            Command::OpenRemote { .. } => {
                Err("Copilot cloud sessions need the Copilot engine.".into())
            }
            Command::Activate(chat) => self.activate(chat),
            Command::Approve { request_id, allow } => {
                self.decisions.approve(&request_id, allow);
                Ok(())
            }
            Command::Answer { .. } => Err(format!("{} asked no question.", self.agent)),
            Command::ListSessions => self.publish_library().await,
            Command::Refresh => self.refresh().await,
            Command::RenameSession { session, name } => self.rename_session(&session, &name).await,
            Command::DeleteSession(session) => self.delete_session(&session).await,
            Command::ExportSession { session, path } => self.export_session(&session, &path).await,
            Command::Shutdown => Ok(()),
        }
    }

    async fn chat_command(&mut self, id: ChatId, command: ChatCommand) -> Result<(), String> {
        if matches!(command, ChatCommand::Close) {
            return self.close_chat(id).await;
        }
        if matches!(command, ChatCommand::Abort) {
            return self.cancel_chat(id).await;
        }
        if !self.chats.contains_key(&id)
            && self.recoveries.contains_key(&id)
            && matches!(
                command,
                ChatCommand::Send { .. } | ChatCommand::RunCommand { .. }
            )
        {
            self.start_replacement(id).await?;
        }
        let agent = self.agent;
        let chat = self
            .chats
            .get_mut(&id)
            .ok_or_else(|| "That chat is no longer open.".to_string())?;
        if chat.recovering
            && !chat.ready
            && matches!(
                command,
                ChatCommand::Send { .. } | ChatCommand::RunCommand { .. }
            )
        {
            if chat.queued.is_some() {
                return Err(format!(
                    "{agent} is still answering. Wait for it, or press Stop."
                ));
            }
            chat.queued = Some(command);
            return Ok(());
        }
        apply_chat(chat, command, agent, &self.events).await?;
        self.active = id;
        self.update_library(id);
        self.save_tabs()
    }
}

async fn apply_chat(
    chat: &mut AcpChat,
    command: ChatCommand,
    agent: &str,
    events: &EventQueue,
) -> Result<(), String> {
    match command {
        ChatCommand::Send {
            text,
            context,
            images,
        } => chat.send(&text, &context, &images, events).await,
        ChatCommand::Abort => chat.cancel().await,
        ChatCommand::SetModel { model, effort } => chat.set_model(&model, effort).await,
        ChatCommand::RunCommand { name, args } => {
            let text = format!("/{name} {args}");
            chat.send(text.trim_end(), &[], &[], events).await
        }
        ChatCommand::Rename(name) => chat.rename(&name, events).await,
        ChatCommand::CloseCanvas(_) | ChatCommand::OpenCanvas(_) => {
            Err("Native canvases require Copilot.".into())
        }
        ChatCommand::SetAgent(_) => Err("Custom agents require Copilot.".into()),
        ChatCommand::SetMode(_) => Err(format!("{agent} keeps its own modes.")),
        ChatCommand::SetYolo(_) => Err(format!("{agent} decides which requests need approval.")),
        ChatCommand::Close => Ok(()),
    }
}
