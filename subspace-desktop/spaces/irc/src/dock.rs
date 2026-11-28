//! The conversations as a GPUI Kit dock area in the workspace's pane: each
//! opened channel, private conversation or server buffer is a panel (tabbed,
//! split side by side or above and below, moved and closed by the user). The
//! buffer list shows in the workspace's navigator and the focused channel's
//! member list in its inspector; whether each shows is kept here. The layout
//! is saved in buffer terms and restored on launch.

mod layout;
mod members;
mod open;
mod panels;
mod persist;

use std::collections::HashMap;

use gpui_kit::component::dock::{DockArea, DockEvent};
use gpui_kit::{AnyWindowHandle, App, AppContext as _, Context, Entity, Subscription, Window};
use subspace_client_irc::BufferKey;

use crate::Irc;
use crate::conversation::Conversation;
use crate::members::MemberList;
use crate::view::Hosted;

use layout::Side;
pub(crate) use members::focused_members;
pub(crate) use open::{Where, close, open, request, toggle_buffers, toggle_members};
use panels::EmptyPanel;

/// The dock and what it shows, owned by the board.
pub(crate) struct IrcDock {
    window: AnyWindowHandle,
    pub area: Entity<DockArea>,
    /// Whether the navigator shows the buffer list.
    buffers: Side,
    /// Whether the inspector shows the member list.
    members: Side,
    placeholder: Entity<EmptyPanel>,
    /// The placeholder fills the centre while no conversation is open.
    placeholder_shown: bool,
    /// The conversation of each open panel.
    pub open: HashMap<BufferKey, Hosted<Conversation>>,
    /// Each channel's member list, kept while the board lives.
    member_lists: HashMap<BufferKey, Hosted<MemberList>>,
    /// The conversation the user works in: the last one shown or focused.
    pub focused: Option<BufferKey>,
    /// The stored layout was applied for the current hub.
    restored: bool,
    /// Automatic closes must not replace the user's saved Local layout.
    retirement: u64,
    retiring: bool,
    _subscriptions: Vec<Subscription>,
}

impl IrcDock {
    /// The dock for `board`; the centre is filled, and the side docks'
    /// visibility read, once the hub is bound ([`restore`]).
    pub fn new(window: &mut Window, cx: &mut Context<Irc>) -> Self {
        let area = subspace_framework::dashboard::tabs::dock::area("irc-dock", Some(1), window, cx);
        let placeholder = cx.new(EmptyPanel::new);
        let subscriptions = vec![
            cx.subscribe_in(&area, window, |board, _, event, window, cx| {
                if matches!(event, DockEvent::LayoutChanged) {
                    persist::layout_changed(board, window, cx);
                }
            }),
        ];
        Self {
            window: window.window_handle(),
            area,
            buffers: Side::default(),
            members: Side::default(),
            placeholder,
            placeholder_shown: false,
            open: HashMap::new(),
            member_lists: HashMap::new(),
            focused: None,
            restored: false,
            retirement: 0,
            retiring: false,
            _subscriptions: subscriptions,
        }
    }

    /// Whether the navigator shows the buffer list.
    pub fn buffers_open(&self) -> bool {
        self.buffers.open
    }

    /// Whether the inspector shows the member list.
    pub fn members_open(&self) -> bool {
        self.members.open
    }

    /// Forgets the conversations of a hub that was replaced, so the stored
    /// layout is applied again for the new one.
    pub fn reset(&mut self) {
        self.open.clear();
        self.member_lists.clear();
        self.focused = None;
        self.restored = false;
    }
}

/// Runs `change` on the board with its window, after the current update:
/// list and conversation events arrive without a window, and the dock edits
/// need one.
pub(crate) fn with_window(
    board: &Irc,
    cx: &mut Context<Irc>,
    change: impl FnOnce(&mut Irc, &mut Window, &mut Context<Irc>) + 'static,
) {
    let (window, owner) = (board.dock.window, board.me.clone());
    let app: &mut App = cx;
    app.defer(move |cx| {
        _ = window.update(cx, |_, window, cx| {
            _ = owner.update(cx, |board, cx| change(board, window, cx));
        });
    });
}

/// Close autojoin panels that no longer belong to the active initiative.
pub(crate) fn close_retired(board: &Irc, retired: Vec<BufferKey>, cx: &mut Context<Irc>) {
    with_window(board, cx, move |board, window, cx| {
        board.dock.retiring = true;
        board.dock.retirement = board.dock.retirement.wrapping_add(1);
        let retirement = board.dock.retirement;
        for buffer in retired {
            if board
                .hub
                .as_ref()
                .is_some_and(|hub| hub.read(cx).state.joins.retired(&buffer))
            {
                close(board, &buffer, window, cx);
            }
        }
        // The close event can add the empty placeholder and enqueue another
        // layout save. Keep the guard through the entire current effect queue.
        cx.spawn(async move |owner, cx| {
            _ = owner.update(cx, |board, _| {
                if board.dock.retirement == retirement {
                    board.dock.retiring = false;
                }
            });
        })
        .detach();
    });
}

/// Every conversation on screen: the displayed tab of each group.
pub(crate) fn shown(board: &Irc, cx: &App) -> Vec<BufferKey> {
    persist::current(board, cx).shown()
}

/// Applies the stored layout once the hub is bound, or opens the first
/// network's server buffer when there is none.
pub(crate) fn restore(board: &mut Irc, cx: &mut Context<Irc>) {
    if board.dock.restored || board.hub.is_none() {
        return;
    }
    board.dock.restored = true;
    with_window(board, cx, persist::restore);
}

#[cfg(test)]
mod tests;
