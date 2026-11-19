//! The collapsed application toolbar: one menu that lists every global control.

use std::rc::Rc;

use gpui_kit::IntoElement;
use gpui_kit::component::{Sizable as _, button::ButtonVariants as _};
use subspace_dashboard::icon_button::IconButton;
use subspace_dashboard::measure;
use subspace_dashboard::overflow_toolbar::{OverflowUnit, PopupEntry};
use subspace_lucide::IconName;

use super::globals::GlobalItem;

const MENU_ID: &str = "toolbar-menu";
const MENU_LABEL: &str = "Toolbar menu";

/// The overflow chevron, if the window is too narrow even for the menu,
/// lists the same rows.
pub(super) fn menu_unit(items: Vec<GlobalItem>) -> OverflowUnit {
    let entries = items
        .iter()
        .cloned()
        .map(|item| PopupEntry::new(move || item.menu_item()))
        .collect();
    let items = Rc::new(items);
    OverflowUnit::inline(
        MENU_ID,
        measure::UNMEASURED,
        move |_, _| menu_button(items.clone()),
        entries,
    )
}

fn menu_button(items: Rc<Vec<GlobalItem>>) -> impl IntoElement {
    subspace_dashboard::anchored::dropdown_menu(
        IconButton::new(MENU_ID)
            .ghost()
            .small()
            .icon(IconName::Hamburger)
            .tooltip(MENU_LABEL)
            .accessibility_label(MENU_LABEL),
        move |menu, _, _| {
            items
                .iter()
                .fold(menu, |menu, item| menu.item(item.menu_item()))
        },
    )
}
