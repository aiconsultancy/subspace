//! Shared presentation for the desktop's initiative menus. They offer the
//! person's initiatives only: someone who belongs to one always has one
//! selected, so there is no choice of none.

use gpui_kit::{App, Entity, Pixels, Styled as _, Window, px};
use subspace_framework::dashboard::action::menu_button;
use subspace_framework::dashboard::icon_button::IconButton;
use subspace_framework::dashboard::menu::{Menu, MenuAnchors, MenuContents, MenuOption};
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
