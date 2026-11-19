//! Board toolbar: connect or disconnect the selected network, join a
//! channel, show members, and leave or close the selected buffer.

use gpui_kit::component::{Disableable as _, menu::PopupMenuItem};
use gpui_kit::{App, Context, Entity, IntoElement as _, Window};
use subspace_client_irc::{BufferKey, Command, ConnectionStatus, NetworkId};
use subspace_framework::dashboard::action::{Intent, action_button, toolbar_button};
use subspace_framework::dashboard::anchored;
use subspace_framework::dashboard::overflow_toolbar::{OverflowToolbar, OverflowUnit, PopupEntry};
use subspace_framework::dashboard::panel::IconName;

use crate::remove::{self, Removal};
use crate::{Irc, dock, join};

const UNMEASURED_WIDTH: f32 = 0.;

/// Connected, or on the way there: Disconnect applies.
pub(crate) fn is_online(status: &ConnectionStatus) -> bool {
    !matches!(
        status,
        ConnectionStatus::Disconnected | ConnectionStatus::Failed { .. }
    )
}

pub(crate) fn render(board: &mut Irc, cx: &mut Context<Irc>) -> Option<Entity<OverflowToolbar>> {
    let Some(state) = board
        .hub
        .as_ref()
        .map(|hub| &hub.read(cx).state)
        .filter(|state| !state.networks().is_empty())
    else {
        board.toolbar = None;
        return None;
    };
    let network = board.selected_network(cx);
    let status = network
        .as_ref()
        .and_then(|id| state.network(id))
        .map(|known| &known.status);
    let units = vec![
        connection(board, network, status.is_some_and(is_online)),
        join(board, join_unavailable(status)),
        members(board, cx),
        more(board, cx),
    ];
    Some(OverflowToolbar::sync(&mut board.toolbar, units, cx))
}

struct Control {
    id: &'static str,
    label: &'static str,
    icon: IconName,
    unavailable: Option<&'static str>,
    active: Option<bool>,
}

impl Control {
    fn unit(self, run: impl Fn(&mut Window, &mut App) + Clone + 'static) -> OverflowUnit {
        let Self {
            id,
            label,
            icon,
            unavailable,
            active,
        } = self;
        let run_popup = run.clone();
        let popup = PopupEntry::new(move || {
            let run = run_popup.clone();
            let title =
                unavailable.map_or_else(|| label.to_owned(), |reason| format!("{label}: {reason}"));
            PopupMenuItem::new(title)
                .icon(icon)
                .disabled(unavailable.is_some())
                .checked(active.unwrap_or(false))
                .on_click(move |_, window, cx| run(window, cx))
        });
        // A changed icon needs a different projection key. The button ID is stable.
        OverflowUnit::inline(
            label,
            UNMEASURED_WIDTH,
            move |_, cx| {
                let run = run.clone();
                let button = match active {
                    Some(active) => {
                        toolbar_button(id, icon, label, (Intent::Neutral, true), cx).latched(active)
                    }
                    None => action_button(id, Intent::Neutral, Some(icon), label),
                };
                button
                    .disabled(unavailable.is_some())
                    .tooltip(unavailable.unwrap_or(label))
                    .on_click(move |_, window, cx| run(window, cx))
            },
            vec![popup],
        )
    }
}

fn connection(board: &Irc, network: Option<NetworkId>, online: bool) -> OverflowUnit {
    let (icon, label) = if online {
        (IconName::Unplug, "Disconnect")
    } else {
        (IconName::Plug, "Connect")
    };
    let owner = board.me.clone();
    Control {
        id: "irc-connection",
        label,
        icon,
        unavailable: network.is_none().then_some("Select a network"),
        active: None,
    }
    .unit(move |_, cx| {
        let Some(network) = network.clone() else {
            return;
        };
        let command = if online {
            Command::Disconnect {
                network,
                reason: String::new(),
            }
        } else {
            Command::Connect(network)
        };
        _ = owner.update(cx, |board, cx| board.send(command, cx));
    })
}

/// Why Join is unavailable for the selected network's `status`, if it is.
pub(crate) fn join_unavailable(status: Option<&ConnectionStatus>) -> Option<&'static str> {
    match status {
        None => Some("Select a network to join a channel"),
        Some(ConnectionStatus::Connected { .. }) => None,
        Some(_) => Some("Connect to the network to join a channel"),
    }
}

fn join(board: &Irc, unavailable: Option<&'static str>) -> OverflowUnit {
    let owner = board.me.clone();
    Control {
        id: "irc-join",
        label: "Join…",
        icon: IconName::Hash,
        unavailable,
        active: None,
    }
    .unit(move |window, cx| {
        _ = owner.update(cx, |board, cx| join::open(board, window, cx));
    })
}

fn members(board: &Irc, cx: &App) -> OverflowUnit {
    let owner = board.me.clone();
    Control {
        id: "irc-members-toggle",
        label: "Members",
        icon: IconName::Users,
        unavailable: None,
        active: Some(board.dock.members_open(cx)),
    }
    .unit(move |window, cx| {
        _ = owner.update(cx, |board, cx| dock::toggle_members(board, window, cx));
    })
}

/// Project the removal choices directly into the overflow menu.
fn more(board: &Irc, cx: &App) -> OverflowUnit {
    let owner = board.me.clone();
    let items = removal_items(board, cx);
    let entries = items
        .iter()
        .map(|(removal, buffer)| {
            let (owner, removal, buffer) = (owner.clone(), *removal, buffer.clone());
            PopupEntry::new(move || remove::item(owner.clone(), removal, buffer.clone()))
        })
        .collect();
    // Leaving a joined room and removing an unjoined one have different icons.
    let key = items[0].0.label();
    OverflowUnit::inline(
        key,
        UNMEASURED_WIDTH,
        move |_, _| {
            let (owner, items) = (owner.clone(), items.clone());
            let button = action_button("irc-more", Intent::Quiet, Some(IconName::Ellipsis), "More")
                .dropdown_caret(true);
            anchored::dropdown_menu(button, move |menu, _, _| {
                items.iter().fold(menu, |menu, (removal, buffer)| {
                    menu.item(remove::item(owner.clone(), *removal, buffer.clone()))
                })
            })
            .into_any_element()
        },
        entries,
    )
}

fn removal_items(board: &Irc, cx: &App) -> [(Removal, Option<BufferKey>); 2] {
    let selected = board.selected_buffer().cloned();
    let state = board.hub.as_ref().map(|hub| &hub.read(cx).state);
    let removal = selected
        .as_ref()
        .zip(state)
        .and_then(|(buffer, state)| Removal::now(state, buffer));
    let removable = selected
        .zip(state)
        .filter(|(buffer, state)| remove::available(state, buffer))
        .map(|(buffer, _)| buffer);
    menu_items(removal, removable)
}

/// The More menu's two removal items, for a channel and for a private
/// conversation, each enabled with `removable` when the selected row is of
/// its kind. The channel item names what removing the selected channel does.
fn menu_items(
    removal: Option<Removal>,
    removable: Option<BufferKey>,
) -> [(Removal, Option<BufferKey>); 2] {
    let channel = match removal {
        Some(Removal::Unlist) => Removal::Unlist,
        _ => Removal::Leave,
    };
    let enabled = |kind: Removal| removable.clone().filter(|_| removal == Some(kind));
    [
        (channel, enabled(channel)),
        (Removal::Close, enabled(Removal::Close)),
    ]
}

#[cfg(test)]
mod tests {
    use subspace_client_irc::ConnectionStatus;

    use super::{Removal, is_online, join_unavailable, menu_items};
    use crate::fixtures::{channel, query};

    #[test]
    fn the_more_menu_names_and_enables_the_selected_row_s_removal() {
        let (room, mira) = (channel("#ops"), query("mira"));
        assert_eq!(
            menu_items(Some(Removal::Unlist), Some(room.clone())),
            [
                (Removal::Unlist, Some(room.clone())),
                (Removal::Close, None)
            ],
            "a channel we are no longer in is removed, not left"
        );
        assert_eq!(
            menu_items(Some(Removal::Leave), None),
            [(Removal::Leave, None), (Removal::Close, None)],
            "joined while disconnected: unavailable"
        );
        assert_eq!(
            menu_items(Some(Removal::Close), Some(mira.clone())),
            [(Removal::Leave, None), (Removal::Close, Some(mira))]
        );
        assert_eq!(
            menu_items(None, None),
            [(Removal::Leave, None), (Removal::Close, None)]
        );
    }

    #[test]
    fn join_needs_a_connected_network() {
        let connected = ConnectionStatus::Connected { nick: "me".into() };
        assert_eq!(join_unavailable(Some(&connected)), None);
        assert_eq!(
            join_unavailable(Some(&ConnectionStatus::Registering)),
            Some("Connect to the network to join a channel")
        );
        assert_eq!(
            join_unavailable(Some(&ConnectionStatus::Disconnected)),
            Some("Connect to the network to join a channel")
        );
        assert!(join_unavailable(None).is_some());
    }

    #[test]
    fn disconnect_applies_while_connecting_or_connected() {
        assert!(is_online(&ConnectionStatus::Connecting));
        assert!(is_online(&ConnectionStatus::Connected {
            nick: "me".into()
        }));
        assert!(is_online(&ConnectionStatus::Reconnecting {
            attempt: 1,
            delay_secs: 5,
            error: String::new()
        }));
        assert!(!is_online(&ConnectionStatus::Disconnected));
        assert!(!is_online(&ConnectionStatus::Failed {
            error: "refused".into()
        }));
    }
}
