//! Registration-derived destinations, kept independent of native tree state.

use std::collections::HashMap;

use gpui_kit::component::tree::TreeItem;
use subspace_dashboard::settings::SettingsSection;
use subspace_lucide::IconName;

use super::{NavIcon, SettingsPage};

pub(super) struct NavigationModel {
    pub(super) app_items: Vec<TreeItem>,
    pub(super) board_items: Vec<TreeItem>,
    pub(super) pages: HashMap<String, SettingsPage>,
    pub(super) icons: HashMap<String, NavIcon>,
}

#[cfg(test)]
pub(super) fn navigation_model(boards: &[crate::registry::BoardRegistration]) -> NavigationModel {
    with_extensions(boards, &[], false, false)
}

/// The Terminal page, after the built-in pages, for a product with a terminal.
pub(super) const TERMINAL_ITEM: AppItem = (
    "app:terminal",
    "Terminal",
    SettingsPage::Terminal,
    IconName::SquareTerminal,
);

pub(super) fn with_extensions(
    boards: &[crate::registry::BoardRegistration],
    extensions: &[crate::SettingsExtension],
    terminal: bool,
    account: bool,
) -> NavigationModel {
    let mut model = NavigationModel {
        app_items: Vec::new(),
        board_items: Vec::new(),
        pages: HashMap::new(),
        icons: HashMap::new(),
    };
    let terminal = terminal.then_some(TERMINAL_ITEM);
    for (id, label, page, icon) in app_items(account).into_iter().chain(terminal) {
        model.app_items.push(TreeItem::new(id, label));
        model.pages.insert(id.into(), page);
        model.icons.insert(id.into(), NavIcon::Lucide(icon));
    }
    for extension in extensions {
        let id = format!("extension:{}", extension.id);
        model
            .app_items
            .push(TreeItem::new(id.clone(), extension.title));
        model
            .pages
            .insert(id.clone(), SettingsPage::Extension(extension.id));
        model.icons.insert(id, NavIcon::Lucide(extension.icon));
    }
    for registration in boards {
        let id = format!("board:{}", registration.id);
        let children = section_items(
            registration.id,
            registration.settings.sections,
            (&[], &id),
            &mut model,
        );
        model
            .board_items
            .push(TreeItem::new(id.clone(), registration.title.to_string()).children(children));
        model
            .pages
            .insert(id.clone(), SettingsPage::Board(registration.id));
        let icon = registration
            .icon
            .map(NavIcon::Svg)
            .or_else(|| registration.settings.icon.map(NavIcon::Lucide))
            .unwrap_or(NavIcon::Lucide(IconName::Settings));
        model.icons.insert(id, icon);
    }
    model
}

fn section_items(
    board: &'static str,
    sections: &'static [SettingsSection],
    parent: (&[usize], &str),
    model: &mut NavigationModel,
) -> Vec<TreeItem> {
    sections
        .iter()
        .enumerate()
        .map(|(index, section)| {
            let mut path = parent.0.to_vec();
            path.push(index);
            let id = format!("{}/{}", parent.1, section.id);
            let children = section_items(board, section.children, (&path, &id), model);
            model
                .pages
                .insert(id.clone(), SettingsPage::BoardSection { board, path });
            model
                .icons
                .insert(id.clone(), NavIcon::Lucide(section.icon));
            TreeItem::new(id, section.title).children(children)
        })
        .collect()
}

type AppItem = (&'static str, &'static str, SettingsPage, IconName);

/// The Application pages in order. Debug builds end with Developer.
pub(super) fn app_items(account: bool) -> Vec<AppItem> {
    let mut items = vec![
        (
            "app:general",
            "General",
            SettingsPage::General,
            IconName::Settings,
        ),
        (
            "app:account",
            "Account",
            SettingsPage::Account,
            IconName::KeyRound,
        ),
        ("app:chat", "Chat", SettingsPage::Chat, IconName::Bot),
        ("app:mcp", "MCP", SettingsPage::Mcp, IconName::Plug),
        (
            "app:appearance",
            "Appearance",
            SettingsPage::Appearance,
            IconName::Palette,
        ),
        (
            "app:layout",
            "Layout",
            SettingsPage::Layout,
            IconName::PanelsTopLeft,
        ),
        (
            "app:notifications",
            "Notifications",
            SettingsPage::Notifications,
            IconName::Bell,
        ),
        (
            "app:experiments",
            "Experiments",
            SettingsPage::Experiments,
            IconName::FlaskConical,
        ),
    ];
    if !account {
        items.retain(|(_, _, page, _)| *page != SettingsPage::Account);
    }
    #[cfg(debug_assertions)]
    items.push((
        DEVELOPER_ITEM,
        "Developer",
        SettingsPage::Developer,
        IconName::SquareDashedMousePointer,
    ));
    items
}

/// The last Application page, where arrows cross to the board pages.
#[cfg(test)]
pub(in crate::settings) fn last_app_item() -> (String, SettingsPage) {
    let Some((id, _, page, _)) = app_items(false).pop() else {
        return (String::new(), SettingsPage::General);
    };
    (format!("settings-{id}"), page)
}

/// The Developer page's navigation id.
#[cfg(debug_assertions)]
pub(in crate::settings) const DEVELOPER_ITEM: &str = "app:developer";

#[cfg(test)]
mod tests {
    use super::{NavIcon, app_items, navigation_model};

    #[test]
    fn board_roots_use_registered_logos_before_settings_icon_fallbacks() {
        let mut board = crate::settings::navigation_tests::registration();
        board.icon = Some("logos/example.svg");
        board.settings.icon = Some(subspace_lucide::IconName::Palette);
        assert_eq!(
            navigation_model(std::slice::from_ref(&board)).icons["board:future"],
            NavIcon::Svg("logos/example.svg")
        );
        board.icon = None;
        assert_eq!(
            navigation_model(std::slice::from_ref(&board)).icons["board:future"],
            NavIcon::Lucide(subspace_lucide::IconName::Palette)
        );
        board.settings.icon = None;
        assert_eq!(
            navigation_model(std::slice::from_ref(&board)).icons["board:future"],
            NavIcon::Lucide(subspace_lucide::IconName::Settings)
        );
    }

    #[test]
    fn app_pages_precede_registered_boards_and_use_an_experiment_icon() {
        let items = app_items(true);
        let experiments_icon = items
            .iter()
            .find(|item| item.0 == "app:experiments")
            .unwrap()
            .3;
        let labels: Vec<_> = items.iter().map(|item| item.1).collect();
        assert_eq!(
            labels,
            [
                "General",
                "Account",
                "Chat",
                "MCP",
                "Appearance",
                "Layout",
                "Notifications",
                "Experiments",
                "Developer"
            ]
        );
        assert_eq!(
            items.iter().find(|item| item.0 == "app:mcp").unwrap().2,
            super::SettingsPage::Mcp
        );
        assert_eq!(experiments_icon, subspace_lucide::IconName::FlaskConical);
    }

    #[test]
    fn app_pages_omit_account_without_a_provider() {
        assert!(
            app_items(false)
                .iter()
                .all(|(_, _, page, _)| *page != super::SettingsPage::Account)
        );
    }
}
