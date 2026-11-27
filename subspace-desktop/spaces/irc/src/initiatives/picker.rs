//! The searchable Initiative picker uses the same treatment as Issues. Its
//! button leads the pane's toolbar; its hint goes to the status bar.

use crate::Irc;
use gpui_kit::component::menu::PopupMenuItem;
use gpui_kit::{AnyElement, App, Context, Entity, IntoElement as _, Window, div};
use subspace_api_client::initiatives::Catalog;
use subspace_framework::dashboard::PaletteCommand;
use subspace_framework::dashboard::anchored::Anchor;
use subspace_framework::dashboard::menu::{Menu, MenuAnchors, MenuEvent};
use subspace_framework::dashboard::overflow_toolbar::{OverflowToolbar, OverflowUnit, PopupEntry};
use subspace_framework::dashboard::panel::IconName;

const ANCHOR: &str = "irc-initiative";
/// Units are measured when they first draw.
const UNMEASURED_WIDTH: f32 = 0.;

#[derive(Default)]
pub(crate) struct Picker {
    anchors: MenuAnchors,
    menu: Option<Entity<Menu>>,
    revision: u64,
}

impl Picker {
    pub(crate) fn host(&self) -> Option<Entity<Anchor>> {
        self.anchors.view()
    }

    /// Whether the chooser is open, from its own button or from the overflow
    /// menu's chevron.
    pub(crate) fn is_open(&self) -> bool {
        self.menu.is_some()
    }

    pub(crate) fn dismiss(&mut self, window: &mut Window, cx: &mut App) {
        self.anchors.dismiss(window, cx);
        self.menu = None;
    }
}

/// What the status bar adds about initiatives: that the API cannot be
/// reached, in the words every Space uses for it, the connection the
/// selected initiative still needs, that the memberships are being read or
/// unavailable, or that the person belongs to no initiative yet.
pub(crate) fn hint(board: &Irc, cx: &App) -> Option<String> {
    if !subspace_api_client::capabilities().initiative_channels {
        return None;
    }
    let hub = board.hub.as_ref()?.read(cx);
    if hub.api_unreachable(cx) {
        return Some(subspace_api_client::unreachable_message());
    }
    let catalog = hub.initiatives.catalog.read(cx);
    if catalog.has_memberships()
        && let Some(missing) = hub.missing_initiative_network()
    {
        return Some(missing);
    }
    catalog.hint()
}

/// The picker shows while any initiative (or the default server) has a
/// connection, including while the selected one's server has none, so
/// another initiative can be chosen. It offers initiatives only: a member
/// always has one selected, so Local channels is not a choice.
fn catalog<'a>(board: &'a Irc, cx: &'a App) -> Option<&'a Catalog> {
    // Where the organisation does not serve initiative channels and fleets,
    // Channels uses its configured connections and offers no initiative
    // choice.
    if !subspace_api_client::capabilities().initiative_channels {
        return None;
    }
    let hub = board.hub.as_ref()?.read(cx);
    let catalog = hub.initiatives.catalog.read(cx);
    (catalog.has_memberships() && hub.serves_any_initiative(catalog.memberships()))
        .then_some(catalog)
}

/// The picker's button, while there is an initiative to choose.
fn button(board: &Irc, cx: &mut Context<Irc>) -> Option<AnyElement> {
    let button = catalog(board, cx)?
        .button("irc-initiative-button")
        .on_click(cx.listener(|this, _, window, cx| open(this, window, cx)));
    Some(board.initiative_menu.anchors.trigger(ANCHOR, button, cx))
}

/// The picker as a unit of the toolbar's leading overflow. Its key is the
/// picker's anchor, so a hidden picker opens from the overflow chevron once
/// [`set_toolbar`] names the toolbar.
pub(crate) fn unit(board: &Irc, cx: &App) -> Option<OverflowUnit> {
    let name = catalog(board, cx)?
        .selected()
        .map_or_else(|| "Initiative".to_owned(), |item| item.name.clone());
    let owner = board.me.clone();
    let opener = owner.clone();
    let entry = PopupEntry::new(move || {
        let opener = opener.clone();
        PopupMenuItem::new(format!("Initiative: {name}…"))
            .icon(IconName::Users)
            .on_click(move |_, window, cx| {
                _ = opener.update(cx, |board, cx| open(board, window, cx));
            })
    });
    Some(OverflowUnit::inline(
        ANCHOR,
        UNMEASURED_WIDTH,
        move |_, cx| {
            owner
                .update(cx, |board, cx| button(board, cx))
                .ok()
                .flatten()
                .unwrap_or_else(|| div().into_any_element())
        },
        vec![entry],
    ))
}

/// The toolbar the picker's unit sits in.
pub(crate) fn set_toolbar(board: &Irc, toolbar: &Entity<OverflowToolbar>) {
    board.initiative_menu.anchors.set_toolbar(toolbar);
}

pub(crate) fn command(board: &Irc, cx: &App) -> Option<PaletteCommand> {
    catalog(board, cx)?;
    let board = board.me.clone();
    Some(
        PaletteCommand::callback("Switch initiative…", move |window, cx| {
            _ = board.update(cx, |board, cx| open(board, window, cx));
        })
        .keywords("channels irc team"),
    )
}

fn open(board: &mut Irc, window: &mut Window, cx: &mut Context<Irc>) {
    let Some(catalog) = catalog(board, cx) else {
        return;
    };
    let revision = catalog.revision();
    let menu = catalog.menu();
    let anchor = board.initiative_menu.anchors.anchor(ANCHOR, cx);
    let Some(menu) = board.initiative_menu.anchors.open(anchor, menu, window, cx) else {
        return;
    };
    cx.subscribe(&menu, move |board, _, event: &MenuEvent, cx| {
        if matches!(event, MenuEvent::Query(_)) {
            return;
        }
        board.initiative_menu.menu = None;
        if let MenuEvent::Pick(value) = event
            && let Some(hub) = &board.hub
        {
            let catalog = hub.read(cx).initiatives.catalog.clone();
            catalog.update(cx, |catalog, cx| {
                if catalog.revision() == revision {
                    catalog.pick(value, cx);
                }
            });
        }
        cx.notify();
    })
    .detach();
    board.initiative_menu.menu = Some(menu);
    board.initiative_menu.revision = revision;
    cx.notify();
}

pub(crate) fn dismiss_stale_picker(board: &Irc, cx: &mut Context<Irc>) {
    if is_stale(board, cx) {
        crate::dock::with_window(board, cx, |board, window, cx| {
            if is_stale(board, cx) {
                board.initiative_menu.dismiss(window, cx);
                cx.notify();
            }
        });
    }
}

/// Whether the chooser is open, wherever it is anchored, on a catalogue that
/// has changed since.
fn is_stale(board: &Irc, cx: &App) -> bool {
    board.initiative_menu.is_open()
        && catalog(board, cx)
            .is_none_or(|catalog| catalog.revision() != board.initiative_menu.revision)
}
