//! The searchable Initiative picker uses the same treatment as Issues. Its
//! button leads the pane's toolbar; its hint goes to the status bar.

use crate::Irc;
use gpui_kit::{AnyElement, App, Context, Entity, Window};
use subspace_api_client::initiatives::Catalog;
use subspace_framework::dashboard::PaletteCommand;
use subspace_framework::dashboard::anchored::Anchor;
use subspace_framework::dashboard::menu::{Menu, MenuAnchors, MenuEvent};

const ANCHOR: &str = "irc-initiative";
const LOCAL_LABEL: &str = "Local channels";

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

    pub(crate) fn dismiss(&mut self, window: &mut Window, cx: &mut App) {
        self.anchors.dismiss(window, cx);
        self.menu = None;
    }
}

/// What the status bar adds about initiatives: the connection the selected
/// one still needs, or that the memberships are being read or unavailable.
pub(crate) fn hint(board: &Irc, cx: &App) -> Option<String> {
    if !subspace_api_client::capabilities().initiative_channels {
        return None;
    }
    let hub = board.hub.as_ref()?.read(cx);
    let catalog = hub.initiatives.catalog.read(cx);
    if catalog.has_memberships()
        && let Some(missing) = hub.missing_initiative_network()
    {
        return Some(missing);
    }
    catalog.hint().map(str::to_owned)
}

/// The picker shows while any initiative (or the default server) has a
/// connection, including while the selected one's server has none, so
/// another initiative can be chosen.
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
pub(crate) fn button(board: &Irc, cx: &mut Context<Irc>) -> Option<AnyElement> {
    let button = catalog(board, cx)?
        .button("irc-initiative-button", LOCAL_LABEL)
        .on_click(cx.listener(|this, _, window, cx| open(this, window, cx)));
    Some(board.initiative_menu.anchors.trigger(ANCHOR, button, cx))
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
    let menu = catalog.menu(LOCAL_LABEL);
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

fn is_stale(board: &Irc, cx: &App) -> bool {
    board.initiative_menu.host().is_some()
        && catalog(board, cx)
            .is_none_or(|catalog| catalog.revision() != board.initiative_menu.revision)
}
