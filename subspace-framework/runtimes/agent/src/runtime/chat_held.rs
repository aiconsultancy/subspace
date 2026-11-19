//! Commands that arrive while a turn runs. Copilot applies a setting to the
//! next turn, so the chat holds the latest choice of each kind and applies it
//! when the turn ends, instead of refusing it. Another command that fails
//! during the turn is a note: an error would end the turn the panel shows
//! while Copilot keeps working.

use std::mem::discriminant;

use super::{Chat, ChatCommand, ChatEvent, Env, Event, display};

const HELD_NOTE: &str = "This setting applies when the current reply finishes.";

/// The latest held choice of each setting, in the order they were made.
#[derive(Default)]
pub(super) struct HeldSettings(Vec<ChatCommand>);

impl HeldSettings {
    /// Holds a setting, replacing an earlier choice of the same kind. Any
    /// other command is returned unchanged.
    fn hold(&mut self, command: ChatCommand) -> Option<ChatCommand> {
        if !is_setting(&command) {
            return Some(command);
        }
        self.0
            .retain(|held| discriminant(held) != discriminant(&command));
        self.0.push(command);
        None
    }

    fn take(&mut self) -> Vec<ChatCommand> {
        std::mem::take(&mut self.0)
    }
}

/// The commands that change what the next turn uses.
fn is_setting(command: &ChatCommand) -> bool {
    matches!(
        command,
        ChatCommand::SetModel { .. }
            | ChatCommand::SetAgent(_)
            | ChatCommand::SetMode(_)
            | ChatCommand::SetYolo(_)
    )
}

/// What a command result means for the panel: a failure while the turn keeps
/// running becomes a note, any other result is unchanged.
fn while_running(result: Result<(), String>, busy: bool) -> Result<Option<String>, String> {
    match result {
        Err(error) if busy => Ok(Some(error)),
        result => result.map(|()| None),
    }
}

impl Chat {
    pub(in crate::runtime) async fn apply(
        &mut self,
        env: &Env<'_>,
        command: ChatCommand,
    ) -> Result<(), String> {
        self.reconcile(env);
        let Some(command) = self.hold_while_busy(env, command).await else {
            return Ok(());
        };
        let result = self.apply_now(env, command).await;
        self.publish(env);
        if let Some(note) = while_running(result, self.busy)? {
            display::note(env.events, self.id, note).await;
        }
        Ok(())
    }

    /// While a turn runs, holds a setting and says so in the chat. Returns
    /// the command when it must run now.
    async fn hold_while_busy(
        &mut self,
        env: &Env<'_>,
        command: ChatCommand,
    ) -> Option<ChatCommand> {
        if !self.busy {
            return Some(command);
        }
        let unheld = self.held.hold(command);
        if unheld.is_none() {
            display::note(env.events, self.id, HELD_NOTE.into()).await;
        }
        unheld
    }

    /// Applies the settings held during the turn that just ended.
    pub(super) async fn apply_held(&mut self, env: &Env<'_>) {
        for command in self.held.take() {
            let result = match command {
                ChatCommand::SetYolo(enabled) => self.change_access(env, enabled).await,
                setting => self.configure(env, setting).await,
            };
            if let Err(error) = result {
                env.events
                    .push(Event::of(self.id, ChatEvent::Error(error)))
                    .await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{HeldSettings, while_running};
    use crate::{ChatCommand, ChatMode};

    #[test]
    fn a_failure_during_a_turn_is_a_note_and_otherwise_an_error() {
        assert_eq!(
            while_running(Err("refused".into()), true),
            Ok(Some("refused".into())),
            "the turn keeps running, so the panel must not end it"
        );
        assert_eq!(
            while_running(Err("failed".into()), false),
            Err("failed".into())
        );
        assert_eq!(while_running(Ok(()), true), Ok(None));
    }

    fn model(name: &str) -> ChatCommand {
        ChatCommand::SetModel {
            model: name.into(),
            effort: None,
        }
    }

    #[test]
    fn the_latest_choice_of_each_setting_waits_in_order() {
        let mut held = HeldSettings::default();
        assert!(held.hold(model("first")).is_none());
        assert!(held.hold(ChatCommand::SetMode(ChatMode::Plan)).is_none());
        assert!(held.hold(model("second")).is_none());
        let taken = held.take();
        assert!(matches!(
            taken.as_slice(),
            [ChatCommand::SetMode(ChatMode::Plan), ChatCommand::SetModel { model, .. }]
                if model == "second"
        ));
        assert!(held.take().is_empty(), "held settings apply once");
    }

    #[test]
    fn only_settings_are_held() {
        let mut held = HeldSettings::default();
        assert!(matches!(
            held.hold(ChatCommand::Rename("name".into())),
            Some(ChatCommand::Rename(_))
        ));
        assert!(held.hold(ChatCommand::SetYolo(false)).is_none());
    }
}
