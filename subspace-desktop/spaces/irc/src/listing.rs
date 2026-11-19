//! The server's channels we are not in: joining one from the list, and
//! asking the server for its list again.

use gpui_kit::component::menu::PopupMenuItem;
use gpui_kit::{App, Context, WeakEntity};
use subspace_client_irc::{BufferKey, Command, ConnectionStatus, NetworkId};
use subspace_framework::dashboard::panel::IconName;
use subspace_framework::dashboard::row_menu::MenuChat;
use subspace_framework::dashboard::{Group, Row, RowMenu, list};

use crate::state::IrcState;
use crate::{Irc, rows};

/// The row menu's and palette's label for asking the server again.
pub(crate) const REFRESH: &str = "Refresh channel list";

/// Whether `network` is connected, which joining and listing need.
pub(crate) fn connected(state: &IrcState, network: &NetworkId) -> bool {
    state
        .network(network)
        .is_some_and(|known| matches!(known.status, ConnectionStatus::Connected { .. }))
}

/// Joins channel `buffer` from the list; it opens once the server says we
/// joined. While the network is not connected, the runtime says so in its
/// server buffer.
pub(crate) fn join(board: &Irc, buffer: &BufferKey, cx: &mut App) {
    if let Some(hub) = &board.hub {
        hub.update(cx, |hub, _| {
            let command = hub.state.join_listed(buffer);
            hub.send(command);
        });
    }
}

/// Selects listed channel `channel`, which we are not in, unfolding its
/// network, for Enter to join.
pub(crate) fn reveal(board: &mut Irc, channel: &BufferKey, cx: &mut Context<Irc>) {
    let key = rows::listed_key(channel);
    list::update(board, cx, |list| {
        list.reveal(&key);
        list.close_detail();
    });
}

/// Asks `network` for its channels again.
pub(crate) fn refresh(board: &Irc, network: &NetworkId, cx: &mut App) {
    board.send(
        Command::ListChannels {
            network: network.clone(),
        },
        cx,
    );
}

/// **Join**, for a channel row we are not in; unavailable while its network
/// is not connected.
pub(crate) fn join_item(owner: WeakEntity<Irc>, buffer: BufferKey, enabled: bool) -> PopupMenuItem {
    PopupMenuItem::new("Join")
        .icon(IconName::Hash)
        .disabled(!enabled)
        .on_click(move |_, _, cx| {
            let buffer = buffer.clone();
            _ = owner.update(cx, |board, cx| join(board, &buffer, cx));
        })
}

/// **Refresh channel list**, for a network header; unavailable while the
/// network is not connected.
pub(crate) fn refresh_item(
    owner: WeakEntity<Irc>,
    network: NetworkId,
    enabled: bool,
) -> PopupMenuItem {
    PopupMenuItem::new(REFRESH)
        .icon(IconName::RefreshCw)
        .disabled(!enabled)
        .on_click(move |_, _, cx| {
            let network = network.clone();
            _ = owner.update(cx, |board, cx: &mut Context<Irc>| {
                refresh(board, &network, cx)
            });
        })
}

/// A channel we are not in (`listed`) has nothing to open or add to chat:
/// Copy title, then Join.
pub(crate) fn row_menu(row: &Row, listed: bool) -> RowMenu {
    let menu = RowMenu::from_row(row);
    match listed {
        true => RowMenu {
            chat: MenuChat::Off,
            ..menu.opens_details(false)
        },
        false => menu,
    }
}

/// A network header offers Copy title, then Refresh channel list.
pub(crate) fn group_menu(group: &Group) -> RowMenu {
    RowMenu {
        chat: MenuChat::Off,
        ..RowMenu::new(group.name.clone()).opens_details(false)
    }
}

/// **Join** for the board's selected channel we are not in, or **Refresh
/// channel list** for its selected network header.
pub(crate) fn menu_item(board: &Irc, cx: &App) -> Option<PopupMenuItem> {
    let state = &board.hub.as_ref()?.read(cx).state;
    if let Some(channel) = board.selected_listed() {
        let enabled = connected(state, &channel.network);
        return Some(join_item(board.me.clone(), channel.clone(), enabled));
    }
    let header = board.selected_buffer().is_none();
    let network = board.selected_network(cx).filter(|_| header)?;
    let enabled = connected(state, &network);
    Some(refresh_item(board.me.clone(), network, enabled))
}
