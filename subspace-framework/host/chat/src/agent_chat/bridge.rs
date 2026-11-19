//! The subscriptions that join the input, toolbar and slash list to the
//! runtime: each component reports what the user chose, and the panel turns it
//! into a command.

use super::input::{ChatInput, ChatInputEvent};
use super::logic::{effort_for, sdk_mode};
use super::slash::{SlashEvent, SlashList};
use super::toolbar::{ChatToolbar, ToolbarEvent};
use super::{ChatCommand, ChatPanel, Context, Entity, Preference, Window};

impl ChatPanel {
    pub(super) fn on_input_event(
        &mut self,
        _: &Entity<ChatInput>,
        event: &ChatInputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            ChatInputEvent::Submit(text) => self.submit(text.clone(), window, cx),
            ChatInputEvent::Changed(text) => self.filter_commands(text, cx),
            ChatInputEvent::Complete => self.slash.update(cx, |slash, cx| slash.complete(cx)),
            ChatInputEvent::Next => self.slash.update(cx, |slash, cx| slash.step(1, cx)),
            ChatInputEvent::Previous => self.slash.update(cx, |slash, cx| slash.step(-1, cx)),
            ChatInputEvent::Cancel => self.slash.update(cx, |slash, cx| slash.close(cx)),
            ChatInputEvent::PasteAttachments(paste) => self.paste_attachments(paste.batch(), cx),
        }
    }

    fn filter_commands(&mut self, text: &str, cx: &mut Context<Self>) {
        let open = self.slash.update(cx, |slash, cx| {
            slash.set_text(text, cx);
            slash.is_open()
        });
        self.input
            .update(cx, |input, cx| input.set_completing(open, cx));
    }

    pub(super) fn on_slash_event(
        &mut self,
        _: &Entity<SlashList>,
        event: &SlashEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            SlashEvent::Complete { text, run: true } => {
                self.submit(text.trim().to_owned(), window, cx);
            }
            SlashEvent::Complete { text, run: false } => {
                let text = text.clone();
                self.input
                    .update(cx, |input, cx| input.set_text(&text, window, cx));
            }
            SlashEvent::Close => {}
        }
        self.input
            .update(cx, |input, cx| input.set_completing(false, cx));
        self.input.update(cx, |input, cx| input.focus(window, cx));
    }

    pub(super) fn on_toolbar_event(
        &mut self,
        _: &Entity<ChatToolbar>,
        event: &ToolbarEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            ToolbarEvent::AttachFile => self.attach(cx),
            ToolbarEvent::AddSelection => self.add_current(cx),
            ToolbarEvent::ToggleAutoContext => {
                self.toggle_preference(Preference::AutoContext, cx);
            }
            ToolbarEvent::ToggleTools => self.toggle_preference(Preference::Tools, cx),
            ToolbarEvent::ToggleThinking => self.toggle_preference(Preference::Thinking, cx),
            ToolbarEvent::SetMode(mode) => {
                if self.capabilities().copilot_controls {
                    self.send_chat(ChatCommand::SetMode(sdk_mode(*mode)), cx);
                } else {
                    self.tab_mut().set_error(
                        "Interaction modes are unavailable with Pi. Choose Copilot in Settings to use them."
                            .into(),
                    );
                }
            }
            ToolbarEvent::SetApproveAll(approve_all) => self.request_yolo(*approve_all, cx),
            ToolbarEvent::SetAgent(agent) => {
                if self.capabilities().copilot_controls {
                    self.send_chat(ChatCommand::SetAgent(agent.to_string()), cx);
                } else {
                    self.tab_mut().set_error(
                        "Custom agents are unavailable with Pi. Choose Copilot in Settings to use them."
                            .into(),
                    );
                }
            }
            ToolbarEvent::SetModel { model } => self.choose_model(model.clone().as_ref(), cx),
            ToolbarEvent::SetEffort(effort) => {
                self.choose_effort(effort.as_ref().map(ToString::to_string), cx);
            }
            ToolbarEvent::Send => self.send_input(window, cx),
            ToolbarEvent::Stop => {
                if self.send_chat(ChatCommand::Abort, cx) {
                    self.tab_mut().cancel_completion_alert();
                }
            }
        }
        self.log_accepted_errors(cx);
        self.sync_toolbar(cx);
    }

    /// Keep the reasoning effort when the new model supports it.
    fn choose_model(&mut self, model: &str, cx: &mut Context<Self>) {
        let effort = effort_for(&self.models, model, self.tab().effort.as_deref());
        self.send_chat(
            ChatCommand::SetModel {
                model: model.to_owned(),
                effort,
            },
            cx,
        );
    }

    fn choose_effort(&mut self, effort: Option<String>, cx: &mut Context<Self>) {
        let Some(model) = self.tab().model.clone() else {
            return;
        };
        let effort = effort_for(&self.models, &model, effort.as_deref());
        self.send_chat(ChatCommand::SetModel { model, effort }, cx);
    }

    fn send_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.input.read(cx).text(cx);
        if text.trim().is_empty() {
            return;
        }
        self.submit(text, window, cx);
    }
}
