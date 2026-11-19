//! Shared presentation for the desktop's initiative menus.

use gpui_kit::{Pixels, Styled as _, px};
use subspace_framework::dashboard::action::menu_button;
use subspace_framework::dashboard::icon_button::IconButton;
use subspace_framework::dashboard::menu::{MenuContents, MenuOption};
use subspace_framework::lucide::IconName;

use super::{CLEAR, Catalog};

const PICKER_MAX_WIDTH: Pixels = px(280.);

impl Catalog {
    /// Common toolbar trigger for both spaces. The clear label explains the
    /// unscoped mode of the space that presents it.
    // qual:api
    pub fn button(&self, id: &'static str, clear_label: &str) -> IconButton {
        let label = self
            .selected()
            .map_or(clear_label, |item| item.name.as_str());
        menu_button(id, IconName::Users, label.to_owned(), false)
            .max_w(PICKER_MAX_WIDTH)
            .accessibility_label(format!("Initiative: {label}"))
    }

    // qual:api
    pub fn menu(&self, clear_label: &str) -> MenuContents {
        let options = self
            .items
            .iter()
            .map(|item| {
                MenuOption::new(item.id.clone(), item.name.clone())
                    .checked(self.selected.as_ref() == Some(&item.id))
            })
            .chain(std::iter::once(
                MenuOption::new(CLEAR, clear_label.to_owned()).checked(self.selected.is_none()),
            ))
            .collect();
        MenuContents::new("Initiative", "Filter initiatives", options)
    }
}
