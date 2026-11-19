use super::logic::{
    apply_transcript, became_ready, bool_preference, change_preference, clamped_width,
    context_snapshot, include_current, input_intent, latest_commands, remove_reference,
    resolve_pending, skill_commands,
};
use super::tabs::{Tab, tab_index};
use super::{
    AgentHandle, ChatCommand, ChatPanel, ChatPanelEvent, Command, Context, InputIntent, Preference,
    WIDTH_KEY, Window, prefs,
};

impl ChatPanel {
    /// Asks the runtime for a fresh session list. The shell calls this for a
    /// dashboard that lists the saved chats.
    pub fn refresh_sessions(&mut self, cx: &mut Context<Self>) {
        self.send(Command::ListSessions, cx);
    }

    pub(super) fn reconnect(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.restart_runtime(cx);
        self.input.update(cx, |input, cx| input.clear(window, cx));
        self.input.update(cx, |input, cx| input.focus(window, cx));
    }

    /// Stops the runtime and starts a new one with fresh chats. A failed
    /// chat's failure and last message wait for its restored tab.
    pub(super) fn restart_runtime(&mut self, cx: &mut Context<Self>) {
        let _ = self
            .handle
            .take()
            .map(|handle| handle.send(Command::Shutdown));
        let carried = super::carry::take(&mut self.tabs);
        self.start(cx);
        self.carried = carried;
        self.sync_toolbar(cx);
        cx.notify();
    }

    pub(super) fn refresh_context(&mut self, cx: &mut Context<Self>) {
        let _ = self.providers.context.clone().map(|provider| {
            let context = provider(cx);
            let changed = self.replace_current(context);
            let _ = changed.then(|| cx.notify());
        });
    }

    pub(super) fn send(&mut self, command: Command, cx: &mut Context<Self>) -> bool {
        if self.is_disabled() {
            return false;
        }
        let engine = self.engine_label();
        let result = self
            .handle
            .as_ref()
            .ok_or_else(|| format!("Open chat to connect {engine}."))
            .and_then(|handle| handle.send(command));
        let sent = self.command_result(result);
        self.log_accepted_errors(cx);
        cx.notify();
        sent
    }

    /// Sends a command to the chat the panel shows.
    pub(super) fn send_chat(&mut self, command: ChatCommand, cx: &mut Context<Self>) -> bool {
        let command = self.chat_command(command);
        self.send(command, cx)
    }

    pub(super) fn add_current(&mut self, cx: &mut Context<Self>) {
        self.refresh_context(cx);
        let current = self.current.clone();
        let tab = self.tab_mut();
        tab.context = context_snapshot(&tab.context, current, true);
        cx.notify();
    }

    pub(super) fn remove_context(&mut self, key: &str, cx: &mut Context<Self>) {
        self.refresh_context(cx);
        let current = self.current.clone();
        let mut automatic = self.auto_context;
        let disabled = remove_reference(
            &mut self.tab_mut().context,
            current.as_ref(),
            &mut automatic,
            key,
        );
        self.auto_context = automatic;
        let _ = disabled.then(|| prefs::set(cx, "chat.auto-context", "0"));
        self.sync_toolbar(cx);
        cx.notify();
    }

    /// The intents the panel answers itself, without a runtime command.
    fn handled_here(
        &mut self,
        intent: &InputIntent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        match intent {
            InputIntent::NewChat => self.new_tab(window, cx),
            InputIntent::CloseChat => self.close_tab(self.tab().id, window, cx),
            InputIntent::OpenSessions => self.show_sessions(window, cx),
            InputIntent::OpenModels => self
                .toolbar
                .update(cx, |toolbar, cx| toolbar.open_models(window, cx)),
            _ => return false,
        }
        true
    }

    pub(super) fn submit(&mut self, text: String, window: &mut Window, cx: &mut Context<Self>) {
        self.refresh_context(cx);
        let intent = input_intent(&text, &self.commands);
        if self.handled_here(&intent, window, cx) {
            self.clear_input(window, cx);
            return;
        }
        let context = context_snapshot(
            &self.tab().context,
            self.current.clone(),
            include_current(&intent, self.auto_context),
        );
        let mut submission = self.prepare_submission(intent, context);
        self.log_accepted_errors(cx);
        let accepted = submission
            .command
            .take()
            .map(|command| self.send(command, cx))
            .unwrap_or(submission.clear);
        let _ = submission
            .answer_id
            .as_deref()
            .filter(|_| accepted)
            .map(|id| resolve_pending(&mut self.tab_mut().pending, id));
        let clear = self.finish_submission(submission, accepted);
        let _ = clear.then(|| self.clear_input(window, cx));
        self.sync_toolbar(cx);
        cx.notify();
    }

    fn clear_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.tab_mut().draft.clear();
        self.input.update(cx, |input, cx| input.clear(window, cx));
        self.slash.update(cx, |slash, cx| slash.set_text("", cx));
        self.input
            .update(cx, |input, cx| input.set_completing(false, cx));
    }

    /// Keeps the unsent text with the tab it was typed in.
    pub(super) fn remember_draft(&mut self, cx: &mut Context<Self>) {
        let draft = self.input.read(cx).text(cx);
        self.tab_mut().draft = draft;
    }

    pub(super) fn show_draft(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let draft = self.tab().draft.clone();
        self.input
            .update(cx, |input, cx| input.set_text(&draft, window, cx));
        self.input.update(cx, |input, cx| input.focus(window, cx));
    }

    pub(super) fn request_yolo(&mut self, approve_all: bool, cx: &mut Context<Self>) {
        if self.reject_unavailable_copilot_controls(cx) {
            return;
        }
        self.tab_mut().confirm_yolo = approve_all;
        if !approve_all {
            self.send_chat(ChatCommand::SetYolo(false), cx);
        }
        cx.notify();
    }

    pub(super) fn confirm_mode(&mut self, cx: &mut Context<Self>) {
        if self.reject_unavailable_copilot_controls(cx) {
            self.tab_mut().confirm_yolo = false;
            return;
        }
        self.send_chat(ChatCommand::SetYolo(true), cx);
        self.tab_mut().confirm_yolo = false;
        cx.notify();
    }

    pub(super) fn toggle_preference(&mut self, preference: Preference, cx: &mut Context<Self>) {
        let (key, enabled) = change_preference(
            preference,
            &mut self.show_tools,
            &mut self.show_thinking,
            &mut self.auto_context,
        );
        prefs::set(cx, key, bool_preference(enabled));
        self.sync_toolbar(cx);
        cx.notify();
    }

    pub(super) fn resize(&mut self, width: f32, window: &Window, cx: &mut Context<Self>) {
        self.width = clamped_width(width, f32::from(window.viewport_size().width));
        prefs::set(cx, WIDTH_KEY, self.width.to_string());
        cx.notify();
    }

    pub(super) fn answer(&mut self, id: String, answer: String, cx: &mut Context<Self>) -> bool {
        let accepted = self.send(
            Command::Answer {
                request_id: id.clone(),
                answer,
            },
            cx,
        );
        let _ = accepted.then(|| resolve_pending(&mut self.tab_mut().pending, &id));
        cx.notify();
        accepted
    }

    pub(super) fn approve(&mut self, id: String, allow: bool, cx: &mut Context<Self>) {
        let accepted = self.send(
            Command::Approve {
                request_id: id.clone(),
                allow,
            },
            cx,
        );
        let _ = accepted.then(|| resolve_pending(&mut self.tab_mut().pending, &id));
        cx.notify();
    }

    /// Applies one runtime event and the transcript row it produced.
    pub(super) fn receive(&mut self, event: super::Event, cx: &mut Context<Self>) {
        self.persist_confirmed_model(&event, cx);
        let update = self.apply_event(event);
        self.log_accepted_errors(cx);
        let Some((chat, update)) = update else {
            return;
        };
        let Some(index) = tab_index(&self.tabs, chat) else {
            return;
        };
        if let Some(tab) = self.tabs.get_mut(index) {
            apply_transcript(&mut tab.entries, update.transcript);
        }
        let _ = update
            .attention
            .map(|kind| cx.emit(ChatPanelEvent::Attention { kind, chat }));
    }

    fn persist_confirmed_model(&self, event: &super::Event, cx: &mut Context<Self>) {
        persist_confirmed_model(self.engine, event, cx);
    }

    /// Moves every running turn's status phrase on, and says whether any of
    /// them changed.
    fn advance_status(&mut self) -> bool {
        let mut changed = false;
        for turn in self.tabs.iter_mut().filter_map(Tab::running_mut) {
            changed |= turn.advance();
        }
        changed
    }

    /// Sends what a chat held while it was starting.
    fn flush_queued(&mut self, cx: &mut Context<Self>) {
        for (chat, queued) in self.strip().ready_to_send() {
            let command = queued.clone().into_command();
            let sent = self.send(Command::Chat { chat, command }, cx);
            let _ = sent
                .then(|| tab_index(&self.tabs, chat))
                .flatten()
                .and_then(|index| self.tabs.get_mut(index))
                .map(|tab| tab.sent(queued));
        }
    }

    pub(super) fn poll(&mut self, cx: &mut Context<Self>) {
        self.refresh_context(cx);
        let _ = self.end_stopped_turns().then(|| cx.notify());
        let events = self
            .handle
            .as_ref()
            .map(AgentHandle::take_events)
            .unwrap_or_default();
        if events.is_empty() {
            // A running turn still moves its status line on.
            let _ = self.advance_status().then(|| cx.notify());
            return;
        }
        let skills = latest_commands(&events);
        let ready = became_ready(&events);
        for event in events {
            self.receive(event, cx);
        }
        let _ = skills.map(|skills| {
            let skills = skill_commands(&skills);
            self.slash
                .update(cx, |slash, cx| slash.set_skills(&skills, cx));
        });
        let _ = ready.then(|| self.send(Command::ListSessions, cx));
        self.flush_queued(cx);
        self.advance_status();
        self.sync_toolbar(cx);
        self.publish_sessions(cx);
        self.scroll.scroll_to_bottom();
        cx.notify();
    }
}

fn persist_confirmed_model(engine: super::ChatEngine, event: &super::Event, cx: &mut super::App) {
    let Some(model) = confirmed_model(event) else {
        return;
    };
    let key = match engine {
        super::ChatEngine::Off => return,
        super::ChatEngine::Copilot => "chat.default-model.copilot",
        super::ChatEngine::Pi => "chat.default-model.pi",
        super::ChatEngine::Acp => "chat.default-model.acp",
    };
    prefs::set(cx, key, model);
    let _ = prefs::save(cx);
}

fn confirmed_model(event: &super::Event) -> Option<&str> {
    match event {
        super::Event::Chat {
            event: super::ChatEvent::ModelConfirmed { model },
            ..
        } => Some(model),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{confirmed_model, persist_confirmed_model};
    use crate::prefs;
    use subspace_agent::{ChatEvent, Event, ModelChoice};

    #[test]
    fn only_the_explicit_confirmation_event_is_persistable() {
        assert_eq!(
            confirmed_model(&Event::Chat {
                chat: 1,
                event: ChatEvent::ModelConfirmed {
                    model: "gpt-5".into()
                }
            }),
            Some("gpt-5")
        );
        assert_eq!(
            confirmed_model(&Event::Models(Vec::<ModelChoice>::new())),
            None
        );
        assert_eq!(
            confirmed_model(&Event::Chat {
                chat: 1,
                event: ChatEvent::SelectedModel {
                    model: "catalog".into(),
                    effort: None,
                }
            }),
            None
        );
    }

    #[gpui_kit::test]
    fn confirmed_model_uses_the_shared_prefs_owner_and_survives_later_save(
        cx: &mut gpui_kit::TestAppContext,
    ) {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("prefs.json");
        cx.update(|cx| {
            prefs::init_file(Some(file.clone()), cx);
            persist_confirmed_model(
                subspace_agent::ChatEngine::Copilot,
                &Event::Chat {
                    chat: 1,
                    event: ChatEvent::ModelConfirmed {
                        model: "gpt-5".into(),
                    },
                },
                cx,
            );
            prefs::set(cx, "unrelated.preference", "later");
            prefs::save(cx).unwrap();
            assert_eq!(
                prefs::get(cx, "chat.default-model.copilot").as_deref(),
                Some("gpt-5")
            );
            assert_eq!(
                prefs::get(cx, "unrelated.preference").as_deref(),
                Some("later")
            );
            prefs::init_file(Some(file), cx);
            assert_eq!(
                prefs::get(cx, "chat.default-model.copilot").as_deref(),
                Some("gpt-5")
            );
            assert_eq!(
                prefs::get(cx, "unrelated.preference").as_deref(),
                Some("later")
            );
            persist_confirmed_model(
                subspace_agent::ChatEngine::Copilot,
                &Event::Models(Vec::new()),
                cx,
            );
            assert_eq!(
                prefs::get(cx, "chat.default-model.copilot").as_deref(),
                Some("gpt-5")
            );
        });
    }
}
