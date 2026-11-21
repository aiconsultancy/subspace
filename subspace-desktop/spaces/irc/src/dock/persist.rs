//! The layout saved after every change and restored once the hub is bound:
//! the centre's splits and tab groups read as buffers, and whether the
//! workspace's navigator and inspector show.

use std::collections::HashMap;

use gpui_kit::component::dock::{
    DockArea, DockLayout, DockPlacement, PaneNode, PaneRef, PanelId, panel_handle,
};
use gpui_kit::{App, Axis as GpuiAxis, Context, Window, px};
use subspace_client_irc::{BufferKey, NetworkId};
use subspace_framework::dashboard::prefs;

use super::layout::{self, Axis, KEY, Layout, Pane};
use super::open::{self, Where};
use crate::{Irc, view};

/// Saves the visible dock layout.
pub(super) fn layout_changed(board: &mut Irc, _: &mut Window, cx: &mut Context<Irc>) {
    // Tabs and splits change which conversations are on screen.
    view::publish_visible(board, cx);
    if !board.dock.restored {
        return;
    }
    save(board, cx);
    cx.notify();
}

/// Keeps the choice after a switch showed or hid the navigator or the
/// inspector, and redraws the workspace.
pub(super) fn docks_changed(board: &mut Irc, cx: &mut Context<Irc>) {
    if board.dock.restored {
        save(board, cx);
    }
    cx.notify();
}

fn save(board: &Irc, cx: &mut App) {
    let mut layout = current(board, cx);
    let previous = prefs::get(cx, KEY);
    if keep_local_centre(board, cx) {
        layout.center = layout::decode(previous.as_deref()).and_then(|stored| stored.center);
    }
    let text = layout::encode(&layout);
    if previous.as_deref() != Some(text.as_str()) {
        prefs::set(cx, KEY, text);
    }
}

fn keep_local_centre(board: &Irc, cx: &App) -> bool {
    board.dock.retiring
        || board
            .hub
            .as_ref()
            .is_some_and(|hub| hub.read(cx).initiatives.selected_for_runtime())
}

/// The centre the area shows, in buffer terms.
fn read(area: &DockArea, ids: &HashMap<PanelId, BufferKey>) -> Option<Pane> {
    area.layout(DockPlacement::Center)
        .and_then(|tree| pane(tree.root(), ids))
}

/// One container in buffer terms; `None` when it holds no conversation.
fn pane(node: &PaneNode, ids: &HashMap<PanelId, BufferKey>) -> Option<Pane> {
    match node.kind() {
        PaneRef::Tabs { panels, active_ix } => {
            let buffers: Vec<BufferKey> = panels
                .iter()
                .filter_map(|id| ids.get(id).cloned())
                .collect();
            let active = panels
                .get(active_ix)
                .and_then(|id| ids.get(id))
                .and_then(|active| buffers.iter().position(|buffer| buffer == active))
                .unwrap_or_default();
            (!buffers.is_empty()).then_some(Pane::Tabs { buffers, active })
        }
        PaneRef::Split {
            axis,
            children,
            sizes,
        } => {
            let children: Vec<(Pane, Option<f32>)> = children
                .iter()
                .zip(sizes)
                .filter_map(|(child, size)| {
                    pane(child, ids).map(|pane| (pane, size.map(f32::from)))
                })
                .collect();
            let axis = match axis {
                GpuiAxis::Horizontal => Axis::Horizontal,
                GpuiAxis::Vertical => Axis::Vertical,
            };
            match children.len() {
                0 => None,
                1 => children.into_iter().next().map(|(pane, _)| pane),
                _ => Some(Pane::Split { axis, children }),
            }
        }
    }
}

/// Applies the stored layout: which side docks show, then the centre's
/// conversations on networks that are still configured and not removed from
/// the list. Without one, the first network's server buffer opens.
pub(super) fn restore(board: &mut Irc, window: &mut Window, cx: &mut Context<Irc>) {
    let networks: Vec<NetworkId> = board
        .networks
        .iter()
        .map(|network| network.id.clone())
        .collect();
    let removed = board
        .hub
        .as_ref()
        .map(|hub| hub.read(cx).state.removed.clone())
        .unwrap_or_default();
    let mut layout = layout::decode(prefs::get(cx, KEY).as_deref()).unwrap_or_default();
    layout.prune(&|buffer| {
        networks.contains(&buffer.network)
            && removed.get(buffer).is_none()
            && board
                .hub
                .as_ref()
                .is_none_or(|hub| !hub.read(cx).state.joins.retired(buffer))
    });
    board.dock.buffers = layout.buffers;
    board.dock.members = layout.members;
    cx.notify();
    let center = layout
        .center
        .as_ref()
        .and_then(|center| build(board, center, window, cx));
    match center {
        Some(center) => {
            board
                .dock
                .area
                .update(cx, |area, cx| area.set_center(center, window, cx));
            if let Some(buffer) = layout.first_active() {
                open::open(board, &buffer, Where::Tab, window, cx);
            }
        }
        None => match networks.first() {
            Some(network) => {
                let server = crate::state::server_key(network);
                open::open(board, &server, Where::Tab, window, cx);
            }
            None => open::show_placeholder(board, window, cx),
        },
    }
}

/// The dock layout of `pane`, creating its conversations.
fn build(
    board: &mut Irc,
    pane: &Pane,
    window: &mut Window,
    cx: &mut Context<Irc>,
) -> Option<DockLayout> {
    match pane {
        Pane::Tabs { buffers, active } => {
            let mut tabs = DockLayout::tabs();
            for buffer in buffers {
                let entity = open::conversation(board, buffer, window, cx)?;
                tabs = tabs.panel_view(panel_handle(entity), cx);
            }
            Some(tabs.active_index(*active))
        }
        Pane::Split { axis, children } => {
            let split = match axis {
                Axis::Horizontal => DockLayout::h_split(),
                Axis::Vertical => DockLayout::v_split(),
            };
            Some(children.iter().fold(split, |split, (child, size)| {
                match build(board, child, window, cx) {
                    Some(child) => split.child(child, size.map(px)),
                    None => split,
                }
            }))
        }
    }
}

/// The layout the dock shows now, for tests.
#[cfg(test)]
pub(super) fn layout_of(board: &Irc, cx: &gpui_kit::App) -> Layout {
    current(board, cx)
}

/// The layout the dock shows now, in buffer terms.
pub(super) fn current(board: &Irc, cx: &gpui_kit::App) -> Layout {
    let ids: HashMap<PanelId, BufferKey> = board
        .dock
        .open
        .iter()
        .map(|(buffer, hosted)| (PanelId::from(hosted.entity.entity_id()), buffer.clone()))
        .collect();
    Layout {
        center: read(board.dock.area.read(cx), &ids),
        buffers: board.dock.buffers,
        members: board.dock.members,
    }
}
