//! The board's view: the workspace (buffer list, conversations, member
//! list), rows from the state, and keeping the hub told which buffer is read.

use gpui_kit::{AnyElement, App, Context, Entity, SharedString, WeakEntity, Window};
use subspace_client_irc::{BufferKey, Target};
use subspace_framework::dashboard::panel::IconName;
use subspace_framework::dashboard::row_menu::{MenuCommand, MenuGroup};
use subspace_framework::dashboard::{
    Detail, Group, ListDashboard, ListState, Row, RowMenu, Tokens, list,
};

use crate::dock::{self, IrcDock, Where};
use crate::{Irc, hub, listing, remove, rows, setup, transcript};

mod conversations;
mod details;
mod empty;
mod frame;
mod hosting;
mod navigator;

use details::{kind, kind_icon};

pub(crate) use details::{server_summary, status_line};
pub(crate) use frame::{space_docks, toggle_space_dock};
pub(crate) use hosting::{Hosted, open_channel};
/// Fewer lines than this on open asks the server for recent history.
const HISTORY_ON_OPEN: usize = 50;
/// Lines asked for per history request.
pub(crate) const HISTORY_PAGE: u16 = 100;

pub(crate) fn new(window: &mut Window, cx: &mut Context<Irc>) -> Irc {
    let focus = cx.focus_handle();
    let subscriptions = vec![cx.observe_window_activation(window, |board, window, cx| {
        board.window_active = window.is_window_active();
        publish_visible(board, cx);
    })];
    let mut board = Irc {
        me: cx.entity().downgrade(),
        focus,
        list: ListState::default(),
        buffers: Default::default(),
        listed: Default::default(),
        group_networks: Default::default(),
        ships: Default::default(),
        people: Default::default(),
        seen_fleets: Default::default(),
        hub: None,
        hub_subscription: None,
        networks: Vec::new(),
        active: false,
        find_revision: 0,
        window_active: window.is_window_active(),
        dock: IrcDock::new(window, cx),
        join: None,
        initiative_menu: Default::default(),
        toolbar: None,
        reveal: None,
        search: std::rc::Rc::new(subspace_framework::dashboard::ItemSearch::new([])),
        available_height: subspace_framework::dashboard::measure::unmeasured(cx),
        text_edge: subspace_framework::dashboard::workspace::text_edge(&Tokens::current(cx)),
        _subscriptions: subscriptions,
    };
    bind_hub(&mut board, cx);
    board
}

/// Follows the process hub: it starts after the board in some launches and
/// is replaced when Settings turns the board off and on again.
pub(crate) fn bind_hub(board: &mut Irc, cx: &mut Context<Irc>) {
    let current = hub::entity(cx);
    if current == board.hub {
        return;
    }
    board.hub_subscription = current
        .as_ref()
        .map(|hub| cx.observe(hub, |board, _, cx| sync(board, cx)));
    board.hub = current;
    board.dock.reset();
    board.find_revision = board.find_revision.wrapping_add(1);
    let owner = cx.entity().downgrade();
    cx.defer(move |cx| {
        _ = owner.update(cx, |board, cx| {
            if board.active {
                crate::initiatives::refresh(board, cx);
            }
            cx.notify();
        });
    });
    sync(board, cx);
}

/// Rebuilds the rows and the open transcript from the hub's state.
pub(crate) fn sync(board: &mut Irc, cx: &mut Context<Irc>) {
    let Some(hub) = board.hub.clone() else {
        return;
    };
    let state = &hub.read(cx).state;
    let retired: Vec<BufferKey> = board
        .dock
        .open
        .keys()
        .filter(|buffer| state.joins.retired(buffer))
        .cloned()
        .collect();
    board.networks = state.networks().to_vec();
    navigator::apply(board, &hub, cx);
    let opened = hub.update(cx, |hub, _| hub.state.take_opened());
    if let Some(query) = opened {
        reveal(board, &query, cx);
    }
    crate::initiatives::dismiss_stale_picker(board, cx);
    dock::restore(board, cx);
    if !retired.is_empty() {
        dock::close_retired(board, retired, cx);
    }
    publish_visible(board, cx);
    crate::reveal::settle(board, cx);
    cx.notify();
}

/// Tells the hub, while the board and its window are in front, which buffer
/// the user is reading, so it reads as it arrives, and which conversations
/// are on screen, which raise no notification.
pub(crate) fn publish_visible(board: &mut Irc, cx: &mut Context<Irc>) {
    let front = board.active && board.window_active;
    let visible = front.then(|| board.dock.focused.clone()).flatten();
    let shown = if front {
        dock::shown(board, cx)
    } else {
        Vec::new()
    };
    if let Some(hub) = board.hub.clone() {
        hub.update(cx, |hub, cx| hub.set_visible(visible, shown, cx));
    }
}

/// Selects buffer `buffer` in the list and shows its conversation.
pub(crate) fn reveal(board: &mut Irc, buffer: &BufferKey, cx: &mut Context<Irc>) {
    reveal_row(board, buffer, cx);
    dock::request(board, buffer.clone(), Where::Tab, cx);
}

/// Shows the buffer a notification names. A private conversation closed
/// since opens again, as choosing its person in the member list does; a
/// channel left since stays left.
pub(crate) fn show_notified(board: &mut Irc, buffer: &BufferKey, cx: &mut Context<Irc>) {
    if board
        .hub
        .as_ref()
        .is_some_and(|hub| hub.read(cx).state.joins.retired(buffer))
    {
        return;
    }
    let removed = board
        .hub
        .as_ref()
        .is_some_and(|hub| hub.read(cx).state.removed.get(buffer).is_some());
    match (&buffer.target, removed) {
        (_, false) => reveal(board, buffer, cx),
        (Target::Query(nick), true) => board.open_query(&buffer.network, nick, cx),
        _ => {}
    }
}

/// Selects buffer `buffer` in the list, unfolding its network.
pub(crate) fn reveal_row(board: &mut Irc, buffer: &BufferKey, cx: &mut Context<Irc>) {
    let key = rows::row_key(buffer);
    if board.list.selected() == Some(&key) {
        return;
    }
    list::update(board, cx, |list| {
        list.reveal(&key);
        list.close_detail();
    });
}

/// Recent history for a buffer that has little to show.
pub(crate) fn load_recent(board: &mut Irc, buffer: &BufferKey, cx: &mut Context<Irc>) {
    let Some(hub) = board.hub.clone() else {
        return;
    };
    let few = hub
        .read(cx)
        .state
        .buffer(buffer)
        .is_some_and(|held| held.entries.len() < HISTORY_ON_OPEN);
    if few && buffer.target != Target::Server {
        transcript::load_older(&hub, buffer, cx);
    }
}

impl ListDashboard for Irc {
    fn list(&self) -> &ListState {
        &self.list
    }

    fn list_mut(&mut self) -> &mut ListState {
        &mut self.list
    }

    /// Rows have no details panel; the conversation shows in the pane.
    fn detail(&self, key: &str) -> Option<Detail> {
        let buffer = self.buffers.get(key)?;
        let title = self.buffer_name(buffer);
        Some(Detail {
            kind: kind(&buffer.target).into(),
            id: title.clone(),
            title,
            ..Default::default()
        })
    }

    /// A ship channel leads with its captain's agent icon as the Icons
    /// setting asks, else the quiet `#`; a person in `People` with a
    /// presence dot; any other buffer with its kind's quiet glyph.
    fn row_leading(&self, row: &Row, cx: &App) -> Option<AnyElement> {
        navigator::leading(self, row, cx)
    }

    /// `People` leads with the people glyph; the other groups with their
    /// connection state's dot.
    fn group_leading(&self, group: &Group, cx: &App) -> Option<AnyElement> {
        let network = self.group_networks.get(&group.key)?;
        (rows::people_group_key(network) == group.key).then(|| navigator::people_mark(cx))
    }

    /// Networks and their buffers start on the workspace's one text edge,
    /// as navigator rows do.
    fn list_side_inset(&self) -> Option<gpui_kit::Pixels> {
        Some(self.text_edge)
    }

    /// A mention's `@` mark, then an unread count or a channel's user
    /// count, as a navigator row's quiet count, then a ship's one dot at the
    /// trailing edge.
    fn row_trailing(&self, row: &Row, cx: &App) -> Option<AnyElement> {
        Some(navigator::trailing(self, row, cx))
    }

    /// Every row with unread lines reads emphasised.
    fn row_emphasis(&self, row: &Row, cx: &App) -> bool {
        navigator::unread(self, &row.key, cx)
    }

    fn row_accessible_label(&self, row: &Row, cx: &App) -> SharedString {
        if self.listed.contains_key(&row.key) {
            return rows::listed_accessible_label(row);
        }
        let base = match self.buffer_state(&row.key, cx) {
            Some(buffer) => rows::accessible_label(&row.title, &buffer),
            None => row.title.clone(),
        };
        let mark = navigator::spoken_mark(self, &row.key);
        rows::activity::spoken(&base, mark.as_deref()).into()
    }

    /// A click selects a channel we are not in; a double click joins it.
    fn activates_on_click(&self, row: &Row) -> bool {
        !self.listed.contains_key(&row.key)
    }

    fn row_menu(&self, row: &Row) -> RowMenu {
        listing::row_menu(row, self.listed.contains_key(&row.key))
    }

    fn group_menu(&self, group: &Group) -> Option<RowMenu> {
        Some(listing::group_menu(group))
    }

    fn empty_message(&self) -> (SharedString, SharedString) {
        (setup::TITLE.into(), setup::HINT.into())
    }

    /// Enter or a click on a buffer opens its conversation in the dock;
    /// Enter or a double click on a channel we are not in joins it.
    fn on_activate(&mut self, key: &SharedString, cx: &mut Context<Self>) -> bool {
        if let Some(channel) = self.listed.get(key) {
            listing::join(self, channel, cx);
            return true;
        }
        let Some(buffer) = self.buffers.get(key).cloned() else {
            return false;
        };
        cx.emit(subspace_framework::dashboard::DashboardEvent::Opened(
            key.clone(),
        ));
        dock::request(self, buffer, Where::Tab, cx);
        true
    }

    /// Joining a listed channel or refreshing a network's list; for a
    /// buffer, opening beside or below, then leaving a channel or closing a
    /// private conversation. The server row offers no removal.
    fn row_commands(board: &Entity<Self>, cx: &App) -> Vec<MenuCommand> {
        let this = board.read(cx);
        if let Some(command) = listing::menu_command(this, cx) {
            return vec![command];
        }
        let Some(buffer) = this.selected_buffer().cloned() else {
            return Vec::new();
        };
        let mut commands = open_beside(&this.me, &buffer);
        let state = this.hub.as_ref().map(|hub| &hub.read(cx).state);
        commands.extend(state.and_then(|state| remove::command(this.me.clone(), state, &buffer)));
        commands
    }

    fn can_navigate(&self, _cx: &App) -> bool {
        self.join.is_none() && self.initiative_menu.host().is_none()
    }
}

/// Open to the right and Open below, in the Open group.
fn open_beside(owner: &WeakEntity<Irc>, buffer: &BufferKey) -> Vec<MenuCommand> {
    [
        ("Open to the right", IconName::PanelRight, Where::Right),
        ("Open below", IconName::PanelBottom, Where::Below),
    ]
    .into_iter()
    .map(|(label, icon, place)| {
        let (owner, buffer) = (owner.clone(), buffer.clone());
        MenuCommand::new(MenuGroup::Open, label, icon, move |_, cx| {
            let buffer = buffer.clone();
            _ = owner.update(cx, |board, cx| dock::request(board, buffer, place, cx));
        })
    })
    .collect()
}
