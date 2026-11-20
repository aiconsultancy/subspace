//! The searchable Initiative picker uses the same treatment as Issues.

use crate::Irc;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, Context, Entity, InteractiveElement as _, IntoElement as _,
    ParentElement as _, Styled as _, TestSupportExt as _, Window, div, px,
};
use subspace_api_client::initiatives::Catalog;
use subspace_framework::dashboard::anchored::Anchor;
use subspace_framework::dashboard::icon_button::Metrics;
use subspace_framework::dashboard::menu::{Menu, MenuAnchors, MenuEvent};
use subspace_framework::dashboard::{Dashboard as _, PaletteCommand, list};

pub(crate) const SPACE_INSET: gpui_kit::Pixels = px(24.);
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

pub(crate) fn heading(board: &mut Irc, cx: &mut Context<Irc>) -> AnyElement {
    let toolbar = crate::toolbar::render(board, cx);
    v_flex()
        .flex_shrink_0()
        .child(list::header(
            Irc::ID,
            Irc::TITLE,
            scope(board, cx),
            SPACE_INSET,
            cx,
        ))
        .children(toolbar.map(|toolbar| {
            h_flex()
                .id("irc-controls")
                .test_support()
                .w_full()
                .px(SPACE_INSET)
                .pb_2()
                .gap_2()
                .children(button(board, cx))
                .child(
                    div()
                        .ml_auto()
                        .flex_shrink_1()
                        .min_w(Metrics::current(cx).overflow_width())
                        .child(toolbar),
                )
        }))
        .into_any_element()
}

fn scope(board: &Irc, cx: &App) -> String {
    let status = board
        .status(cx)
        .map_or_else(|| "IRC".into(), |status| format!("IRC · {status}"));
    initiative_hint(board, cx).map_or_else(|| status.clone(), |hint| format!("{status} · {hint}"))
}

fn initiative_hint(board: &Irc, cx: &App) -> Option<String> {
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

fn button(board: &Irc, cx: &mut Context<Irc>) -> Option<AnyElement> {
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
