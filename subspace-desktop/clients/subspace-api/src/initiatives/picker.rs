//! Shared presentation for the desktop's initiative menus. They offer the
//! person's initiatives only: someone who belongs to one always has one
//! selected, so there is no choice of none.

use gpui_kit::component::menu::PopupMenuItem;
use gpui_kit::{AnyElement, App, Entity, IntoElement as _, Pixels, Styled as _, Window, div, px};
use subspace_framework::dashboard::action::menu_button;
use subspace_framework::dashboard::icon_button::IconButton;
use subspace_framework::dashboard::menu::{Menu, MenuAnchors, MenuContents, MenuOption};
use subspace_framework::dashboard::overflow_toolbar::{OverflowUnit, PopupEntry};
use subspace_framework::lucide::IconName;

use super::{CHECKING, Catalog, NO_MEMBERSHIPS, UNAVAILABLE};

const PICKER_MAX_WIDTH: Pixels = px(280.);
const TITLE: &str = "Initiative";

impl Catalog {
    /// Open the searchable picker and capture the catalogue revision it represents.
    pub fn open_picker(
        catalog: &Entity<Self>,
        anchors: &MenuAnchors,
        key: &'static str,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<(Entity<Menu>, u64)> {
        let catalog = catalog.read(cx);
        let revision = catalog.revision();
        let contents = catalog.menu();
        let anchor = anchors.anchor(key, cx);
        anchors
            .open(anchor, contents, window, cx)
            .map(|menu| (menu, revision))
    }

    /// What the picker's Space says about the memberships: that the API
    /// could not be reached, in the words every Space uses for it, that
    /// they could not be read, that the person belongs to no initiative
    /// yet, or that they are being read.
    // qual:api
    pub fn hint(&self) -> Option<String> {
        if self.outcome.unreachable {
            return Some(crate::unreachable_message());
        }
        let hint = match () {
            () if self.outcome.failed => UNAVAILABLE,
            () if self.no_memberships() => NO_MEMBERSHIPS,
            () if self.fetch.is_some() && self.items.is_empty() => CHECKING,
            () => return None,
        };
        Some(hint.to_owned())
    }

    /// Common toolbar trigger for both spaces, named after the selected
    /// initiative.
    // qual:api
    pub fn button(&self, id: &'static str) -> IconButton {
        let label = self.selected().map_or(TITLE, |item| item.name.as_str());
        menu_button(id, IconName::Users, label.to_owned(), false)
            .max_w(PICKER_MAX_WIDTH)
            .accessibility_label(format!("{TITLE}: {label}"))
    }

    /// The picker as a unit of a toolbar's overflow, keyed `key`, the
    /// picker's anchor, so a hidden picker opens from the overflow menu's
    /// chevron. Inline it is the Space's `button`, which the Space renders
    /// through its own state; in the menu it is an **Initiative: name…**
    /// entry that runs `open`, unavailable while `disabled`.
    pub fn overflow_unit(
        &self,
        key: &'static str,
        disabled: bool,
        button: impl Fn(&mut Window, &mut App) -> Option<AnyElement> + 'static,
        open: impl Fn(&mut Window, &mut App) + 'static,
    ) -> OverflowUnit {
        let name = self
            .selected()
            .map_or_else(|| TITLE.to_owned(), |item| item.name.clone());
        let open = std::rc::Rc::new(open);
        let entry = PopupEntry::new(move || {
            let open = open.clone();
            PopupMenuItem::new(format!("{TITLE}: {name}…"))
                .icon(IconName::Users)
                .disabled(disabled)
                .on_click(move |_, window, cx| open(window, cx))
        });
        OverflowUnit::inline(
            key,
            OverflowUnit::UNMEASURED_WIDTH,
            move |window, cx| button(window, cx).unwrap_or_else(|| div().into_any_element()),
            vec![entry],
        )
    }

    /// The person's initiatives in the API's order, the selected one checked.
    // qual:api
    pub fn menu(&self) -> MenuContents {
        let options = self
            .items
            .iter()
            .map(|item| {
                MenuOption::new(item.id.clone(), item.name.clone())
                    .checked(self.selected.as_ref() == Some(&item.id))
            })
            .collect();
        MenuContents::new(TITLE, "Filter initiatives", options)
    }
}
