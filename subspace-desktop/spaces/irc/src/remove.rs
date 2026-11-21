//! Removing a channel or private conversation from the list, from its row
//! menu or the toolbar's More menu: **Leave channel** parts a joined channel,
//! **Remove from list** removes a channel we are no longer in, and **Close
//! conversation** closes a private one. Each closes the buffer's panel, and
//! the item stays out of the list after a restart, a reconnect or a history
//! replay (`state::Removed`). The server buffer cannot be removed.

use gpui_kit::component::menu::PopupMenuItem;
use gpui_kit::{Context, WeakEntity};
use subspace_client_irc::{BufferKey, Command, ConnectionStatus, Target};
use subspace_framework::dashboard::panel::IconName;
use subspace_framework::dashboard::row_menu::{MenuCommand, MenuGroup};

use crate::dock::Where;
use crate::state::{IrcState, server_key};
use crate::{Irc, dock, view};

/// How a buffer leaves the list, which names its menu item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Removal {
    /// Part a joined channel and remove it.
    Leave,
    /// Remove a channel we are no longer in, such as after a kick: there is
    /// nothing to leave.
    Unlist,
    /// Close the private conversation until the next message.
    Close,
}

impl Removal {
    /// How `buffer` leaves the list, given whether we are `joined` to it; the
    /// server buffer does not.
    pub fn of(buffer: &BufferKey, joined: bool) -> Option<Self> {
        match (&buffer.target, joined) {
            (Target::Channel(_), true) => Some(Self::Leave),
            (Target::Channel(_), false) => Some(Self::Unlist),
            (Target::Query(_), _) => Some(Self::Close),
            (Target::Server, _) => None,
        }
    }

    /// How `buffer` leaves the list as `state` holds it now.
    pub fn now(state: &IrcState, buffer: &BufferKey) -> Option<Self> {
        let joined = state.buffer(buffer).is_some_and(|held| held.joined);
        Self::of(buffer, joined)
    }

    pub(crate) fn label(self) -> &'static str {
        self.look().0
    }

    fn icon(self) -> IconName {
        self.look().1
    }

    /// The command's label and icon, in one place per variant.
    fn look(self) -> (&'static str, IconName) {
        match self {
            Self::Leave => ("Leave channel", IconName::LogOut),
            Self::Unlist => ("Remove from list", IconName::X),
            Self::Close => ("Close conversation", IconName::X),
        }
    }
}

/// Whether `buffer` can leave the list now. Parting a joined channel needs
/// its network connected; a channel we are not in, and a private
/// conversation, can always go.
pub(crate) fn available(state: &IrcState, buffer: &BufferKey) -> bool {
    match Removal::now(state, buffer) {
        None => false,
        Some(Removal::Unlist | Removal::Close) => true,
        Some(Removal::Leave) => state
            .network(&buffer.network)
            .is_some_and(|network| matches!(network.status, ConnectionStatus::Connected { .. })),
    }
}

/// The menu item that removes `buffer` the `removal` way; disabled without
/// a buffer.
pub(crate) fn item(
    owner: WeakEntity<Irc>,
    removal: Removal,
    buffer: Option<BufferKey>,
) -> PopupMenuItem {
    PopupMenuItem::new(removal.label())
        .icon(removal.icon())
        .disabled(buffer.is_none())
        .on_click(move |_, _, cx| {
            if let Some(buffer) = buffer.clone() {
                _ = owner.update(cx, |board, cx| remove(board, &buffer, cx));
            }
        })
}

/// The row menu's command that removes `buffer`, in the Remove group, as
/// `state` holds it now; none for the server buffer. Leaving a channel
/// waits for its network to be connected.
pub(crate) fn command(
    owner: WeakEntity<Irc>,
    state: &IrcState,
    buffer: &BufferKey,
) -> Option<MenuCommand> {
    let removal = Removal::now(state, buffer)?;
    let target = buffer.clone();
    let command = MenuCommand::new(
        MenuGroup::Remove,
        removal.label(),
        removal.icon(),
        move |_, cx| {
            _ = owner.update(cx, |board, cx| remove(board, &target, cx));
        },
    );
    Some(command.disabled(!available(state, buffer), crate::listing::NOT_CONNECTED))
}

/// Parts `buffer` when it is a joined channel, tells the runtime to forget
/// it, removes it from the list, saving that, and closes its panel. The
/// conversation the user was in stays in front, or the network's server
/// buffer when that was `buffer`, so the list keeps a selection.
pub(crate) fn remove(board: &mut Irc, buffer: &BufferKey, cx: &mut Context<Irc>) {
    let Some(hub) = board.hub.clone() else {
        return;
    };
    hub.update(cx, |hub, cx| {
        let removal = Removal::now(&hub.state, buffer);
        for command in commands(buffer, removal) {
            hub.send(command);
        }
        hub.remove(buffer, cx);
    });
    let closing = buffer.clone();
    dock::with_window(board, cx, move |board, window, cx| {
        dock::close(board, &closing, window, cx);
    });
    view::sync(board, cx);
    let front = next_in_front(board.dock.focused.as_ref(), buffer);
    dock::request(board, front, Where::Tab, cx);
}

/// What the runtime is told when `buffer` leaves the list: a joined channel
/// is parted, and every removed buffer is forgotten, so a channel is not
/// joined at connect and a private conversation's notices go to the server
/// buffer again.
fn commands(buffer: &BufferKey, removal: Option<Removal>) -> Vec<Command> {
    let part = (removal == Some(Removal::Leave)).then(|| Command::Part {
        buffer: buffer.clone(),
        reason: String::new(),
    });
    let forget = removal.map(|_| Command::Forget {
        buffer: buffer.clone(),
    });
    part.into_iter().chain(forget).collect()
}

/// What stays in front after `removed` leaves: the conversation in front,
/// or the network's server buffer when that was `removed`.
fn next_in_front(focused: Option<&BufferKey>, removed: &BufferKey) -> BufferKey {
    focused
        .filter(|focused| *focused != removed)
        .cloned()
        .unwrap_or_else(|| server_key(&removed.network))
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use subspace_client_irc::{Command, ConnectionStatus, Event};

    use super::{Removal, available, commands, next_in_front};
    use crate::fixtures::{channel, config, network_id, query};
    use crate::state::{IrcState, server_key};

    #[test]
    fn leaving_parts_a_joined_channel_and_every_removal_is_forgotten() {
        let (room, mira) = (channel("#ops"), query("mira"));
        let forget = |buffer: &subspace_client_irc::BufferKey| Command::Forget {
            buffer: buffer.clone(),
        };
        let part = Command::Part {
            buffer: room.clone(),
            reason: String::new(),
        };
        assert_eq!(commands(&room, Some(Removal::Leave)), [part, forget(&room)]);
        assert_eq!(
            commands(&room, Some(Removal::Unlist)),
            [forget(&room)],
            "after a kick"
        );
        assert_eq!(commands(&mira, Some(Removal::Close)), [forget(&mira)]);
        assert!(commands(&server_key(&network_id()), None).is_empty());
    }

    #[test]
    fn the_label_follows_what_removing_does_and_when_it_can() {
        let mut state = IrcState::new(&[config()], Vec::new());
        let room = channel("#ops");
        let server = server_key(&network_id());
        assert_eq!(Removal::now(&state, &room), Some(Removal::Unlist));
        assert_eq!(Removal::now(&state, &query("mira")), Some(Removal::Close));
        assert_eq!(Removal::now(&state, &server), None);
        assert!(available(&state, &room) && !available(&state, &server));

        let joined = Event::Joined {
            buffer: room.clone(),
        };
        state.apply(joined, Utc::now());
        assert_eq!(Removal::now(&state, &room), Some(Removal::Leave));
        assert!(
            !available(&state, &room),
            "a joined channel cannot be left while disconnected"
        );
        let connected = Event::Status {
            network: network_id(),
            status: ConnectionStatus::Connected {
                nick: "subspace".into(),
            },
        };
        state.apply(connected, Utc::now());
        assert!(available(&state, &room));
    }

    #[test]
    fn the_conversation_in_front_stays_unless_it_was_removed() {
        let (room, mira) = (channel("#ops"), query("mira"));
        assert_eq!(next_in_front(Some(&room), &mira), room);
        assert_eq!(next_in_front(Some(&mira), &mira), server_key(&network_id()));
        assert_eq!(next_in_front(None, &mira), server_key(&network_id()));
    }
}
