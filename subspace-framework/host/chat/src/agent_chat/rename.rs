//! Renaming a tab in place: double-clicking its title opens an input over
//! it, Enter saves the name through the runtime and Escape cancels.

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::{Entity, Window};

use super::{ChatCommand, ChatId, ChatPanel, Context};

impl ChatPanel {
    /// Opens the editor over tab `id` with its current title selected.
    pub(super) fn start_rename(&mut self, id: ChatId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(title) = self
            .tabs
            .iter()
            .find(|tab| tab.id == id)
            .map(|tab| tab.title.to_string())
        else {
            return;
        };
        self.renaming = Some(id);
        self.rename_input.update(cx, |input, cx| {
            input.set_value(title, window, cx);
            input.focus(window, cx);
            input.select_all(window, cx);
        });
        cx.notify();
    }

    pub(super) fn cancel_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.renaming.take().is_none() {
            return;
        }
        self.input.update(cx, |input, cx| input.focus(window, cx));
        cx.notify();
    }

    /// Saves the edited name. The runtime confirms it with a title event, so
    /// the tab is not renamed locally first.
    pub(super) fn save_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(chat) = self.renaming.take() else {
            return;
        };
        let name = self.rename_input.read(cx).value().to_string();
        self.send(
            super::Command::Chat {
                chat,
                command: ChatCommand::Rename(name),
            },
            cx,
        );
        self.input.update(cx, |input, cx| input.focus(window, cx));
        cx.notify();
    }

    pub(super) fn on_rename_event(
        &mut self,
        _: &Entity<InputState>,
        event: &InputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            InputEvent::PressEnter { .. } => self.save_rename(window, cx),
            InputEvent::Blur => self.cancel_rename(window, cx),
            _ => {}
        }
    }
}
