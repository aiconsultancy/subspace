//! Opening, focusing, splitting and closing conversation panels, the member
//! list following the focused channel, and showing or hiding the workspace's
//! side docks.

use gpui_kit::base::Placement;
use gpui_kit::component::dock::{DockPlacement, InsertTarget, PanelId, panel_handle};
use gpui_kit::{AppContext as _, Context, Entity, Window};
use subspace_client_irc::BufferKey;

use crate::Irc;
use crate::conversation::{Conversation, ConversationEvent};
use crate::issues;
use crate::markup::Ref;
use crate::pull_requests;
use crate::view::{self, Hosted};

/// Where a buffer opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Where {
    /// A tab in the group of the focused conversation, or its own tab when
    /// it is open already.
    Tab,
    /// A new group beside the focused conversation.
    Right,
    /// A new group under the focused conversation.
    Below,
}

impl Where {
    fn placement(self) -> Option<Placement> {
        match self {
            Self::Tab => None,
            Self::Right => Some(Placement::Right),
            Self::Below => Some(Placement::Bottom),
        }
    }
}

fn panel_id(conversation: &Entity<Conversation>) -> PanelId {
    PanelId::from(conversation.entity_id())
}

/// The conversation of `buffer`, created and subscribed to the first time.
pub(super) fn conversation(
    board: &mut Irc,
    buffer: &BufferKey,
    window: &mut Window,
    cx: &mut Context<Irc>,
) -> Option<Entity<Conversation>> {
    open_conversation(board, buffer).or_else(|| new_conversation(board, buffer, window, cx))
}

fn open_conversation(board: &Irc, buffer: &BufferKey) -> Option<Entity<Conversation>> {
    board
        .dock
        .open
        .get(buffer)
        .map(|hosted| hosted.entity.clone())
}

fn new_conversation(
    board: &mut Irc,
    buffer: &BufferKey,
    window: &mut Window,
    cx: &mut Context<Irc>,
) -> Option<Entity<Conversation>> {
    let hub = board.hub.clone()?;
    let entity = cx.new(|cx| Conversation::new(buffer.clone(), hub, window, cx));
    let subscription = cx.subscribe_in(&entity, window, conversation_event);
    let hosted = Hosted {
        entity: entity.clone(),
        _subscription: subscription,
    };
    board.dock.open.insert(buffer.clone(), hosted);
    let available = *board.available_height.read(cx);
    entity.update(cx, |conversation, cx| {
        conversation.set_available_height(available, cx);
    });
    Some(entity)
}

/// Shows `buffer` in the centre (`Where`), focuses its composer and asks
/// for recent history when it has little to show.
pub(crate) fn open(
    board: &mut Irc,
    buffer: &BufferKey,
    place: Where,
    window: &mut Window,
    cx: &mut Context<Irc>,
) {
    let anchor = board
        .dock
        .focused
        .as_ref()
        .and_then(|focused| board.dock.open.get(focused))
        .map(|hosted| panel_id(&hosted.entity));
    let Some(entity) = conversation(board, buffer, window, cx) else {
        return;
    };
    let id = panel_id(&entity);
    let area = board.dock.area.clone();
    area.update(cx, |area, cx| {
        let held = area.panel(id).is_some();
        if !held {
            area.add_panel_view(
                panel_handle(entity.clone()),
                DockPlacement::Center,
                None,
                window,
                cx,
            );
        }
        let node = anchor
            .filter(|anchor| *anchor != id)
            .and_then(|anchor| area.layout(DockPlacement::Center)?.find_panel_node(anchor));
        match (place.placement(), node) {
            (Some(placement), Some(node)) => {
                let target = InsertTarget::Split {
                    node,
                    placement,
                    size: None,
                };
                area.move_panel(id, target, window, cx);
            }
            (None, Some(node)) if !held => {
                let target = InsertTarget::Tabs {
                    node,
                    ix: None,
                    activate: true,
                };
                area.move_panel(id, target, window, cx);
            }
            _ => area.select_panel(id, window, cx),
        }
    });
    hide_placeholder(board, window, cx);
    entity.update(cx, |conversation, cx| {
        conversation.focus_composer(window, cx)
    });
    super::members::focus(board, buffer, cx);
    view::load_recent(board, buffer, cx);
}

/// Closes the panel of `buffer`, when it has one.
pub(crate) fn close(
    board: &mut Irc,
    buffer: &BufferKey,
    window: &mut Window,
    cx: &mut Context<Irc>,
) {
    let Some(entity) = board
        .dock
        .open
        .get(buffer)
        .map(|hosted| hosted.entity.clone())
    else {
        return;
    };
    board
        .dock
        .area
        .update(cx, |area, cx| area.remove_panel(entity, window, cx));
}

/// Shows or hides the navigator's buffer list.
pub(crate) fn toggle_buffers(board: &mut Irc, cx: &mut Context<Irc>) {
    board.dock.buffers.open = !board.dock.buffers.open;
    super::persist::docks_changed(board, cx);
}

/// Shows or hides the inspector's member list.
pub(crate) fn toggle_members(board: &mut Irc, cx: &mut Context<Irc>) {
    board.dock.members.open = !board.dock.members.open;
    super::persist::docks_changed(board, cx);
}

/// The centre's placeholder goes once a conversation is open.
fn hide_placeholder(board: &mut Irc, window: &mut Window, cx: &mut Context<Irc>) {
    if !board.dock.placeholder_shown {
        return;
    }
    board.dock.placeholder_shown = false;
    let placeholder = board.dock.placeholder.clone();
    board
        .dock
        .area
        .update(cx, |area, cx| area.remove_panel(placeholder, window, cx));
}

/// The placeholder fills the centre when the last conversation closes.
pub(super) fn show_placeholder(board: &mut Irc, window: &mut Window, cx: &mut Context<Irc>) {
    if board.dock.placeholder_shown || !board.dock.open.is_empty() {
        return;
    }
    board.dock.placeholder_shown = true;
    let placeholder = panel_handle(board.dock.placeholder.clone());
    board.dock.area.update(cx, |area, cx| {
        area.add_panel_view(placeholder, DockPlacement::Center, None, window, cx);
    });
}

fn conversation_event(
    board: &mut Irc,
    conversation: &Entity<Conversation>,
    event: &ConversationEvent,
    window: &mut Window,
    cx: &mut Context<Irc>,
) {
    let buffer = conversation.read(cx).buffer.clone();
    match event {
        ConversationEvent::Open(Ref::Nick(nick)) => board.open_query(&buffer.network, nick, cx),
        ConversationEvent::Open(Ref::Channel(name)) => {
            view::open_channel(board, &buffer.network, name, cx);
        }
        ConversationEvent::Open(Ref::Issue(identifier)) => issues::open(identifier, &buffer, cx),
        ConversationEvent::Open(Ref::PullRequest(found)) => {
            pull_requests::open(found, &buffer, cx);
        }
        ConversationEvent::Focused => super::members::focus(board, &buffer, cx),
        ConversationEvent::FindScopeChanged => {
            if board.active && board.dock.focused.as_ref() == Some(&buffer) {
                board.find_revision = board.find_revision.wrapping_add(1);
                cx.notify();
            }
        }
        ConversationEvent::Closed => closed(board, conversation, window, cx),
        ConversationEvent::OpenThread(root) => {
            crate::thread::open(board, &buffer, root.clone(), window, cx);
        }
    }
}

/// Forgets a closed panel's conversation, unless its buffer was opened
/// again since, as a restored layout does.
fn closed(
    board: &mut Irc,
    conversation: &Entity<Conversation>,
    window: &mut Window,
    cx: &mut Context<Irc>,
) {
    let buffer = conversation.read(cx).buffer.clone();
    let current = board
        .dock
        .open
        .get(&buffer)
        .is_some_and(|hosted| hosted.entity == *conversation);
    if !current {
        return;
    }
    board.dock.open.remove(&buffer);
    if board.dock.focused.as_ref() == Some(&buffer) {
        board.dock.focused = None;
        board.find_revision = board.find_revision.wrapping_add(1);
        cx.notify();
        view::publish_visible(board, cx);
    }
    show_placeholder(board, window, cx);
}

/// A buffer opened from a reference, a notification or `/query`: shown in
/// the focused conversation's group.
pub(crate) fn request(board: &Irc, buffer: BufferKey, place: Where, cx: &mut Context<Irc>) {
    super::with_window(board, cx, move |board, window, cx| {
        open(board, &buffer, place, window, cx);
    });
}
