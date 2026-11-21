//! The plus and its menu. The plus is the shared ghost `IconButton`, as the
//! collapsed toolbar menu's button is, and opens a native menu through
//! `anchored::dropdown_menu`. In the title bar it is the tab strip's last
//! overflow unit, so it sits right after the last tab; when the strip runs
//! out of room it moves into the chevron menu as its last entry,
//! **Add a Space…**, which opens the same menu in the chevron's place.

use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::menu::{PopupMenu, PopupMenuItem};
use gpui_kit::component::{Icon, Sizable as _, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    App, Context, InteractiveElement as _, IntoElement, ParentElement as _, Role, SharedString,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, WeakEntity, Window, div,
};
use subspace_dashboard::Tokens;
use subspace_dashboard::anchored::dropdown_menu;
use subspace_dashboard::icon_button::IconButton;
use subspace_dashboard::measure;
use subspace_dashboard::overflow_toolbar::{OverflowUnit, PopupEntry};
use subspace_lucide::IconName;

use super::super::SubspaceApp;
use super::super::cover::cover_while_open;
use super::{ADD_SPACE, ADD_SPACE_ENTRY, ALL_ON, MANAGE_SPACES, OffSpace, Row};

/// The plus's element id in the title bar.
pub(in crate::app) const TITLE_BAR_ID: &str = "add-space";
/// The plus's element id at the end of the navigation rail.
pub(in crate::app) const RAIL_ID: &str = "add-space-rail";

/// The plus as the tab strip's last overflow unit.
pub(in crate::app) fn tab_unit(app: WeakEntity<SubspaceApp>) -> OverflowUnit {
    let inline_app = app.clone();
    OverflowUnit::inline(
        TITLE_BAR_ID,
        measure::UNMEASURED,
        move |_, _| button(TITLE_BAR_ID, inline_app.clone()),
        vec![overflow_entry(app)],
    )
}

/// The plus, with element id `id`: it opens the add menu.
pub(in crate::app) fn button(id: &'static str, app: WeakEntity<SubspaceApp>) -> impl IntoElement {
    dropdown_menu(
        IconButton::new(id)
            .ghost()
            .small()
            .icon(IconName::Plus)
            .tooltip(ADD_SPACE)
            .accessibility_label(ADD_SPACE),
        move |menu, window, cx| build(menu, &app, window, cx),
    )
}

/// The chevron menu's last entry while the plus has no room: it replaces
/// the chevron menu with the add menu, on the same anchor.
fn overflow_entry(app: WeakEntity<SubspaceApp>) -> PopupEntry {
    PopupEntry::anchored(move |anchor| {
        let app = app.clone();
        PopupMenuItem::new(ADD_SPACE_ENTRY)
            .icon(IconName::Plus)
            .on_click(move |_, window, cx| {
                let app = app.clone();
                anchor.host().update(cx, |host, cx| {
                    host.show_menu_with(window, cx, move |menu, window, cx| {
                        build(menu, &app, window, cx)
                    });
                });
            })
    })
}

/// Builds the menu from the Spaces that are off when it opens. While open,
/// it covers the board area, as a Space tab's menu does.
fn build(
    menu: PopupMenu,
    app: &WeakEntity<SubspaceApp>,
    window: &mut Window,
    cx: &mut Context<PopupMenu>,
) -> PopupMenu {
    cover_while_open(app, window, cx);
    let rows = super::menu_rows(super::off_spaces(cx));
    fill(menu, rows, app)
}

fn fill(menu: PopupMenu, rows: Vec<Row>, app: &WeakEntity<SubspaceApp>) -> PopupMenu {
    rows.into_iter().fold(menu, |menu, row| match row {
        Row::Title => menu.label(ADD_SPACE),
        Row::Space(space) => menu.item(space_item(space, app.clone())),
        Row::AllOn => menu.label(ALL_ON),
        Row::Separator => menu.separator(),
        Row::Manage => menu.item(manage_item(app.clone())),
    })
}

/// One Space: its glyph, title and one-line description. Choosing it turns
/// the Space on and switches to it.
fn space_item(space: OffSpace, app: WeakEntity<SubspaceApp>) -> PopupMenuItem {
    let (id, icon) = (space.id, space.icon);
    PopupMenuItem::element(move |_, cx| space_row(&space, cx))
        .when_some(icon, |item, path| item.icon(Icon::default().path(path)))
        .on_click(move |_, window, cx| {
            _ = app.update(cx, |app, cx| app.open_space(id, window, cx));
        })
}

/// The item's text. A custom menu item takes no label of its own, so the
/// row is an accessible label holding the Space's title, from which the
/// item takes its name. The description is not part of the name.
fn space_row(space: &OffSpace, cx: &App) -> impl IntoElement + use<> {
    let tokens = Tokens::current(cx);
    let description = space.description.clone();
    v_flex()
        .id(SharedString::from(format!("add-space-item-{}", space.id)))
        .test_support()
        .role(Role::Label)
        .aria_value(space.title.clone())
        .min_w_0()
        .child(space.title.clone())
        .when(!description.is_empty(), |row| {
            row.child(
                div()
                    .text_color(tokens.muted)
                    .whitespace_nowrap()
                    .child(description),
            )
        })
}

fn manage_item(app: WeakEntity<SubspaceApp>) -> PopupMenuItem {
    PopupMenuItem::new(MANAGE_SPACES)
        .icon(IconName::Settings)
        .on_click(move |_, window, cx| {
            _ = app.update(cx, |app, cx| app.manage_spaces(window, cx));
        })
}
