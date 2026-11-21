//! Add a Space from the title bar: the plus after the last Space tab, and at
//! the end of the navigation rail, lists the Spaces that are switched off.
//! Choosing one saves and applies the same choice as turning the Space on in
//! Settings, then switches to it. It is the mirror of closing a Space from
//! its tab (`space_menu`), and shares its write.

pub(super) mod menu;

use gpui_kit::{App, Context, SharedString, Window};
use subspace_dashboard::{PaletteCommand, PaletteOption, SubList, Toast, prefs};

use super::{ACTIVE_KEY, OpenSettings, SubspaceApp};
use crate::registry::{self, BoardFactories, BoardRegistration};
use crate::settings::board_choice::{self, Applied};

/// The menu's wording.
pub(super) const ADD_SPACE: &str = "Add a Space";
/// The overflow menu's entry and the palette command, which open a list.
pub(super) const ADD_SPACE_ENTRY: &str = "Add a Space…";
pub(super) const MANAGE_SPACES: &str = "Manage Spaces…";
pub(super) const ALL_ON: &str = "Every Space is on.";
const CHOOSE_SPACE: &str = "Choose a Space to add";
const PALETTE_KEYWORDS: &str = "add open turn on enable space board plus new";
/// The palette option for Manage Spaces…; a Space id never starts with `:`.
const MANAGE_VALUE: &str = ":manage-spaces";

/// A Space the product includes that has no tab, as the add menu shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct OffSpace {
    pub(super) id: &'static str,
    pub(super) title: SharedString,
    /// The board's own one-line `DESCRIPTION`.
    pub(super) description: SharedString,
    pub(super) icon: Option<&'static str>,
}

/// One line of the add menu, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Row {
    Title,
    Space(OffSpace),
    /// The quiet line shown when no Space is switched off.
    AllOn,
    Separator,
    Manage,
}

/// What a palette option of **Add a Space…** does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Pick<'a> {
    Space(&'a str),
    Manage,
}

/// The Spaces the menu offers: those in `included` (the product's Spaces,
/// in switcher order) that `on` (the Spaces with a tab) leaves out.
pub(super) fn offered(included: Vec<OffSpace>, on: &[&str]) -> Vec<OffSpace> {
    included
        .into_iter()
        .filter(|space| !on.contains(&space.id))
        .collect()
}

/// The menu: its title, each Space that is off (or the quiet line when
/// every Space is on), then **Manage Spaces…** after a separator.
pub(super) fn menu_rows(offered: Vec<OffSpace>) -> Vec<Row> {
    let mut rows = vec![Row::Title];
    if offered.is_empty() {
        rows.push(Row::AllOn);
    } else {
        rows.extend(offered.into_iter().map(Row::Space));
    }
    rows.extend([Row::Separator, Row::Manage]);
    rows
}

/// The notice after Space `title` was turned on.
pub(super) fn opened_message(title: &str, applied: Applied) -> String {
    match applied {
        Applied::Now => format!("Opened {title}."),
        Applied::AfterRestart => format!("Restart the app to open {title}."),
    }
}

/// The notice when Space `title` could not be turned on.
pub(super) fn open_failed_message(title: &str, error: &str) -> String {
    format!("Couldn't open {title}: {error}")
}

/// The Space whose Settings page **Manage Spaces…** opens: the first one
/// that is off, or else `first`, the first Space in Settings' Spaces group.
pub(super) fn manage_target(
    offered: &[OffSpace],
    first: Option<&'static str>,
) -> Option<&'static str> {
    offered.first().map(|space| space.id).or(first)
}

/// The palette's list for **Add a Space…**: each Space that is off, with its
/// description, then **Manage Spaces…**.
pub(super) fn palette_options(offered: &[OffSpace]) -> Vec<PaletteOption> {
    offered
        .iter()
        .map(|space| {
            PaletteOption::new(space.id, space.title.clone()).hint(space.description.clone())
        })
        .chain([PaletteOption::new(MANAGE_VALUE, MANAGE_SPACES)])
        .collect()
}

/// The palette list's placeholder, which says when every Space is on.
pub(super) fn palette_placeholder(offered: &[OffSpace]) -> &'static str {
    if offered.is_empty() {
        ALL_ON
    } else {
        CHOOSE_SPACE
    }
}

/// What the palette option with `value` does.
pub(super) fn pick(value: &str) -> Pick<'_> {
    if value == MANAGE_VALUE {
        Pick::Manage
    } else {
        Pick::Space(value)
    }
}

/// The Spaces that are switched off now, in switcher order: the product's
/// Spaces that the running board set leaves out.
pub(super) fn off_spaces(cx: &App) -> Vec<OffSpace> {
    let on: Vec<&'static str> = cx
        .try_global::<BoardFactories>()
        .map(|boards| boards.0.iter().map(|board| board.id).collect())
        .unwrap_or_default();
    let included = registry::included_registrations(cx)
        .iter()
        .map(off_space)
        .collect();
    offered(included, &on)
}

fn off_space(registration: &BoardRegistration) -> OffSpace {
    OffSpace {
        id: registration.id,
        title: registration.title.to_string().into(),
        description: registration.description.to_string().into(),
        icon: registration.icon,
    }
}

/// The palette command **Add a Space…**: the same choices as the menu, as a
/// palette list.
pub(super) fn palette_command(cx: &mut Context<SubspaceApp>) -> PaletteCommand {
    let spaces = off_spaces(cx);
    let app = cx.weak_entity();
    let list = SubList::new(
        palette_placeholder(&spaces),
        palette_options(&spaces),
        move |value, window, cx| {
            _ = app.update(cx, |app, cx| match pick(value) {
                Pick::Space(id) => app.open_space(id, window, cx),
                Pick::Manage => app.manage_spaces(window, cx),
            });
        },
    );
    PaletteCommand::sub(ADD_SPACE_ENTRY, list).keywords(PALETTE_KEYWORDS)
}

impl SubspaceApp {
    /// Turns Space `id` on as its Settings switch and Save do, then makes it
    /// current, or the Space the next start opens on when the change waits
    /// for a restart. Does nothing for a Space that is not off.
    pub(super) fn open_space(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(space) = off_spaces(cx).into_iter().find(|space| space.id == id) else {
            return;
        };
        match board_choice::save_board_enabled(space.id, true, window, cx) {
            Ok(applied) => self.opened_space(&space, applied, window, cx),
            Err(error) => {
                self.push_toast(Toast::error(open_failed_message(&space.title, &error)), cx)
            }
        }
    }

    fn opened_space(
        &mut self,
        space: &OffSpace,
        applied: Applied,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match applied {
            Applied::Now => self.show_opened(space.id, window, cx),
            // The tab comes after a restart; it should then be the current
            // Space, as it would have been now.
            Applied::AfterRestart => prefs::set(cx, ACTIVE_KEY, space.id),
        }
        self.push_toast(Toast::info(opened_message(&space.title, applied)), cx);
    }

    /// Adds the new tab now, rather than when the board-set observer runs
    /// after this update, and switches to it.
    fn show_opened(&mut self, id: &'static str, window: &mut Window, cx: &mut Context<Self>) {
        self.reconcile_boards(window, cx);
        if let Some(index) = self.dashboards.iter().position(|board| board.id == id) {
            self.switch_to(index, window, cx);
        }
    }

    /// Manage Spaces… opens Settings in its Spaces group, at the first Space
    /// that is off, or at the first Space when every one is on.
    pub(super) fn manage_spaces(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let first = registry::included_registrations(cx)
            .first()
            .map(|board| board.id);
        match manage_target(&off_spaces(cx), first) {
            Some(id) => self.open_board_settings(id, window, cx),
            None => self.open_settings(&OpenSettings, window, cx),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ALL_ON, Applied, CHOOSE_SPACE, MANAGE_SPACES, MANAGE_VALUE, OffSpace, Pick, Row,
        manage_target, menu_rows, offered, open_failed_message, opened_message, palette_options,
        palette_placeholder, pick,
    };

    fn space(id: &'static str) -> OffSpace {
        OffSpace {
            id,
            title: id.to_uppercase().into(),
            description: format!("About {id}").into(),
            icon: None,
        }
    }

    fn ids(spaces: &[OffSpace]) -> Vec<&'static str> {
        spaces.iter().map(|space| space.id).collect()
    }

    #[test]
    fn the_menu_offers_the_spaces_without_a_tab_in_product_order() {
        let included = vec![
            space("alpha"),
            space("beta"),
            space("gamma"),
            space("delta"),
        ];
        let shown = offered(included, &["gamma", "alpha"]);
        assert_eq!(ids(&shown), ["beta", "delta"]);
        assert!(offered(vec![space("alpha")], &["alpha"]).is_empty());
    }

    #[test]
    fn the_menu_lists_the_off_spaces_then_manage_spaces() {
        assert_eq!(
            menu_rows(vec![space("beta"), space("delta")]),
            [
                Row::Title,
                Row::Space(space("beta")),
                Row::Space(space("delta")),
                Row::Separator,
                Row::Manage,
            ]
        );
    }

    #[test]
    fn with_every_space_on_the_menu_says_so_and_keeps_manage_spaces() {
        assert_eq!(
            menu_rows(Vec::new()),
            [Row::Title, Row::AllOn, Row::Separator, Row::Manage]
        );
        assert_eq!(palette_placeholder(&[]), ALL_ON);
        assert_eq!(palette_placeholder(&[space("beta")]), CHOOSE_SPACE);
    }

    #[test]
    fn the_three_notices_mirror_closing() {
        assert_eq!(opened_message("Actions", Applied::Now), "Opened Actions.");
        assert_eq!(
            opened_message("Actions", Applied::AfterRestart),
            "Restart the app to open Actions."
        );
        assert_eq!(
            open_failed_message("Actions", "the config file changed"),
            "Couldn't open Actions: the config file changed"
        );
    }

    #[test]
    fn manage_spaces_opens_the_first_off_space_or_else_the_first_space() {
        assert_eq!(
            manage_target(&[space("beta"), space("delta")], Some("alpha")),
            Some("beta")
        );
        assert_eq!(manage_target(&[], Some("alpha")), Some("alpha"));
        assert_eq!(manage_target(&[], None), None);
    }

    #[test]
    fn the_palette_lists_the_same_choices_and_tells_them_apart() {
        let options = palette_options(&[space("beta")]);
        let labels: Vec<_> = options
            .iter()
            .map(|option| option.label.to_string())
            .collect();
        assert_eq!(labels, ["BETA", MANAGE_SPACES]);
        assert_eq!(options[0].hint.as_deref(), Some("About beta"));
        assert_eq!(pick(&options[0].value), Pick::Space("beta"));
        assert_eq!(pick(&options[1].value), Pick::Manage);
        assert_eq!(pick(MANAGE_VALUE), Pick::Manage);
    }
}
