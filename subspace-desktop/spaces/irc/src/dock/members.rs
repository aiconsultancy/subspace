//! The member list dock following the conversation the user works in.

use gpui_kit::{AppContext as _, Context, Entity};
use subspace_client_irc::{BufferKey, Target};

use super::open::toggle_members;
use crate::Irc;
use crate::members::{MemberList, MemberListEvent};
use crate::view::{self, Hosted};

/// `buffer` is the conversation the user works in: the member list follows
/// it, the list selects it, and the hub reads it as it arrives. Repeating it
/// changes nothing.
pub(super) fn focus(board: &mut Irc, buffer: &BufferKey, cx: &mut Context<Irc>) {
    let changed = board.dock.focused.as_ref() != Some(buffer);
    board.dock.focused = Some(buffer.clone());
    for (key, hosted) in &board.dock.open {
        let active = key == buffer;
        hosted.entity.update(cx, |view, cx| {
            if view.find_active != active {
                view.find_active = active;
                cx.notify();
            }
        });
    }
    if changed {
        board.find_revision = board.find_revision.wrapping_add(1);
        cx.notify();
    }
    let list = member_list(board, buffer, cx);
    board
        .dock
        .members
        .update(cx, |panel, cx| panel.set_list(list, cx));
    view::reveal_row(board, buffer, cx);
    view::publish_visible(board, cx);
}

/// The member list of channel `buffer`, created the first time.
fn member_list(
    board: &mut Irc,
    buffer: &BufferKey,
    cx: &mut Context<Irc>,
) -> Option<Entity<MemberList>> {
    channel(buffer)?;
    open_member_list(board, buffer).or_else(|| new_member_list(board, buffer, cx))
}

fn channel(buffer: &BufferKey) -> Option<()> {
    matches!(buffer.target, Target::Channel(_)).then_some(())
}

fn open_member_list(board: &Irc, buffer: &BufferKey) -> Option<Entity<MemberList>> {
    board
        .dock
        .member_lists
        .get(buffer)
        .map(|hosted| hosted.entity.clone())
}

fn new_member_list(
    board: &mut Irc,
    buffer: &BufferKey,
    cx: &mut Context<Irc>,
) -> Option<Entity<MemberList>> {
    let hub = board.hub.clone()?;
    let entity = cx.new(|cx| MemberList::new(buffer.clone(), hub, cx));
    let subscription = cx.subscribe(&entity, member_event);
    let hosted = Hosted {
        entity: entity.clone(),
        _subscription: subscription,
    };
    board.dock.member_lists.insert(buffer.clone(), hosted);
    Some(entity)
}

fn member_event(
    board: &mut Irc,
    list: Entity<MemberList>,
    event: &MemberListEvent,
    cx: &mut Context<Irc>,
) {
    match event {
        MemberListEvent::OpenQuery(nick) => {
            let network = list.read(cx).buffer().network.clone();
            board.open_query(&network, nick, cx);
        }
        MemberListEvent::Close => super::with_window(board, cx, toggle_members),
    }
}
