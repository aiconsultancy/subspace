//! The actions of the pane's toolbar: find in the conversation, the More
//! menu, which connects or disconnects the selected network, joins a
//! channel, and leaves or closes the selected buffer, then Refresh. While the navigator is
//! hidden they act on the conversation the toolbar names instead. The
//! navigator's header repeats Join, and the command palette offers Join,
//! Connect and Disconnect too. The workspace's own switch shows and hides
//! the members.

use gpui_kit::component::{Disableable as _, menu::PopupMenuItem};
use gpui_kit::{App, Context, Entity, IntoElement as _, Window};
use subspace_client_irc::{BufferKey, Command, ConnectionStatus, NetworkId};
use subspace_framework::dashboard::Dashboard as _;
use subspace_framework::dashboard::action::{Intent, action_button, icon_button};
use subspace_framework::dashboard::anchored;
use subspace_framework::dashboard::find_in_page;
use subspace_framework::dashboard::icon_button::IconButton;
use subspace_framework::dashboard::overflow_toolbar::{OverflowToolbar, OverflowUnit, PopupEntry};
use subspace_framework::dashboard::panel::IconName;

use crate::remove::{self, Removal};
use crate::{Irc, join};

const UNMEASURED_WIDTH: f32 = 0.;
/// The find button's label, also its menu entry when the toolbar overflows.
const FIND: &str = "Find in conversation";

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
    // While the navigator is hidden, the conversation the toolbar names.
    let network = board.toolbar_network();
    let status = network
        .as_ref()
        .and_then(|id| state.network(id))
        .map(|known| &known.status);
    let network_items = [
        connection(board, network, status.is_some_and(is_online)),
        join(board, join_unavailable(status)),
    ];
    let units = vec![find(), more(board, network_items, cx), refresh(board)];
    Some(OverflowToolbar::sync(&mut board.toolbar, units, cx))
}

/// A command of the More menu, which runs `run` when chosen.
#[derive(Clone)]
struct Control {
    label: &'static str,
    icon: IconName,
    unavailable: Option<&'static str>,
    run: std::rc::Rc<dyn Fn(&mut Window, &mut App)>,
}

impl Control {
    /// The menu item, disabled with its reason while unavailable.
    fn item(&self) -> PopupMenuItem {
        let run = self.run.clone();
        let title = self.unavailable.map_or_else(
            || self.label.to_owned(),
            |reason| format!("{}: {reason}", self.label),
        );
        PopupMenuItem::new(title)
            .icon(self.icon)
            .disabled(self.unavailable.is_some())
            .on_click(move |_, window, cx| run(window, cx))
    }
}

/// The search button, which opens find in the conversation in front.
fn find() -> OverflowUnit {
    let open = |window: &mut Window, cx: &mut App| {
        window.dispatch_action(Box::new(find_in_page::Open), cx);
    };
    let popup = PopupEntry::new(move || {
        PopupMenuItem::new(FIND)
            .icon(IconName::Search)
            .on_click(move |_, window, cx| open(window, cx))
    });
    OverflowUnit::inline(
        FIND,
        UNMEASURED_WIDTH,
        move |_, _| {
            icon_button("irc-find", Intent::Quiet, IconName::Search, FIND)
                .focus_on_click()
                .tooltip(FIND)
                .on_click(move |_, window, cx| open(window, cx))
        },
        vec![popup],
    )
}

/// Refresh, at the right edge: it reads the networks and initiatives again.
fn refresh(board: &Irc) -> OverflowUnit {
    let owner = board.me.clone();
    OverflowUnit::refresh("irc-refresh", move |window, cx| {
        _ = owner.update(cx, |board, cx| board.reload(window, cx));
    })
}

fn connection(board: &Irc, network: Option<NetworkId>, online: bool) -> Control {
    let (icon, label) = if online {
        (IconName::Unplug, "Disconnect")
    } else {
        (IconName::Plug, "Connect")
    };
    let owner = board.me.clone();
    Control {
        label,
        icon,
        unavailable: network.is_none().then_some("Select a network"),
        run: std::rc::Rc::new(move |_, cx| {
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
        }),
    }
}

/// Why Join is unavailable for the selected network's `status`, if it is.
pub(crate) fn join_unavailable(status: Option<&ConnectionStatus>) -> Option<&'static str> {
    match status {
        None => Some("Select a network to join a channel"),
        Some(ConnectionStatus::Connected { .. }) => None,
        Some(_) => Some("Connect to the network to join a channel"),
    }
}

fn join(board: &Irc, unavailable: Option<&'static str>) -> Control {
    let owner = board.me.clone();
    Control {
        label: "Join…",
        icon: IconName::Hash,
        unavailable,
        run: std::rc::Rc::new(move |window, cx| {
            _ = owner.update(cx, |board, cx| join::open(board, window, cx));
        }),
    }
}

/// The navigator header's Join: the same dialog behind a glyph, unavailable
/// for the same reasons as **Join…**.
pub(crate) fn navigator_join(board: &Irc, cx: &mut Context<Irc>) -> IconButton {
    const LABEL: &str = "Join a channel";
    let network = board.selected_network();
    let unavailable = board.hub.as_ref().and_then(|hub| {
        let state = &hub.read(cx).state;
        let known = network.as_ref().and_then(|id| state.network(id));
        join_unavailable(known.map(|known| &known.status))
    });
    icon_button("irc-navigator-join", Intent::Quiet, IconName::Plus, LABEL)
        .disabled(unavailable.is_some())
        .tooltip(unavailable.unwrap_or(LABEL))
        .on_click(cx.listener(|board, _, window, cx| join::open(board, window, cx)))
}

/// The More menu: the network's commands, Connect or Disconnect and
/// **Join…**, then the selected buffer's removal choices. Its entries are
/// projected straight into the overflow menu.
fn more(board: &Irc, network_items: [Control; 2], cx: &App) -> OverflowUnit {
    let owner = board.me.clone();
    let items = removal_items(board, cx);
    let commands = network_items
        .iter()
        .cloned()
        .map(|control| PopupEntry::new(move || control.item()));
    let separator = PopupEntry::new(|| PopupMenuItem::Separator);
    let removals = items.iter().map(|(removal, buffer)| {
        let (owner, removal, buffer) = (owner.clone(), *removal, buffer.clone());
        PopupEntry::new(move || remove::item(owner.clone(), removal, buffer.clone()))
    });
    let entries = commands
        .chain(std::iter::once(separator))
        .chain(removals)
        .collect();
    // Each entry's icon can change, which needs a different projection key:
    // connecting and disconnecting, leaving a joined room and removing an
    // unjoined one.
    let key = format!("{}|{}", network_items[0].label, items[0].0.label());
    OverflowUnit::inline(
        key,
        UNMEASURED_WIDTH,
        move |_, _| {
            let (owner, items, network_items) =
                (owner.clone(), items.clone(), network_items.clone());
            let button = action_button("irc-more", Intent::Quiet, Some(IconName::Ellipsis), "More")
                .dropdown_caret(true);
            anchored::dropdown_menu(button, move |menu, _, _| {
                let menu = network_items
                    .iter()
                    .fold(menu, |menu, control| menu.item(control.item()))
                    .separator();
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
    let selected = board.toolbar_buffer().cloned();
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
