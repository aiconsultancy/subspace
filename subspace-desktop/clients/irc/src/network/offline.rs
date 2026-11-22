//! Answers to owner commands while a network has no session, so none is
//! dropped silently.

use chrono::{DateTime, Utc};

use crate::model::{
    BufferKey, Command, Event, Line, LineKind, NetworkConfig, ReactionError, SendResult, Target,
    Update,
};

/// What a command gets while disconnected or connecting.
enum Reply {
    Event(Event),
    Error { buffer: BufferKey, text: String },
}

impl Reply {
    fn event(self, now: DateTime<Utc>) -> Event {
        match self {
            Self::Event(event) => event,
            Self::Error { buffer, text } => Event::Line(Line {
                buffer,
                id: None,
                reply_to: None,
                client_tags: Default::default(),
                time: now,
                kind: LineKind::Error,
                from: None,
                account: None,
                text,
                highlight: false,
                own: false,
            }),
        }
    }
}

/// The event answering `command` without a session: an error line for
/// what needs the server, an opened query, and an ended history request
/// (not a complete one: history may exist once connected). Read markers
/// and typing notices lapse quietly.
pub(super) fn reply(config: &NetworkConfig, command: &Command) -> Option<Event> {
    let name = match config.name.is_empty() {
        true => &config.id.0,
        false => &config.name,
    };
    describe(command, name).map(|reply| reply.event(Utc::now()))
}

fn describe(command: &Command, name: &str) -> Option<Reply> {
    let server = |network: &crate::model::NetworkId| BufferKey {
        network: network.clone(),
        target: Target::Server,
    };
    let reply = match command {
        Command::Input { buffer, .. }
        | Command::Say { buffer, .. }
        | Command::Reply { buffer, .. } => Reply::Error {
            buffer: buffer.clone(),
            text: format!("Not connected to {name}"),
        },
        Command::SendAll { buffer, id, .. } => Reply::Event(Event::Sent {
            buffer: buffer.clone(),
            id: *id,
            result: SendResult::Refused(format!("Not connected to {name}")),
        }),
        Command::Join {
            network, channel, ..
        } => Reply::Error {
            buffer: server(network),
            text: format!("Not connected to {name}: cannot join {channel}"),
        },
        Command::Part { buffer, .. } => Reply::Error {
            buffer: server(&buffer.network),
            text: format!("Not connected to {name}: cannot leave {}", target(buffer)),
        },
        Command::Query { network, nick } => Reply::Event(Event::Joined {
            buffer: BufferKey {
                network: network.clone(),
                target: Target::Query(nick.clone()),
            },
        }),
        Command::LoadHistory { buffer, .. } => Reply::Event(Event::History {
            buffer: buffer.clone(),
            lines: Vec::new(),
            complete: false,
        }),
        Command::LoadAround { buffer, .. } => Reply::Event(Event::Backfill {
            buffer: buffer.clone(),
            lines: Vec::new(),
        }),
        Command::MarkRead { .. }
        | Command::React { .. }
        | Command::Unreact { .. }
        | Command::Anchors { .. }
        | Command::Typing { .. }
        | Command::Forget { .. }
        | Command::SetAutojoin { .. }
        | Command::ListChannels { .. }
        | Command::Configure(_)
        | Command::Connect(_)
        | Command::Disconnect { .. } => return None,
    };
    Some(reply)
}

/// A reaction command's refusal without a session: nothing is sent.
pub(crate) fn refuse_reaction(command: &Command) -> Option<Update> {
    match command {
        Command::React { buffer, msgid, .. } | Command::Unreact { buffer, msgid, .. } => {
            Some(Update::ReactionRefused {
                buffer: buffer.clone(),
                msgid: msgid.clone(),
                error: ReactionError::NotConnected,
            })
        }
        _ => None,
    }
}

fn target(buffer: &BufferKey) -> &str {
    match &buffer.target {
        Target::Channel(name) | Target::Query(name) => name,
        Target::Server => "the server buffer",
    }
}

#[cfg(test)]
mod tests {
    use super::reply;
    use crate::model::{
        BufferKey, Command, Event, LineKind, NetworkConfig, NetworkId, SendResult, Target,
    };

    fn config() -> NetworkConfig {
        serde_json::from_str(r#"{"id":"local","name":"Local","host":"127.0.0.1","nickname":"me"}"#)
            .unwrap()
    }

    fn key(target: Target) -> BufferKey {
        BufferKey {
            network: NetworkId("local".into()),
            target,
        }
    }

    #[test]
    fn a_join_while_offline_is_reported_in_the_server_buffer() {
        let join = Command::Join {
            network: NetworkId("local".into()),
            channel: "#room".into(),
            key: None,
        };
        let Some(Event::Line(line)) = reply(&config(), &join) else {
            panic!("expected an error line");
        };
        assert_eq!(line.buffer, key(Target::Server));
        assert_eq!(line.kind, LineKind::Error);
        assert_eq!(line.text, "Not connected to Local: cannot join #room");
    }

    #[test]
    fn a_message_while_offline_is_reported_in_its_buffer() {
        let room = key(Target::Channel("#room".into()));
        let say = Command::Say {
            buffer: room.clone(),
            text: "hello".into(),
            action: false,
        };
        let Some(Event::Line(line)) = reply(&config(), &say) else {
            panic!("expected an error line");
        };
        assert_eq!(line.buffer, room);
        assert_eq!(line.text, "Not connected to Local");
    }

    #[test]
    fn an_all_or_nothing_send_while_offline_is_refused() {
        let room = key(Target::Channel("#room".into()));
        let send = Command::SendAll {
            buffer: room.clone(),
            id: 3,
            lines: vec!["hello".into()],
        };
        assert_eq!(
            reply(&config(), &send),
            Some(Event::Sent {
                buffer: room,
                id: 3,
                result: SendResult::Refused("Not connected to Local".into()),
            })
        );
    }

    #[test]
    fn history_requests_end_and_queries_open_while_offline() {
        let room = key(Target::Channel("#room".into()));
        let history = Command::LoadHistory {
            buffer: room.clone(),
            before: None,
            limit: 100,
        };
        assert_eq!(
            reply(&config(), &history),
            Some(Event::History {
                buffer: room.clone(),
                lines: Vec::new(),
                complete: false
            })
        );
        let query = Command::Query {
            network: NetworkId("local".into()),
            nick: "bob".into(),
        };
        assert_eq!(
            reply(&config(), &query),
            Some(Event::Joined {
                buffer: key(Target::Query("bob".into()))
            })
        );
        let typing = Command::Typing {
            buffer: room,
            state: crate::model::Typing::Active,
        };
        assert_eq!(reply(&config(), &typing), None);
    }
}
