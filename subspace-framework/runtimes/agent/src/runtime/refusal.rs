//! A message the runtime refused. The panel starts a turn when it hands a
//! message over, so a refusal while the chat runs no turn also ends that
//! turn. A refused steering message leaves the running turn to its own end.
//! Any other error the runtime reports for a chat is a notice and ends
//! nothing.

use super::pi::command_error;
use super::{ChatCommand, ChatEvent, ChatId, Command, Event, EventQueue};

/// The chat a command sends a message to, which starts or steers a turn.
pub(super) fn message_chat(command: &Command) -> Option<ChatId> {
    match command {
        Command::Chat {
            chat,
            command: ChatCommand::Send { .. } | ChatCommand::RunCommand { .. },
        } => Some(*chat),
        _ => None,
    }
}

/// Reports a command that failed, in its chat when it has one. `ended` is
/// the chat whose refused message started no turn.
pub(super) async fn report_failed(
    events: &EventQueue,
    chat: Option<ChatId>,
    error: String,
    ended: Option<ChatId>,
) {
    events.push(command_error(chat, error)).await;
    if let Some(chat) = ended {
        events.push(Event::of(chat, ChatEvent::TurnDone)).await;
    }
}

/// A message that started no turn, as a skill that only shows a note, ends
/// the turn the panel started for it, unless one runs.
#[cfg(feature = "copilot")]
pub(super) async fn ran_nothing(events: &EventQueue, chat: ChatId, running: bool) {
    if !running {
        events.push(Event::of(chat, ChatEvent::TurnDone)).await;
    }
}

/// The message that would have started a turn was refused: its error, then
/// the end of the turn the panel started for it.
pub(super) async fn refused(events: &EventQueue, chat: ChatId, error: String) {
    report_failed(events, Some(chat), error, Some(chat)).await;
}

#[cfg(test)]
mod tests {
    use super::message_chat;
    use crate::{ChatCommand, Command};

    #[test]
    fn only_a_message_starts_or_steers_a_turn() {
        let send = Command::Chat {
            chat: 3,
            command: ChatCommand::Send {
                text: "hello".into(),
                context: Vec::new(),
                images: Vec::new(),
            },
        };
        let skill = Command::Chat {
            chat: 4,
            command: ChatCommand::RunCommand {
                name: "review".into(),
                args: String::new(),
            },
        };
        let rename = Command::Chat {
            chat: 5,
            command: ChatCommand::Rename("name".into()),
        };
        assert_eq!(message_chat(&send), Some(3));
        assert_eq!(message_chat(&skill), Some(4));
        assert_eq!(message_chat(&rename), None);
        assert_eq!(message_chat(&Command::ListSessions), None);
    }
}
